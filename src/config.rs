use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortMode {
    /// Shuffle headlines randomly
    Random,
    /// Group headlines by source/publication
    BySource,
    /// Newest headlines first (default)
    #[default]
    ByDate,
    /// Oldest headlines first
    ByDateAsc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PauseMode {
    /// Pause when mouse hovers over ticker
    #[default]
    Hover,
    /// Pause when terminal window is focused
    Focus,
    /// Never auto-pause
    Never,
}

#[derive(Parser, Debug)]
#[command(name = "chyron")]
#[command(about = "A TUI news ticker displaying RSS headlines like a stock ticker")]
pub struct CliArgs {
    /// Path to config file (default: ~/.config/chyron/config.toml)
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Path to feeds file (default: ~/.newsboat/urls or ~/.config/chyron/urls)
    #[arg(short, long)]
    pub feeds: Option<PathBuf>,

    /// Delimiter between headlines
    #[arg(short, long)]
    pub delimiter: Option<String>,

    /// Scroll speed in characters per second
    #[arg(short, long)]
    pub speed: Option<u32>,

    /// Sorting mode for headlines
    #[arg(long, value_enum)]
    pub sort: Option<SortMode>,

    /// Pause mode: hover, focus, or never
    #[arg(long, value_enum)]
    pub pause: Option<PauseMode>,

    /// Feed refresh interval in minutes
    #[arg(long)]
    pub refresh_minutes: Option<u64>,

    /// Maximum age of headlines in hours
    #[arg(long)]
    pub max_age_hours: Option<u64>,

    /// Maximum headlines per feed
    #[arg(long)]
    pub max_per_feed: Option<usize>,

    /// Maximum total headlines in rotation
    #[arg(long)]
    pub max_total: Option<usize>,

    /// Hide source prefix on headlines
    #[arg(long)]
    pub hide_source: bool,

    /// Show source prefix on headlines
    #[arg(long)]
    pub show_source: bool,

    /// Validate feeds and exit
    #[arg(long)]
    pub validate: bool,

    /// Show status bar with controls and state
    #[arg(long)]
    pub status_bar: bool,

    /// Hide status bar
    #[arg(long)]
    pub no_status_bar: bool,
}

/// TOML config file structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileConfig {
    pub feeds: Option<String>,
    pub delimiter: Option<String>,
    pub speed: Option<u32>,
    pub sort: Option<SortMode>,
    pub pause: Option<PauseMode>,
    pub refresh_minutes: Option<u64>,
    pub max_age_hours: Option<u64>,
    pub max_per_feed: Option<usize>,
    pub max_total: Option<usize>,
    pub show_source: Option<bool>,
    pub status_bar: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub feeds_path: PathBuf,
    pub delimiter: String,
    pub speed: u32,
    pub sort: SortMode,
    pub pause_mode: PauseMode,
    pub refresh_interval: Duration,
    pub max_age: Duration,
    pub max_per_feed: usize,
    pub max_total: usize,
    pub show_source: bool,
    pub validate_only: bool,
    pub show_status_bar: bool,
}

impl Config {
    pub fn load(args: CliArgs) -> Result<Self> {
        // Load config file if it exists
        let config_path = args.config.clone().unwrap_or_else(|| {
            get_config_dir().join("config.toml")
        });

        let file_config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;
            toml::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?
        } else {
            FileConfig::default()
        };

        // CLI args override file config, file config overrides defaults
        let feeds_path = if let Some(path) = args.feeds {
            path
        } else if let Some(path) = &file_config.feeds {
            PathBuf::from(path)
        } else {
            discover_feeds_file()?
        };

        let delimiter = args.delimiter
            .or(file_config.delimiter)
            .unwrap_or_else(|| " ••• ".to_string());

        let speed = args.speed
            .or(file_config.speed)
            .unwrap_or(8);

        let sort = args.sort
            .or(file_config.sort)
            .unwrap_or_default();

        let pause_mode = args.pause
            .or(file_config.pause)
            .unwrap_or_default();

        let refresh_minutes = args.refresh_minutes
            .or(file_config.refresh_minutes)
            .unwrap_or(5);

        let max_age_hours = args.max_age_hours
            .or(file_config.max_age_hours)
            .unwrap_or(24);

        let max_per_feed = args.max_per_feed
            .or(file_config.max_per_feed)
            .unwrap_or(10);

        let max_total = args.max_total
            .or(file_config.max_total)
            .unwrap_or(100);

        // For booleans, CLI flags override file config
        let show_source = if args.hide_source {
            false
        } else if args.show_source {
            true
        } else {
            file_config.show_source.unwrap_or(true)
        };

        let show_status_bar = if args.no_status_bar {
            false
        } else if args.status_bar {
            true
        } else {
            file_config.status_bar.unwrap_or(false)
        };

        Ok(Self {
            feeds_path,
            delimiter,
            speed,
            sort,
            pause_mode,
            refresh_interval: Duration::from_secs(refresh_minutes * 60),
            max_age: Duration::from_secs(max_age_hours * 3600),
            max_per_feed,
            max_total,
            show_source,
            validate_only: args.validate,
            show_status_bar,
        })
    }
}

fn get_config_dir() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("", "", "chyron") {
        proj_dirs.config_dir().to_path_buf()
    } else {
        dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("chyron")
    }
}

/// Discover feeds file in priority order:
/// 1. ~/.newsboat/urls
/// 2. ~/.config/chyron/urls
fn discover_feeds_file() -> Result<PathBuf> {
    // Try newsboat first
    let home = dirs_next::home_dir().context("Could not determine home directory")?;
    let newsboat_path = home.join(".newsboat").join("urls");
    if newsboat_path.exists() {
        return Ok(newsboat_path);
    }

    // Try XDG config
    let config_dir = get_config_dir();
    let config_path = config_dir.join("urls");
    if config_path.exists() {
        return Ok(config_path);
    }

    // Return this path even if it doesn't exist, so error message is helpful
    Ok(config_path)
}

/// Generate example config file content
pub fn example_config() -> &'static str {
    r#"# Chyron configuration

# Path to feeds file (default: ~/.newsboat/urls or ~/.config/chyron/urls)
# feeds = "~/.config/chyron/urls"

# Delimiter between headlines
delimiter = " ••• "

# Scroll speed in characters per second
speed = 8

# Sort mode: random, by_source, by_date, by_date_asc
sort = "by_date"

# Pause mode: hover (pause on mouse hover), focus (pause when window focused), never
pause = "hover"

# Feed refresh interval in minutes
refresh_minutes = 5

# Maximum age of headlines in hours
max_age_hours = 24

# Maximum headlines per feed
max_per_feed = 10

# Maximum total headlines in rotation
max_total = 100

# Show source prefix on headlines [Source Name]
show_source = true

# Show status bar at bottom
status_bar = false
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_sort_mode() {
        assert_eq!(SortMode::default(), SortMode::ByDate);
    }

    #[test]
    fn test_default_pause_mode() {
        assert_eq!(PauseMode::default(), PauseMode::Hover);
    }

    #[test]
    fn test_parse_file_config() {
        let toml = r#"
            delimiter = " | "
            speed = 12
            sort = "random"
            pause = "focus"
        "#;
        let config: FileConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.delimiter, Some(" | ".to_string()));
        assert_eq!(config.speed, Some(12));
        assert_eq!(config.sort, Some(SortMode::Random));
        assert_eq!(config.pause, Some(PauseMode::Focus));
    }
}

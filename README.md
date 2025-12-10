# chyron

A TUI RSS reader that displays RSS/Atom headlines like a crawling ticker.

## Features

- Scrolling ticker with smooth animation
- Clickable headlines (OSC 8 hyperlinks in supported terminals)
- Auto-pauses when mouse hovers over ticker for easy clicking
- Reads feeds from newsboat config or custom file
- TOML configuration file support
- Configurable speed, delimiter, sorting, and more

## Installation

```bash
cargo build --release
cp target/release/chyron ~/.local/bin/
```

## Usage

```bash
# Run with defaults (reads ~/.config/chyron/config.toml for settings)
chyron

# Validate feeds without running ticker
chyron --validate

# Custom speed and delimiter (overrides config file)
chyron --speed 12 --delimiter " | "

# Show status bar with controls
chyron --status-bar
```

## Configuration

Chyron uses a TOML config file at `~/.config/chyron/config.toml`. CLI arguments override config file settings.

Example config:

```toml
# Path to feeds file (default: ~/.newsboat/urls or ~/.config/chyron/urls)
# feeds = "~/.config/chyron/urls"

# Delimiter between headlines
delimiter = " ••• "

# Scroll speed in characters per second
speed = 8

# Sort mode: random, by_source, by_date, by_date_asc
sort = "by_date"

# Pause mode: hover (mouse hover), focus (window focus), never
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
```

## Feed Configuration

Chyron looks for feeds in this order:

1. `~/.newsboat/urls` - if you use newsboat
2. `~/.config/chyron/urls` - app-specific config

Format is one URL per line (newsboat-compatible):

```
https://example.com/feed.xml
https://news.ycombinator.com/rss
https://www.theverge.com/rss/index.xml "tech"
```

Tags after URLs are ignored.

## Controls

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit |
| `Space` | Toggle pause |
| `+` / `=` | Increase speed |
| `-` / `_` | Decrease speed |
| `r` | Refresh feeds |
| `Ctrl+C` | Quit |
| Mouse click | Open headline link in browser |

## CLI Options

All CLI options override config file settings.

| Option | Description |
|--------|-------------|
| `-c, --config <PATH>` | Path to config file |
| `-f, --feeds <PATH>` | Path to feeds file |
| `-d, --delimiter <STR>` | Separator between headlines |
| `-s, --speed <N>` | Scroll speed (characters/second) |
| `--sort <MODE>` | Sort: `random`, `by-source`, `by-date`, `by-date-asc` |
| `--pause <MODE>` | Pause: `hover`, `focus`, `never` |
| `--refresh-minutes <N>` | Feed refresh interval |
| `--max-age-hours <N>` | Drop headlines older than this |
| `--max-per-feed <N>` | Max headlines per feed |
| `--max-total <N>` | Max total headlines in rotation |
| `--show-source` | Show `[Source]` prefix |
| `--hide-source` | Hide `[Source]` prefix |
| `--status-bar` | Show status bar |
| `--no-status-bar` | Hide status bar |
| `--validate` | Check feeds and exit |

## Pause Modes

- **hover** (default): Pause when mouse hovers over the ticker line
- **focus**: Pause when terminal window has focus
- **never**: Never auto-pause (use spacebar for manual pause)

## Terminal Compatibility

Clickable links require a terminal with OSC 8 hyperlink support:

- iTerm2
- kitty
- WezTerm
- Alacritty (0.11+)
- Windows Terminal
- Most modern terminal emulators

## License

MIT

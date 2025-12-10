use crate::config::{Config, SortMode};
use crate::feeds::Headline;
use chrono::Utc;
use rand::seq::SliceRandom;

/// Manages the scrolling ticker state and headline rotation
pub struct Ticker {
    /// All headlines currently in rotation
    headlines: Vec<Headline>,
    /// The rendered ticker text (headlines joined with delimiter)
    ticker_text: String,
    /// Cached char vector for efficient indexing
    ticker_chars: Vec<char>,
    /// Segments mapping character ranges to URLs for click detection
    segments: Vec<TickerSegment>,
    /// Current scroll offset as float for smooth scrolling
    offset: f64,
    /// Characters per second
    speed: u32,
    /// Delimiter between headlines
    delimiter: String,
    /// Whether to show source prefix
    show_source: bool,
    /// Whether ticker is paused
    paused: bool,
}

/// A segment of the ticker text that maps to a URL
#[derive(Debug, Clone)]
pub struct TickerSegment {
    pub start: usize,
    pub end: usize,
    pub url: Option<String>,
}

impl Ticker {
    pub fn new(config: &Config) -> Self {
        Self {
            headlines: Vec::new(),
            ticker_text: String::new(),
            ticker_chars: Vec::new(),
            segments: Vec::new(),
            offset: 0.0,
            speed: config.speed,
            delimiter: config.delimiter.clone(),
            show_source: config.show_source,
            paused: false,
        }
    }

    /// Update headlines and rebuild the ticker text
    pub fn set_headlines(&mut self, mut headlines: Vec<Headline>, sort: SortMode) {
        // Sort headlines according to mode
        match sort {
            SortMode::Random => {
                let mut rng = rand::rng();
                headlines.shuffle(&mut rng);
            }
            SortMode::BySource => {
                headlines.sort_by(|a, b| a.source.cmp(&b.source));
            }
            SortMode::ByDate => {
                headlines.sort_by(|a, b| {
                    let a_date = a.published.unwrap_or(Utc::now());
                    let b_date = b.published.unwrap_or(Utc::now());
                    b_date.cmp(&a_date) // newest first
                });
            }
            SortMode::ByDateAsc => {
                headlines.sort_by(|a, b| {
                    let a_date = a.published.unwrap_or(Utc::now());
                    let b_date = b.published.unwrap_or(Utc::now());
                    a_date.cmp(&b_date) // oldest first
                });
            }
        }

        self.headlines = headlines;
        self.rebuild_ticker_text();

        // Reset offset if it's now out of bounds
        let len = self.ticker_chars.len() as f64;
        if len > 0.0 && self.offset >= len {
            self.offset = 0.0;
        }
    }

    /// Rebuild the ticker text from current headlines
    fn rebuild_ticker_text(&mut self) {
        self.segments.clear();

        if self.headlines.is_empty() {
            self.ticker_text = "No headlines available. Check your feed configuration.".to_string();
            return;
        }

        let mut text = String::new();
        let mut pos = 0;

        for (idx, headline) in self.headlines.iter().enumerate() {
            if idx > 0 {
                text.push_str(&self.delimiter);
                pos += self.delimiter.chars().count();
            }

            let segment_start = pos;

            // Add source prefix if enabled
            let display_text = if self.show_source {
                format!("[{}] {}", headline.source, headline.title)
            } else {
                headline.title.clone()
            };

            text.push_str(&display_text);
            pos += display_text.chars().count();

            self.segments.push(TickerSegment {
                start: segment_start,
                end: pos,
                url: headline.url.clone(),
            });
        }

        // Add trailing delimiter for seamless looping
        text.push_str(&self.delimiter);

        self.ticker_chars = text.chars().collect();
        self.ticker_text = text;
    }

    /// Advance the ticker by the given time delta
    pub fn tick(&mut self, delta_secs: f64) {
        if self.paused || self.ticker_chars.is_empty() {
            return;
        }

        let len = self.ticker_chars.len() as f64;
        self.offset += delta_secs * self.speed as f64;

        // Wrap around
        if self.offset >= len {
            self.offset -= len;
        }
    }

    /// Get the fractional part of offset (0.0 to 1.0) for sub-character rendering
    pub fn get_fractional_offset(&self) -> f64 {
        self.offset.fract()
    }

    /// Get the visible portion of ticker text for a given width
    /// Returns (text, fractional_offset) where fractional_offset is 0.0-1.0
    pub fn get_visible_text(&self, width: usize) -> String {
        if self.ticker_chars.is_empty() {
            return String::new();
        }

        let len = self.ticker_chars.len();
        let base_offset = self.offset as usize;

        let mut result = String::with_capacity(width + 1);
        // Get one extra char for smooth scrolling effect
        for i in 0..=width {
            let idx = (base_offset + i) % len;
            result.push(self.ticker_chars[idx]);
        }
        result
    }

    /// Get segments that are visible at the current offset for a given width
    pub fn get_visible_segments(&self, width: usize) -> Vec<VisibleSegment> {
        if self.ticker_chars.is_empty() {
            return Vec::new();
        }

        let len = self.ticker_chars.len();
        let mut visible = Vec::new();
        let base_offset = self.offset as usize;

        for segment in &self.segments {
            // Check if segment overlaps with visible window
            // Account for wrapping
            let vis_start = base_offset;
            let vis_end = base_offset + width;

            // Segment could appear in original position or wrapped
            for wrap_offset in [0, len] {
                let seg_start = segment.start + wrap_offset;
                let seg_end = segment.end + wrap_offset;

                if seg_start < vis_end && seg_end > vis_start {
                    let start_in_view = seg_start.saturating_sub(vis_start);
                    let end_in_view = (seg_end - vis_start).min(width);

                    if start_in_view < width && end_in_view > start_in_view {
                        visible.push(VisibleSegment {
                            start: start_in_view,
                            end: end_in_view,
                            url: segment.url.clone(),
                        });
                    }
                }
            }
        }

        visible
    }

    /// Find URL at a given screen position (x coordinate)
    pub fn get_url_at_position(&self, x: usize, width: usize) -> Option<String> {
        let segments = self.get_visible_segments(width);
        for segment in segments {
            if x >= segment.start && x < segment.end {
                return segment.url;
            }
        }
        None
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub fn headline_count(&self) -> usize {
        self.headlines.len()
    }

    pub fn set_speed(&mut self, speed: u32) {
        self.speed = speed;
    }

    pub fn speed(&self) -> u32 {
        self.speed
    }
}

/// A segment visible on screen with its position
#[derive(Debug, Clone)]
pub struct VisibleSegment {
    pub start: usize,
    pub end: usize,
    pub url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            feeds_path: std::path::PathBuf::new(),
            delimiter: " | ".to_string(),
            speed: 10,
            sort: SortMode::ByDate,
            pause_mode: crate::config::PauseMode::Hover,
            refresh_interval: std::time::Duration::from_secs(300),
            max_age: std::time::Duration::from_secs(86400),
            max_per_feed: 10,
            max_total: 100,
            show_source: false,
            validate_only: false,
            show_status_bar: false,
        }
    }

    #[test]
    fn test_ticker_basic() {
        let config = test_config();
        let mut ticker = Ticker::new(&config);

        let headlines = vec![
            Headline {
                title: "Hello".to_string(),
                url: Some("https://example.com".to_string()),
                source: "Test".to_string(),
                published: None,
            },
            Headline {
                title: "World".to_string(),
                url: None,
                source: "Test".to_string(),
                published: None,
            },
        ];

        ticker.set_headlines(headlines, SortMode::ByDate);
        assert_eq!(ticker.headline_count(), 2);

        let visible = ticker.get_visible_text(5);
        // Returns width+1 chars for smooth scrolling
        assert_eq!(visible.chars().count(), 6);
    }

    #[test]
    fn test_ticker_pause() {
        let config = test_config();
        let mut ticker = Ticker::new(&config);
        assert!(!ticker.is_paused());

        ticker.pause();
        assert!(ticker.is_paused());

        ticker.resume();
        assert!(!ticker.is_paused());
    }
}

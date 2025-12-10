use crate::config::{Config, RotationMode, SortMode};
use crate::feeds::Headline;
use chrono::Utc;
use rand::seq::SliceRandom;
use std::collections::HashSet;

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
    /// Rotation mode (fair vs continuous)
    rotation_mode: RotationMode,
    /// URLs of headlines that have been fully shown (for fair rotation)
    shown_urls: HashSet<String>,
    /// Index of current headline being displayed (for tracking when shown)
    current_headline_idx: usize,
    /// Character position where current headline ends
    current_headline_end: usize,
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
            rotation_mode: config.rotation,
            shown_urls: HashSet::new(),
            current_headline_idx: 0,
            current_headline_end: 0,
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

        // For fair rotation, prioritize unshown headlines
        if self.rotation_mode == RotationMode::Fair {
            // Partition into unshown and shown
            let (unshown, shown): (Vec<_>, Vec<_>) = headlines
                .into_iter()
                .partition(|h| !self.is_headline_shown(h));

            // Clean up shown_urls: remove any that aren't in the new headline set
            let all_urls: HashSet<String> = unshown
                .iter()
                .chain(shown.iter())
                .filter_map(|h| h.url.clone())
                .collect();
            self.shown_urls.retain(|url| all_urls.contains(url));

            // If all headlines have been shown, reset tracking
            if unshown.is_empty() && !shown.is_empty() {
                self.shown_urls.clear();
                headlines = shown;
            } else {
                // Put unshown first, then shown
                headlines = unshown;
                headlines.extend(shown);
            }
        }

        self.headlines = headlines;
        self.rebuild_ticker_text();

        // Reset offset if it's now out of bounds
        let len = self.ticker_chars.len() as f64;
        if len > 0.0 && self.offset >= len {
            self.offset = 0.0;
        }

        // Reset tracking for new headline set
        self.current_headline_idx = 0;
        self.current_headline_end = if !self.segments.is_empty() {
            self.segments[0].end
        } else {
            0
        };
    }

    /// Check if a headline has been shown (by URL or title if no URL)
    fn is_headline_shown(&self, headline: &Headline) -> bool {
        if let Some(url) = &headline.url {
            self.shown_urls.contains(url)
        } else {
            // For headlines without URLs, use title as key
            self.shown_urls.contains(&headline.title)
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

        let old_offset = self.offset as usize;
        let len = self.ticker_chars.len() as f64;
        self.offset += delta_secs * self.speed as f64;

        // Wrap around
        if self.offset >= len {
            self.offset -= len;
        }

        // Track shown headlines for fair rotation
        if self.rotation_mode == RotationMode::Fair && !self.headlines.is_empty() {
            let new_offset = self.offset as usize;

            // Check if we've scrolled past the end of the current headline
            // A headline is "shown" once its end position has scrolled off the left edge
            if new_offset > old_offset {
                // Normal forward scrolling
                if old_offset < self.current_headline_end && new_offset >= self.current_headline_end {
                    self.mark_current_headline_shown();
                    self.advance_to_next_headline();
                }
            } else if new_offset < old_offset {
                // Wrapped around - mark current and reset
                self.mark_current_headline_shown();
                self.current_headline_idx = 0;
                self.current_headline_end = if !self.segments.is_empty() {
                    self.segments[0].end
                } else {
                    0
                };
            }
        }
    }

    /// Mark the current headline as shown
    fn mark_current_headline_shown(&mut self) {
        if self.current_headline_idx < self.headlines.len() {
            let key = if let Some(url) = &self.headlines[self.current_headline_idx].url {
                url.clone()
            } else {
                self.headlines[self.current_headline_idx].title.clone()
            };
            self.shown_urls.insert(key);
        }
    }

    /// Advance tracking to the next headline
    fn advance_to_next_headline(&mut self) {
        self.current_headline_idx += 1;
        if self.current_headline_idx < self.segments.len() {
            self.current_headline_end = self.segments[self.current_headline_idx].end;
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
            click_modifier: crate::config::ClickModifier::None,
            rotation: RotationMode::Continuous,
            config_path: None,
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

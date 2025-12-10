use crate::ticker::Ticker;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style, Stylize},
    widgets::Widget,
};
use std::io::{self, Write};

/// Widget for rendering the ticker with clickable links
pub struct TickerWidget<'a> {
    ticker: &'a Ticker,
    hovered_x: Option<u16>,
}

impl<'a> TickerWidget<'a> {
    pub fn new(ticker: &'a Ticker) -> Self {
        Self {
            ticker,
            hovered_x: None,
        }
    }

    pub fn hovered(mut self, x: Option<u16>) -> Self {
        self.hovered_x = x;
        self
    }
}

impl Widget for TickerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let width = area.width as usize;
        let visible_text = self.ticker.get_visible_text(width);
        let visible_segments = self.ticker.get_visible_segments(width);
        let frac = self.ticker.get_fractional_offset();
        let chars: Vec<char> = visible_text.chars().collect();

        // Render character by character
        // We get width+1 chars, and use fractional offset to blend between them
        for i in 0..width {
            let x = area.x + i as u16;
            let y = area.y;

            // Select character based on fractional offset
            // When frac > 0.5, we're closer to showing the next character
            let char_idx = if frac > 0.5 { i + 1 } else { i };
            let ch = chars.get(char_idx).copied().unwrap_or(' ');

            // Check if this position is part of a clickable segment
            let is_clickable = visible_segments
                .iter()
                .any(|seg| i >= seg.start && i < seg.end && seg.url.is_some());

            // Check if this position is being hovered
            let is_hovered = self.hovered_x.map(|hx| hx == x).unwrap_or(false);

            let style = if is_hovered && is_clickable {
                Style::default().fg(Color::Cyan).underlined()
            } else if is_clickable {
                Style::default().underlined()
            } else {
                Style::default()
            };

            buf[(x, y)].set_char(ch).set_style(style);
        }
    }
}

/// Write OSC 8 hyperlinks directly to terminal for click support
/// This bypasses ratatui's buffer to inject escape sequences
pub struct HyperlinkRenderer {
    buffer: Vec<u8>,
}

impl HyperlinkRenderer {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Render ticker line with embedded hyperlinks
    pub fn render_ticker_line(
        &mut self,
        ticker: &Ticker,
        width: usize,
        row: u16,
    ) -> io::Result<()> {
        self.buffer.clear();

        let visible_text = ticker.get_visible_text(width);
        let visible_segments = ticker.get_visible_segments(width);
        let frac = ticker.get_fractional_offset();
        let all_chars: Vec<char> = visible_text.chars().collect();

        // Apply same fractional offset logic as widget
        let chars: Vec<char> = (0..width)
            .map(|i| {
                let char_idx = if frac > 0.5 { i + 1 } else { i };
                all_chars.get(char_idx).copied().unwrap_or(' ')
            })
            .collect();

        // Move cursor to position
        write!(self.buffer, "\x1b[{};1H", row + 1)?;

        let mut pos = 0;
        while pos < chars.len() && pos < width {
            // Find if we're starting a segment
            if let Some(seg) = visible_segments
                .iter()
                .find(|s| s.start == pos && s.url.is_some())
            {
                let url = seg.url.as_ref().unwrap();
                let end = seg.end.min(width);
                let segment_text: String = chars[pos..end].iter().collect();

                // Write hyperlink with OSC 8
                write!(self.buffer, "\x1b]8;;{}\x07{}\x1b]8;;\x07", url, segment_text)?;
                pos = end;
            } else {
                // Regular character
                write!(self.buffer, "{}", chars[pos])?;
                pos += 1;
            }
        }

        // Pad remaining width
        for _ in pos..width {
            write!(self.buffer, " ")?;
        }

        Ok(())
    }

    /// Flush buffer to stdout
    pub fn flush(&self) -> io::Result<()> {
        let mut stdout = io::stdout();
        stdout.write_all(&self.buffer)?;
        stdout.flush()
    }
}

/// Status bar widget showing ticker state
pub struct StatusBar<'a> {
    headline_count: usize,
    paused: bool,
    speed: u32,
    status_msg: Option<&'a str>,
}

impl<'a> StatusBar<'a> {
    pub fn new(ticker: &Ticker) -> Self {
        Self {
            headline_count: ticker.headline_count(),
            paused: ticker.is_paused(),
            speed: ticker.speed(),
            status_msg: None,
        }
    }

    pub fn with_message(mut self, msg: &'a str) -> Self {
        self.status_msg = Some(msg);
        self
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 {
            return;
        }

        let pause_indicator = if self.paused { "⏸ PAUSED" } else { "▶ PLAYING" };

        let status = if let Some(msg) = self.status_msg {
            format!(
                " {} | {} headlines | speed: {} | {} ",
                pause_indicator, self.headline_count, self.speed, msg
            )
        } else {
            format!(
                " {} | {} headlines | speed: {} | q=quit space=pause ±=speed ",
                pause_indicator, self.headline_count, self.speed
            )
        };

        let style = Style::default().fg(Color::DarkGray);

        for (i, ch) in status.chars().enumerate() {
            if i >= area.width as usize {
                break;
            }
            buf[(area.x + i as u16, area.y)]
                .set_char(ch)
                .set_style(style);
        }
    }
}


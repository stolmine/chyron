use crate::config::{Config, PauseMode};
use crate::feeds::{self, Headline};
use crate::ticker::Ticker;
use crate::ui::{HyperlinkRenderer, StatusBar, TickerWidget};
use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers,
    MouseEventKind,
};
use crossterm::terminal::{
    self, DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, cursor};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub struct App {
    config: Config,
    ticker: Arc<RwLock<Ticker>>,
    client: reqwest::Client,
    feed_urls: Vec<String>,
    running: bool,
    status_message: Option<String>,
    mouse_x: Option<u16>,
    mouse_y: Option<u16>,
    terminal_focused: bool,
    last_refresh: Instant,
    ticker_row: u16,
}

impl App {
    pub async fn new(config: Config) -> Result<Self> {
        let client = feeds::create_http_client()?;
        let feed_urls = feeds::parse_feeds_file(&config.feeds_path).await?;
        let ticker = Arc::new(RwLock::new(Ticker::new(&config)));

        Ok(Self {
            config,
            ticker,
            client,
            feed_urls,
            running: true,
            status_message: None,
            mouse_x: None,
            mouse_y: None,
            terminal_focused: true,
            last_refresh: Instant::now(),
            ticker_row: 0,
        })
    }

    /// Fetch all feeds and update ticker
    pub async fn refresh_feeds(&mut self) -> Result<()> {
        let mut all_headlines: Vec<Headline> = Vec::new();

        for url in &self.feed_urls {
            match feeds::fetch_feed(
                &self.client,
                url,
                self.config.max_per_feed,
                self.config.max_age,
            )
            .await
            {
                Ok((_source, mut headlines)) => {
                    all_headlines.append(&mut headlines);
                }
                Err(e) => {
                    eprintln!("Error fetching {}: {}", url, e);
                }
            }
        }

        // Apply max_total limit
        all_headlines.truncate(self.config.max_total);

        let mut ticker = self.ticker.write().await;
        ticker.set_headlines(all_headlines, self.config.sort);
        self.last_refresh = Instant::now();

        Ok(())
    }

    /// Main application loop
    pub async fn run(&mut self) -> Result<()> {
        // Initial feed fetch
        self.status_message = Some("Loading feeds...".to_string());
        self.refresh_feeds().await?;
        self.status_message = None;

        // Setup terminal
        let mut terminal = self.setup_terminal()?;

        let tick_rate = Duration::from_millis(16); // ~60 FPS
        let mut last_tick = Instant::now();

        while self.running {
            // Handle events
            if event::poll(Duration::from_millis(1))? {
                self.handle_event().await?;
            }

            // Update ticker
            let elapsed = last_tick.elapsed();
            if elapsed >= tick_rate {
                let delta = elapsed.as_secs_f64();
                {
                    let mut ticker = self.ticker.write().await;

                    // Handle pause mode
                    match self.config.pause_mode {
                        PauseMode::Hover => {
                            let mouse_on_ticker = self.terminal_focused
                                && self.mouse_y.map(|y| y == self.ticker_row).unwrap_or(false);
                            if mouse_on_ticker {
                                ticker.pause();
                            } else {
                                ticker.resume();
                            }
                        }
                        PauseMode::Focus => {
                            if self.terminal_focused {
                                ticker.pause();
                            } else {
                                ticker.resume();
                            }
                        }
                        PauseMode::Never => {
                            // Don't auto-pause, only manual pause via spacebar
                        }
                    }

                    ticker.tick(delta);
                }
                last_tick = Instant::now();

                // Check if refresh needed
                if self.last_refresh.elapsed() >= self.config.refresh_interval {
                    self.refresh_feeds().await?;
                }
            }

            // Render
            self.render(&mut terminal).await?;
        }

        self.restore_terminal(&mut terminal)?;
        Ok(())
    }

    fn setup_terminal(&self) -> Result<Terminal<CrosstermBackend<Stdout>>> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            DisableLineWrap,
            event::EnableFocusChange,
            cursor::Hide
        )?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(terminal)
    }

    fn restore_terminal(
        &self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        terminal::disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            EnableLineWrap,
            event::DisableFocusChange,
            cursor::Show
        )?;
        Ok(())
    }

    async fn handle_event(&mut self) -> Result<()> {
        match event::read()? {
            Event::Key(key) => {
                self.handle_key(key.code, key.modifiers).await?;
            }
            Event::Mouse(mouse) => {
                self.handle_mouse(mouse).await?;
            }
            Event::FocusGained => {
                self.terminal_focused = true;
            }
            Event::FocusLost => {
                self.terminal_focused = false;
                // Clear mouse position so ticker resumes
                self.mouse_x = None;
                self.mouse_y = None;
            }
            Event::Resize(_, _) => {
                // Terminal will handle redraw
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.running = false;
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
            }
            KeyCode::Char(' ') => {
                let mut ticker = self.ticker.write().await;
                ticker.toggle_pause();
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let mut ticker = self.ticker.write().await;
                let speed = ticker.speed();
                ticker.set_speed(speed.saturating_add(2).min(100));
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                let mut ticker = self.ticker.write().await;
                let speed = ticker.speed();
                ticker.set_speed(speed.saturating_sub(2).max(1));
            }
            KeyCode::Char('r') => {
                self.status_message = Some("Refreshing feeds...".to_string());
                self.refresh_feeds().await?;
                self.status_message = None;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_mouse(&mut self, mouse: event::MouseEvent) -> Result<()> {
        match mouse.kind {
            MouseEventKind::Moved => {
                self.mouse_x = Some(mouse.column);
                self.mouse_y = Some(mouse.row);
            }
            MouseEventKind::Down(event::MouseButton::Left) => {
                // Check for click on hyperlink
                let ticker = self.ticker.read().await;
                let term_width = terminal::size()?.0 as usize;
                if let Some(url) = ticker.get_url_at_position(mouse.column as usize, term_width) {
                    drop(ticker);
                    self.open_url(&url)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn open_url(&self, url: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open").arg(url).spawn()?;
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open").arg(url).spawn()?;
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", url])
                .spawn()?;
        }
        Ok(())
    }

    async fn render(
        &self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let ticker = self.ticker.read().await;
        let mouse_x = self.mouse_x;
        let status_msg = self.status_message.clone();
        let show_status = self.config.show_status_bar;

        // Calculate ticker row position for centering
        let size = terminal.size()?;
        let content_height = if show_status { 2 } else { 1 };
        let top_padding = size.height.saturating_sub(content_height) / 2;
        self.ticker_row = top_padding;

        terminal.draw(|frame| {
            let area = frame.area();

            // Create layout with centering
            let outer_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(top_padding),
                    Constraint::Length(content_height),
                    Constraint::Min(0),
                ])
                .split(area);

            let content_area = outer_chunks[1];

            if show_status {
                // Split content area into ticker and status bar
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Length(1)])
                    .split(content_area);

                // Render ticker
                let ticker_widget = TickerWidget::new(&ticker).hovered(mouse_x);
                frame.render_widget(ticker_widget, chunks[0]);

                // Render status bar
                let status_bar = if let Some(msg) = &status_msg {
                    StatusBar::new(&ticker).with_message(msg)
                } else {
                    StatusBar::new(&ticker)
                };
                frame.render_widget(status_bar, chunks[1]);
            } else {
                // Just ticker, centered
                let ticker_widget = TickerWidget::new(&ticker).hovered(mouse_x);
                frame.render_widget(ticker_widget, content_area);
            }
        })?;

        // Render hyperlinks overlay (OSC 8) at the correct row
        let mut renderer = HyperlinkRenderer::new();
        renderer.render_ticker_line(&ticker, size.width as usize, self.ticker_row)?;
        renderer.flush()?;

        Ok(())
    }
}

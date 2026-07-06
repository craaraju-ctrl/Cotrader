//! rat-tui — Modern Terminal UI for Trading Real-time Edge Decision Optimisation.
//!
//! Component-based architecture using ratatui 0.30+.
//!
//! Architecture:
//!   App State → Components → Render
//!   Each component handles its own events and rendering.
//!
//! Keys:
//!   q / Ctrl-C   Quit
//!   Tab / 1-9    Switch tabs
//!   /            Search/filter
//!   ?            Toggle help overlay
//!   ↑ / ↓        Navigate
//!   Enter        Select/Confirm
//!   Esc          Back/Cancel

mod app;
mod theme;
mod components;
mod api_client;

use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, Tab, POLL_INTERVAL};
use components::Component;
use components::header::Header;
use components::tabs::TabsComponent;
use components::dashboard::DashboardComponent;
use components::positions::PositionsComponent;
use components::command_palette::CommandPalette;
use components::orderbook::OrderBookComponent;
use components::agents::AgentsComponent;
use components::performance::PerformanceComponent;
use components::health::HealthComponent;
use components::settings::SettingsComponent;
use components::help::HelpComponent;
use components::footer::FooterComponent;
use components::status_bar::StatusBarComponent;
use api_client::{ApiMessage, StatusMsg, start_api_client};

struct AppController {
    app: App,
    rx: std::sync::mpsc::Receiver<ApiMessage>,
    cmd_tx: std::sync::mpsc::Sender<StatusMsg>,
    header: Header,
    tabs: TabsComponent,
    dashboard: DashboardComponent,
    positions: PositionsComponent,
    orderbook: OrderBookComponent,
    agents: AgentsComponent,
    performance: PerformanceComponent,
    health: HealthComponent,
    settings: SettingsComponent,
    help: HelpComponent,
    footer: FooterComponent,
    status_bar: StatusBarComponent,
    command_palette: CommandPalette,
}

impl AppController {
    fn new() -> Self {
        // Determine orchestrator API base URL from env or default
        let api_base = std::env::var("RAT_API_URL")
            .unwrap_or_else(|_| "http://localhost:8082/api".to_string());
        let (rx, cmd_tx) = start_api_client(&api_base);
        Self {
            app: App::new(),
            rx,
            cmd_tx,
            header: Header,
            tabs: TabsComponent,
            dashboard: DashboardComponent,
            positions: PositionsComponent,
            orderbook: OrderBookComponent,
            agents: AgentsComponent,
            performance: PerformanceComponent,
            health: HealthComponent,
            settings: SettingsComponent,
            help: HelpComponent,
            footer: FooterComponent,
            status_bar: StatusBarComponent,
    command_palette: components::command_palette::CommandPalette,
        }
    }

    fn handle_event(&mut self, event: Event) -> bool {
        match event {
            Event::Key(key) => {
                // Global keys
                if key.code == KeyCode::Char('q') && !self.app.show_search {
                    return true;
                }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return true;
                }
                if key.code == KeyCode::Char('?') && self.app.selected_tab != Tab::Help {
                    self.app.show_help = !self.app.show_help;
                    return false;
                }
                if key.code == KeyCode::Char('/') {
                    self.app.show_search = !self.app.show_search;
                    self.app.search_query.clear();
                    return false;
                }
                if key.code == KeyCode::Esc {
                    if self.app.show_search {
                        self.app.show_search = false;
                        self.app.search_query.clear();
                    } else if self.app.show_help {
                        self.app.show_help = false;
                    }
                    return false;
                }

                // Command palette (Ctrl+K)
                if key.code == KeyCode::Char('k') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.app.show_command_palette = !self.app.show_command_palette;
                    return false;
                }

                // Panel focus cycling (Alt+Up/Alt+Down)
                if key.modifiers.contains(KeyModifiers::ALT) {
                    match key.code {
                        KeyCode::Up => {
                            self.app.focused_panel = self.app.focused_panel.saturating_sub(1);
                            return false;
                        }
                        KeyCode::Down => {
                            if self.app.focused_panel < 4 {
                                self.app.focused_panel += 1;
                            }
                            return false;
                        }
                        _ => {}
                    }
                }

                // Command palette handles keys when open
                if self.app.show_command_palette {
                    if self.command_palette.handle_key(key, &mut self.app) {
                        return false;
                    }
                }

                // Let tabs handle first
                if self.tabs.handle_key(key, &mut self.app) {
                    return false;
                }

                // Let active tab component handle
                match self.app.selected_tab {
                    Tab::Dashboard => {
                        match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                if self.app.selected_symbol_idx > 0 {
                                    self.app.selected_symbol_idx -= 1;
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if self.app.selected_symbol_idx + 1 < self.app.watchlist.len() {
                                    self.app.selected_symbol_idx += 1;
                                }
                            }
                            KeyCode::Enter => {
                                if let Some(sym) = self.app.watchlist.get(self.app.selected_symbol_idx) {
                                    self.app.selected_symbol = sym.clone();
                                    // Point the order-book depth poller at the
                                    // newly selected symbol.
                                    self.app.pending_command = Some(
                                        crate::api_client::StatusMsg::SelectSymbol(sym.clone()),
                                    );
                                }
                            }
                            KeyCode::Char('y') => {
                                // Copy selected symbol to clipboard
                                if let Some(sym) = self.app.watchlist.get(self.app.selected_symbol_idx) {
                                    let text = format!("{} ${:.2}",
                                        sym,
                                        self.app.market_data.get(sym).map(|m| m.price).unwrap_or(0.0)
                                    );
                                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                        let _ = clipboard.set_text(&text);
                                        self.app.show_toast(&format!("Copied: {}", text), crate::app::AlertLevel::Info);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Tab::Trading => {
                        self.positions.handle_key(key, &mut self.app);
                        if key.code == KeyCode::Char('y') {
                            // Copy selected position to clipboard
                            if let Some(pos) = self.app.positions.get(self.app.selected_position_idx) {
                                let text = format!("{} {} {:.4} @ ${:.2} P&L: ${:+.2}",
                                    pos.symbol, pos.side, pos.size, pos.entry_price, pos.pnl
                                );
                                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                    let _ = clipboard.set_text(&text);
                                    self.app.show_toast(&format!("Copied: {}", text), crate::app::AlertLevel::Info);
                                }
                            }
                        }
                    }
                    Tab::Orderbook => {}
                    Tab::Positions => {
                        self.positions.handle_key(key, &mut self.app);
                        if key.code == KeyCode::Char('y') {
                            if let Some(pos) = self.app.positions.get(self.app.selected_position_idx) {
                                let text = format!("{} {} {:.4} @ ${:.2} P&L: ${:+.2}",
                                    pos.symbol, pos.side, pos.size, pos.entry_price, pos.pnl
                                );
                                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                    let _ = clipboard.set_text(&text);
                                    self.app.show_toast(&format!("Copied: {}", text), crate::app::AlertLevel::Info);
                                }
                            }
                        }
                    }
                    Tab::Agents => {
                        self.agents.handle_key(key, &mut self.app);
                    }
                    Tab::Performance => {
                        match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                if self.app.scroll_offset > 0 { self.app.scroll_offset -= 1; }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                self.app.scroll_offset += 1;
                                self.app.clamp_scroll();
                            }
                            KeyCode::Home => { self.app.scroll_offset = 0; }
                            _ => {}
                        }
                    }
                    Tab::PolicyCache => {
                        match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                if self.app.scroll_offset > 0 { self.app.scroll_offset -= 1; }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                self.app.scroll_offset += 1;
                                self.app.clamp_scroll();
                            }
                            _ => {}
                        }
                    }
                    Tab::Health => {
                        if key.code == KeyCode::Char('r') {
                            let _ = self.cmd_tx.send(StatusMsg::RefreshHealth);
                            self.app.action_running = true;
                            self.app.action_message = Some(("Refreshing health...".to_string(), std::time::Instant::now()));
                        }
                    }
                    Tab::Settings => {
                        if key.code == KeyCode::Enter {
                            let _ = self.cmd_tx.send(StatusMsg::ToggleMode);
                            self.app.action_running = true;
                            self.app.action_message = Some(("Switching mode...".to_string(), std::time::Instant::now()));
                        }
                        self.settings.handle_key(key, &mut self.app);
                    }
                    Tab::Help => {}
                }

                false
            }
            Event::Mouse(mouse) => {
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        // Scroll up in current tab
                        if self.app.scroll_offset > 0 {
                            self.app.scroll_offset -= 1;
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        // Scroll down in current tab
                        self.app.scroll_offset += 1;
                        self.app.clamp_scroll();
                    }
                    MouseEventKind::Down(_) => {
                        // Left click — select item based on Y position
                        let y = mouse.row;
                        let x = mouse.column;
                        // Get terminal size for tab width calculation
                        let term_size = crossterm::terminal::size().unwrap_or((80, 24));
                        // Tab bar is at row 2 (after 2-row header)
                        if y == 2 {
                            // Click on tab bar — calculate which tab based on x position
                            let tab_count = 10u16;
                            let tab_width = term_size.0 / tab_count;
                            let clicked_tab = (x / tab_width).min(tab_count - 1) as usize;
                            if let Some(tab) = crate::app::Tab::from_index(clicked_tab) {
                                self.app.selected_tab = tab;
                                self.app.scroll_offset = 0;
                            }
                        } else if y > 2 {
                            // Click in content area — map to watchlist position
                            let content_row = y.saturating_sub(5) as usize; // account for header + tabs + spacing
                            if content_row < self.app.watchlist.len() {
                                self.app.selected_symbol_idx = content_row;
                                self.app.selected_symbol = self.app.watchlist[content_row].clone();
                            }
                        }
                    }
                    _ => {}
                }
                false
            }
            Event::Resize(_w, _h) => {
                // Terminal resized — the render loop will pick up new size automatically
                false
            }
            _ => false,
        }
    }

    fn render(&mut self, frame: &mut ratatui::Frame) {
        use ratatui::layout::{Constraint, Direction, Layout};

        let size = frame.area();

        // Guard against tiny terminals
        if size.width < 60 || size.height < 20 {
            let msg = ratatui::widgets::Paragraph::new("Terminal too small. Resize to at least 60x20.")
                .style(ratatui::style::Style::default().fg(ratatui::style::Color::Red))
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(msg, size);
            return;
        }

        // Main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),  // Header
                Constraint::Length(1),  // Tabs
                Constraint::Min(5),    // Content
                Constraint::Length(1),  // Status bar
                Constraint::Length(1),  // Footer
            ])
            .split(size);

        // Render header
        self.header.render(frame, chunks[0], &self.app);

        // Render tabs
        self.tabs.render(frame, chunks[1], &self.app);

        // Render active tab content
        match self.app.selected_tab {
            Tab::Dashboard => self.dashboard.render(frame, chunks[2], &self.app),
            Tab::Trading => self.positions.render(frame, chunks[2], &self.app),
            Tab::Orderbook => self.orderbook.render(frame, chunks[2], &self.app),
            Tab::Positions => self.positions.render(frame, chunks[2], &self.app),
            Tab::Agents => self.agents.render(frame, chunks[2], &self.app),
            Tab::Performance => self.performance.render(frame, chunks[2], &self.app),
            Tab::PolicyCache => self.performance.render(frame, chunks[2], &self.app),
            Tab::Health => self.health.render(frame, chunks[2], &self.app),
            Tab::Settings => self.settings.render(frame, chunks[2], &self.app),
            Tab::Help => self.help.render(frame, chunks[2], &self.app),
        }

        // Render status bar
        self.status_bar.render(frame, chunks[3], &self.app);

        // Render footer
        self.footer.render(frame, chunks[4], &self.app);

        // Render help overlay if active
        if self.app.show_help {
            let overlay_area = ratatui::layout::Rect {
                x: size.width / 6,
                y: size.height / 6,
                width: size.width * 2 / 3,
                height: size.height * 2 / 3,
            };
            self.help.render(frame, overlay_area, &self.app);
        }

        // Render command palette if active
        if self.app.show_command_palette {
            let palette_area = ratatui::layout::Rect {
                x: size.width / 4,
                y: size.height / 4,
                width: size.width / 2,
                height: size.height / 2,
            };
            self.command_palette.render(frame, palette_area, &self.app);
        }

        // Render toast notifications (bottom-right corner)
        self.app.clean_toasts();
        if !self.app.toasts.is_empty() {
            use ratatui::widgets::{Block, Borders, Paragraph};
            let toast_height = self.app.toasts.len() as u16 + 2;
            let toast_area = ratatui::layout::Rect {
                x: size.width.saturating_sub(40),
                y: size.height.saturating_sub(toast_height + 1),
                width: 38.min(size.width),
                height: toast_height,
            };
            let toast_lines: Vec<ratatui::text::Line> = self.app.toasts.iter().map(|t| {
                let color = match t.level {
                    crate::app::AlertLevel::Critical => ratatui::style::Color::Red,
                    crate::app::AlertLevel::Warning => ratatui::style::Color::Yellow,
                    crate::app::AlertLevel::Trade => ratatui::style::Color::Green,
                    _ => ratatui::style::Color::White,
                };
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    format!(" {}", t.message),
                    ratatui::style::Style::default().fg(color),
                ))
            }).collect();
            let toast_block = Block::default()
                .title(" Notifications ")
                .borders(Borders::ALL)
                .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::Gray));
            let toast = Paragraph::new(toast_lines).block(toast_block);
            frame.render_widget(toast, toast_area);
        }
    }

    fn update(&mut self) {
        // Drain all pending API messages and update app state
        while let Ok(msg) = self.rx.try_recv() {
            api_client::process_message(msg, &mut self.app);
        }

        // Send any pending commands from component actions (e.g., command palette)
        if let Some(cmd) = self.app.pending_command.take() {
            self.app.action_running = true;
            self.app.action_message = Some(("Processing...".to_string(), std::time::Instant::now()));
            let _ = self.cmd_tx.send(cmd);
        }

        // Recompute risk metrics from current portfolio state
        self.app.recompute_risk();

        // Generate alerts from NEW pipeline events only (avoid toast spam)
        let new_event_count = self.app.pipeline_events.len();
        if new_event_count > self.app.last_shown_event_idx {
            let new_events: Vec<_> = self.app.pipeline_events.iter()
                .skip(self.app.last_shown_event_idx)
                .take(new_event_count - self.app.last_shown_event_idx)
                .cloned()
                .collect();
            for event in &new_events {
                if event.action == "BUY" || event.action == "SELL" {
                    let level = if event.confidence > 0.8 {
                        crate::app::AlertLevel::Trade
                    } else {
                        crate::app::AlertLevel::Info
                    };
                    self.app.show_toast(
                        &format!("{} {} (conf: {:.0}%)", event.action, event.symbol, event.confidence * 100.0),
                        level,
                    );
                }
            }
            self.app.last_shown_event_idx = new_event_count;
        }

        // Expire action_running after 5 seconds
        if self.app.action_running {
            if let Some((_, start)) = &self.app.action_message {
                if start.elapsed().as_secs() > 5 {
                    self.app.action_running = false;
                    self.app.action_message = None;
                }
            }
        }

        // Update ticker animation
        self.app.ticker_offset = self.app.ticker_offset.wrapping_add(1);

        // Update components
        self.dashboard.update(&mut self.app);
        self.positions.update(&mut self.app);
        self.orderbook.update(&mut self.app);
        self.agents.update(&mut self.app);
        self.performance.update(&mut self.app);
    }
}

fn main() -> anyhow::Result<()> {
    if !io::stdout().is_terminal() {
        anyhow::bail!("TUI requires an interactive terminal.");
    }

    // Restore the terminal BEFORE the panic message prints — otherwise a
    // panic leaves the shell in raw mode + alternate screen and the message
    // is invisible, which makes TUI panics look like silent freezes.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        default_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut controller = AppController::new();
    let mut last_tick = Instant::now();

    let result = (|| -> anyhow::Result<()> {
        loop {
            terminal.draw(|f| controller.render(f))?;

            let timeout = POLL_INTERVAL
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_millis(50));

            if event::poll(timeout)? {
                if controller.handle_event(event::read()?) {
                    return Ok(());
                }
            }

            if last_tick.elapsed() >= POLL_INTERVAL {
                controller.update();
                last_tick = Instant::now();
            }
        }
    })();

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        println!("{err:?}");
    }

    Ok(())
}

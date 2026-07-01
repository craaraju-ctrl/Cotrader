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
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
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
}

impl AppController {
    fn new() -> Self {
        // Determine orchestrator API base URL from env or default
        let api_base = std::env::var("RAT_API_URL")
            .unwrap_or_else(|_| "http://localhost:8080/api".to_string());
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
                if key.code == KeyCode::Char('?') {
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

                // Let tabs handle first
                if self.tabs.handle_key(key, &mut self.app) {
                    return false;
                }

                // Let active tab component handle
                match self.app.selected_tab {
                    Tab::Dashboard => {}
                    Tab::Trading => {}
                    Tab::Orderbook => {}
                    Tab::Positions => {
                        self.positions.handle_key(key, &mut self.app);
                    }
                    Tab::Agents => {
                        self.agents.handle_key(key, &mut self.app);
                    }
                    Tab::Performance => {}
                    Tab::PolicyCache => {}
                    Tab::Health => {}
                    Tab::Settings => {
                        if key.code == KeyCode::Enter {
                            let _ = self.cmd_tx.send(StatusMsg::ToggleMode);
                        }
                        self.settings.handle_key(key, &mut self.app);
                    }
                    Tab::Help => {}
                }

                false
            }
            Event::Mouse(_) => {
                // Handle mouse events
                false
            }
            Event::Resize(_, _) => {
                false
            }
            _ => false,
        }
    }

    fn render(&self, frame: &mut ratatui::Frame) {
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
            Tab::Trading => self.orderbook.render(frame, chunks[2], &self.app),
            Tab::Orderbook => self.orderbook.render(frame, chunks[2], &self.app),
            Tab::Positions => self.positions.render(frame, chunks[2], &self.app),
            Tab::Agents => self.agents.render(frame, chunks[2], &self.app),
            Tab::Performance => self.performance.render(frame, chunks[2], &self.app),
            Tab::PolicyCache => self.dashboard.render(frame, chunks[2], &self.app),
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
    }

    fn update(&mut self) {
        // Drain all pending API messages and update app state
        while let Ok(msg) = self.rx.try_recv() {
            api_client::process_message(msg, &mut self.app);
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

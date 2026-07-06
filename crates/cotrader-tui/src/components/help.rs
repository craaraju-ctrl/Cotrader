//! Help component — Keyboard shortcuts and usage guide.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Tab};
use crate::components::Component;
use crate::theme::THEME;

pub struct HelpComponent;

impl Component for HelpComponent {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(area);

        // ── Global Navigation ──────────────────────────────────────────────
        let mut nav_content = vec![
            Line::from(Span::styled(
                "GLOBAL",
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Tab / Shift+Tab ", Style::default().fg(THEME.highlight)),
                Span::styled("Switch between tabs", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  1-9 ", Style::default().fg(THEME.highlight)),
                Span::styled("Jump to tab by number", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  0 ", Style::default().fg(THEME.highlight)),
                Span::styled("Jump to Help", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  / ", Style::default().fg(THEME.highlight)),
                Span::styled("Search / Filter", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  ? ", Style::default().fg(THEME.highlight)),
                Span::styled("Toggle this help", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+K ", Style::default().fg(THEME.highlight)),
                Span::styled("Command palette", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  Esc ", Style::default().fg(THEME.highlight)),
                Span::styled("Close / Cancel", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  q / Ctrl+C ", Style::default().fg(THEME.highlight)),
                Span::styled("Quit application", Style::default().fg(THEME.text)),
            ]),
        ];

        // ── Per-tab shortcuts ──────────────────────────────────────────────
        nav_content.push(Line::from(""));
        nav_content.push(Line::from(Span::styled(
            match app.selected_tab {
                Tab::Dashboard => "DASHBOARD",
                Tab::Trading => "TRADING",
                Tab::Orderbook => "ORDERBOOK",
                Tab::Positions => "POSITIONS",
                Tab::Agents => "AGENTS",
                Tab::Performance => "PERFORMANCE",
                Tab::PolicyCache => "POLICY CACHE",
                Tab::Health => "HEALTH",
                Tab::Settings => "SETTINGS",
                Tab::Help => "HELP",
            },
            Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
        )));
        nav_content.push(Line::from(""));

        match app.selected_tab {
            Tab::Dashboard => {
                nav_content.push(Line::from(vec![Span::styled("  Up/Down or j/k ", Style::default().fg(THEME.highlight)), Span::styled("Navigate watchlist", Style::default().fg(THEME.text))]));
                nav_content.push(Line::from(vec![Span::styled("  Enter ", Style::default().fg(THEME.highlight)), Span::styled("Select symbol (updates orderbook)", Style::default().fg(THEME.text))]));
            }
            Tab::Trading | Tab::Positions => {
                nav_content.push(Line::from(vec![Span::styled("  Up/Down or j/k ", Style::default().fg(THEME.highlight)), Span::styled("Navigate positions", Style::default().fg(THEME.text))]));
                nav_content.push(Line::from(vec![Span::styled("  Enter ", Style::default().fg(THEME.highlight)), Span::styled("Toggle position detail", Style::default().fg(THEME.text))]));
                nav_content.push(Line::from(vec![Span::styled("  Esc ", Style::default().fg(THEME.highlight)), Span::styled("Close detail view", Style::default().fg(THEME.text))]));
            }
            Tab::Performance | Tab::PolicyCache => {
                nav_content.push(Line::from(vec![Span::styled("  Up/Down or j/k ", Style::default().fg(THEME.highlight)), Span::styled("Scroll content", Style::default().fg(THEME.text))]));
                nav_content.push(Line::from(vec![Span::styled("  Home ", Style::default().fg(THEME.highlight)), Span::styled("Scroll to top", Style::default().fg(THEME.text))]));
            }
            Tab::Health => {
                nav_content.push(Line::from(vec![Span::styled("  r ", Style::default().fg(THEME.highlight)), Span::styled("Force refresh health data", Style::default().fg(THEME.text))]));
            }
            Tab::Settings => {
                nav_content.push(Line::from(vec![Span::styled("  Enter ", Style::default().fg(THEME.highlight)), Span::styled("Toggle paper/live mode", Style::default().fg(THEME.text))]));
            }
            Tab::Agents | Tab::Orderbook | Tab::Help => {
                nav_content.push(Line::from(vec![Span::styled("  Up/Down or j/k ", Style::default().fg(THEME.highlight)), Span::styled("Navigate", Style::default().fg(THEME.text))]));
            }
        }

        let nav_block = Block::default()
            .title(format!(" KEYBOARD SHORTCUTS — {} ", app.selected_tab as usize + 1))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let nav_para = Paragraph::new(nav_content).block(nav_block);
        frame.render_widget(nav_para, chunks[0]);

        // ── About ──────────────────────────────────────────────────────────
        let about_content = vec![
            Line::from(Span::styled(
                "CoTrader",
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Autonomous Trading System",
                Style::default().fg(THEME.text_dim),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Architecture:",
                Style::default().fg(THEME.highlight),
            )),
            Line::from("  • Tredo Exchange (External Matching Engine)"),
            Line::from("  • CoTrader (Autonomous Trading Brain)"),
            Line::from("  • Agentic Memory (Shared Intelligence)"),
            Line::from(""),
            Line::from(Span::styled(
                "Features:",
                Style::default().fg(THEME.highlight),
            )),
            Line::from("  • 5-Layer Adversarial Pipeline"),
            Line::from("  • Real-time WebSocket Streaming"),
            Line::from("  • Self-Evolving Memory System"),
            Line::from("  • Multi-Broker Registry"),
            Line::from("  • Paper & Live Trading Modes"),
        ];

        let about_block = Block::default()
            .title(" ABOUT ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let about_para = Paragraph::new(about_content).block(about_block);
        frame.render_widget(about_para, chunks[1]);
    }
}

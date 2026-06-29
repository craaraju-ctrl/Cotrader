//! Help component — Keyboard shortcuts and usage guide.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct HelpComponent;

impl Component for HelpComponent {
    fn render(&self, frame: &mut Frame, area: Rect, _app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(area);

        // ── Navigation ─────────────────────────────────────────────────────
        let nav_content = vec![
            Line::from(Span::styled(
                "NAVIGATION",
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
                Span::styled("  Esc ", Style::default().fg(THEME.highlight)),
                Span::styled("Close / Cancel", Style::default().fg(THEME.text)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "TRADING",
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  y ", Style::default().fg(THEME.highlight)),
                Span::styled("Open trade form", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  t ", Style::default().fg(THEME.highlight)),
                Span::styled("Toggle trade entry", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  Enter ", Style::default().fg(THEME.highlight)),
                Span::styled("Submit order", Style::default().fg(THEME.text)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "VIEW",
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  ↑ / ↓ ", Style::default().fg(THEME.highlight)),
                Span::styled("Navigate rows", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  ← / → ", Style::default().fg(THEME.highlight)),
                Span::styled("Navigate columns", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  s ", Style::default().fg(THEME.highlight)),
                Span::styled("Sort table", Style::default().fg(THEME.text)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "SYSTEM",
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  r ", Style::default().fg(THEME.highlight)),
                Span::styled("Force refresh", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  q / Ctrl+C ", Style::default().fg(THEME.highlight)),
                Span::styled("Quit application", Style::default().fg(THEME.text)),
            ]),
        ];

        let nav_block = Block::default()
            .title(" KEYBOARD SHORTCUTS ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let nav_para = Paragraph::new(nav_content).block(nav_block);
        frame.render_widget(nav_para, chunks[0]);

        // ── About ──────────────────────────────────────────────────────────
        let about_content = vec![
            Line::from(Span::styled(
                "RAT Agent",
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Realtime Autonomous Trading Agent",
                Style::default().fg(THEME.text_dim),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Version: 0.1.0",
                Style::default().fg(THEME.text),
            )),
            Line::from(Span::styled(
                "Ratatui: 0.30",
                Style::default().fg(THEME.text),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Architecture:",
                Style::default().fg(THEME.highlight),
            )),
            Line::from("  • Tredo Exchange (Matching Engine)"),
            Line::from("  • RAT (Autonomous Trading Brain)"),
            Line::from("  • Agentic Memory (Shared Intelligence)"),
            Line::from(""),
            Line::from(Span::styled(
                "Features:",
                Style::default().fg(THEME.highlight),
            )),
            Line::from("  • 5-Layer Adversarial Pipeline"),
            Line::from("  • Real-time WebSocket Streaming"),
            Line::from("  • Self-Evolving Memory System"),
            Line::from("  • Multi-Broker Support"),
            Line::from("  • Paper & Live Trading"),
        ];

        let about_block = Block::default()
            .title(" ABOUT ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let about_para = Paragraph::new(about_content).block(about_block);
        frame.render_widget(about_para, chunks[1]);
    }
}

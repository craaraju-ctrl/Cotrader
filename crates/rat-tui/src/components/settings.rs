//! Settings component — Live configuration from /api/status with save/update.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct SettingsComponent;

impl Component for SettingsComponent {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(25),
                Constraint::Min(30),
            ])
            .split(area);

        // ── Settings menu (left sidebar) ──────────────────────────────────
        let menu_items = vec![
            ListItem::new(Line::from(Span::styled(
                " Trading ",
                Style::default().fg(THEME.text),
            ))),
            ListItem::new(Line::from(Span::styled(
                " Risk Parameters ",
                Style::default().fg(THEME.text),
            ))),
            ListItem::new(Line::from(Span::styled(
                " Agents ",
                Style::default().fg(THEME.text),
            ))),
            ListItem::new(Line::from(Span::styled(
                " Display ",
                Style::default().fg(THEME.text),
            ))),
            ListItem::new(Line::from(Span::styled(
                " System ",
                Style::default().fg(THEME.text),
            ))),
        ];

        let menu = List::new(menu_items)
            .block(
                Block::default()
                    .title(" SETTINGS ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(THEME.border)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(40, 40, 60))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("► ");

        frame.render_widget(menu, chunks[0]);

        // ── Settings content (right panel) ────────────────────────────────
        let mut content: Vec<Line> = Vec::new();

        // ── Trading Mode ──────────────────────────────────────────────────
        content.push(Line::from(Span::styled(
            " Trading Mode",
            Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
        )));
        content.push(Line::from(""));

        let mode_color = if app.trading_mode == "paper" {
            THEME.positive
        } else {
            THEME.negative
        };
        let mode_label = if app.trading_mode == "paper" {
            "● PAPER"
        } else {
            "● LIVE"
        };
        content.push(Line::from(vec![
            Span::styled("  Mode:          ", Style::default().fg(THEME.text_dim)),
            Span::styled(mode_label, Style::default().fg(mode_color).add_modifier(Modifier::BOLD)),
        ]));
        content.push(Line::from(vec![
            Span::styled("  Broker:        ", Style::default().fg(THEME.text_dim)),
            Span::styled(&app.broker_name, Style::default().fg(THEME.text)),
        ]));
        content.push(Line::from(vec![
            Span::styled("  Trading:       ", Style::default().fg(THEME.text_dim)),
            Span::styled(
                if app.trading_enabled { "Enabled" } else { "Disabled" },
                Style::default().fg(if app.trading_enabled { THEME.positive } else { THEME.negative }),
            ),
        ]));
        content.push(Line::from(vec![
            Span::styled("  Open Positions:", Style::default().fg(THEME.text_dim)),
            Span::styled(
                format!(" {}", app.open_position_count),
                Style::default().fg(THEME.text),
            ),
        ]));

        content.push(Line::from(""));

        // ── Portfolio Summary ─────────────────────────────────────────────
        content.push(Line::from(Span::styled(
            " Portfolio",
            Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
        )));
        content.push(Line::from(""));
        content.push(Line::from(vec![
            Span::styled("  Equity:        ", Style::default().fg(THEME.text_dim)),
            Span::styled(
                format!("${:.2}", app.portfolio.equity),
                Style::default().fg(THEME.text).add_modifier(Modifier::BOLD),
            ),
        ]));
        content.push(Line::from(vec![
            Span::styled("  Cash:          ", Style::default().fg(THEME.text_dim)),
            Span::styled(
                format!("${:.2}", app.portfolio.cash),
                Style::default().fg(THEME.text),
            ),
        ]));
        content.push(Line::from(vec![
            Span::styled("  Unrealized P&L:", Style::default().fg(THEME.text_dim)),
            Span::styled(
                format!(" ${:+.2}", app.portfolio.unrealized_pnl),
                Style::default().fg(if app.portfolio.unrealized_pnl >= 0.0 { THEME.positive } else { THEME.negative }),
            ),
        ]));
        content.push(Line::from(vec![
            Span::styled("  Realized P&L:  ", Style::default().fg(THEME.text_dim)),
            Span::styled(
                format!(" ${:+.2}", app.portfolio.realized_pnl),
                Style::default().fg(if app.portfolio.realized_pnl >= 0.0 { THEME.positive } else { THEME.negative }),
            ),
        ]));
        content.push(Line::from(vec![
            Span::styled("  Max Drawdown:  ", Style::default().fg(THEME.text_dim)),
            Span::styled(
                format!("{:.1}%", app.portfolio.max_drawdown),
                Style::default().fg(if app.portfolio.max_drawdown > 5.0 { THEME.warning } else { THEME.text }),
            ),
        ]));

        content.push(Line::from(""));

        // ── System ────────────────────────────────────────────────────────
        content.push(Line::from(Span::styled(
            " System",
            Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
        )));
        content.push(Line::from(""));
        content.push(Line::from(vec![
            Span::styled("  WebSocket:     ", Style::default().fg(THEME.text_dim)),
            Span::styled(
                if app.ws_connected { "Connected" } else { "Disconnected" },
                Style::default().fg(if app.ws_connected { THEME.positive } else { THEME.negative }),
            ),
        ]));
        content.push(Line::from(vec![
            Span::styled("  Agents Active: ", Style::default().fg(THEME.text_dim)),
            Span::styled(
                format!("{}", app.agents.len()),
                Style::default().fg(THEME.text),
            ),
        ]));
        content.push(Line::from(vec![
            Span::styled("  COT Entries:   ", Style::default().fg(THEME.text_dim)),
            Span::styled(
                format!("{}", app.cot_log.len()),
                Style::default().fg(THEME.text),
            ),
        ]));

        content.push(Line::from(""));
        content.push(Line::from(Span::styled(
            " Press Enter to toggle Paper/Live mode",
            Style::default().fg(THEME.text_dim).add_modifier(Modifier::ITALIC),
        )));

        // ── Error banner ──────────────────────────────────────────────
        if let Some(ref err) = app.error {
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                format!(" ⚠ {}", err),
                Style::default().fg(THEME.negative).add_modifier(Modifier::BOLD),
            )));
        }

        let content_block = Block::default()
            .title(" CONFIGURATION ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let content_para = Paragraph::new(content).block(content_block);
        frame.render_widget(content_para, chunks[1]);
    }
}

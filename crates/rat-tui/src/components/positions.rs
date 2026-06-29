//! Positions component — Open positions with P&L visualization.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct PositionsComponent;

impl Component for PositionsComponent {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(8),
                Constraint::Length(4),
            ])
            .split(area);

        // ── Positions table ────────────────────────────────────────────────
        let header = Row::new(["Symbol", "Side", "Size", "Entry", "Mark", "P&L", "P&L%", "Liq Price"])
            .style(Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD))
            .height(1);

        let rows: Vec<Row> = app.positions.iter().map(|pos| {
            let pnl_color = if pos.pnl >= 0.0 { THEME.positive } else { THEME.negative };
            let side_color = if pos.side == "Long" { THEME.positive } else { THEME.negative };

            Row::new(vec![
                ratatui::widgets::Cell::from(Span::styled(
                    &pos.symbol,
                    Style::default().fg(THEME.text).add_modifier(Modifier::BOLD),
                )),
                ratatui::widgets::Cell::from(Span::styled(&pos.side, Style::default().fg(side_color))),
                ratatui::widgets::Cell::from(format!("{:.4}", pos.size)),
                ratatui::widgets::Cell::from(format!("${:.2}", pos.entry_price)),
                ratatui::widgets::Cell::from(format!("${:.2}", pos.mark_price)),
                ratatui::widgets::Cell::from(Span::styled(
                    format!("${:+.2}", pos.pnl),
                    Style::default().fg(pnl_color),
                )),
                ratatui::widgets::Cell::from(Span::styled(
                    format!("{:+.2}%", pos.pnl_pct),
                    Style::default().fg(pnl_color),
                )),
                ratatui::widgets::Cell::from(Span::styled(
                    format!("${:.2}", pos.liquidation_price),
                    Style::default().fg(THEME.warning),
                )),
            ])
        }).collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(8),
                Constraint::Length(6),
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Length(12),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .title(" OPEN POSITIONS ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(THEME.border)),
        );

        frame.render_widget(table, chunks[0]);

        // ── Summary bar ────────────────────────────────────────────────────
        let total_margin: f64 = app.positions.iter().map(|p| p.size * p.entry_price / p.leverage as f64).sum();
        let total_unrealized: f64 = app.positions.iter().map(|p| p.pnl).sum();

        let summary = vec![
            Line::from(vec![
                Span::styled("  Positions: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{}", app.positions.len()), Style::default().fg(THEME.brand)),
                Span::styled(" │ Margin: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("${:.2}", total_margin), Style::default().fg(THEME.text)),
                Span::styled(" │ Unrealized P&L: ", Style::default().fg(THEME.text_dim)),
                Span::styled(
                    format!("${:+.2}", total_unrealized),
                    Style::default().fg(if total_unrealized >= 0.0 { THEME.positive } else { THEME.negative }),
                ),
            ]),
        ];

        let summary_para = Paragraph::new(summary)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(THEME.border)),
            );
        frame.render_widget(summary_para, chunks[1]);
    }
}

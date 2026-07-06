//! Positions component — Open positions with P&L visualization.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use ratatui::Frame;
use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct PositionsComponent;

impl Component for PositionsComponent {
    fn handle_key(&mut self, key: KeyEvent, app: &mut App) -> bool {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if app.selected_position_idx > 0 {
                    app.selected_position_idx -= 1;
                }
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.selected_position_idx + 1 < app.positions.len() {
                    app.selected_position_idx += 1;
                }
                true
            }
            KeyCode::Enter => {
                if !app.positions.is_empty() {
                    app.show_position_detail = !app.show_position_detail;
                }
                true
            }
            KeyCode::Esc => {
                if app.show_position_detail {
                    app.show_position_detail = false;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        if app.show_position_detail {
            // ── Position detail view ────────────────────────────────────────
            self.render_detail_view(frame, area, app);
        } else {
            // ── Positions table ────────────────────────────────────────────
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(8),
                    Constraint::Length(4),
                ])
                .split(area);

            self.render_table(frame, chunks[0], app);
            self.render_summary(frame, chunks[1], app);
        }
    }
}

impl PositionsComponent {
    fn render_table(&self, frame: &mut Frame, area: Rect, app: &App) {
        let header = Row::new(["Symbol", "Side", "Size", "Entry", "Mark", "P&L", "P&L%", "Liq Price"])
            .style(Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD))
            .height(1);

        // Filter positions by search query if active
        let filtered: Vec<(usize, &crate::app::Position)> = if app.show_search && !app.search_query.is_empty() {
            let q = app.search_query.to_uppercase();
            app.positions.iter().enumerate().filter(|(_, p)| p.symbol.to_uppercase().contains(&q)).collect()
        } else {
            app.positions.iter().enumerate().collect()
        };

        let rows: Vec<Row> = filtered.iter().map(|(i, pos)| {
            let pnl_color = if pos.pnl >= 0.0 { THEME.positive } else { THEME.negative };
            let side_color = if pos.side == "Long" { THEME.positive } else { THEME.negative };
            let is_selected = *i == app.selected_position_idx;
            let bg = if is_selected { Color::Rgb(40, 40, 60) } else { Color::Rgb(30, 30, 46) };

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
            .style(Style::default().bg(bg))
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
                .title(" OPEN POSITIONS (Enter for detail) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(THEME.border)),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("► ");

        frame.render_widget(table, area);
    }

    fn render_detail_view(&self, frame: &mut Frame, area: Rect, app: &App) {
        let pos = app.positions.get(app.selected_position_idx);
        if let Some(pos) = pos {
            let pnl_color = if pos.pnl >= 0.0 { THEME.positive } else { THEME.negative };
            let detail_lines = vec![
                Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(&pos.symbol, Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD)),
                    Span::styled("  —  ", Style::default().fg(THEME.text_dim)),
                    Span::styled(&pos.side, Style::default().fg(if pos.side == "Long" { THEME.positive } else { THEME.negative }).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("    Size:     ", Style::default().fg(THEME.text_dim)),
                    Span::styled(format!("{:.4}", pos.size), Style::default().fg(THEME.text)),
                ]),
                Line::from(vec![
                    Span::styled("    Entry:    ", Style::default().fg(THEME.text_dim)),
                    Span::styled(format!("${:.2}", pos.entry_price), Style::default().fg(THEME.text)),
                ]),
                Line::from(vec![
                    Span::styled("    Mark:     ", Style::default().fg(THEME.text_dim)),
                    Span::styled(format!("${:.2}", pos.mark_price), Style::default().fg(THEME.highlight)),
                ]),
                Line::from(vec![
                    Span::styled("    P&L:      ", Style::default().fg(THEME.text_dim)),
                    Span::styled(format!("${:+.2}", pos.pnl), Style::default().fg(pnl_color).add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" ({:+.2}%)", pos.pnl_pct), Style::default().fg(pnl_color)),
                ]),
                Line::from(vec![
                    Span::styled("    Leverage: ", Style::default().fg(THEME.text_dim)),
                    Span::styled(format!("{}x", pos.leverage), Style::default().fg(THEME.warning)),
                ]),
                Line::from(vec![
                    Span::styled("    Liquidation: ", Style::default().fg(THEME.text_dim)),
                    Span::styled(format!("${:.2}", pos.liquidation_price), Style::default().fg(THEME.negative)),
                ]),
            ];
            let block = Block::default()
                .title(" POSITION DETAIL (Esc to close) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(THEME.border_focus));
            let para = Paragraph::new(detail_lines).block(block);
            frame.render_widget(para, area);
        } else {
            // Fallback if position index is invalid — just render the table
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(8),
                    Constraint::Length(4),
                ])
                .split(area);
            self.render_table(frame, chunks[0], app);
            self.render_summary(frame, chunks[1], app);
        }
    }

    fn render_summary(&self, frame: &mut Frame, area: Rect, app: &App) {
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
        frame.render_widget(summary_para, area);
    }
}

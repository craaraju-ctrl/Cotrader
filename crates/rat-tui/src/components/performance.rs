//! Performance component — Charts and metrics visualization.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct PerformanceComponent;

impl Component for PerformanceComponent {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),
                Constraint::Min(8),
            ])
            .split(area);

        self.render_metrics_cards(frame, chunks[0], app);
        self.render_equity_history(frame, chunks[1], app);
    }
}

impl PerformanceComponent {
    fn render_metrics_cards(&self, frame: &mut Frame, area: Rect, app: &App) {
        let card_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(area);

        // ── Sharpe Ratio ───────────────────────────────────────────────────
        let sharpe_block = Block::default()
            .title(" SHARPE RATIO ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let sharpe_para = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("{:.2}", app.portfolio.sharpe_ratio),
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Risk-adjusted return",
                Style::default().fg(THEME.text_dim),
            )),
        ]).block(sharpe_block);

        frame.render_widget(sharpe_para, card_chunks[0]);

        // ── Max Drawdown ───────────────────────────────────────────────────
        let dd_block = Block::default()
            .title(" MAX DRAWDOWN ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let dd_gauge = Gauge::default()
            .block(dd_block)
            .gauge_style(Style::default().fg(THEME.negative).bg(THEME.surface))
            .ratio(app.portfolio.max_drawdown / 100.0)
            .label(Span::styled(
                format!("{:.1}%", app.portfolio.max_drawdown),
                Style::default().fg(THEME.text),
            ));

        frame.render_widget(dd_gauge, card_chunks[1]);

        // ── Win/Loss Ratio ─────────────────────────────────────────────────
        let wl_block = Block::default()
            .title(" WIN/LOSS RATIO ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let win_rate = app.portfolio.win_rate;
        let wl_para = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("{:.1}%", win_rate),
                Style::default().fg(THEME.positive).add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled("W: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{}", app.portfolio.winning_trades), THEME.positive_style()),
                Span::styled(" │ L: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{}", app.portfolio.losing_trades), THEME.negative_style()),
            ]),
        ]).block(wl_block);

        frame.render_widget(wl_para, card_chunks[2]);
    }

    fn render_equity_history(&self, frame: &mut Frame, area: Rect, app: &App) {
        // Simple text-based sparkline for equity history
        let history = &app.equity_history;
        let block = Block::default()
            .title(" EQUITY HISTORY ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        if history.is_empty() {
            let empty = Paragraph::new(Span::styled(
                "No equity history available yet",
                THEME.dim_style(),
            )).block(block);
            frame.render_widget(empty, area);
            return;
        }

        let min = history.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = history.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let range = if max - min > 0.0 { max - min } else { 1.0 };

        // Create a simple ASCII sparkline
        let height = area.height.saturating_sub(2) as usize;
        let width = area.width.saturating_sub(2) as usize;
        let step = if history.len() > width { history.len() / width } else { 1 };

        let mut lines: Vec<Line> = Vec::new();
        for row in 0..height {
            let mut line_spans = vec![];
            for col in (0..width).step_by(1) {
                let idx = (col * step).min(history.len().saturating_sub(1));
                let val = history[idx];
                let normalized = ((val - min) / range * height as f64) as usize;
                let inverted_row = height - 1 - row;

                if inverted_row == normalized {
                    let color = if val >= history.first().copied().unwrap_or(0.0) {
                        THEME.positive
                    } else {
                        THEME.negative
                    };
                    line_spans.push(Span::styled("●", Style::default().fg(color)));
                } else {
                    line_spans.push(Span::styled(" ", Style::default()));
                }
            }
            lines.push(Line::from(line_spans));
        }

        let sparkline = Paragraph::new(lines).block(block);
        frame.render_widget(sparkline, area);
    }
}

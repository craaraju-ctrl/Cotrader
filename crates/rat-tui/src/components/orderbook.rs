//! Order book component — Depth visualization with bid/ask walls.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct OrderBookComponent;

impl Component for OrderBookComponent {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Length(20),
                Constraint::Percentage(50),
            ])
            .split(area);

        // ── Asks (sell orders) — reversed so lowest ask is at bottom ────────
        self.render_asks(frame, chunks[0], app);

        // ── Spread / Mid price ─────────────────────────────────────────────
        self.render_spread(frame, chunks[1], app);

        // ── Bids (buy orders) ──────────────────────────────────────────────
        self.render_bids(frame, chunks[2], app);
    }
}

impl OrderBookComponent {
    fn render_asks(&self, frame: &mut Frame, area: Rect, app: &App) {
        let mut asks = app.orderbook.asks.clone();
        asks.reverse();
        let max_qty = asks.iter().map(|l| l.quantity).fold(0.0_f64, f64::max);

        let rows: Vec<Row> = asks.iter().map(|level| {
            let depth_pct = if max_qty > 0.0 { level.quantity / max_qty } else { 0.0 };
            let bar_width = (depth_pct * 20.0) as usize;
            let bar: String = "█".repeat(bar_width);
            let padding: String = "░".repeat(20 - bar_width);

            Row::new(vec![
                ratatui::widgets::Cell::from(Span::styled(
                    format!("${:.2}", level.price),
                    Style::default().fg(THEME.negative),
                )),
                ratatui::widgets::Cell::from(format!("{:.4}", level.quantity)),
                ratatui::widgets::Cell::from(Span::styled(
                    format!("{}{}", bar, padding),
                    Style::default().fg(Color::Rgb(180, 60, 60)),
                )),
                ratatui::widgets::Cell::from(format!("{:.2}", level.total)),
            ])
        }).collect();

        let header = Row::new(["Price", "Qty", "Depth", "Total"])
            .style(Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD))
            .height(1);

        let table = Table::new(
            rows,
            [Constraint::Length(12), Constraint::Length(12), Constraint::Length(22), Constraint::Length(12)],
        )
        .header(header)
        .block(
            Block::default()
                .title(" ASKS (Sell) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(THEME.negative)),
        );

        frame.render_widget(table, area);
    }

    fn render_bids(&self, frame: &mut Frame, area: Rect, app: &App) {
        let bids = &app.orderbook.bids;
        let max_qty = bids.iter().map(|l| l.quantity).fold(0.0_f64, f64::max);

        let rows: Vec<Row> = bids.iter().map(|level| {
            let depth_pct = if max_qty > 0.0 { level.quantity / max_qty } else { 0.0 };
            let bar_width = (depth_pct * 20.0) as usize;
            let bar: String = "█".repeat(bar_width);
            let padding: String = "░".repeat(20 - bar_width);

            Row::new(vec![
                ratatui::widgets::Cell::from(Span::styled(
                    format!("${:.2}", level.price),
                    Style::default().fg(THEME.positive),
                )),
                ratatui::widgets::Cell::from(format!("{:.4}", level.quantity)),
                ratatui::widgets::Cell::from(Span::styled(
                    format!("{}{}", bar, padding),
                    Style::default().fg(Color::Rgb(0, 140, 60)),
                )),
                ratatui::widgets::Cell::from(format!("{:.2}", level.total)),
            ])
        }).collect();

        let header = Row::new(["Price", "Qty", "Depth", "Total"])
            .style(Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD))
            .height(1);

        let table = Table::new(
            rows,
            [Constraint::Length(12), Constraint::Length(12), Constraint::Length(22), Constraint::Length(12)],
        )
        .header(header)
        .block(
            Block::default()
                .title(" BIDS (Buy) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(THEME.positive)),
        );

        frame.render_widget(table, area);
    }

    fn render_spread(&self, frame: &mut Frame, area: Rect, app: &App) {
        let mid = app.orderbook.mid_price;
        let spread = app.orderbook.spread;
        let spread_pct = if mid > 0.0 { (spread / mid * 100.0) } else { 0.0 };

        let content = vec![
            Line::from(Span::styled(
                "MID",
                Style::default().fg(THEME.text_dim),
            )),
            Line::from(Span::styled(
                format!("${:.2}", mid),
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "SPREAD",
                Style::default().fg(THEME.text_dim),
            )),
            Line::from(Span::styled(
                format!("${:.2}", spread),
                Style::default().fg(THEME.highlight),
            )),
            Line::from(Span::styled(
                format!("({:.4}%)", spread_pct),
                Style::default().fg(THEME.text_dim),
            )),
        ];

        let block = Block::default()
            .title(" SPREAD ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let para = Paragraph::new(content)
            .block(block)
            .alignment(ratatui::layout::Alignment::Center);

        frame.render_widget(para, area);
    }
}

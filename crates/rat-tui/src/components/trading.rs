//! Trading component — Open orders with quick trade actions.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct TradingComponent;

impl Component for TradingComponent {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(6),
                Constraint::Length(4),
            ])
            .split(area);

        self.render_open_orders(frame, chunks[0], app);
        self.render_quick_actions(frame, chunks[1], app);
    }
}

impl TradingComponent {
    fn render_open_orders(&self, frame: &mut Frame, area: Rect, app: &App) {
        let header = Row::new(["ID", "Symbol", "Side", "Type", "Qty", "Price", "Status"])
            .style(Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD))
            .height(1);

        let rows: Vec<Row> = app.open_orders.iter().map(|order| {
            let id = order.get("id").and_then(|v| v.as_str())
                .or_else(|| order.get("order_id").and_then(|v| v.as_str()))
                .unwrap_or("-");
            let symbol = order.get("symbol").and_then(|v| v.as_str()).unwrap_or("-");
            let side = order.get("side").and_then(|v| v.as_str()).unwrap_or("-");
            let order_type = order.get("type").and_then(|v| v.as_str())
                .or_else(|| order.get("order_type").and_then(|v| v.as_str()))
                .unwrap_or("-");
            let qty = order.get("quantity").and_then(|v| v.as_f64())
                .or_else(|| order.get("qty").and_then(|v| v.as_f64()))
                .unwrap_or(0.0);
            let price = order.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let status = order.get("status").and_then(|v| v.as_str()).unwrap_or("pending");

            let side_color = match side.to_lowercase().as_str() {
                "buy" | "long" => THEME.positive,
                "sell" | "short" => THEME.negative,
                _ => THEME.text,
            };
            let status_color = match status.to_lowercase().as_str() {
                "filled" => THEME.positive,
                "cancelled" | "rejected" => THEME.negative,
                "partial" => THEME.warning,
                _ => THEME.text_dim,
            };

            let display_id = if id.len() > 8 { &id[..8] } else { id };

            Row::new(vec![
                ratatui::widgets::Cell::from(Span::styled(
                    display_id.to_string(),
                    Style::default().fg(THEME.text_dim),
                )),
                ratatui::widgets::Cell::from(Span::styled(
                    symbol.to_string(),
                    Style::default().fg(THEME.text).add_modifier(Modifier::BOLD),
                )),
                ratatui::widgets::Cell::from(Span::styled(
                    side.to_uppercase(),
                    Style::default().fg(side_color),
                )),
                ratatui::widgets::Cell::from(order_type.to_string()),
                ratatui::widgets::Cell::from(format!("{:.4}", qty)),
                ratatui::widgets::Cell::from(format!("${:.2}", price)),
                ratatui::widgets::Cell::from(Span::styled(
                    status.to_string(),
                    Style::default().fg(status_color),
                )),
            ])
        }).collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(6),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Length(10),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .title(format!(" OPEN ORDERS ({}) ", app.open_orders.len()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(THEME.border)),
        );

        frame.render_widget(table, area);
    }

    fn render_quick_actions(&self, frame: &mut Frame, area: Rect, app: &App) {
        let total_orders = app.open_orders.len();
        let buy_orders = app.open_orders.iter().filter(|o| {
            o.get("side").and_then(|v| v.as_str())
                .map(|s| matches!(s.to_lowercase().as_str(), "buy" | "long"))
                .unwrap_or(false)
        }).count();
        let sell_orders = total_orders - buy_orders;

        let content = vec![
            Line::from(vec![
                Span::styled("  Symbol: ", Style::default().fg(THEME.text_dim)),
                Span::styled(&app.selected_symbol, Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD)),
                Span::styled(" │ ", Style::default().fg(THEME.border)),
                Span::styled("Open: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{}", total_orders), Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  Buy: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{}", buy_orders), Style::default().fg(THEME.positive)),
                Span::styled(" │ Sell: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{}", sell_orders), Style::default().fg(THEME.negative)),
                Span::styled(" │ ", Style::default().fg(THEME.border)),
                Span::styled("Press y to open trade form", Style::default().fg(THEME.text_dim)),
            ]),
        ];

        let block = Block::default()
            .title(" TRADE SUMMARY ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let para = Paragraph::new(content).block(block);
        frame.render_widget(para, area);
    }
}

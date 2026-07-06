//! Header component — Brand, status indicators, and live price ticker.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct Header;

impl Component for Header {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(area);

        // ── Brand line ─────────────────────────────────────────────────────
        let ws_status = if app.ws_connected {
            Span::styled(" ● LIVE ", Style::default().fg(THEME.positive).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(" ○ OFFLINE ", Style::default().fg(THEME.negative))
        };

        let time_str = chrono::Local::now().format("%H:%M:%S").to_string();

        let mut brand_line = vec![
            Span::styled("  RAT ", Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD)),
            Span::styled("│ Trading Real-time Edge Decision Optimisation ", THEME.dim_style()),
        ];

        if let Some(md) = app.market_data.get(&app.selected_symbol) {
            let change_color = if md.change_24h >= 0.0 { THEME.positive } else { THEME.negative };
            let change_str = format!("{:+.2}%", md.change_24h);
            brand_line.push(Span::styled(format!(" {} ", app.selected_symbol), THEME.brand_style()));
            brand_line.push(Span::styled(format!("${:.2} ", md.price), Style::default().fg(THEME.text)));
            brand_line.push(Span::styled(change_str, Style::default().fg(change_color)));
        } else {
            brand_line.push(Span::styled(format!(" {} ", app.selected_symbol), THEME.brand_style()));
            brand_line.push(Span::styled("loading...", THEME.dim_style()));
        }

        brand_line.push(Span::styled(" │ ", THEME.dim_style()));
        brand_line.push(Span::styled(format!("{} ", time_str), Style::default().fg(THEME.text_dim)));
        brand_line.push(ws_status);

        let brand = Paragraph::new(Line::from(brand_line))
            .style(Style::default().bg(THEME.surface));
        frame.render_widget(brand, chunks[0]);

        // ── Ticker tape ────────────────────────────────────────────────────
        let mut ticker_spans: Vec<Span> = Vec::new();
        for (i, sym) in app.watchlist.iter().enumerate() {
            if i > 0 {
                ticker_spans.push(Span::styled("  │  ", Style::default().fg(THEME.border)));
            }
            if let Some(md) = app.market_data.get(sym) {
                let color = if md.change_24h >= 0.0 { THEME.positive } else { THEME.negative };
                let arrow = if md.change_24h >= 0.0 { "▲" } else { "▼" };
                ticker_spans.push(Span::styled(
                    format!("{} {} ${:.2} {:+.2}%", sym, arrow, md.price, md.change_24h),
                    Style::default().fg(color),
                ));
            } else {
                ticker_spans.push(Span::styled(format!("{} ...", sym), THEME.dim_style()));
            }
        }

        let ticker = Paragraph::new(Line::from(ticker_spans))
            .style(Style::default().bg(THEME.surface));
        frame.render_widget(ticker, chunks[1]);
    }
}

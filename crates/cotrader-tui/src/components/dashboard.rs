//! Dashboard component — Main overview with portfolio metrics and market overview.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct DashboardComponent;

impl Component for DashboardComponent {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),   // Portfolio summary cards
                Constraint::Min(8),     // Market overview table
                Constraint::Length(5),   // Risk metrics
                Constraint::Length(6),   // Quick actions / status
            ])
            .split(area);

        self.render_portfolio_cards(frame, chunks[0], app);
        self.render_market_table(frame, chunks[1], app);
        self.render_risk_metrics(frame, chunks[2], app);
        self.render_status_bar(frame, chunks[3], app);
    }
}

impl DashboardComponent {
    fn render_portfolio_cards(&self, frame: &mut Frame, area: Rect, app: &App) {
        let card_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .split(area);

        // ── Equity Card ────────────────────────────────────────────────────
        let equity = app.portfolio.equity;
        let cash = app.portfolio.cash;
        let equity_pct = if equity > 0.0 { (cash / equity * 100.0) as u16 } else { 100 };
        let equity_block = Block::default()
            .title(" EQUITY ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let equity_gauge = Gauge::default()
            .block(equity_block)
            .gauge_style(Style::default().fg(THEME.positive).bg(THEME.surface))
            .ratio((equity_pct as f64 / 100.0).clamp(0.0, 1.0))
            .label(Span::styled(
                format!("{}% liquid", equity_pct),
                Style::default().fg(THEME.text),
            ));

        frame.render_widget(equity_gauge, card_chunks[0]);

        // ── P&L Card ───────────────────────────────────────────────────────
        let pnl = app.portfolio.unrealized_pnl + app.portfolio.realized_pnl;
        let pnl_color = if pnl >= 0.0 { THEME.positive } else { THEME.negative };
        let pnl_block = Block::default()
            .title(" P&L ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let pnl_content = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("${:+.2}", pnl),
                Style::default().fg(pnl_color).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("Unrealized: ${:+.2}", app.portfolio.unrealized_pnl),
                Style::default().fg(if app.portfolio.unrealized_pnl >= 0.0 { THEME.positive } else { THEME.negative }),
            )),
            Line::from(Span::styled(
                format!("Realized: ${:+.2}", app.portfolio.realized_pnl),
                Style::default().fg(THEME.text_dim),
            )),
        ])
        .block(pnl_block);

        frame.render_widget(pnl_content, card_chunks[1]);

        // ── Win Rate Card ──────────────────────────────────────────────────
        let win_rate = app.portfolio.win_rate;
        let wr_block = Block::default()
            .title(" WIN RATE ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let wr_gauge = Gauge::default()
            .block(wr_block)
            .gauge_style(Style::default().fg(THEME.accent).bg(THEME.surface))
            .ratio((win_rate / 100.0).clamp(0.0, 1.0))
            .label(Span::styled(
                format!("{:.1}%", win_rate),
                Style::default().fg(THEME.text),
            ));

        frame.render_widget(wr_gauge, card_chunks[2]);

        // ── Trades Card ────────────────────────────────────────────────────
        let trades_block = Block::default()
            .title(" TRADES TODAY ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let trades_content = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("{}", app.portfolio.total_trades),
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled("W: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{}", app.portfolio.winning_trades), THEME.positive_style()),
                Span::styled("  L: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{}", app.portfolio.losing_trades), THEME.negative_style()),
            ]),
            Line::from(Span::styled(
                format!("DD: {:.1}%", app.portfolio.max_drawdown),
                Style::default().fg(if app.portfolio.max_drawdown > 5.0 { THEME.warning } else { THEME.text_dim }),
            )),
        ])
        .block(trades_block);

        frame.render_widget(trades_content, card_chunks[3]);
    }

    fn render_market_table(&self, frame: &mut Frame, area: Rect, app: &App) {
        let header_cells = ["Symbol", "Price", "24h%", "Volume", "Bid", "Ask", "Spread"]
            .iter()
            .map(|h| {
                ratatui::widgets::Cell::from(*h).style(
                    Style::default()
                        .fg(THEME.brand)
                        .add_modifier(Modifier::BOLD),
                )
            });
        let header = Row::new(header_cells)
            .style(Style::default().bg(THEME.surface))
            .height(1);

        // Filter watchlist by search query if active
        let filtered: Vec<&String> = if app.show_search && !app.search_query.is_empty() {
            let q = app.search_query.to_uppercase();
            app.watchlist.iter().filter(|s| s.to_uppercase().contains(&q)).collect()
        } else {
            app.watchlist.iter().collect()
        };

        let rows: Vec<Row> = filtered.iter().map(|sym| {
            let default_md = crate::app::MarketData {
                symbol: (*sym).clone(),
                price: 0.0,
                change_24h: 0.0,
                volume: 0.0,
                bid: 0.0,
                ask: 0.0,
                spread: 0.0,
                high_24h: 0.0,
                low_24h: 0.0,
            };
            let md = app.market_data.get(*sym).unwrap_or(&default_md);
            let change_color = if md.change_24h >= 0.0 { THEME.positive } else { THEME.negative };
                let is_selected = *sym == &app.selected_symbol;
                let bg = if is_selected { Color::Rgb(40, 40, 60) } else { Color::Rgb(30, 30, 46) };

                Row::new(vec![
                    ratatui::widgets::Cell::from(Span::styled(
                        sym.clone(),
                        Style::default().fg(THEME.text).add_modifier(if is_selected { Modifier::BOLD } else { Modifier::empty() }),
                    )),
                    ratatui::widgets::Cell::from(format!("${:.2}", md.price)),
                    ratatui::widgets::Cell::from(Span::styled(
                        format!("{:+.2}%", md.change_24h),
                        Style::default().fg(change_color),
                    )),
                    ratatui::widgets::Cell::from(format!("{:.0}", md.volume)),
                    ratatui::widgets::Cell::from(format!("${:.2}", md.bid)),
                    ratatui::widgets::Cell::from(format!("${:.2}", md.ask)),
                    ratatui::widgets::Cell::from(format!("${:.2}", md.spread)),
                ])
                .style(Style::default().bg(bg))
            })
        .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(8),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(10),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .title(" MARKET WATCHLIST ")
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

    fn render_risk_metrics(&self, frame: &mut Frame, area: Rect, app: &App) {
        let risk = &app.risk;
        let gauge_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .split(area);

        // VaR 95%
        let var95_block = Block::default()
            .title(" VaR 95% ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));
        let var95_ratio = (risk.var_95 / app.portfolio.equity.max(1.0)).clamp(0.0, 1.0);
        let var95_gauge = Gauge::default()
            .block(var95_block)
            .gauge_style(Style::default().fg(THEME.negative).bg(THEME.surface))
            .ratio(var95_ratio)
            .label(Span::styled(
                format!("${:.0}", risk.var_95),
                Style::default().fg(THEME.text),
            ));
        frame.render_widget(var95_gauge, gauge_chunks[0]);

        // Volatility
        let vol_block = Block::default()
            .title(" Daily Vol ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));
        let vol_gauge = Gauge::default()
            .block(vol_block)
            .gauge_style(Style::default().fg(THEME.warning).bg(THEME.surface))
            .ratio((risk.daily_volatility / 10.0).clamp(0.0, 1.0))
            .label(Span::styled(
                format!("{:.2}%", risk.daily_volatility),
                Style::default().fg(THEME.text),
            ));
        frame.render_widget(vol_gauge, gauge_chunks[1]);

        // Margin Usage
        let margin_block = Block::default()
            .title(" Margin Used ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));
        let margin_color = if risk.margin_usage > 80.0 { THEME.negative }
            else if risk.margin_usage > 50.0 { THEME.warning }
            else { THEME.positive };
        let margin_gauge = Gauge::default()
            .block(margin_block)
            .gauge_style(Style::default().fg(margin_color).bg(THEME.surface))
            .ratio((risk.margin_usage / 100.0).clamp(0.0, 1.0))
            .label(Span::styled(
                format!("{:.1}%", risk.margin_usage),
                Style::default().fg(THEME.text),
            ));
        frame.render_widget(margin_gauge, gauge_chunks[2]);

        // Concentration
        let conc_block = Block::default()
            .title(" Concentration ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));
        let conc_color = if risk.concentration_pct > 50.0 { THEME.negative }
            else if risk.concentration_pct > 25.0 { THEME.warning }
            else { THEME.positive };
        let conc_gauge = Gauge::default()
            .block(conc_block)
            .gauge_style(Style::default().fg(conc_color).bg(THEME.surface))
            .ratio((risk.concentration_pct / 100.0).clamp(0.0, 1.0))
            .label(Span::styled(
                format!("{:.1}%", risk.concentration_pct),
                Style::default().fg(THEME.text),
            ));
        frame.render_widget(conc_gauge, gauge_chunks[3]);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(area);

        // ── Agent Status ───────────────────────────────────────────────────
        let agent_status = if let Some(first_agent) = app.agents.first() {
            let status_color = match first_agent.status.as_str() {
                "active" | "running" => THEME.positive,
                "error" | "failed" => THEME.negative,
                _ => THEME.text_dim,
            };
            vec![
                Line::from(vec![
                    Span::styled("  🤖 ", Style::default().fg(THEME.accent)),
                    Span::styled(&first_agent.name, Style::default().fg(THEME.text)),
                    Span::styled(" │ ", Style::default().fg(THEME.border)),
                    Span::styled(&first_agent.status, Style::default().fg(status_color)),
                    Span::styled(" │ ", Style::default().fg(THEME.border)),
                    Span::styled(format!("conf: {:.0}%", first_agent.confidence * 100.0), Style::default().fg(THEME.text_dim)),
                ]),
            ]
        } else {
            vec![Line::from(Span::styled("  🤖 No agents active", THEME.dim_style()))]
        };

        let agent_para = Paragraph::new(agent_status)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(THEME.border)),
            );
        frame.render_widget(agent_para, chunks[0]);

        // ── System Status ──────────────────────────────────────────────────
        let ws_indicator = if app.ws_connected {
            Span::styled("● WS ", Style::default().fg(THEME.positive))
        } else {
            Span::styled("○ WS ", Style::default().fg(THEME.negative))
        };

        let system_status = vec![
            Line::from(vec![
                Span::styled("  ", Style::default()),
                ws_indicator,
                Span::styled("│ ", Style::default().fg(THEME.border)),
                Span::styled(
                    format!("Uptime: {}", app.health.uptime),
                    Style::default().fg(THEME.text_dim),
                ),
                Span::styled(" │ ", Style::default().fg(THEME.border)),
                Span::styled(
                    format!("CPU: {:.1}%", app.health.cpu_usage),
                    Style::default().fg(THEME.text_dim),
                ),
            ]),
        ];

        let system_para = Paragraph::new(system_status)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(THEME.border)),
            );
        frame.render_widget(system_para, chunks[1]);
    }
}

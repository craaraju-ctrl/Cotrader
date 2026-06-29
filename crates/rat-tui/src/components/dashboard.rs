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
                Constraint::Min(10),     // Market overview table
                Constraint::Length(6),   // Quick actions / status
            ])
            .split(area);

        self.render_portfolio_cards(frame, chunks[0], app);
        self.render_market_table(frame, chunks[1], app);
        self.render_status_bar(frame, chunks[2], app);
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

        let equity_content = vec![
            Line::from(Span::styled(
                format!("${:.2}", equity),
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("Cash: ${:.2}", cash),
                Style::default().fg(THEME.text_dim),
            )),
        ];

        let equity_gauge = Gauge::default()
            .block(equity_block)
            .gauge_style(Style::default().fg(THEME.positive).bg(THEME.surface))
            .ratio(equity_pct as f64 / 100.0)
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
            .ratio(win_rate / 100.0)
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

        let rows: Vec<Row> = app.watchlist.iter().filter_map(|sym| {
            app.market_data.get(sym).map(|md| {
                let change_color = if md.change_24h >= 0.0 { THEME.positive } else { THEME.negative };
                let is_selected = *sym == app.selected_symbol;
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
        }).collect();

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
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("► ");

        frame.render_widget(table, area);
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

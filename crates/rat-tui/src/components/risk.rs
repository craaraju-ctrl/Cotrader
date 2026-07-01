//! Risk Dashboard component — VaR, portfolio beta, exposure metrics.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Sparkline};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct RiskDashboard;

impl Component for RiskDashboard {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let risk = &app.risk;

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),  // Risk gauges row
                Constraint::Length(8),  // Exposure chart
                Constraint::Min(5),    // Position risk details
            ])
            .split(area);

        // ── Risk Gauges Row ───────────────────────────────────────────────
        let gauge_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .split(chunks[0]);

        // VaR 95%
        let var95_block = Block::default()
            .title(" VaR 95% ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));
        let var95_gauge = Gauge::default()
            .block(var95_block)
            .gauge_style(Style::default().fg(THEME.negative).bg(THEME.surface))
            .ratio((risk.var_95 / app.portfolio.equity.max(1.0)).min(1.0))
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
            .ratio((risk.daily_volatility / 10.0).min(1.0))
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
            .ratio((risk.margin_usage / 100.0).min(1.0))
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
            .ratio((risk.concentration_pct / 100.0).min(1.0))
            .label(Span::styled(
                format!("{:.1}%", risk.concentration_pct),
                Style::default().fg(THEME.text),
            ));
        frame.render_widget(conc_gauge, gauge_chunks[3]);

        // ── Exposure & Equity Sparkline ───────────────────────────────────
        let chart_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(chunks[1]);

        // Equity sparkline
        let eq_spark_data: Vec<u64> = if app.equity_history.len() >= 2 {
            let min = app.equity_history.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = app.equity_history.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let range = (max - min).max(1.0);
            app.equity_history.iter().map(|v| ((v - min) / range * 200.0) as u64).collect()
        } else {
            vec![100; 20] // Default flat line
        };
        let eq_spark = Sparkline::default()
            .block(Block::default()
                .title(" EQUITY TREND ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(THEME.border)))
            .data(&eq_spark_data)
            .style(Style::default().fg(THEME.positive));
        frame.render_widget(eq_spark, chart_chunks[0]);

        // P&L sparkline
        let pnl_spark_data: Vec<u64> = if app.pnl_history.len() >= 2 {
            let min = app.pnl_history.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = app.pnl_history.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let range = (max - min).max(1.0);
            app.pnl_history.iter().map(|v| ((v - min) / range * 200.0) as u64).collect()
        } else {
            vec![100; 20]
        };
        let pnl_spark = Sparkline::default()
            .block(Block::default()
                .title(" P&L TREND ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(THEME.border)))
            .data(&pnl_spark_data)
            .style(Style::default().fg(THEME.accent));
        frame.render_widget(pnl_spark, chart_chunks[1]);

        // ── Risk Summary Table ────────────────────────────────────────────
        let risk_lines = vec![
            Line::from(vec![
                Span::styled("  VaR 95%:        ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("${:.2}", risk.var_95), Style::default().fg(THEME.negative).add_modifier(Modifier::BOLD)),
                Span::styled("   VaR 99%: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("${:.2}", risk.var_99), Style::default().fg(THEME.negative)),
            ]),
            Line::from(vec![
                Span::styled("  Sharpe Ratio:   ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{:.2}", risk.sharpe_ratio), Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD)),
                Span::styled("   Beta:     ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{:.2}", risk.portfolio_beta), Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("  Exposure:       ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("${:.0}", risk.total_exposure), Style::default().fg(THEME.text)),
                Span::styled(format!(" ({:.1}%)", risk.exposure_pct), Style::default().fg(THEME.text_dim)),
                Span::styled("   At Risk:  ", Style::default().fg(THEME.text_dim)),
                Span::styled(
                    format!("{} positions", risk.at_risk_positions),
                    if risk.at_risk_positions > 0 { Style::default().fg(THEME.negative) } else { Style::default().fg(THEME.positive) },
                ),
            ]),
            Line::from(vec![
                Span::styled("  Daily Vol:      ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{:.2}%", risk.daily_volatility), Style::default().fg(THEME.warning)),
                Span::styled("   Margin:   ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{:.1}%", risk.margin_usage), Style::default().fg(if risk.margin_usage > 80.0 { THEME.negative } else { THEME.text })),
            ]),
        ];

        let risk_table = Paragraph::new(risk_lines)
            .block(
                Block::default()
                    .title(" RISK SUMMARY ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(THEME.border)),
            );
        frame.render_widget(risk_table, chunks[2]);
    }
}

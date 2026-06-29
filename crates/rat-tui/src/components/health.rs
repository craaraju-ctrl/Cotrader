//! Health component — System health and service status.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct HealthComponent;

impl Component for HealthComponent {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),
                Constraint::Min(8),
            ])
            .split(area);

        self.render_system_metrics(frame, chunks[0], app);
        self.render_service_table(frame, chunks[1], app);
    }
}

impl HealthComponent {
    fn render_system_metrics(&self, frame: &mut Frame, area: Rect, app: &App) {
        let card_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(area);

        // ── CPU Usage ──────────────────────────────────────────────────────
        let cpu_block = Block::default()
            .title(" CPU USAGE ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let cpu_gauge = Gauge::default()
            .block(cpu_block)
            .gauge_style(Style::default().fg(THEME.info).bg(THEME.surface))
            .ratio(app.health.cpu_usage / 100.0)
            .label(Span::styled(
                format!("{:.1}%", app.health.cpu_usage),
                Style::default().fg(THEME.text),
            ));

        frame.render_widget(cpu_gauge, card_chunks[0]);

        // ── Memory Usage ───────────────────────────────────────────────────
        let mem_block = Block::default()
            .title(" MEMORY USAGE ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let mem_gauge = Gauge::default()
            .block(mem_block)
            .gauge_style(Style::default().fg(THEME.accent).bg(THEME.surface))
            .ratio(app.health.memory_usage / 100.0)
            .label(Span::styled(
                format!("{:.1}%", app.health.memory_usage),
                Style::default().fg(THEME.text),
            ));

        frame.render_widget(mem_gauge, card_chunks[1]);

        // ── Uptime ─────────────────────────────────────────────────────────
        let uptime_block = Block::default()
            .title(" UPTIME ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let uptime_para = Paragraph::new(vec![
            Line::from(Span::styled(
                &app.health.uptime,
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
        ]).block(uptime_block);

        frame.render_widget(uptime_para, card_chunks[2]);
    }

    fn render_service_table(&self, frame: &mut Frame, area: Rect, app: &App) {
        let header = Row::new(["Service", "Status", "Latency", "Last Check"])
            .style(Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD))
            .height(1);

        let rows: Vec<Row> = app.health.services.values().map(|svc| {
            let status_color = match svc.status.as_str() {
                "healthy" | "running" => THEME.positive,
                "error" | "down" => THEME.negative,
                "degraded" => THEME.warning,
                _ => THEME.text_dim,
            };

            let latency_str = if svc.latency_ms > 0.0 {
                format!("{:.1}ms", svc.latency_ms)
            } else {
                "—".to_string()
            };

            let last_check_str = svc.last_check
                .map(|t| {
                    let elapsed = t.elapsed().as_secs();
                    if elapsed < 60 {
                        format!("{}s ago", elapsed)
                    } else {
                        format!("{}m ago", elapsed / 60)
                    }
                })
                .unwrap_or_else(|| "—".to_string());

            Row::new(vec![
                ratatui::widgets::Cell::from(Span::styled(
                    &svc.name,
                    Style::default().fg(THEME.text).add_modifier(Modifier::BOLD),
                )),
                ratatui::widgets::Cell::from(Span::styled(
                    &svc.status,
                    Style::default().fg(status_color),
                )),
                ratatui::widgets::Cell::from(latency_str),
                ratatui::widgets::Cell::from(last_check_str),
            ])
        }).collect();

        let table = Table::new(
            rows,
            [Constraint::Length(15), Constraint::Length(12), Constraint::Length(12), Constraint::Length(15)],
        )
        .header(header)
        .block(
            Block::default()
                .title(" SERVICE STATUS ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(THEME.border)),
        );

        frame.render_widget(table, area);
    }
}

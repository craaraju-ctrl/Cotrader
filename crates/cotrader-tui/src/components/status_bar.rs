//! Status bar component — Service health indicators.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct StatusBarComponent;

impl Component for StatusBarComponent {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let mut spans = vec![
            Span::styled(" Services: ", Style::default().fg(THEME.text_dim)),
        ];

        // Health indicators from live service status (dynamic)
        let service_keys: Vec<_> = app.health.services.keys().collect();
        for (i, svc_name) in service_keys.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("│ ", Style::default().fg(THEME.border)));
            }
            let status_str = app.health.services.get(*svc_name)
                .map(|s| s.status.as_str())
                .unwrap_or("unknown");
            let color = match status_str {
                "healthy" | "running" => THEME.positive,
                "error" | "down" => THEME.negative,
                _ => THEME.muted,
            };
            let icon = if matches!(status_str, "healthy" | "running" | "monitoring" | "connected") {
                "●"
            } else {
                "○"
            };
            spans.push(Span::styled(
                format!("{} {} ", icon, svc_name),
                Style::default().fg(color),
            ));
        }

        // Alert count badge
        let alert_count = app.alerts.iter().filter(|a| !a.read).count();
        if alert_count > 0 {
            spans.push(Span::styled("│ ", Style::default().fg(THEME.border)));
            let alert_color = if alert_count > 5 { THEME.negative } else { THEME.warning };
            spans.push(Span::styled(
                format!("🔔 {} ", alert_count),
                Style::default().fg(alert_color),
            ));
        }

        // Risk metrics summary
        if app.risk.var_95 > 0.0 {
            spans.push(Span::styled("│ ", Style::default().fg(THEME.border)));
            let risk_color = if app.risk.concentration_pct > 50.0 { THEME.negative } else { THEME.text_dim };
            spans.push(Span::styled(
                format!("VaR95: ${:.0} ", app.risk.var_95),
                Style::default().fg(risk_color),
            ));
        }

        let footer = Paragraph::new(Line::from(spans))
            .style(Style::default().bg(THEME.surface));

        frame.render_widget(footer, area);
    }
}

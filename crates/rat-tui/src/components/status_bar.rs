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

        let footer = Paragraph::new(Line::from(spans))
            .style(Style::default().bg(THEME.surface));

        frame.render_widget(footer, area);
    }
}

//! Status bar component — Service health indicators.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
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

        // LLM status
        let llm_status = app.health.services.get("llm");
        let llm_icon = match llm_status.map(|s| s.status.as_str()) {
            Some("healthy") | Some("running") => Span::styled("● LLM ", Style::default().fg(THEME.positive)),
            Some("error") | Some("down") => Span::styled("● LLM ", Style::default().fg(THEME.negative)),
            _ => Span::styled("○ LLM ", Style::default().fg(THEME.muted)),
        };
        spans.push(llm_icon);
        spans.push(Span::styled("│ ", Style::default().fg(THEME.border)));

        // Kronos status
        let kronos_status = app.health.services.get("kronos");
        let kronos_icon = match kronos_status.map(|s| s.status.as_str()) {
            Some("healthy") | Some("running") => Span::styled("● Kronos ", Style::default().fg(THEME.positive)),
            Some("error") | Some("down") => Span::styled("● Kronos ", Style::default().fg(THEME.negative)),
            _ => Span::styled("○ Kronos ", Style::default().fg(THEME.muted)),
        };
        spans.push(kronos_icon);
        spans.push(Span::styled("│ ", Style::default().fg(THEME.border)));

        // Memory status
        let mem_status = app.health.services.get("memory");
        let mem_icon = match mem_status.map(|s| s.status.as_str()) {
            Some("healthy") | Some("running") => Span::styled("● Memory ", Style::default().fg(THEME.positive)),
            Some("error") | Some("down") => Span::styled("● Memory ", Style::default().fg(THEME.negative)),
            _ => Span::styled("○ Memory ", Style::default().fg(THEME.muted)),
        };
        spans.push(mem_icon);

        let footer = Paragraph::new(Line::from(spans))
            .style(Style::default().bg(THEME.surface));

        frame.render_widget(footer, area);
    }
}

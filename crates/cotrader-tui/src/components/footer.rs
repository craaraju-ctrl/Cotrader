//! Footer component — Keybindings and status line.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct FooterComponent;

impl Component for FooterComponent {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let ws_status = if app.ws_connected {
            Span::styled("● Connected ", Style::default().fg(THEME.positive))
        } else {
            Span::styled("○ Disconnected ", Style::default().fg(THEME.negative))
        };

        let action_status = if app.action_running {
            Span::styled(" ⏳ Processing... ", Style::default().fg(THEME.warning))
        } else if let Some((msg, time)) = &app.action_message {
            if time.elapsed() < std::time::Duration::from_secs(5) {
                Span::styled(format!(" {} ", msg), Style::default().fg(THEME.positive))
            } else {
                Span::raw("")
            }
        } else {
            Span::raw("")
        };

        let error_status = if let Some(err) = &app.error {
            Span::styled(format!(" ❌ {} ", err), Style::default().fg(THEME.negative))
        } else {
            Span::raw("")
        };

        // Per-source freshness: a dead endpoint must be VISIBLE. Green dot =
        // last fetch OK; red ✖ with age = failing/stale (dim green >30s = stale).
        let mut source_spans: Vec<Span> = Vec::new();
        for name in ["portfolio", "prices", "status", "health"] {
            if let Some((ok, when)) = app.source_status.get(name) {
                let age = when.elapsed().as_secs();
                let (icon, style) = if *ok && age <= 30 {
                    ("●", Style::default().fg(THEME.positive))
                } else if *ok {
                    ("●", Style::default().fg(THEME.text_dim)) // stale
                } else {
                    ("✖", Style::default().fg(THEME.negative))
                };
                source_spans.push(Span::styled("│", Style::default().fg(THEME.border)));
                source_spans.push(Span::styled(
                    format!(" {} {}{} ", name, icon, if *ok { String::new() } else { format!(" {}s", age) }),
                    style,
                ));
            }
        }

        let focused_indicator = Span::styled(
            format!(" Panel {} ", app.focused_panel + 1),
            Style::default().fg(THEME.info),
        );

        let mut spans = vec![
            Span::styled(" q", Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD)),
            Span::styled(" Quit ", Style::default().fg(THEME.text_dim)),
            Span::styled("│", Style::default().fg(THEME.border)),
            Span::styled(" Tab", Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD)),
            Span::styled(" Switch ", Style::default().fg(THEME.text_dim)),
            Span::styled("│", Style::default().fg(THEME.border)),
            Span::styled(" 1-9", Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD)),
            Span::styled(" Jump ", Style::default().fg(THEME.text_dim)),
            Span::styled("│", Style::default().fg(THEME.border)),
            Span::styled(" /", Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD)),
            Span::styled(" Search ", Style::default().fg(THEME.text_dim)),
            Span::styled("│", Style::default().fg(THEME.border)),
            Span::styled(" ?", Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD)),
            Span::styled(" Help ", Style::default().fg(THEME.text_dim)),
            focused_indicator,
            Span::styled("│", Style::default().fg(THEME.border)),
            ws_status,
        ];
        spans.extend(source_spans);
        spans.push(action_status);
        spans.push(error_status);
        let footer_line = Line::from(spans);

        let footer = Paragraph::new(footer_line)
            .style(Style::default().bg(THEME.surface));

        frame.render_widget(footer, area);
    }
}

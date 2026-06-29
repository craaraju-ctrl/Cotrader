//! Footer component — Keybindings and status line.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
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

        let footer_line = Line::from(vec![
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
            ws_status,
            action_status,
            error_status,
        ]);

        let footer = Paragraph::new(footer_line)
            .style(Style::default().bg(THEME.surface));

        frame.render_widget(footer, area);
    }
}

//! Settings component — Configuration and preferences.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct SettingsComponent;

impl Component for SettingsComponent {
    fn render(&self, frame: &mut Frame, area: Rect, _app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(25),
                Constraint::Min(30),
            ])
            .split(area);

        // ── Settings menu ──────────────────────────────────────────────────
        let menu_items = vec![
            ListItem::new(Line::from(Span::styled(
                " Trading ",
                Style::default().fg(THEME.text),
            ))),
            ListItem::new(Line::from(Span::styled(
                " Risk Parameters ",
                Style::default().fg(THEME.text),
            ))),
            ListItem::new(Line::from(Span::styled(
                " Agents ",
                Style::default().fg(THEME.text),
            ))),
            ListItem::new(Line::from(Span::styled(
                " Display ",
                Style::default().fg(THEME.text),
            ))),
            ListItem::new(Line::from(Span::styled(
                " System ",
                Style::default().fg(THEME.text),
            ))),
        ];

        let menu = List::new(menu_items)
            .block(
                Block::default()
                    .title(" SETTINGS ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(THEME.border)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(40, 40, 60))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("► ");

        frame.render_widget(menu, chunks[0]);

        // ── Settings content ───────────────────────────────────────────────
        let content = vec![
            Line::from(Span::styled(
                "Trading Settings",
                Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Paper Mode: ", Style::default().fg(THEME.text_dim)),
                Span::styled("Enabled", Style::default().fg(THEME.positive)),
            ]),
            Line::from(vec![
                Span::styled("Initial Balance: ", Style::default().fg(THEME.text_dim)),
                Span::styled("$100,000", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("Max Position Size: ", Style::default().fg(THEME.text_dim)),
                Span::styled("10%", Style::default().fg(THEME.text)),
            ]),
            Line::from(vec![
                Span::styled("Risk Per Trade: ", Style::default().fg(THEME.text_dim)),
                Span::styled("1%", Style::default().fg(THEME.text)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Press Enter to modify settings",
                Style::default().fg(THEME.text_dim),
            )),
        ];

        let content_block = Block::default()
            .title(" CONFIGURATION ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let content_para = Paragraph::new(content).block(content_block);
        frame.render_widget(content_para, chunks[1]);
    }
}

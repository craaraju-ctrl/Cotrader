//! Tab navigation component.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs};
use ratatui::Frame;
use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Tab, NUM_TABS};
use crate::components::Component;
use crate::theme::THEME;

pub struct TabsComponent;

impl Component for TabsComponent {
    fn handle_key(&mut self, key: KeyEvent, app: &mut App) -> bool {
        match key.code {
            KeyCode::Tab => {
                app.selected_tab = match app.selected_tab {
                    Tab::Help => Tab::Dashboard,
                    _ => unsafe { std::mem::transmute(app.selected_tab as u8 + 1) },
                };
                app.scroll_offset = 0;
                app.selected_row = 0;
                true
            }
            KeyCode::BackTab => {
                app.selected_tab = match app.selected_tab {
                    Tab::Dashboard => Tab::Help,
                    _ => unsafe { std::mem::transmute(app.selected_tab as u8 - 1) },
                };
                app.scroll_offset = 0;
                app.selected_row = 0;
                true
            }
            KeyCode::Char(c @ '1'..='9') => {
                let idx = (c as usize - '1' as usize).min(NUM_TABS - 1);
                app.selected_tab = unsafe { std::mem::transmute(idx as u8) };
                app.scroll_offset = 0;
                app.selected_row = 0;
                true
            }
            KeyCode::Char('0') => {
                app.selected_tab = Tab::Help;
                app.scroll_offset = 0;
                true
            }
            _ => false,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let titles: Vec<Line> = (0..NUM_TABS)
            .map(|i| {
                let tab: Tab = unsafe { std::mem::transmute(i as u8) };
                let title = format!("{} {}", tab.icon(), tab.title());
                if i == app.selected_tab as usize {
                    Line::from(Span::styled(
                        title,
                        Style::default()
                            .fg(THEME.brand)
                            .add_modifier(Modifier::BOLD),
                    ))
                } else {
                    Line::from(Span::styled(title, Style::default().fg(THEME.text_dim)))
                }
            })
            .collect();

        let tabs = Tabs::new(titles)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(THEME.border)),
            )
            .select(app.selected_tab as usize)
            .style(Style::default().fg(THEME.text))
            .highlight_style(
                Style::default()
                    .fg(THEME.brand)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_widget(tabs, area);
    }
}

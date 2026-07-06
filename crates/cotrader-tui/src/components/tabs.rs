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

/// Convert a byte index to a Tab variant safely.
/// Returns None if the index is out of range.
fn tab_from_index(idx: u8) -> Option<Tab> {
    match idx {
        0 => Some(Tab::Dashboard),
        1 => Some(Tab::Trading),
        2 => Some(Tab::Orderbook),
        3 => Some(Tab::Positions),
        4 => Some(Tab::Agents),
        5 => Some(Tab::Performance),
        6 => Some(Tab::PolicyCache),
        7 => Some(Tab::Health),
        8 => Some(Tab::Settings),
        9 => Some(Tab::Help),
        _ => None,
    }
}

/// Get the next tab in order, wrapping around to Dashboard after Help.
fn next_tab(tab: Tab) -> Tab {
    let next_idx = ((tab as u8) + 1) % NUM_TABS as u8;
    tab_from_index(next_idx).unwrap_or(Tab::Dashboard)
}

/// Get the previous tab in order, wrapping from Dashboard to Help.
fn prev_tab(tab: Tab) -> Tab {
    let prev_idx = (tab as u8 + NUM_TABS as u8 - 1) % NUM_TABS as u8;
    tab_from_index(prev_idx).unwrap_or(Tab::Dashboard)
}

pub struct TabsComponent;

impl Component for TabsComponent {
    fn handle_key(&mut self, key: KeyEvent, app: &mut App) -> bool {
        match key.code {
            KeyCode::Tab => {
                app.selected_tab = next_tab(app.selected_tab);
                app.scroll_offset = 0;
                app.selected_row = 0;
                true
            }
            KeyCode::BackTab => {
                app.selected_tab = prev_tab(app.selected_tab);
                app.scroll_offset = 0;
                app.selected_row = 0;
                true
            }
            KeyCode::Char(c @ '1'..='9') => {
                let idx = (c as u8 - b'1').min(NUM_TABS as u8 - 1);
                if let Some(tab) = tab_from_index(idx) {
                    app.selected_tab = tab;
                }
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
                let tab = tab_from_index(i as u8).unwrap_or(Tab::Dashboard);
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

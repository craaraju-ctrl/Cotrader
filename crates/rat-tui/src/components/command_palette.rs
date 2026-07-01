//! Command palette component — Quick navigation and actions (Ctrl+K style).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;
use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Tab};
use crate::components::Component;
use crate::theme::THEME;

/// A single command in the palette
struct Command {
    name: &'static str,
    shortcut: &'static str,
    description: &'static str,
    action: fn(&mut App),
}

fn get_commands() -> Vec<Command> {
    vec![
        Command { name: "Dashboard", shortcut: "1", description: "Portfolio overview & market watchlist", action: |app| { app.selected_tab = Tab::Dashboard; } },
        Command { name: "Trading", shortcut: "2", description: "Orderbook & trade execution", action: |app| { app.selected_tab = Tab::Trading; } },
        Command { name: "Orderbook", shortcut: "3", description: "Full depth order book view", action: |app| { app.selected_tab = Tab::Orderbook; } },
        Command { name: "Positions", shortcut: "4", description: "Open positions & P&L", action: |app| { app.selected_tab = Tab::Positions; } },
        Command { name: "Agents", shortcut: "5", description: "Agent hierarchy & COT log", action: |app| { app.selected_tab = Tab::Agents; } },
        Command { name: "Performance", shortcut: "6", description: "Charts & performance metrics", action: |app| { app.selected_tab = Tab::Performance; } },
        Command { name: "Policy", shortcut: "7", description: "Agent policy cache & decisions", action: |app| { app.selected_tab = Tab::PolicyCache; } },
        Command { name: "Risk", shortcut: "8", description: "Risk dashboard & VaR metrics", action: |app| { app.selected_tab = Tab::Health; } },
        Command { name: "Settings", shortcut: "9", description: "Configuration & mode toggle", action: |app| { app.selected_tab = Tab::Settings; } },
        Command { name: "Help", shortcut: "0", description: "Keyboard shortcuts & about", action: |app| { app.selected_tab = Tab::Help; } },
        Command { name: "Toggle Mode", shortcut: "", description: "Switch paper/live trading mode", action: |app| { app.pending_command = Some(crate::api_client::StatusMsg::ToggleMode); } },
        Command { name: "Clear Error", shortcut: "", description: "Dismiss any error messages", action: |app| { app.clear_error(); } },
        Command { name: "Reset Drawdown", shortcut: "", description: "Reset max drawdown counter", action: |app| { app.portfolio.max_drawdown = 0.0; } },
    ]
}

pub struct CommandPalette;

impl Component for CommandPalette {
    fn handle_key(&mut self, key: KeyEvent, app: &mut App) -> bool {
        if !app.show_command_palette {
            return false;
        }

        match key.code {
            KeyCode::Esc => {
                app.show_command_palette = false;
                app.command_palette_query.clear();
                app.command_palette_selected = 0;
                true
            }
            KeyCode::Up => {
                if app.command_palette_selected > 0 {
                    app.command_palette_selected -= 1;
                }
                true
            }
            KeyCode::Down => {
                let commands = filtered_commands(&app.command_palette_query);
                if app.command_palette_selected < commands.len().saturating_sub(1) {
                    app.command_palette_selected += 1;
                }
                true
            }
            KeyCode::Enter => {
                let commands = filtered_commands(&app.command_palette_query);
                if let Some(cmd) = commands.get(app.command_palette_selected) {
                    (cmd.action)(app);
                }
                app.show_command_palette = false;
                app.command_palette_query.clear();
                app.command_palette_selected = 0;
                true
            }
            KeyCode::Char(c) => {
                app.command_palette_query.push(c);
                app.command_palette_selected = 0;
                true
            }
            KeyCode::Backspace => {
                app.command_palette_query.pop();
                app.command_palette_selected = 0;
                true
            }
            _ => false,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        if !app.show_command_palette {
            return;
        }

        // Dim background overlay
        let overlay = ratatui::widgets::Block::default()
            .style(Style::default().bg(Color::Rgb(0, 0, 0)));
        frame.render_widget(overlay, area);

        // Center the palette
        let palette_width = area.width.min(60);
        let palette_height = area.height.min(25);
        let x = (area.width.saturating_sub(palette_width)) / 2;
        let y = area.height / 6;
        let palette_area = Rect::new(x, y, palette_width, palette_height);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Search input
                Constraint::Min(5),    // Command list
                Constraint::Length(1), // Footer hint
            ])
            .split(palette_area);

        // Search input
        let query_display = if app.command_palette_query.is_empty() {
            "Type to search commands...".to_string()
        } else {
            app.command_palette_query.clone()
        };
        let input = Paragraph::new(Line::from(vec![
            Span::styled("❯ ", Style::default().fg(THEME.brand)),
            Span::styled(
                query_display,
                if app.command_palette_query.is_empty() {
                    Style::default().fg(THEME.muted)
                } else {
                    Style::default().fg(THEME.text)
                },
            ),
        ]))
        .block(
            Block::default()
                .title(" COMMAND PALETTE ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(THEME.brand)),
        );
        frame.render_widget(input, chunks[0]);

        // Filtered commands
        let commands = filtered_commands(&app.command_palette_query);
        let items: Vec<ListItem> = commands.iter().enumerate().map(|(i, cmd)| {
            let is_selected = i == app.command_palette_selected;
            let bg = if is_selected { Color::Rgb(40, 40, 60) } else { Color::Rgb(20, 20, 30) };

            let line = Line::from(vec![
                Span::styled(
                    format!("  {:<20}", cmd.name),
                    Style::default().fg(if is_selected { THEME.brand } else { THEME.text })
                        .add_modifier(if is_selected { Modifier::BOLD } else { Modifier::empty() }),
                ),
                Span::styled(
                    format!("{:<6}", cmd.shortcut),
                    Style::default().fg(THEME.highlight),
                ),
                Span::styled(
                    cmd.description,
                    Style::default().fg(THEME.text_dim),
                ),
            ]);

            ListItem::new(line).style(Style::default().bg(bg))
        }).collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::LEFT | Borders::RIGHT)
                    .border_style(Style::default().fg(THEME.border)),
            );

        frame.render_widget(list, chunks[1]);

        // Footer
        let footer = Paragraph::new(Line::from(vec![
            Span::styled(" ↑↓ ", Style::default().fg(THEME.brand)),
            Span::styled("navigate ", Style::default().fg(THEME.text_dim)),
            Span::styled("│ ", Style::default().fg(THEME.border)),
            Span::styled("Enter ", Style::default().fg(THEME.brand)),
            Span::styled("select ", Style::default().fg(THEME.text_dim)),
            Span::styled("│ ", Style::default().fg(THEME.border)),
            Span::styled("Esc ", Style::default().fg(THEME.brand)),
            Span::styled("close", Style::default().fg(THEME.text_dim)),
        ]))
        .style(Style::default().bg(THEME.surface));
        frame.render_widget(footer, chunks[2]);
    }
}

fn filtered_commands(query: &str) -> Vec<Command> {
    let commands = get_commands();
    if query.is_empty() {
        return commands;
    }
    let q = query.to_lowercase();
    commands.into_iter()
        .filter(|cmd| {
            cmd.name.to_lowercase().contains(&q)
                || cmd.description.to_lowercase().contains(&q)
                || cmd.shortcut.contains(&q)
        })
        .collect()
}

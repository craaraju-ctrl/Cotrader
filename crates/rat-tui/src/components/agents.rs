//! Agents component — Agent hierarchy tree with live status.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;

pub struct AgentsComponent;

impl Component for AgentsComponent {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(60),
            ])
            .split(area);

        // ── Agent tree ─────────────────────────────────────────────────────
        self.render_agent_tree(frame, chunks[0], app);

        // ── Agent details ──────────────────────────────────────────────────
        self.render_agent_details(frame, chunks[1], app);
    }
}

impl AgentsComponent {
    fn render_agent_tree(&self, frame: &mut Frame, area: Rect, app: &App) {
        let items: Vec<ListItem> = app.agents.iter().enumerate().map(|(i, agent)| {
            let status_icon = match agent.status.as_str() {
                "active" | "running" => Span::styled("● ", Style::default().fg(THEME.positive)),
                "error" | "failed" => Span::styled("● ", Style::default().fg(THEME.negative)),
                "idle" => Span::styled("● ", Style::default().fg(THEME.muted)),
                _ => Span::styled("○ ", Style::default().fg(THEME.text_dim)),
            };

            let prefix = if i == 0 { "├─ " } else if i == app.agents.len() - 1 { "└─ " } else { "├─ " };

            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(THEME.border)),
                status_icon,
                Span::styled(
                    &agent.name,
                    Style::default().fg(THEME.text).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ", Style::default()),
                Span::styled(
                    format!("({:.0}%)", agent.confidence * 100.0),
                    Style::default().fg(THEME.text_dim),
                ),
            ]);

            ListItem::new(line)
        }).collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" AGENT HIERARCHY ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(THEME.border)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(40, 40, 60))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("► ");

        frame.render_widget(list, area);
    }

    fn render_agent_details(&self, frame: &mut Frame, area: Rect, app: &App) {
        let selected_idx = app.selected_row.min(app.agents.len().saturating_sub(1));
        let agent = app.agents.get(selected_idx);

        let content = if let Some(agent) = agent {
            vec![
                Line::from(Span::styled(
                    &agent.name,
                    Style::default().fg(THEME.brand).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(THEME.text_dim)),
                    Span::styled(
                        &agent.status,
                        Style::default().fg(match agent.status.as_str() {
                            "active" => THEME.positive,
                            "error" => THEME.negative,
                            _ => THEME.text,
                        }),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Confidence: ", Style::default().fg(THEME.text_dim)),
                    Span::styled(format!("{:.1}%", agent.confidence * 100.0), THEME.highlight_style()),
                ]),
                Line::from(""),
                Line::from(Span::styled("Last Action:", Style::default().fg(THEME.text_dim))),
                Line::from(Span::styled(
                    &agent.last_action,
                    Style::default().fg(THEME.text),
                )),
                Line::from(""),
                Line::from(Span::styled("Reasoning:", Style::default().fg(THEME.text_dim))),
                Line::from(Span::styled(
                    &agent.reason,
                    Style::default().fg(THEME.text_dim),
                )),
            ]
        } else {
            vec![Line::from(Span::styled(
                "Select an agent to view details",
                THEME.dim_style(),
            ))]
        };

        let block = Block::default()
            .title(" AGENT DETAILS ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let para = Paragraph::new(content).block(block);
        frame.render_widget(para, area);
    }
}

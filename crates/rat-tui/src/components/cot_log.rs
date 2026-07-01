//! COT log component — Chain-of-thought log with real-time entries.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::components::Component;
use crate::theme::THEME;
use crossterm::event::{KeyCode, KeyEvent};

pub struct CotLogComponent;

impl Component for CotLogComponent {
    fn handle_key(&mut self, key: KeyEvent, app: &mut App) -> bool {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let max = app.cot_log.len().saturating_sub(1);
                if app.cot_log_row < max {
                    app.cot_log_row += 1;
                }
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.cot_log_row > 0 {
                    app.cot_log_row -= 1;
                }
                true
            }
            KeyCode::Char('g') => {
                app.cot_log_row = 0;
                true
            }
            KeyCode::Char('G') => {
                app.cot_log_row = app.cot_log.len().saturating_sub(1);
                true
            }
            _ => false,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(2),
            ])
            .split(area);

        self.render_log(frame, chunks[0], app);
        self.render_status(frame, chunks[1], app);
    }
}

impl CotLogComponent {
    fn render_log(&self, frame: &mut Frame, area: Rect, app: &App) {
        let visible_height = area.height.saturating_sub(2) as usize;

        if app.cot_log.is_empty() {
            let empty = Paragraph::new(Line::from(Span::styled(
                "No COT entries yet — waiting for agent pipeline data...",
                THEME.dim_style(),
            )))
            .block(
                Block::default()
                    .title(" CHAIN-OF-THOUGHT LOG ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(THEME.border)),
            );
            frame.render_widget(empty, area);
            return;
        }

        let total = app.cot_log.len();
        let scroll = app.cot_log_row.min(total.saturating_sub(1));

        let lines: Vec<Line> = app.cot_log.iter().enumerate().skip(scroll).take(visible_height).map(|(i, entry)| {
            let agent = entry.get("agent").and_then(|v| v.as_str()).unwrap_or("?");
            let action = entry.get("action").and_then(|v| v.as_str()).unwrap_or("");
            let reason = entry.get("reason").and_then(|v| v.as_str())
                .or_else(|| entry.get("reasoning").and_then(|v| v.as_str()))
                .unwrap_or("");
            let confidence = entry.get("confidence").and_then(|v| v.as_f64());
            let symbol = entry.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
            let timestamp = entry.get("timestamp").and_then(|v| v.as_str())
                .or_else(|| entry.get("time").and_then(|v| v.as_str()))
                .unwrap_or("");

            let agent_color = match agent {
                "HardRulesGate" | "hard_rules" => THEME.warning,
                "Identifier" | "identifier" => THEME.highlight,
                "Verifier" | "verifier" => THEME.accent,
                "DebateLayer" | "debate" => THEME.info,
                "Judge" | "judge" => THEME.brand,
                "StrategyDecision" | "strategy" => THEME.positive,
                "RiskEngine" | "risk" => THEME.negative,
                _ => THEME.text,
            };

            let mut spans = vec![
                Span::styled(
                    format!("{} ", if i == app.cot_log_row { "►" } else { " " }),
                    Style::default().fg(THEME.border),
                ),
                Span::styled(
                    format!("[{}] ", agent),
                    Style::default().fg(agent_color).add_modifier(Modifier::BOLD),
                ),
            ];

            if !symbol.is_empty() {
                spans.push(Span::styled(
                    format!("{} ", symbol),
                    Style::default().fg(THEME.brand),
                ));
            }

            if !action.is_empty() {
                let action_color = match action {
                    "PASS" | "EXECUTE" | "BUY" | "SELL" => THEME.positive,
                    "FAIL" | "HALT" | "SKIP" => THEME.negative,
                    "HOLD" | "DEBATE" => THEME.warning,
                    _ => THEME.text_dim,
                };
                spans.push(Span::styled(
                    format!("{} ", action),
                    Style::default().fg(action_color).add_modifier(Modifier::BOLD),
                ));
            }

            if let Some(conf) = confidence {
                spans.push(Span::styled(
                    format!("{:.0}% ", conf * 100.0),
                    Style::default().fg(THEME.text_dim),
                ));
            }

            if !reason.is_empty() {
                let display_reason = if reason.len() > 80 { &reason[..80] } else { reason };
                spans.push(Span::styled(
                    display_reason.to_string(),
                    Style::default().fg(THEME.text_dim),
                ));
            }

            if !timestamp.is_empty() {
                let ts = if timestamp.len() > 19 { &timestamp[..19] } else { timestamp };
                spans.push(Span::styled(
                    format!(" {}", ts),
                    Style::default().fg(THEME.muted),
                ));
            }

            Line::from(spans)
        }).collect();

        let block = Block::default()
            .title(format!(" COT LOG ({}/{}) ", scroll + 1, total))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let para = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });

        frame.render_widget(para, area);
    }

    fn render_status(&self, frame: &mut Frame, area: Rect, app: &App) {
        let total = app.cot_log.len();
        let selected = if total > 0 { app.cot_log_row + 1 } else { 0 };

        let content = vec![
            Line::from(vec![
                Span::styled("  Entries: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{}", total), Style::default().fg(THEME.brand)),
                Span::styled(" │ Position: ", Style::default().fg(THEME.text_dim)),
                Span::styled(format!("{}/{}", selected, total), Style::default().fg(THEME.text)),
                Span::styled(" │ ", Style::default().fg(THEME.border)),
                Span::styled("j/k: scroll  g/G: top/bottom", Style::default().fg(THEME.text_dim)),
            ]),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(THEME.border));

        let para = Paragraph::new(content).block(block);
        frame.render_widget(para, area);
    }
}

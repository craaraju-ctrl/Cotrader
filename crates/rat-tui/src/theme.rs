//! Theme system — Consistent color palette for the trading terminal.

use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub brand: Color,
    pub positive: Color,
    pub negative: Color,
    pub neutral: Color,
    pub highlight: Color,
    pub muted: Color,
    pub surface: Color,
    pub border: Color,
    pub border_focus: Color,
    pub text: Color,
    pub text_dim: Color,
    pub warning: Color,
    pub info: Color,
    pub accent: Color,
}

pub const THEME: Theme = Theme {
    brand: Color::Rgb(252, 213, 53),      // Trading yellow
    positive: Color::Rgb(0, 200, 83),      // Green
    negative: Color::Rgb(255, 82, 82),     // Red
    neutral: Color::Rgb(158, 158, 158),    // Gray
    highlight: Color::Rgb(100, 181, 246),  // Blue
    muted: Color::Rgb(117, 117, 117),      // Dim gray
    surface: Color::Rgb(30, 30, 46),       // Dark surface
    border: Color::Rgb(69, 69, 85),        // Border gray
    border_focus: Color::Rgb(252, 213, 53), // Brand for focus
    text: Color::Rgb(205, 214, 244),       // Light text
    text_dim: Color::Rgb(147, 153, 178),   // Dim text
    warning: Color::Rgb(255, 183, 77),     // Orange
    info: Color::Rgb(129, 212, 250),       // Light blue
    accent: Color::Rgb(186, 104, 200),     // Purple
};

impl Theme {
    pub fn positive_style(&self) -> Style {
        Style::default().fg(self.positive)
    }

    pub fn negative_style(&self) -> Style {
        Style::default().fg(self.negative)
    }

    pub fn brand_style(&self) -> Style {
        Style::default().fg(self.brand).add_modifier(Modifier::BOLD)
    }

    pub fn dim_style(&self) -> Style {
        Style::default().fg(self.text_dim)
    }

    pub fn highlight_style(&self) -> Style {
        Style::default().fg(self.highlight).add_modifier(Modifier::BOLD)
    }

    pub fn muted_style(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    pub fn accent_style(&self) -> Style {
        Style::default().fg(self.accent)
    }
}

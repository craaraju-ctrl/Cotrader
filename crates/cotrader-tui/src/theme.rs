//! Theme system — Consistent color palette for the trading terminal.
#![allow(dead_code)]

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

/// Dark theme (default) — Catppuccin-inspired
pub const DARK_THEME: Theme = Theme {
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

/// Light theme — for bright terminals
pub const LIGHT_THEME: Theme = Theme {
    brand: Color::Rgb(180, 130, 0),       // Dark yellow
    positive: Color::Rgb(0, 140, 60),      // Dark green
    negative: Color::Rgb(200, 40, 40),     // Dark red
    neutral: Color::Rgb(120, 120, 120),    // Gray
    highlight: Color::Rgb(30, 100, 200),   // Dark blue
    muted: Color::Rgb(160, 160, 160),      // Light gray
    surface: Color::Rgb(245, 245, 250),    // Light surface
    border: Color::Rgb(200, 200, 210),     // Border gray
    border_focus: Color::Rgb(180, 130, 0), // Brand for focus
    text: Color::Rgb(30, 30, 40),          // Dark text
    text_dim: Color::Rgb(120, 120, 140),   // Dim text
    warning: Color::Rgb(200, 120, 0),      // Dark orange
    info: Color::Rgb(0, 100, 180),         // Dark blue
    accent: Color::Rgb(140, 60, 160),      // Dark purple
};

/// Monochrome theme — high contrast for accessibility
pub const MONO_THEME: Theme = Theme {
    brand: Color::White,
    positive: Color::White,
    negative: Color::White,
    neutral: Color::Gray,
    highlight: Color::White,
    muted: Color::DarkGray,
    surface: Color::Black,
    border: Color::Gray,
    border_focus: Color::White,
    text: Color::White,
    text_dim: Color::Gray,
    warning: Color::White,
    info: Color::White,
    accent: Color::White,
};

/// Currently active theme — change this to switch themes
pub const THEME: Theme = DARK_THEME;

/// Get theme by type index (0=Dark, 1=Light, 2=Mono)
pub fn theme_by_index(index: usize) -> &'static Theme {
    match index {
        0 => &DARK_THEME,
        1 => &LIGHT_THEME,
        2 => &MONO_THEME,
        _ => &DARK_THEME,
    }
}

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

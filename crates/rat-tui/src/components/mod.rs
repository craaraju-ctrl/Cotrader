//! Component architecture — Each UI element is a self-contained component.

pub mod header;
pub mod tabs;
pub mod dashboard;
pub mod positions;
pub mod orderbook;
pub mod agents;
pub mod performance;
pub mod health;
pub mod settings;
pub mod help;
pub mod footer;
pub mod status_bar;

use ratatui::layout::Rect;
use ratatui::Frame;
use crossterm::event::{KeyEvent, MouseEvent};

use crate::app::App;

/// Component trait — All UI elements implement this.
pub trait Component {
    /// Handle keyboard events. Return true if consumed.
    fn handle_key(&mut self, key: KeyEvent, app: &mut App) -> bool {
        let _ = (key, app);
        false
    }

    /// Handle mouse events. Return true if consumed.
    #[allow(dead_code)]
    fn handle_mouse(&mut self, mouse: MouseEvent, app: &mut App) -> bool {
        let _ = (mouse, app);
        false
    }

    /// Update component state (called each tick).
    fn update(&mut self, app: &mut App) {
        let _ = app;
    }

    /// Render the component.
    fn render(&self, frame: &mut Frame, area: Rect, app: &App);
}

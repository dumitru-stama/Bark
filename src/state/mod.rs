pub mod mode;
pub mod panel;
pub mod app;
pub mod ui_state;
pub mod command_state;
pub mod background;

pub use ui_state::UiState;
pub use command_state::CommandState;

/// Which panel is currently active
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
}
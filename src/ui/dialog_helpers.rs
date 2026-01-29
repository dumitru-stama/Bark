//! Dialog rendering helper utilities.
//!
//! Provides common operations for dialog widget rendering to eliminate
//! duplicate code across dialog implementations.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};

use super::Theme;

/// Pre-computed styles for dialog rendering.
pub struct DialogStyles {
    pub border: Style,
    pub title: Style,
    pub label: Style,
    pub input_focused: Style,
    pub input_selected: Style,  // When text is selected (highlighted)
    pub input_unfocused: Style,
    pub button_focused: Style,
    pub button_unfocused: Style,
    pub help: Style,
    pub bg: Style,
}

impl DialogStyles {
    /// Create dialog styles from theme with given background color.
    pub fn new(theme: &Theme, bg_color: Color, border_color: Color) -> Self {
        Self {
            border: Style::default().fg(border_color),
            title: Style::default().bg(bg_color).fg(theme.dialog_title).add_modifier(Modifier::BOLD),
            label: Style::default().bg(bg_color).fg(theme.dialog_text),
            input_focused: Style::default().bg(theme.dialog_input_focused_bg).fg(theme.dialog_input_focused_fg),
            input_selected: Style::default().bg(theme.dialog_input_selected_bg).fg(theme.dialog_input_selected_fg),
            input_unfocused: Style::default().bg(bg_color).fg(theme.dialog_input_unfocused_fg),
            button_focused: Style::default().fg(theme.dialog_button_focused_fg).bg(theme.dialog_button_focused_bg).add_modifier(Modifier::BOLD),
            button_unfocused: Style::default().fg(theme.dialog_button_unfocused).bg(bg_color),
            help: Style::default().bg(bg_color).fg(theme.dialog_help),
            bg: Style::default().bg(bg_color),
        }
    }
}

/// Helper functions for dialog rendering.
pub struct DialogRenderer;

impl DialogRenderer {
    /// Calculate centered dialog position and return the dialog area.
    /// Returns None if the area is too small.
    pub fn center_dialog(area: Rect, width: u16, height: u16, min_width: u16) -> Option<Rect> {
        if area.width < min_width || area.height < height {
            return None;
        }

        let dialog_width = width.min(area.width.saturating_sub(4));
        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;

        Some(Rect {
            x,
            y,
            width: dialog_width,
            height,
        })
    }

    /// Fill dialog area with background color.
    pub fn fill_background(area: Rect, buf: &mut Buffer, style: Style) {
        for row in area.y..area.y + area.height {
            for col in area.x..area.x + area.width {
                buf[(col, row)].set_char(' ').set_style(style);
            }
        }
    }

    /// Draw dialog border using box-drawing characters.
    pub fn draw_border(area: Rect, buf: &mut Buffer, style: Style) {
        // Top border
        buf[(area.x, area.y)].set_char('┌').set_style(style);
        buf[(area.x + area.width - 1, area.y)].set_char('┐').set_style(style);
        for col in area.x + 1..area.x + area.width - 1 {
            buf[(col, area.y)].set_char('─').set_style(style);
        }

        // Bottom border
        buf[(area.x, area.y + area.height - 1)].set_char('└').set_style(style);
        buf[(area.x + area.width - 1, area.y + area.height - 1)].set_char('┘').set_style(style);
        for col in area.x + 1..area.x + area.width - 1 {
            buf[(col, area.y + area.height - 1)].set_char('─').set_style(style);
        }

        // Side borders
        for row in area.y + 1..area.y + area.height - 1 {
            buf[(area.x, row)].set_char('│').set_style(style);
            buf[(area.x + area.width - 1, row)].set_char('│').set_style(style);
        }
    }

    /// Draw centered title on the top border.
    pub fn draw_title(area: Rect, buf: &mut Buffer, title: &str, style: Style) {
        let title_x = area.x + (area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, area.y, title, style);
    }

    /// Draw a horizontal row of buttons, centered.
    /// Returns the x positions of each button for focus highlighting.
    pub fn draw_buttons(
        area: Rect,
        buf: &mut Buffer,
        y_offset: u16,
        buttons: &[(&str, bool)], // (text, is_focused)
        focused_style: Style,
        unfocused_style: Style,
    ) {
        let button_y = area.y + y_offset;

        // Calculate total width of all buttons with spacing
        let total_width: usize = buttons.iter()
            .map(|(text, _)| text.len())
            .sum::<usize>() + (buttons.len().saturating_sub(1)) * 2;

        let mut x = area.x + (area.width.saturating_sub(total_width as u16)) / 2;

        for (text, is_focused) in buttons {
            let style = if *is_focused { focused_style } else { unfocused_style };
            buf.set_string(x, button_y, text, style);
            x += text.len() as u16 + 2;
        }
    }

    /// Draw an input field with optional focus highlighting.
    /// Handles text truncation for long inputs.
    pub fn draw_input_field(
        buf: &mut Buffer,
        x: u16,
        y: u16,
        width: usize,
        text: &str,
        style: Style,
    ) {
        // Clear input area
        for col in x..x + width as u16 {
            buf[(col, y)].set_char(' ').set_style(style);
        }

        // Truncate from start if too long
        let max_display = width.saturating_sub(1);
        let display_text = if text.len() > max_display {
            let skip = text.len() - max_display;
            format!("…{}", &text[skip + 1..])
        } else {
            text.to_string()
        };

        buf.set_string(x, y, &display_text, style);
    }

    /// Draw help text centered at the bottom of dialog.
    pub fn draw_help(area: Rect, buf: &mut Buffer, text: &str, style: Style) {
        let help_x = area.x + (area.width.saturating_sub(text.len() as u16)) / 2;
        buf.set_string(help_x, area.y + area.height - 2, text, style);
    }

    /// Draw a checkbox with label.
    #[allow(dead_code, clippy::too_many_arguments)]
    pub fn draw_checkbox(
        buf: &mut Buffer,
        x: u16,
        y: u16,
        label: &str,
        checked: bool,
        focused: bool,
        focused_style: Style,
        unfocused_style: Style,
    ) {
        let checkbox = if checked { "[x]" } else { "[ ]" };
        let style = if focused { focused_style } else { unfocused_style };
        buf.set_string(x, y, format!("{} {}", checkbox, label), style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_center_dialog() {
        let area = Rect { x: 0, y: 0, width: 80, height: 24 };
        let result = DialogRenderer::center_dialog(area, 40, 10, 20);
        assert!(result.is_some());
        let dialog = result.unwrap();
        assert_eq!(dialog.width, 40);
        assert_eq!(dialog.height, 10);
        assert_eq!(dialog.x, 20); // (80 - 40) / 2
        assert_eq!(dialog.y, 7);  // (24 - 10) / 2
    }

    #[test]
    fn test_center_dialog_too_small() {
        let area = Rect { x: 0, y: 0, width: 15, height: 24 };
        let result = DialogRenderer::center_dialog(area, 40, 10, 20);
        assert!(result.is_none());
    }
}

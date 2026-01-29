//! Viewer plugin menu widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use super::Theme;

/// Viewer plugin selection menu - shown as overlay
pub struct ViewerPluginMenu<'a> {
    /// List of plugins (name, can_handle)
    plugins: &'a [(String, bool)],
    /// Currently selected index (0 = built-in viewer)
    selected: usize,
    /// Theme
    theme: &'a Theme,
}

impl<'a> ViewerPluginMenu<'a> {
    pub fn new(plugins: &'a [(String, bool)], selected: usize, theme: &'a Theme) -> Self {
        Self { plugins, selected, theme }
    }
}

impl Widget for ViewerPluginMenu<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate menu dimensions
        let item_count = 1 + self.plugins.len(); // built-in + plugins
        let menu_height = (item_count + 4).min(area.height as usize) as u16; // +4 for border and title
        let menu_width = 40.min(area.width.saturating_sub(4));

        // Center the menu
        let menu_x = area.x + (area.width.saturating_sub(menu_width)) / 2;
        let menu_y = area.y + (area.height.saturating_sub(menu_height)) / 2;

        let menu_area = Rect {
            x: menu_x,
            y: menu_y,
            width: menu_width,
            height: menu_height,
        };

        // Styles - use copy dialog colors
        let border_style = Style::default().fg(self.theme.dialog_copy_border);
        let title_style = Style::default().fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let normal_style = Style::default().fg(self.theme.dialog_text).bg(self.theme.dialog_copy_bg);
        let selected_style = Style::default()
            .fg(self.theme.dialog_button_focused_fg)
            .bg(self.theme.dialog_button_focused_bg)
            .add_modifier(Modifier::BOLD);
        let disabled_style = Style::default().fg(self.theme.viewer_line_number).bg(self.theme.dialog_copy_bg);

        // Clear menu area with background
        for y in menu_area.y..menu_area.y + menu_area.height {
            for x in menu_area.x..menu_area.x + menu_area.width {
                buf[(x, y)].set_char(' ').set_style(normal_style);
            }
        }

        // Draw border
        // Top border
        buf[(menu_area.x, menu_area.y)].set_char('┌').set_style(border_style);
        buf[(menu_area.x + menu_area.width - 1, menu_area.y)].set_char('┐').set_style(border_style);
        for x in menu_area.x + 1..menu_area.x + menu_area.width - 1 {
            buf[(x, menu_area.y)].set_char('─').set_style(border_style);
        }

        // Bottom border
        buf[(menu_area.x, menu_area.y + menu_area.height - 1)].set_char('└').set_style(border_style);
        buf[(menu_area.x + menu_area.width - 1, menu_area.y + menu_area.height - 1)].set_char('┘').set_style(border_style);
        for x in menu_area.x + 1..menu_area.x + menu_area.width - 1 {
            buf[(x, menu_area.y + menu_area.height - 1)].set_char('─').set_style(border_style);
        }

        // Side borders
        for y in menu_area.y + 1..menu_area.y + menu_area.height - 1 {
            buf[(menu_area.x, y)].set_char('│').set_style(border_style);
            buf[(menu_area.x + menu_area.width - 1, y)].set_char('│').set_style(border_style);
        }

        // Title
        let title = " Select Viewer ";
        let title_x = menu_area.x + (menu_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, menu_area.y, title, title_style);

        // Menu items
        let content_x = menu_area.x + 2;
        let content_width = menu_area.width.saturating_sub(4) as usize;
        let mut y = menu_area.y + 2;

        // Built-in viewer (always first)
        let item_style = if self.selected == 0 { selected_style } else { normal_style };
        let prefix = if self.selected == 0 { "► " } else { "  " };
        let text = format!("{}{}", prefix, "Built-in Viewer");
        let display: String = text.chars().take(content_width).collect();

        // Clear the line first
        for x in menu_area.x + 1..menu_area.x + menu_area.width - 1 {
            buf[(x, y)].set_char(' ').set_style(item_style);
        }
        buf.set_string(content_x, y, &display, item_style);
        y += 1;

        // Plugin items
        for (i, (name, can_handle)) in self.plugins.iter().enumerate() {
            if y >= menu_area.y + menu_area.height - 1 {
                break;
            }

            let idx = i + 1;
            let is_selected = self.selected == idx;
            let item_style = if is_selected {
                selected_style
            } else if *can_handle {
                normal_style
            } else {
                disabled_style
            };

            let prefix = if is_selected { "► " } else { "  " };
            let suffix = if !*can_handle { " (N/A)" } else { "" };
            let text = format!("{}{}{}", prefix, name, suffix);
            let display: String = text.chars().take(content_width).collect();

            // Clear the line first
            for x in menu_area.x + 1..menu_area.x + menu_area.width - 1 {
                buf[(x, y)].set_char(' ').set_style(item_style);
            }
            buf.set_string(content_x, y, &display, item_style);
            y += 1;
        }

        // Help text at bottom
        let help = "Enter:Select  Esc:Cancel";
        let help_y = menu_area.y + menu_area.height - 1;
        let help_x = menu_area.x + (menu_area.width.saturating_sub(help.len() as u16)) / 2;
        buf.set_string(help_x, help_y, help, border_style);
    }
}

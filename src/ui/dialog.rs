//! Confirmation dialog widget

use std::path::PathBuf;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

use crate::state::mode::FileOperation;
use super::Theme;

/// Confirmation dialog for file operations
pub struct ConfirmDialog<'a> {
    operation: &'a FileOperation,
    sources: &'a [PathBuf],
    dest_input: &'a str,
    focus: usize,
    input_selected: bool,
    apply_all: bool,
    theme: &'a Theme,
}

impl<'a> ConfirmDialog<'a> {
    pub fn new(
        operation: &'a FileOperation,
        sources: &'a [PathBuf],
        dest_input: &'a str,
        _cursor_pos: usize,
        focus: usize,
        input_selected: bool,
        apply_all: bool,
        theme: &'a Theme,
    ) -> Self {
        Self {
            operation,
            sources,
            dest_input,
            focus,
            input_selected,
            apply_all,
            theme,
        }
    }
}

impl Widget for ConfirmDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let is_delete = matches!(self.operation, FileOperation::Delete);
        // Show checkbox when deleting a single directory
        let show_checkbox = is_delete
            && self.sources.len() == 1
            && self.sources[0].is_dir();

        // Dialog dimensions (smaller for delete, taller with checkbox)
        let dialog_width = 60.min(area.width.saturating_sub(4));
        let dialog_height = if is_delete {
            if show_checkbox { 10 } else { 8 }
        } else {
            10
        };

        if area.width < 20 || area.height < dialog_height {
            return;
        }

        // Center the dialog
        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect {
            x,
            y,
            width: dialog_width,
            height: dialog_height,
        };

        // Background and border colors based on operation
        let (bg_color, border_color) = match self.operation {
            FileOperation::Copy => (self.theme.dialog_copy_bg, self.theme.dialog_copy_border),
            FileOperation::Move => (self.theme.dialog_move_bg, self.theme.dialog_move_border),
            FileOperation::Delete => (self.theme.dialog_delete_bg, self.theme.dialog_delete_border),
        };

        // Styles
        let border_style = Style::default().fg(border_color);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let label_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let input_style_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let input_style_selected = Style::default().bg(self.theme.dialog_input_selected_bg).fg(self.theme.dialog_input_selected_fg);
        let input_style_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_input_unfocused_fg);
        let button_style_focused = Style::default().fg(self.theme.dialog_button_focused_fg).bg(self.theme.dialog_button_focused_bg).add_modifier(Modifier::BOLD);
        let button_style_unfocused = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);
        let delete_button_focused = Style::default().fg(self.theme.dialog_delete_button_focused_fg).bg(self.theme.dialog_delete_button_focused_bg).add_modifier(Modifier::BOLD);
        let delete_button_unfocused = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);
        let warning_style = Style::default().bg(bg_color).fg(self.theme.dialog_warning).add_modifier(Modifier::BOLD);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help);

        // Draw background (clear area)
        let bg_style = Style::default().bg(bg_color);
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Draw border
        // Top border
        buf[(dialog_area.x, dialog_area.y)].set_char('┌').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('┐').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }

        // Bottom border
        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('└').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('┘').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }

        // Side borders
        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = match self.operation {
            FileOperation::Copy => " Copy ",
            FileOperation::Move => " Move ",
            FileOperation::Delete => " Delete ",
        };
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        // Content area
        let content_x = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;

        if is_delete {
            // Delete dialog - simpler layout
            // Warning message (line 2)
            let warning = "Are you sure you want to delete:";
            buf.set_string(content_x, dialog_area.y + 2, warning, warning_style);

            // File info (line 4)
            let file_info = if self.sources.len() == 1 {
                format!("\"{}\"", self.sources[0].file_name().unwrap_or_default().to_string_lossy())
            } else {
                format!("{} files", self.sources.len())
            };
            let truncated_info: String = file_info.chars().take(content_width).collect();
            buf.set_string(content_x, dialog_area.y + 4, &truncated_info, label_style);

            // Checkbox row (only for single directory) and buttons shift down
            let button_offset = if show_checkbox { 2 } else { 0 };

            if show_checkbox {
                // Checkbox (focus 1 when shown)
                let checkbox_y = dialog_area.y + 6;
                let check_char = if self.apply_all { 'x' } else { ' ' };
                let checkbox_text = format!("[{}] Apply for all", check_char);
                let checkbox_style = if self.focus == 1 { button_style_focused } else { label_style };
                buf.set_string(content_x, checkbox_y, &checkbox_text, checkbox_style);
            }

            // Buttons — when checkbox is shown: Delete=focus 2, Cancel=focus 3
            //           otherwise:              Delete=focus 1, Cancel=focus 2
            let button_y = dialog_area.y + 6 + button_offset;
            let ok_text = "[ Delete ]";
            let cancel_text = "[ Cancel ]";

            let total_button_width = ok_text.len() + 4 + cancel_text.len();
            let button_start_x = dialog_area.x + (dialog_area.width.saturating_sub(total_button_width as u16)) / 2;

            let (del_focus, cancel_focus) = if show_checkbox { (2, 3) } else { (1, 2) };
            let delete_style = if self.focus == del_focus { delete_button_focused } else { delete_button_unfocused };
            let cancel_style = if self.focus == cancel_focus { button_style_focused } else { button_style_unfocused };

            buf.set_string(button_start_x, button_y, ok_text, delete_style);
            buf.set_string(button_start_x + ok_text.len() as u16 + 4, button_y, cancel_text, cancel_style);
        } else {
            // Copy/Move dialog with destination input
            // Source info (line 2)
            let source_label = if self.sources.len() == 1 {
                format!("Copy \"{}\" to:", self.sources[0].file_name().unwrap_or_default().to_string_lossy())
            } else {
                format!("Copy {} files to:", self.sources.len())
            };
            let source_label = match self.operation {
                FileOperation::Copy => source_label,
                FileOperation::Move => source_label.replace("Copy", "Move"),
                FileOperation::Delete => unreachable!(),
            };
            let truncated_label: String = source_label.chars().take(content_width).collect();
            buf.set_string(content_x, dialog_area.y + 2, &truncated_label, label_style);

            // Destination input field (line 4)
            let input_y = dialog_area.y + 4;
            let input_style = if self.focus == 0 {
                if self.input_selected { input_style_selected } else { input_style_focused }
            } else {
                input_style_unfocused
            };
            // Clear input line with input style
            for col in content_x..content_x + content_width as u16 {
                buf[(col, input_y)].set_char(' ').set_style(input_style);
            }

            // Show destination path (truncate from start if too long)
            let max_input_display = content_width.saturating_sub(1);
            let display_input = if self.dest_input.len() > max_input_display {
                let skip = self.dest_input.len() - max_input_display;
                format!("…{}", &self.dest_input[skip + 1..])
            } else {
                self.dest_input.to_string()
            };
            buf.set_string(content_x, input_y, &display_input, input_style);

            // Buttons (line 7)
            let button_y = dialog_area.y + 7;
            let ok_text = "[ OK ]";
            let cancel_text = "[ Cancel ]";

            // Button styles based on focus
            let ok_style = if self.focus == 1 { button_style_focused } else { button_style_unfocused };
            let cancel_style = if self.focus == 2 { button_style_focused } else { button_style_unfocused };

            // Center buttons
            let total_button_width = ok_text.len() + 4 + cancel_text.len();
            let button_start_x = dialog_area.x + (dialog_area.width.saturating_sub(total_button_width as u16)) / 2;

            buf.set_string(button_start_x, button_y, ok_text, ok_style);
            buf.set_string(button_start_x + ok_text.len() as u16 + 4, button_y, cancel_text, cancel_style);

            // Help text (line 8)
            let help_text = "Tab=Switch  Enter=Select  Esc=Cancel";
            let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help_text.len() as u16)) / 2;
            buf.set_string(help_x, dialog_area.y + 8, help_text, help_style);
        }
    }
}

/// Iterative delete confirmation dialog (per-item in a directory)
pub struct DeleteIterativeDialog<'a> {
    items: &'a [PathBuf],
    current: usize,
    apply_all: bool,
    focus: usize,
    theme: &'a Theme,
}

impl<'a> DeleteIterativeDialog<'a> {
    pub fn new(
        items: &'a [PathBuf],
        current: usize,
        apply_all: bool,
        focus: usize,
        theme: &'a Theme,
    ) -> Self {
        Self { items, current, apply_all, focus, theme }
    }
}

impl Widget for DeleteIterativeDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog_width = 60.min(area.width.saturating_sub(4));
        let dialog_height: u16 = 10;

        if area.width < 20 || area.height < dialog_height {
            return;
        }

        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect { x, y, width: dialog_width, height: dialog_height };

        let bg_color = self.theme.dialog_delete_bg;
        let border_color = self.theme.dialog_delete_border;

        let border_style = Style::default().fg(border_color);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let label_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let warning_style = Style::default().bg(bg_color).fg(self.theme.dialog_warning).add_modifier(Modifier::BOLD);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help);
        let button_style_focused = Style::default().fg(self.theme.dialog_button_focused_fg).bg(self.theme.dialog_button_focused_bg).add_modifier(Modifier::BOLD);
        let button_style_unfocused = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);
        let delete_button_focused = Style::default().fg(self.theme.dialog_delete_button_focused_fg).bg(self.theme.dialog_delete_button_focused_bg).add_modifier(Modifier::BOLD);
        let delete_button_unfocused = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);

        // Draw background
        let bg_style = Style::default().bg(bg_color);
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Draw border
        buf[(dialog_area.x, dialog_area.y)].set_char('┌').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('┐').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }
        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('└').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('┘').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }
        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = " Delete ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        let content_x = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;

        // Item name and progress counter (line 2)
        let item = &self.items[self.current];
        let name = item.file_name().unwrap_or_default().to_string_lossy();
        let is_dir = item.is_dir();
        let type_indicator = if is_dir { "/" } else { "" };
        let progress = format!("({}/{})", self.current + 1, self.items.len());

        let delete_label = format!("Delete \"{}{}\"?", name, type_indicator);
        let max_label_width = content_width.saturating_sub(progress.len() + 1);
        let truncated_label: String = delete_label.chars().take(max_label_width).collect();
        buf.set_string(content_x, dialog_area.y + 2, &truncated_label, warning_style);

        // Progress counter (right-aligned on line 2)
        let progress_x = dialog_area.x + dialog_area.width - 2 - progress.len() as u16;
        buf.set_string(progress_x, dialog_area.y + 2, &progress, label_style);

        // Checkbox (line 4) — focus 0
        let check_char = if self.apply_all { 'x' } else { ' ' };
        let checkbox_text = format!("[{}] Apply for all", check_char);
        let checkbox_style = if self.focus == 0 { button_style_focused } else { label_style };
        buf.set_string(content_x, dialog_area.y + 4, &checkbox_text, checkbox_style);

        // Buttons (line 6): Delete=focus 1, Skip=focus 2, Cancel=focus 3
        let button_y = dialog_area.y + 6;
        let del_text = "[ Delete ]";
        let skip_text = "[ Skip ]";
        let cancel_text = "[ Cancel ]";

        let total_button_width = del_text.len() + 3 + skip_text.len() + 3 + cancel_text.len();
        let button_start_x = dialog_area.x + (dialog_area.width.saturating_sub(total_button_width as u16)) / 2;

        let del_style = if self.focus == 1 { delete_button_focused } else { delete_button_unfocused };
        let skip_style = if self.focus == 2 { button_style_focused } else { button_style_unfocused };
        let cancel_style = if self.focus == 3 { button_style_focused } else { button_style_unfocused };

        let mut bx = button_start_x;
        buf.set_string(bx, button_y, del_text, del_style);
        bx += del_text.len() as u16 + 3;
        buf.set_string(bx, button_y, skip_text, skip_style);
        bx += skip_text.len() as u16 + 3;
        buf.set_string(bx, button_y, cancel_text, cancel_style);

        // Help text (line 8)
        let help_text = "Tab=Switch  Enter=Select  Esc=Cancel";
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help_text.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + 8, help_text, help_style);
    }
}

/// Source selector dialog (drives, quick access paths, and remote connections)
pub struct SourceSelector<'a> {
    sources: &'a [crate::providers::PanelSource],
    selected: usize,
    target_panel: &'a crate::state::Side,
    theme: &'a Theme,
}

impl<'a> SourceSelector<'a> {
    pub fn new(
        sources: &'a [crate::providers::PanelSource],
        selected: usize,
        target_panel: &'a crate::state::Side,
        theme: &'a Theme,
    ) -> Self {
        Self {
            sources,
            selected,
            target_panel,
            theme,
        }
    }
}

impl Widget for SourceSelector<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.sources.is_empty() {
            return;
        }

        // Dialog dimensions - wider to accommodate longer names
        let max_name_len = self.sources.iter()
            .map(|s| s.display_name().chars().count())
            .max()
            .unwrap_or(10);
        let dialog_width = ((max_name_len + 6) as u16).clamp(24, 40).min(area.width.saturating_sub(4));
        // Height: 2 (border) + sources count (max 12 visible) + 1 (title line) + 1 (help line)
        let visible_sources = self.sources.len().min(12);
        let dialog_height = (visible_sources + 4) as u16;

        if area.width < 15 || area.height < dialog_height {
            return;
        }

        // Center the dialog
        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect {
            x,
            y,
            width: dialog_width,
            height: dialog_height,
        };

        let bg_color = self.theme.dialog_copy_bg;
        let border_color = self.theme.dialog_copy_border;

        let border_style = Style::default().fg(border_color);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let item_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let selected_style = Style::default().bg(self.theme.cursor_bg).fg(self.theme.cursor_fg).add_modifier(Modifier::BOLD);
        let new_conn_style = Style::default().bg(bg_color).fg(Color::Green);

        // Draw background
        let bg_style = Style::default().bg(bg_color);
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Draw border
        buf[(dialog_area.x, dialog_area.y)].set_char('┌').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('┐').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }

        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('└').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('┘').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }

        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = match self.target_panel {
            crate::state::Side::Left => " Left Panel ",
            crate::state::Side::Right => " Right Panel ",
        };
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        // Calculate scroll offset if more than visible_sources
        let scroll_offset = if self.sources.len() > visible_sources {
            if self.selected < visible_sources / 2 {
                0
            } else if self.selected >= self.sources.len() - visible_sources / 2 {
                self.sources.len() - visible_sources
            } else {
                self.selected - visible_sources / 2
            }
        } else {
            0
        };

        // Draw sources list
        for (i, source) in self.sources.iter().skip(scroll_offset).take(visible_sources).enumerate() {
            let source_idx = scroll_offset + i;
            let row_y = dialog_area.y + 1 + i as u16;

            let is_new_connection = matches!(source, crate::providers::PanelSource::NewConnection { .. } | crate::providers::PanelSource::NewPluginConnection { .. });

            let style = if source_idx == self.selected {
                selected_style
            } else if is_new_connection {
                new_conn_style
            } else {
                item_style
            };

            // Clear the line with the appropriate style
            for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
                buf[(col, row_y)].set_char(' ').set_style(style);
            }

            // Format source display
            let display_name = source.display_name();
            let available_width = (dialog_area.width - 4) as usize;
            let truncated_name = if display_name.chars().count() > available_width {
                let mut s: String = display_name.chars().take(available_width - 1).collect();
                s.push('…');
                s
            } else {
                display_name
            };

            let source_display = format!(" {} ", truncated_name);
            buf.set_string(dialog_area.x + 2, row_y, &source_display, style);
        }

        // Help text at bottom - show F4/F8 if a Provider connection is selected
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help);
        let selected_source = self.sources.get(self.selected);
        let is_provider = matches!(selected_source, Some(crate::providers::PanelSource::Provider { .. }));
        let help_text = if is_provider {
            "Enter:Connect F4:Edit F8:Del"
        } else {
            "Enter to select, Esc to cancel"
        };
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help_text.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + dialog_height - 1, help_text, help_style);
    }
}

/// Mkdir dialog for creating a new directory
pub struct MkdirDialog<'a> {
    name_input: &'a str,
    _cursor_pos: usize,
    focus: usize,
    input_selected: bool,
    theme: &'a Theme,
}

impl<'a> MkdirDialog<'a> {
    pub fn new(
        name_input: &'a str,
        cursor_pos: usize,
        focus: usize,
        input_selected: bool,
        theme: &'a Theme,
    ) -> Self {
        Self {
            name_input,
            _cursor_pos: cursor_pos,
            focus,
            input_selected,
            theme,
        }
    }
}

impl Widget for MkdirDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use super::dialog_helpers::{DialogRenderer, DialogStyles};

        // Center dialog
        let Some(dialog_area) = DialogRenderer::center_dialog(area, 50, 9, 20) else {
            return;
        };

        // Setup styles
        let bg_color = self.theme.dialog_mkdir_bg;
        let border_color = self.theme.dialog_mkdir_border;
        let styles = DialogStyles::new(self.theme, bg_color, border_color);

        // Draw dialog frame
        DialogRenderer::fill_background(dialog_area, buf, styles.bg);
        DialogRenderer::draw_border(dialog_area, buf, styles.border);
        DialogRenderer::draw_title(dialog_area, buf, " Create Directory ", styles.title);

        // Content area
        let content_x = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;

        // Label
        buf.set_string(content_x, dialog_area.y + 2, "Enter directory name:", styles.label);

        // Input field
        let input_style = if self.focus == 0 {
            if self.input_selected { styles.input_selected } else { styles.input_focused }
        } else {
            styles.input_unfocused
        };
        DialogRenderer::draw_input_field(buf, content_x, dialog_area.y + 4, content_width, self.name_input, input_style);

        // Buttons
        DialogRenderer::draw_buttons(
            dialog_area, buf, 6,
            &[("[ OK ]", self.focus == 1), ("[ Cancel ]", self.focus == 2)],
            styles.button_focused, styles.button_unfocused,
        );

        // Help text
        DialogRenderer::draw_help(dialog_area, buf, "Tab=Switch  Enter=Select  Esc=Cancel", styles.help);
    }
}

/// Calculate cursor position for the mkdir dialog input field
pub fn mkdir_cursor_position(area: Rect, name_input: &str, cursor_pos: usize) -> (u16, u16) {
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 9;

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let content_x = x + 2;
    let content_width = dialog_width.saturating_sub(4) as usize;
    let input_y = y + 4;

    // Calculate visible cursor position
    let max_input_display = content_width.saturating_sub(1);
    let cursor_x = if name_input.len() > max_input_display {
        content_x + max_input_display as u16
    } else {
        content_x + cursor_pos.min(name_input.len()) as u16
    };

    (cursor_x, input_y)
}

/// Calculate cursor position for the dialog input field
pub fn dialog_cursor_position(area: Rect, dest_input: &str, cursor_pos: usize) -> (u16, u16) {
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 10;

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let content_x = x + 2;
    let content_width = dialog_width.saturating_sub(4) as usize;
    let input_y = y + 4;

    // Calculate visible cursor position
    let max_input_display = content_width.saturating_sub(1);
    let cursor_x = if dest_input.len() > max_input_display {
        // Input is truncated, cursor is at end
        content_x + max_input_display as u16
    } else {
        content_x + cursor_pos.min(dest_input.len()) as u16
    };

    (cursor_x, input_y)
}

/// Command history dialog (Alt+H)
pub struct CommandHistoryDialog<'a> {
    history: &'a [String],
    selected: usize,
    scroll: usize,
    theme: &'a Theme,
}

impl<'a> CommandHistoryDialog<'a> {
    pub fn new(
        history: &'a [String],
        selected: usize,
        scroll: usize,
        theme: &'a Theme,
    ) -> Self {
        Self {
            history,
            selected,
            scroll,
            theme,
        }
    }
}

impl Widget for CommandHistoryDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Dialog is 6 less in each dimension than the terminal
        let dialog_width = area.width.saturating_sub(6);
        let dialog_height = area.height.saturating_sub(6);

        if dialog_width < 20 || dialog_height < 5 {
            return;
        }

        // Center the dialog
        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect {
            x,
            y,
            width: dialog_width,
            height: dialog_height,
        };

        // Use theme colors (reuse copy dialog colors for history)
        let bg_color = self.theme.dialog_copy_bg;
        let border_color = self.theme.dialog_copy_border;
        let text_color = self.theme.dialog_text;

        // Styles - explicitly remove modifiers to avoid bold remnants from underlying buffer
        let border_style = Style::default().fg(border_color).bg(bg_color).remove_modifier(Modifier::BOLD);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).remove_modifier(Modifier::BOLD);
        let item_style = Style::default().bg(bg_color).fg(text_color).remove_modifier(Modifier::BOLD);
        let selected_style = Style::default().bg(self.theme.cursor_bg).fg(self.theme.cursor_fg).remove_modifier(Modifier::BOLD);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help).remove_modifier(Modifier::BOLD);

        // Draw background (reset all modifiers to clear bold remnants from underlying buffer)
        let bg_style = Style::default().bg(bg_color).remove_modifier(Modifier::all());
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Draw border
        buf[(dialog_area.x, dialog_area.y)].set_char('┌').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('┐').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }

        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('└').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('┘').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }

        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = " Command History ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        // Content area (leave room for border and help text)
        let content_x = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;
        let content_height = dialog_area.height.saturating_sub(4) as usize; // -2 for border, -2 for help

        // Draw history items
        if self.history.is_empty() {
            let empty_msg = "(No commands yet)";
            let msg_x = dialog_area.x + (dialog_area.width.saturating_sub(empty_msg.len() as u16)) / 2;
            buf.set_string(msg_x, dialog_area.y + 2, empty_msg, help_style);
        }

        let visible_count = content_height.min(self.history.len());

        for i in 0..visible_count {
            let history_idx = self.scroll + i;
            if history_idx >= self.history.len() {
                break;
            }

            let cmd = &self.history[history_idx];
            let row_y = dialog_area.y + 2 + i as u16;

            // Choose style based on selection
            let style = if history_idx == self.selected {
                selected_style
            } else {
                item_style
            };

            // Clear the line with the style
            for col in content_x..content_x + content_width as u16 {
                buf[(col, row_y)].set_char(' ').set_style(style);
            }

            // Truncate command if too long
            let display_cmd = if cmd.len() > content_width {
                format!("{}…", &cmd[..content_width - 1])
            } else {
                cmd.clone()
            };

            buf.set_string(content_x, row_y, &display_cmd, style);
        }

        // Help text at bottom
        let help_text = "↑↓=Navigate  Enter=Execute  Esc=Cancel";
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help_text.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + dialog_area.height - 2, help_text, help_style);
    }
}

/// Find files dialog (Alt+/)
pub struct FindFilesDialog<'a> {
    pattern_input: &'a str,
    pattern_case_sensitive: bool,
    content_input: &'a str,
    content_case_sensitive: bool,
    path_input: &'a str,
    recursive: bool,
    focus: usize,
    input_selected: bool,
    theme: &'a Theme,
}

impl<'a> FindFilesDialog<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pattern_input: &'a str,
        pattern_case_sensitive: bool,
        content_input: &'a str,
        content_case_sensitive: bool,
        path_input: &'a str,
        recursive: bool,
        focus: usize,
        input_selected: bool,
        theme: &'a Theme,
    ) -> Self {
        Self {
            pattern_input,
            pattern_case_sensitive,
            content_input,
            content_case_sensitive,
            path_input,
            recursive,
            focus,
            input_selected,
            theme,
        }
    }
}

impl Widget for FindFilesDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Dialog dimensions (increased height for case sensitive checkboxes)
        let dialog_width = 60.min(area.width.saturating_sub(4));
        let dialog_height = 19;

        if area.width < 30 || area.height < dialog_height {
            return;
        }

        // Center the dialog
        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect {
            x,
            y,
            width: dialog_width,
            height: dialog_height,
        };

        // Use copy dialog colors (search is non-destructive)
        let bg_color = self.theme.dialog_copy_bg;
        let border_color = self.theme.dialog_copy_border;

        // Styles
        let border_style = Style::default().fg(border_color);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let label_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let input_style_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let input_style_selected = Style::default().bg(self.theme.dialog_input_selected_bg).fg(self.theme.dialog_input_selected_fg);
        let input_style_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_input_unfocused_fg);
        let button_style_focused = Style::default().fg(self.theme.dialog_button_focused_fg).bg(self.theme.dialog_button_focused_bg).add_modifier(Modifier::BOLD);
        let button_style_unfocused = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);
        let checkbox_style_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let checkbox_style_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help);

        // Draw background
        let bg_style = Style::default().bg(bg_color);
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Draw border
        buf[(dialog_area.x, dialog_area.y)].set_char('┌').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('┐').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }

        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('└').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('┘').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }

        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = " Find Files ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        // Content area
        let content_x = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;
        let max_display = content_width.saturating_sub(1);

        // File pattern label (line 2)
        let pattern_label = "File pattern (* and ? allowed):";
        buf.set_string(content_x, dialog_area.y + 2, pattern_label, label_style);

        // Pattern input field (line 3) - focus 0
        let pattern_y = dialog_area.y + 3;
        let pattern_style = if self.focus == 0 {
            if self.input_selected { input_style_selected } else { input_style_focused }
        } else {
            input_style_unfocused
        };
        for col in content_x..content_x + content_width as u16 {
            buf[(col, pattern_y)].set_char(' ').set_style(pattern_style);
        }
        let display_pattern = if self.pattern_input.len() > max_display {
            let skip = self.pattern_input.len() - max_display;
            format!("…{}", &self.pattern_input[skip + 1..])
        } else {
            self.pattern_input.to_string()
        };
        buf.set_string(content_x, pattern_y, &display_pattern, pattern_style);

        // Pattern case sensitive checkbox (line 4) - focus 1
        let pattern_case_y = dialog_area.y + 4;
        let pattern_case_style = if self.focus == 1 { checkbox_style_focused } else { checkbox_style_unfocused };
        let pattern_case_text = if self.pattern_case_sensitive {
            "[x] Case sensitive"
        } else {
            "[ ] Case sensitive"
        };
        buf.set_string(content_x + 2, pattern_case_y, pattern_case_text, pattern_case_style);

        // Content search label (line 6)
        let content_label = "Containing text (optional):";
        buf.set_string(content_x, dialog_area.y + 6, content_label, label_style);

        // Content input field (line 7) - focus 2
        let content_y = dialog_area.y + 7;
        let content_style = if self.focus == 2 {
            if self.input_selected { input_style_selected } else { input_style_focused }
        } else {
            input_style_unfocused
        };
        for col in content_x..content_x + content_width as u16 {
            buf[(col, content_y)].set_char(' ').set_style(content_style);
        }
        let display_content = if self.content_input.len() > max_display {
            let skip = self.content_input.len() - max_display;
            format!("…{}", &self.content_input[skip + 1..])
        } else {
            self.content_input.to_string()
        };
        buf.set_string(content_x, content_y, &display_content, content_style);

        // Content case sensitive checkbox (line 8) - focus 3
        let content_case_y = dialog_area.y + 8;
        let content_case_style = if self.focus == 3 { checkbox_style_focused } else { checkbox_style_unfocused };
        let content_case_text = if self.content_case_sensitive {
            "[x] Case sensitive"
        } else {
            "[ ] Case sensitive"
        };
        buf.set_string(content_x + 2, content_case_y, content_case_text, content_case_style);

        // Starting path label (line 10)
        let path_label = "Starting path:";
        buf.set_string(content_x, dialog_area.y + 10, path_label, label_style);

        // Path input field (line 11) - focus 4
        let path_y = dialog_area.y + 11;
        let path_style = if self.focus == 4 {
            if self.input_selected { input_style_selected } else { input_style_focused }
        } else {
            input_style_unfocused
        };
        for col in content_x..content_x + content_width as u16 {
            buf[(col, path_y)].set_char(' ').set_style(path_style);
        }
        let display_path = if self.path_input.len() > max_display {
            let skip = self.path_input.len() - max_display;
            format!("…{}", &self.path_input[skip + 1..])
        } else {
            self.path_input.to_string()
        };
        buf.set_string(content_x, path_y, &display_path, path_style);

        // Recursive checkbox (line 13) - focus 5
        let recursive_y = dialog_area.y + 13;
        let recursive_style = if self.focus == 5 { checkbox_style_focused } else { checkbox_style_unfocused };
        let recursive_text = if self.recursive {
            "[x] Recursive"
        } else {
            "[ ] Recursive"
        };
        buf.set_string(content_x, recursive_y, recursive_text, recursive_style);

        // Buttons (line 15) - focus 6 and 7
        let button_y = dialog_area.y + 15;
        let search_text = "[ Search ]";
        let cancel_text = "[ Cancel ]";

        let search_style = if self.focus == 6 { button_style_focused } else { button_style_unfocused };
        let cancel_style = if self.focus == 7 { button_style_focused } else { button_style_unfocused };

        // Center buttons
        let total_button_width = search_text.len() + 4 + cancel_text.len();
        let button_start_x = dialog_area.x + (dialog_area.width.saturating_sub(total_button_width as u16)) / 2;

        buf.set_string(button_start_x, button_y, search_text, search_style);
        buf.set_string(button_start_x + search_text.len() as u16 + 4, button_y, cancel_text, cancel_style);

        // Help text (line 17)
        let help_text = "Tab=Switch  Space=Toggle  Enter=Select  Esc=Cancel";
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help_text.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + 17, help_text, help_style);
    }
}

/// Calculate cursor position for the find files dialog pattern input field
pub fn find_files_pattern_cursor_position(area: Rect, pattern_input: &str, cursor_pos: usize) -> (u16, u16) {
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 19;

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let content_x = x + 2;
    let content_width = dialog_width.saturating_sub(4) as usize;
    let input_y = y + 3;

    let max_input_display = content_width.saturating_sub(1);
    let cursor_x = if pattern_input.len() > max_input_display {
        content_x + max_input_display as u16
    } else {
        content_x + cursor_pos.min(pattern_input.len()) as u16
    };

    (cursor_x, input_y)
}

/// Calculate cursor position for the find files dialog content input field
pub fn find_files_content_cursor_position(area: Rect, content_input: &str, cursor_pos: usize) -> (u16, u16) {
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 19;

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let content_x = x + 2;
    let content_width = dialog_width.saturating_sub(4) as usize;
    let input_y = y + 7;

    let max_input_display = content_width.saturating_sub(1);
    let cursor_x = if content_input.len() > max_input_display {
        content_x + max_input_display as u16
    } else {
        content_x + cursor_pos.min(content_input.len()) as u16
    };

    (cursor_x, input_y)
}

/// Calculate cursor position for the find files dialog path input field
pub fn find_files_path_cursor_position(area: Rect, path_input: &str, cursor_pos: usize) -> (u16, u16) {
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 19;

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let content_x = x + 2;
    let content_width = dialog_width.saturating_sub(4) as usize;
    let input_y = y + 11;

    let max_input_display = content_width.saturating_sub(1);
    let cursor_x = if path_input.len() > max_input_display {
        content_x + max_input_display as u16
    } else {
        content_x + cursor_pos.min(path_input.len()) as u16
    };

    (cursor_x, input_y)
}

/// Viewer search dialog widget
pub struct ViewerSearchDialog<'a> {
    text_input: &'a str,
    case_sensitive: bool,
    hex_input: &'a str,
    focus: usize,
    input_selected: bool,
    theme: &'a Theme,
    match_count: usize,
    current_match: Option<usize>,
}

impl<'a> ViewerSearchDialog<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        text_input: &'a str,
        case_sensitive: bool,
        hex_input: &'a str,
        focus: usize,
        input_selected: bool,
        theme: &'a Theme,
        match_count: usize,
        current_match: Option<usize>,
    ) -> Self {
        Self {
            text_input,
            case_sensitive,
            hex_input,
            focus,
            input_selected,
            theme,
            match_count,
            current_match,
        }
    }
}

impl Widget for ViewerSearchDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Dialog dimensions
        let dialog_width = 60.min(area.width.saturating_sub(4));
        let dialog_height = 14;

        if area.width < 30 || area.height < dialog_height {
            return;
        }

        // Center the dialog
        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect {
            x,
            y,
            width: dialog_width,
            height: dialog_height,
        };

        // Use copy dialog colors (search is non-destructive)
        let bg_color = self.theme.dialog_copy_bg;
        let border_color = self.theme.dialog_copy_border;

        // Styles
        let border_style = Style::default().fg(border_color);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let label_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let input_style_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let input_style_selected = Style::default().bg(self.theme.dialog_input_selected_bg).fg(self.theme.dialog_input_selected_fg);
        let input_style_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_input_unfocused_fg);
        let button_style_focused = Style::default().fg(self.theme.dialog_button_focused_fg).bg(self.theme.dialog_button_focused_bg).add_modifier(Modifier::BOLD);
        let button_style_unfocused = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);
        let checkbox_style_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let checkbox_style_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help);

        // Draw background
        let bg_style = Style::default().bg(bg_color);
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Draw border
        buf[(dialog_area.x, dialog_area.y)].set_char('┌').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('┐').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }

        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('└').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('┘').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }

        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = " Search ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        // Content area
        let content_x = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;
        let max_display = content_width.saturating_sub(1);

        // Text search label (line 2)
        let text_label = "Text (* wildcard):";
        buf.set_string(content_x, dialog_area.y + 2, text_label, label_style);

        // Text input field (line 3) - focus 0
        let text_y = dialog_area.y + 3;
        let text_style = if self.focus == 0 {
            if self.input_selected { input_style_selected } else { input_style_focused }
        } else {
            input_style_unfocused
        };
        for col in content_x..content_x + content_width as u16 {
            buf[(col, text_y)].set_char(' ').set_style(text_style);
        }
        let display_text = if self.text_input.len() > max_display {
            let skip = self.text_input.len() - max_display;
            format!("…{}", &self.text_input[skip + 1..])
        } else {
            self.text_input.to_string()
        };
        buf.set_string(content_x, text_y, &display_text, text_style);

        // Case sensitive checkbox (line 4) - focus 1
        let case_y = dialog_area.y + 4;
        let case_style = if self.focus == 1 { checkbox_style_focused } else { checkbox_style_unfocused };
        let case_text = if self.case_sensitive {
            "[x] Case sensitive"
        } else {
            "[ ] Case sensitive"
        };
        buf.set_string(content_x + 2, case_y, case_text, case_style);

        // Hex search label (line 6)
        let hex_label = "Hex (e.g., 4D 5A or 4D5A):";
        buf.set_string(content_x, dialog_area.y + 6, hex_label, label_style);

        // Hex input field (line 7) - focus 2
        let hex_y = dialog_area.y + 7;
        let hex_style = if self.focus == 2 {
            if self.input_selected { input_style_selected } else { input_style_focused }
        } else {
            input_style_unfocused
        };
        for col in content_x..content_x + content_width as u16 {
            buf[(col, hex_y)].set_char(' ').set_style(hex_style);
        }
        let display_hex = if self.hex_input.len() > max_display {
            let skip = self.hex_input.len() - max_display;
            format!("…{}", &self.hex_input[skip + 1..])
        } else {
            self.hex_input.to_string()
        };
        buf.set_string(content_x, hex_y, &display_hex, hex_style);

        // Buttons (line 9) - focus 3 and 4
        let button_y = dialog_area.y + 9;
        let search_text = "[ Search ]";
        let cancel_text = "[ Cancel ]";

        let search_style = if self.focus == 3 { button_style_focused } else { button_style_unfocused };
        let cancel_style = if self.focus == 4 { button_style_focused } else { button_style_unfocused };

        // Center buttons
        let total_button_width = search_text.len() + 4 + cancel_text.len();
        let button_start_x = dialog_area.x + (dialog_area.width.saturating_sub(total_button_width as u16)) / 2;

        buf.set_string(button_start_x, button_y, search_text, search_style);
        buf.set_string(button_start_x + search_text.len() as u16 + 4, button_y, cancel_text, cancel_style);

        // Status line (line 11) - show match count if any
        let status_text = if self.match_count > 0 {
            if let Some(current) = self.current_match {
                format!("Match {} of {}", current + 1, self.match_count)
            } else {
                format!("{} matches", self.match_count)
            }
        } else {
            String::new()
        };
        if !status_text.is_empty() {
            let status_x = dialog_area.x + (dialog_area.width.saturating_sub(status_text.len() as u16)) / 2;
            buf.set_string(status_x, dialog_area.y + 11, &status_text, label_style);
        }

        // Help text (line 12)
        let help_text = "Tab=Switch  Space=Toggle  Enter=Search  Esc=Cancel";
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help_text.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + 12, help_text, help_style);
    }
}

/// Calculate cursor position for the viewer search dialog text input field
pub fn viewer_search_text_cursor_position(area: Rect, text_input: &str, cursor_pos: usize) -> (u16, u16) {
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 14;

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let content_x = x + 2;
    let content_width = dialog_width.saturating_sub(4) as usize;
    let input_y = y + 3;

    let max_input_display = content_width.saturating_sub(1);
    let cursor_x = if text_input.len() > max_input_display {
        content_x + max_input_display as u16
    } else {
        content_x + cursor_pos.min(text_input.len()) as u16
    };

    (cursor_x, input_y)
}

/// Calculate cursor position for the viewer search dialog hex input field
pub fn viewer_search_hex_cursor_position(area: Rect, hex_input: &str, cursor_pos: usize) -> (u16, u16) {
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 14;

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let content_x = x + 2;
    let content_width = dialog_width.saturating_sub(4) as usize;
    let input_y = y + 7;

    let max_input_display = content_width.saturating_sub(1);
    let cursor_x = if hex_input.len() > max_input_display {
        content_x + max_input_display as u16
    } else {
        content_x + cursor_pos.min(hex_input.len()) as u16
    };

    (cursor_x, input_y)
}

/// Select files by pattern dialog widget
pub struct SelectFilesDialog<'a> {
    pattern_input: &'a str,
    include_dirs: bool,
    focus: usize,
    input_selected: bool,
    theme: &'a Theme,
}

impl<'a> SelectFilesDialog<'a> {
    pub fn new(
        pattern_input: &'a str,
        include_dirs: bool,
        focus: usize,
        input_selected: bool,
        theme: &'a Theme,
    ) -> Self {
        Self {
            pattern_input,
            include_dirs,
            focus,
            input_selected,
            theme,
        }
    }
}

impl Widget for SelectFilesDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Dialog dimensions (compact)
        let dialog_width = 50.min(area.width.saturating_sub(4));
        let dialog_height = 10;

        if area.width < 30 || area.height < dialog_height {
            return;
        }

        // Center the dialog
        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect {
            x,
            y,
            width: dialog_width,
            height: dialog_height,
        };

        // Use move dialog colors (selection is non-destructive but action-oriented)
        let bg_color = self.theme.dialog_move_bg;
        let border_color = self.theme.dialog_move_border;

        // Styles
        let border_style = Style::default().fg(border_color);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let label_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let input_style_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let input_style_selected = Style::default().bg(self.theme.dialog_input_selected_bg).fg(self.theme.dialog_input_selected_fg);
        let input_style_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_input_unfocused_fg);
        let button_style_focused = Style::default().fg(self.theme.dialog_button_focused_fg).bg(self.theme.dialog_button_focused_bg).add_modifier(Modifier::BOLD);
        let button_style_unfocused = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);
        let checkbox_style_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let checkbox_style_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help);

        // Draw background
        let bg_style = Style::default().bg(bg_color);
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Draw border
        buf[(dialog_area.x, dialog_area.y)].set_char('┌').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('┐').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }

        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('└').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('┘').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }

        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = " Select Files ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        // Content area
        let content_x = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;
        let max_display = content_width.saturating_sub(1);

        // Pattern label (line 2)
        let pattern_label = "Pattern (* and ? allowed):";
        buf.set_string(content_x, dialog_area.y + 2, pattern_label, label_style);

        // Pattern input field (line 3) - focus 0
        let pattern_y = dialog_area.y + 3;
        let pattern_style = if self.focus == 0 {
            if self.input_selected { input_style_selected } else { input_style_focused }
        } else {
            input_style_unfocused
        };
        for col in content_x..content_x + content_width as u16 {
            buf[(col, pattern_y)].set_char(' ').set_style(pattern_style);
        }
        let display_pattern = if self.pattern_input.len() > max_display {
            let skip = self.pattern_input.len() - max_display;
            format!("…{}", &self.pattern_input[skip + 1..])
        } else {
            self.pattern_input.to_string()
        };
        buf.set_string(content_x, pattern_y, &display_pattern, pattern_style);

        // Include dirs checkbox (line 5) - focus 1
        let checkbox_y = dialog_area.y + 5;
        let checkbox_style = if self.focus == 1 { checkbox_style_focused } else { checkbox_style_unfocused };
        let checkbox_char = if self.include_dirs { '☑' } else { '☐' };
        let checkbox_text = format!("{} Include folders", checkbox_char);
        buf.set_string(content_x, checkbox_y, &checkbox_text, checkbox_style);

        // Buttons (line 7)
        let button_y = dialog_area.y + 7;
        let select_btn = "[ Select ]";
        let cancel_btn = "[ Cancel ]";

        let buttons_width = select_btn.len() + 2 + cancel_btn.len();
        let buttons_x = dialog_area.x + (dialog_area.width.saturating_sub(buttons_width as u16)) / 2;

        let select_style = if self.focus == 2 { button_style_focused } else { button_style_unfocused };
        let cancel_style = if self.focus == 3 { button_style_focused } else { button_style_unfocused };

        buf.set_string(buttons_x, button_y, select_btn, select_style);
        buf.set_string(buttons_x + select_btn.len() as u16 + 2, button_y, cancel_btn, cancel_style);

        // Help text (line 8)
        let help = "Tab to switch fields, Enter to confirm";
        buf.set_string(content_x, dialog_area.y + 8, help, help_style);
    }
}

/// Calculate cursor position for the select files dialog pattern input field
pub fn select_files_cursor_position(area: Rect, pattern_input: &str, cursor_pos: usize) -> (u16, u16) {
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 10;

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let content_x = x + 2;
    let content_width = dialog_width.saturating_sub(4) as usize;
    let input_y = y + 3;

    let max_input_display = content_width.saturating_sub(1);
    let cursor_x = if pattern_input.len() > max_input_display {
        content_x + max_input_display as u16
    } else {
        content_x + cursor_pos.min(pattern_input.len()) as u16
    };

    (cursor_x, input_y)
}

/// SCP connection dialog for creating/editing remote connections
#[allow(dead_code)]
pub struct ScpConnectDialog<'a> {
    name_input: &'a str,
    name_cursor: usize,
    user_input: &'a str,
    user_cursor: usize,
    host_input: &'a str,
    host_cursor: usize,
    port_input: &'a str,
    port_cursor: usize,
    path_input: &'a str,
    path_cursor: usize,
    password_input: &'a str,
    password_cursor: usize,
    focus: usize,
    input_selected: bool,
    error: Option<&'a str>,
    theme: &'a Theme,
}

impl<'a> ScpConnectDialog<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name_input: &'a str,
        name_cursor: usize,
        user_input: &'a str,
        user_cursor: usize,
        host_input: &'a str,
        host_cursor: usize,
        port_input: &'a str,
        port_cursor: usize,
        path_input: &'a str,
        path_cursor: usize,
        password_input: &'a str,
        password_cursor: usize,
        focus: usize,
        input_selected: bool,
        error: Option<&'a str>,
        theme: &'a Theme,
    ) -> Self {
        Self {
            name_input,
            name_cursor,
            user_input,
            user_cursor,
            host_input,
            host_cursor,
            port_input,
            port_cursor,
            path_input,
            path_cursor,
            password_input,
            password_cursor,
            focus,
            input_selected,
            error,
            theme,
        }
    }
}

impl Widget for ScpConnectDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog_width = 55.min(area.width.saturating_sub(4));
        let dialog_height = 16;

        if area.width < 40 || area.height < dialog_height {
            return;
        }

        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect {
            x,
            y,
            width: dialog_width,
            height: dialog_height,
        };

        // Use a distinct color for SCP dialogs (blue-ish like move dialog)
        let bg_color = self.theme.dialog_move_bg;
        let border_color = self.theme.dialog_move_border;

        let border_style = Style::default().fg(border_color);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let label_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let input_focused_style = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let input_selected_style = Style::default().bg(self.theme.dialog_input_selected_bg).fg(self.theme.dialog_input_selected_fg);
        let input_unfocused_style = Style::default().bg(bg_color).fg(self.theme.dialog_input_unfocused_fg);
        let button_focused_style = Style::default().bg(self.theme.dialog_button_focused_bg).fg(self.theme.dialog_button_focused_fg).add_modifier(Modifier::BOLD);
        let button_unfocused_style = Style::default().bg(bg_color).fg(self.theme.dialog_button_unfocused);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help);
        let error_style = Style::default().bg(bg_color).fg(Color::Red);

        // Draw background
        let bg_style = Style::default().bg(bg_color);
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Draw border
        buf[(dialog_area.x, dialog_area.y)].set_char('╔').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('╗').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('═').set_style(border_style);
        }

        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('╚').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('╝').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('═').set_style(border_style);
        }

        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('║').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('║').set_style(border_style);
        }

        // Title
        let title = " New SCP Connection ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        let content_x = dialog_area.x + 2;
        let label_width = 11;
        let input_x = content_x + label_width;
        let input_width = (dialog_width - 4 - label_width) as usize;

        // Row 1: Name
        let row_y = dialog_area.y + 2;
        buf.set_string(content_x, row_y, "Name:      ", label_style);
        let name_style = if self.focus == 0 {
            if self.input_selected { input_selected_style } else { input_focused_style }
        } else { input_unfocused_style };
        let name_display = format!("{:<width$}", self.name_input, width = input_width);
        buf.set_string(input_x, row_y, &name_display[..input_width.min(name_display.len())], name_style);

        // Row 2: User
        let row_y = dialog_area.y + 3;
        buf.set_string(content_x, row_y, "User:      ", label_style);
        let user_style = if self.focus == 1 {
            if self.input_selected { input_selected_style } else { input_focused_style }
        } else { input_unfocused_style };
        let user_display = format!("{:<width$}", self.user_input, width = input_width);
        buf.set_string(input_x, row_y, &user_display[..input_width.min(user_display.len())], user_style);

        // Row 3: Host
        let row_y = dialog_area.y + 4;
        buf.set_string(content_x, row_y, "Host:      ", label_style);
        let host_style = if self.focus == 2 {
            if self.input_selected { input_selected_style } else { input_focused_style }
        } else { input_unfocused_style };
        let host_display = format!("{:<width$}", self.host_input, width = input_width);
        buf.set_string(input_x, row_y, &host_display[..input_width.min(host_display.len())], host_style);

        // Row 4: Port
        let row_y = dialog_area.y + 5;
        buf.set_string(content_x, row_y, "Port:      ", label_style);
        let port_style = if self.focus == 3 {
            if self.input_selected { input_selected_style } else { input_focused_style }
        } else { input_unfocused_style };
        let port_width = 6;
        let port_display = format!("{:<width$}", self.port_input, width = port_width);
        buf.set_string(input_x, row_y, &port_display[..port_width.min(port_display.len())], port_style);

        // Row 5: Path
        let row_y = dialog_area.y + 6;
        buf.set_string(content_x, row_y, "Path:      ", label_style);
        let path_style = if self.focus == 4 {
            if self.input_selected { input_selected_style } else { input_focused_style }
        } else { input_unfocused_style };
        let path_display = format!("{:<width$}", self.path_input, width = input_width);
        buf.set_string(input_x, row_y, &path_display[..input_width.min(path_display.len())], path_style);

        // Row 6: Password (shown as asterisks)
        let row_y = dialog_area.y + 8;
        buf.set_string(content_x, row_y, "Password:  ", label_style);
        let pass_style = if self.focus == 5 {
            if self.input_selected { input_selected_style } else { input_focused_style }
        } else { input_unfocused_style };
        let pass_masked: String = "*".repeat(self.password_input.len());
        let pass_display = format!("{:<width$}", pass_masked, width = input_width);
        buf.set_string(input_x, row_y, &pass_display[..input_width.min(pass_display.len())], pass_style);

        // Note about password
        let row_y = dialog_area.y + 9;
        buf.set_string(content_x, row_y, "(Leave blank to use SSH agent/key)", help_style);

        // Error message (if any)
        if let Some(err) = self.error {
            let row_y = dialog_area.y + 11;
            let err_display = if err.len() > input_width + label_width as usize {
                &err[..input_width + label_width as usize - 3]
            } else {
                err
            };
            buf.set_string(content_x, row_y, err_display, error_style);
        }

        // Buttons row
        let row_y = dialog_area.y + 13;
        let connect_text = "[ Connect ]";
        let save_text = "[ Save ]";
        let cancel_text = "[ Cancel ]";

        let total_buttons = connect_text.len() + save_text.len() + cancel_text.len() + 4;
        let button_x = dialog_area.x + (dialog_area.width.saturating_sub(total_buttons as u16)) / 2;

        let connect_style = if self.focus == 6 { button_focused_style } else { button_unfocused_style };
        let save_style = if self.focus == 7 { button_focused_style } else { button_unfocused_style };
        let cancel_style = if self.focus == 8 { button_focused_style } else { button_unfocused_style };

        buf.set_string(button_x, row_y, connect_text, connect_style);
        buf.set_string(button_x + connect_text.len() as u16 + 2, row_y, save_text, save_style);
        buf.set_string(button_x + connect_text.len() as u16 + save_text.len() as u16 + 4, row_y, cancel_text, cancel_style);

        // Help text
        let help = "Tab to switch, Enter to confirm, Esc to cancel";
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + 14, help, help_style);
    }
}

/// Calculate cursor position for the SCP connect dialog input fields
#[allow(clippy::too_many_arguments)]
pub fn scp_connect_cursor_position(
    area: Rect,
    focus: usize,
    name_input: &str,
    name_cursor: usize,
    user_input: &str,
    user_cursor: usize,
    host_input: &str,
    host_cursor: usize,
    port_input: &str,
    port_cursor: usize,
    path_input: &str,
    path_cursor: usize,
    password_input: &str,
    password_cursor: usize,
) -> Option<(u16, u16)> {
    // Only show cursor for text input fields (focus 0-5)
    if focus > 5 {
        return None;
    }

    let dialog_width = 55.min(area.width.saturating_sub(4));
    let dialog_height = 16;

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let label_width = 11;
    let input_x = x + 2 + label_width;
    let input_width = (dialog_width - 4 - label_width) as usize;

    let (input, cursor, row_offset) = match focus {
        0 => (name_input, name_cursor, 2),
        1 => (user_input, user_cursor, 3),
        2 => (host_input, host_cursor, 4),
        3 => (port_input, port_cursor, 5),
        4 => (path_input, path_cursor, 6),
        5 => (password_input, password_cursor, 8),
        _ => return None,
    };

    let row_y = y + row_offset;
    let cursor_pos = cursor.min(input.len()).min(input_width.saturating_sub(1));

    Some((input_x + cursor_pos as u16, row_y))
}

/// SCP password prompt dialog (shown when key auth fails)
pub struct ScpPasswordPromptDialog<'a> {
    display_name: &'a str,
    password_input: &'a str,
    focus: usize,
    input_selected: bool,
    error: Option<&'a str>,
    theme: &'a Theme,
}

impl<'a> ScpPasswordPromptDialog<'a> {
    pub fn new(
        display_name: &'a str,
        password_input: &'a str,
        focus: usize,
        input_selected: bool,
        error: Option<&'a str>,
        theme: &'a Theme,
    ) -> Self {
        Self {
            display_name,
            password_input,
            focus,
            input_selected,
            error,
            theme,
        }
    }
}

impl Widget for ScpPasswordPromptDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog_width = 50.min(area.width.saturating_sub(4));
        let dialog_height = if self.error.is_some() { 11 } else { 9 };

        if area.width < 30 || area.height < dialog_height {
            return;
        }

        // Center the dialog
        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect {
            x,
            y,
            width: dialog_width,
            height: dialog_height,
        };

        // Use move dialog colors (connection-related)
        let bg_color = self.theme.dialog_move_bg;
        let border_color = self.theme.dialog_move_border;

        let border_style = Style::default().fg(border_color);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let label_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let input_style_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let input_style_selected = Style::default().bg(self.theme.dialog_input_selected_bg).fg(self.theme.dialog_input_selected_fg);
        let input_style_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_input_unfocused_fg);
        let button_style_focused = Style::default().fg(self.theme.dialog_button_focused_fg).bg(self.theme.dialog_button_focused_bg).add_modifier(Modifier::BOLD);
        let button_style_unfocused = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help);
        let error_style = Style::default().bg(bg_color).fg(Color::Red);

        // Draw background
        let bg_style = Style::default().bg(bg_color);
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Draw border
        buf[(dialog_area.x, dialog_area.y)].set_char('╔').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('╗').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('═').set_style(border_style);
        }

        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('╚').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('╝').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('═').set_style(border_style);
        }

        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('║').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('║').set_style(border_style);
        }

        // Title
        let title = " Password Required ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        let content_x = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;

        // Connection name (line 2)
        let conn_label = format!("Connect to: {}", self.display_name);
        let truncated_label: String = conn_label.chars().take(content_width).collect();
        buf.set_string(content_x, dialog_area.y + 2, &truncated_label, label_style);

        // Password label (line 3)
        buf.set_string(content_x, dialog_area.y + 3, "Password:", label_style);

        // Password input field (line 4)
        let input_y = dialog_area.y + 4;
        let input_style = if self.focus == 0 {
            if self.input_selected { input_style_selected } else { input_style_focused }
        } else {
            input_style_unfocused
        };

        // Clear input line with input style
        for col in content_x..content_x + content_width as u16 {
            buf[(col, input_y)].set_char(' ').set_style(input_style);
        }

        // Show masked password
        let max_display = content_width.saturating_sub(1);
        let masked: String = "*".repeat(self.password_input.len().min(max_display));
        buf.set_string(content_x, input_y, &masked, input_style);

        // Error message if present (line 5)
        let button_line_offset = if let Some(err) = self.error {
            let err_y = dialog_area.y + 6;
            let err_display: String = err.chars().take(content_width).collect();
            buf.set_string(content_x, err_y, &err_display, error_style);
            8  // buttons at line 8
        } else {
            6  // buttons at line 6
        };

        // Buttons
        let button_y = dialog_area.y + button_line_offset;
        let ok_text = "[ OK ]";
        let cancel_text = "[ Cancel ]";

        let ok_style = if self.focus == 1 { button_style_focused } else { button_style_unfocused };
        let cancel_style = if self.focus == 2 { button_style_focused } else { button_style_unfocused };

        // Center buttons
        let total_button_width = ok_text.len() + 4 + cancel_text.len();
        let button_start_x = dialog_area.x + (dialog_area.width.saturating_sub(total_button_width as u16)) / 2;

        buf.set_string(button_start_x, button_y, ok_text, ok_style);
        buf.set_string(button_start_x + ok_text.len() as u16 + 4, button_y, cancel_text, cancel_style);

        // Help text
        let help_text = "Tab=Switch  Enter=Confirm  Esc=Cancel";
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help_text.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + dialog_area.height - 2, help_text, help_style);
    }
}

/// Calculate cursor position for the SCP password prompt dialog
pub fn scp_password_prompt_cursor_position(area: Rect, password_input: &str, cursor_pos: usize) -> (u16, u16) {
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 9;

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let content_x = x + 2;
    let content_width = dialog_width.saturating_sub(4) as usize;
    let input_y = y + 4;

    let max_input_display = content_width.saturating_sub(1);
    let cursor_x = if password_input.len() > max_input_display {
        content_x + max_input_display as u16
    } else {
        content_x + cursor_pos.min(password_input.len()) as u16
    };

    (cursor_x, input_y)
}

/// Archive password prompt dialog (for encrypted archives)
pub struct ArchivePasswordPromptDialog<'a> {
    archive_name: &'a str,
    password_input: &'a str,
    focus: usize,
    input_selected: bool,
    error: Option<&'a str>,
    theme: &'a Theme,
}

impl<'a> ArchivePasswordPromptDialog<'a> {
    pub fn new(
        archive_name: &'a str,
        password_input: &'a str,
        focus: usize,
        input_selected: bool,
        error: Option<&'a str>,
        theme: &'a Theme,
    ) -> Self {
        Self {
            archive_name,
            password_input,
            focus,
            input_selected,
            error,
            theme,
        }
    }
}

impl Widget for ArchivePasswordPromptDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog_width = 50.min(area.width.saturating_sub(4));
        let dialog_height = if self.error.is_some() { 11 } else { 9 };

        if area.width < 30 || area.height < dialog_height {
            return;
        }

        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect { x, y, width: dialog_width, height: dialog_height };

        let bg_color = self.theme.dialog_move_bg;
        let border_color = self.theme.dialog_move_border;

        let border_style = Style::default().fg(border_color);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let label_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let input_style_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let input_style_selected = Style::default().bg(self.theme.dialog_input_selected_bg).fg(self.theme.dialog_input_selected_fg);
        let input_style_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_input_unfocused_fg);
        let button_style_focused = Style::default().fg(self.theme.dialog_button_focused_fg).bg(self.theme.dialog_button_focused_bg).add_modifier(Modifier::BOLD);
        let button_style_unfocused = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help);
        let error_style = Style::default().bg(bg_color).fg(Color::Red);

        // Draw background
        let bg_style = Style::default().bg(bg_color);
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Draw border
        buf[(dialog_area.x, dialog_area.y)].set_char('╔').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('╗').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('═').set_style(border_style);
        }

        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('╚').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('╝').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('═').set_style(border_style);
        }

        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('║').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('║').set_style(border_style);
        }

        // Title
        let title = " Password Required ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        let content_x = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;

        // Archive name (line 2)
        let archive_label = format!("Archive: {}", self.archive_name);
        let truncated_label: String = archive_label.chars().take(content_width).collect();
        buf.set_string(content_x, dialog_area.y + 2, &truncated_label, label_style);

        // Password label (line 3)
        buf.set_string(content_x, dialog_area.y + 3, "Password:", label_style);

        // Password input field (line 4)
        let input_y = dialog_area.y + 4;
        let input_style = if self.focus == 0 {
            if self.input_selected { input_style_selected } else { input_style_focused }
        } else {
            input_style_unfocused
        };

        for col in content_x..content_x + content_width as u16 {
            buf[(col, input_y)].set_char(' ').set_style(input_style);
        }

        let max_display = content_width.saturating_sub(1);
        let masked: String = "*".repeat(self.password_input.len().min(max_display));
        buf.set_string(content_x, input_y, &masked, input_style);

        // Error message if present (line 6)
        let button_line_offset = if let Some(err) = self.error {
            let err_y = dialog_area.y + 6;
            let err_display: String = err.chars().take(content_width).collect();
            buf.set_string(content_x, err_y, &err_display, error_style);
            8
        } else {
            6
        };

        // Buttons
        let button_y = dialog_area.y + button_line_offset;
        let ok_text = "[ OK ]";
        let cancel_text = "[ Cancel ]";

        let ok_style = if self.focus == 1 { button_style_focused } else { button_style_unfocused };
        let cancel_style = if self.focus == 2 { button_style_focused } else { button_style_unfocused };

        let total_button_width = ok_text.len() + 4 + cancel_text.len();
        let button_start_x = dialog_area.x + (dialog_area.width.saturating_sub(total_button_width as u16)) / 2;

        buf.set_string(button_start_x, button_y, ok_text, ok_style);
        buf.set_string(button_start_x + ok_text.len() as u16 + 4, button_y, cancel_text, cancel_style);

        // Help text
        let help_text = "Tab=Switch  Enter=Confirm  Esc=Cancel";
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help_text.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + dialog_area.height - 2, help_text, help_style);
    }
}

/// Calculate cursor position for the archive password prompt dialog
pub fn archive_password_prompt_cursor_position(area: Rect, password_input: &str, cursor_pos: usize, has_error: bool) -> (u16, u16) {
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = if has_error { 11 } else { 9 };

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let content_x = x + 2;
    let content_width = dialog_width.saturating_sub(4) as usize;
    let input_y = y + 4;

    let max_input_display = content_width.saturating_sub(1);
    let cursor_x = if password_input.len() > max_input_display {
        content_x + max_input_display as u16
    } else {
        content_x + cursor_pos.min(password_input.len()) as u16
    };

    (cursor_x, input_y)
}

/// Simple yes/no confirmation dialog
pub struct SimpleConfirmDialog<'a> {
    message: &'a str,
    focus: usize,
    theme: &'a Theme,
}

impl<'a> SimpleConfirmDialog<'a> {
    pub fn new(message: &'a str, focus: usize, theme: &'a Theme) -> Self {
        Self {
            message,
            focus,
            theme,
        }
    }
}

impl Widget for SimpleConfirmDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = self.theme;

        // Calculate dialog size
        let msg_len = self.message.len() as u16;
        let dialog_width = (msg_len + 6).max(30).min(area.width.saturating_sub(4));
        let dialog_height = 6;

        // Center the dialog
        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

        // Use delete dialog colors for confirmation dialogs
        let dialog_bg = Style::default()
            .bg(theme.dialog_delete_bg)
            .fg(theme.dialog_text);
        let title_style = Style::default()
            .fg(theme.dialog_title)
            .bg(theme.dialog_delete_bg)
            .add_modifier(Modifier::BOLD);
        let button_style = Style::default()
            .fg(theme.dialog_button_unfocused)
            .bg(theme.dialog_delete_bg);
        let button_focused = Style::default()
            .fg(theme.dialog_button_focused_fg)
            .bg(theme.dialog_button_focused_bg)
            .add_modifier(Modifier::BOLD);
        let help_style = Style::default()
            .fg(theme.dialog_help)
            .bg(theme.dialog_delete_bg);

        // Clear dialog area with background
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_style(dialog_bg);
                buf[(col, row)].set_char(' ');
            }
        }

        // Draw border
        let border_style = Style::default()
            .fg(theme.dialog_delete_border)
            .bg(theme.dialog_delete_bg);

        // Top border
        buf[(dialog_area.x, dialog_area.y)].set_char('╭').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('╮').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }

        // Bottom border
        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('╰').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('╯').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }

        // Side borders
        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = " Confirm ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        // Message
        let msg_x = dialog_area.x + (dialog_area.width.saturating_sub(self.message.len() as u16)) / 2;
        buf.set_string(msg_x, dialog_area.y + 2, self.message, dialog_bg);

        // Buttons: [ Yes ] [ No ]
        let yes_btn = "[ Yes ]";
        let no_btn = "[ No ]";
        let buttons_width = yes_btn.len() + 2 + no_btn.len();
        let buttons_x = dialog_area.x + (dialog_area.width.saturating_sub(buttons_width as u16)) / 2;

        buf.set_string(
            buttons_x,
            dialog_area.y + 4,
            yes_btn,
            if self.focus == 0 { button_focused } else { button_style },
        );
        buf.set_string(
            buttons_x + yes_btn.len() as u16 + 2,
            dialog_area.y + 4,
            no_btn,
            if self.focus == 1 { button_focused } else { button_style },
        );

        // Help text
        let help = "Y/N, Tab, Enter, Esc";
        if dialog_width > help.len() as u16 + 4 {
            let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help.len() as u16)) / 2;
            buf.set_string(help_x, dialog_area.y + dialog_area.height - 1, help, help_style);
        }
    }
}

/// User menu dialog (F2)
pub struct UserMenuDialog<'a> {
    rules: &'a [crate::config::UserMenuRule],
    selected: usize,
    scroll: usize,
    theme: &'a Theme,
}

impl<'a> UserMenuDialog<'a> {
    pub fn new(
        rules: &'a [crate::config::UserMenuRule],
        selected: usize,
        scroll: usize,
        theme: &'a Theme,
    ) -> Self {
        Self {
            rules,
            selected,
            scroll,
            theme,
        }
    }
}

impl Widget for UserMenuDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Dialog dimensions
        let dialog_width = 50.min(area.width.saturating_sub(4));
        let visible_items = 12.min(self.rules.len().max(1));
        let dialog_height = (visible_items + 4) as u16; // border + title + items + help

        if area.width < 25 || area.height < dialog_height {
            return;
        }

        // Center the dialog
        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect {
            x,
            y,
            width: dialog_width,
            height: dialog_height,
        };

        // Use theme colors
        let bg_color = self.theme.dialog_copy_bg;
        let border_color = self.theme.dialog_copy_border;

        // Styles
        let border_style = Style::default().fg(border_color).bg(bg_color).remove_modifier(Modifier::BOLD);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).remove_modifier(Modifier::BOLD);
        let item_style = Style::default().bg(bg_color).fg(self.theme.dialog_text).remove_modifier(Modifier::BOLD);
        let selected_style = Style::default().bg(self.theme.cursor_bg).fg(self.theme.cursor_fg).remove_modifier(Modifier::BOLD);
        let hotkey_style = Style::default().bg(bg_color).fg(Color::Yellow).remove_modifier(Modifier::BOLD);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help).remove_modifier(Modifier::BOLD);

        // Draw background
        let bg_style = Style::default().bg(bg_color).remove_modifier(Modifier::all());
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Draw border
        buf[(dialog_area.x, dialog_area.y)].set_char('┌').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('┐').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }

        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('└').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('┘').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }

        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = " User Menu ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        // Content area
        let content_x = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;

        // Draw rules
        if self.rules.is_empty() {
            let empty_msg = "(No commands defined)";
            let msg_x = dialog_area.x + (dialog_area.width.saturating_sub(empty_msg.len() as u16)) / 2;
            buf.set_string(msg_x, dialog_area.y + 2, empty_msg, help_style);
        } else {
            for (i, rule) in self.rules.iter().skip(self.scroll).take(visible_items).enumerate() {
                let rule_idx = self.scroll + i;
                let row_y = dialog_area.y + 1 + i as u16;

                let is_selected = rule_idx == self.selected;
                let style = if is_selected { selected_style } else { item_style };

                // Clear the line with the style
                for col in content_x..content_x + content_width as u16 {
                    buf[(col, row_y)].set_char(' ').set_style(style);
                }

                // Format: "[h] Rule name" or "[ ] Rule name"
                let hotkey_prefix = if let Some(ref h) = rule.hotkey {
                    format!("[{}] ", h)
                } else {
                    "[ ] ".to_string()
                };

                // Draw hotkey prefix with special style (unless selected)
                let prefix_style = if is_selected { selected_style } else { hotkey_style };
                buf.set_string(content_x, row_y, &hotkey_prefix, prefix_style);

                // Draw rule name
                let name_x = content_x + hotkey_prefix.len() as u16;
                let available_width = content_width.saturating_sub(hotkey_prefix.len());
                let display_name = if rule.name.len() > available_width {
                    format!("{}…", &rule.name[..available_width - 1])
                } else {
                    rule.name.clone()
                };
                buf.set_string(name_x, row_y, &display_name, style);
            }
        }

        // Help text at bottom
        let help_text = "Enter:Run Ins:Add F4:Edit F8:Del Esc:Close";
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help_text.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + dialog_height - 2, help_text, help_style);
    }
}

/// User menu edit dialog (add/edit a rule)
pub struct UserMenuEditDialog<'a> {
    editing_index: Option<usize>,
    name_input: &'a str,
    command_input: &'a str,
    hotkey_input: &'a str,
    focus: usize,
    input_selected: bool,
    error: Option<&'a str>,
    theme: &'a Theme,
}

impl<'a> UserMenuEditDialog<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        editing_index: Option<usize>,
        name_input: &'a str,
        command_input: &'a str,
        hotkey_input: &'a str,
        focus: usize,
        input_selected: bool,
        error: Option<&'a str>,
        theme: &'a Theme,
    ) -> Self {
        Self {
            editing_index,
            name_input,
            command_input,
            hotkey_input,
            focus,
            input_selected,
            error,
            theme,
        }
    }
}

impl Widget for UserMenuEditDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use super::dialog_helpers::{DialogRenderer, DialogStyles};

        // Dialog dimensions
        let dialog_width = 60.min(area.width.saturating_sub(4));
        let dialog_height = if self.error.is_some() { 15 } else { 14 };

        let Some(dialog_area) = DialogRenderer::center_dialog(area, dialog_width, dialog_height, 30) else {
            return;
        };

        // Setup styles
        let bg_color = self.theme.dialog_mkdir_bg;
        let border_color = self.theme.dialog_mkdir_border;
        let styles = DialogStyles::new(self.theme, bg_color, border_color);

        // Draw dialog frame
        DialogRenderer::fill_background(dialog_area, buf, styles.bg);
        DialogRenderer::draw_border(dialog_area, buf, styles.border);

        // Title
        let title = if self.editing_index.is_some() {
            " Edit Command "
        } else {
            " New Command "
        };
        DialogRenderer::draw_title(dialog_area, buf, title, styles.title);

        // Content area
        let content_x = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;

        // Name label (line 2)
        buf.set_string(content_x, dialog_area.y + 2, "Name:", styles.label);

        // Name input field (line 3)
        let name_style = if self.focus == 0 {
            if self.input_selected { styles.input_selected } else { styles.input_focused }
        } else {
            styles.input_unfocused
        };
        DialogRenderer::draw_input_field(buf, content_x, dialog_area.y + 3, content_width, self.name_input, name_style);

        // Command label (line 5)
        buf.set_string(content_x, dialog_area.y + 5, "Command:", styles.label);

        // Command input field (line 6)
        let cmd_style = if self.focus == 1 {
            if self.input_selected { styles.input_selected } else { styles.input_focused }
        } else {
            styles.input_unfocused
        };
        DialogRenderer::draw_input_field(buf, content_x, dialog_area.y + 6, content_width, self.command_input, cmd_style);

        // Hotkey label (line 8)
        buf.set_string(content_x, dialog_area.y + 8, "Hotkey (optional, single char):", styles.label);

        // Hotkey input field (line 9)
        let hotkey_style = if self.focus == 2 {
            if self.input_selected { styles.input_selected } else { styles.input_focused }
        } else {
            styles.input_unfocused
        };
        // Hotkey is only 1 char, but show a small field
        DialogRenderer::draw_input_field(buf, content_x, dialog_area.y + 9, 3, self.hotkey_input, hotkey_style);

        // Error message if present
        let button_y_offset = if let Some(err) = self.error {
            let error_style = Style::default().bg(bg_color).fg(self.theme.dialog_warning);
            let err_display = if err.len() > content_width {
                format!("{}…", &err[..content_width - 1])
            } else {
                err.to_string()
            };
            buf.set_string(content_x, dialog_area.y + 11, &err_display, error_style);
            12
        } else {
            11
        };

        // Buttons
        DialogRenderer::draw_buttons(
            dialog_area, buf, button_y_offset,
            &[("[ Save ]", self.focus == 3), ("[ Cancel ]", self.focus == 4)],
            styles.button_focused, styles.button_unfocused,
        );

        // Help text showing placeholders
        let help_text = "Placeholders: !.! %f %n %e %d %s";
        DialogRenderer::draw_help(dialog_area, buf, help_text, styles.help);
    }
}

/// Calculate cursor position for user menu edit dialog
#[allow(clippy::too_many_arguments)]
pub fn user_menu_edit_cursor_position(
    area: Rect,
    focus: usize,
    name_input: &str,
    name_cursor: usize,
    command_input: &str,
    command_cursor: usize,
    hotkey_input: &str,
    hotkey_cursor: usize,
) -> Option<(u16, u16)> {
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 14;

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let content_x = x + 2;
    let content_width = dialog_width.saturating_sub(4) as usize;

    let (row_offset, input, cursor, input_width) = match focus {
        0 => (3, name_input, name_cursor, content_width),
        1 => (6, command_input, command_cursor, content_width),
        2 => (9, hotkey_input, hotkey_cursor, 3),
        _ => return None,
    };

    let row_y = y + row_offset;
    let max_display = input_width.saturating_sub(1);
    let cursor_x = if input.len() > max_display {
        content_x + max_display as u16
    } else {
        content_x + cursor.min(input.len()) as u16
    };

    Some((cursor_x, row_y))
}

/// Generic plugin connection dialog that renders fields dynamically
pub struct PluginConnectDialog<'a> {
    pub plugin_name: &'a str,
    pub fields: &'a [crate::plugins::provider_api::DialogField],
    pub values: &'a [String],
    pub focus: usize,
    pub error: Option<&'a str>,
    pub theme: &'a Theme,
    pub input_selected: bool,
}

impl<'a> PluginConnectDialog<'a> {
    pub fn new(
        plugin_name: &'a str,
        fields: &'a [crate::plugins::provider_api::DialogField],
        values: &'a [String],
        focus: usize,
        error: Option<&'a str>,
        theme: &'a Theme,
        input_selected: bool,
    ) -> Self {
        Self {
            plugin_name,
            fields,
            values,
            focus,
            error,
            theme,
            input_selected,
        }
    }
}

impl Widget for PluginConnectDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use crate::plugins::provider_api::DialogFieldType;

        // Calculate dialog size based on number of fields
        // Each field takes 1 row, plus title, error, buttons, and padding
        let num_fields = self.fields.len();
        let dialog_height = (num_fields + 6).min(area.height as usize) as u16;
        let dialog_width = 65.min(area.width.saturating_sub(4));

        // Center the dialog
        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

        // Styles
        let bg_color = self.theme.dialog_copy_bg;
        let border_style = Style::default().fg(self.theme.dialog_copy_border);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let label_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let input_style = Style::default().fg(self.theme.dialog_input_unfocused_fg).bg(bg_color);
        let input_focused_style = Style::default().fg(self.theme.dialog_input_focused_fg).bg(self.theme.dialog_input_focused_bg);
        let selected_style = Style::default().fg(self.theme.dialog_input_selected_fg).bg(self.theme.dialog_input_selected_bg);
        let checkbox_style = Style::default().fg(self.theme.dialog_text).bg(bg_color);
        let button_unfocused_style = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);
        let button_focused_style = Style::default().fg(self.theme.dialog_button_focused_fg).bg(self.theme.dialog_button_focused_bg).add_modifier(Modifier::BOLD);
        let error_style = Style::default().fg(self.theme.dialog_warning).bg(bg_color);
        let help_style = Style::default().fg(self.theme.dialog_help).bg(bg_color);

        // Draw border
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                let ch = if row == dialog_area.y {
                    if col == dialog_area.x {
                        '┌'
                    } else if col == dialog_area.x + dialog_area.width - 1 {
                        '┐'
                    } else {
                        '─'
                    }
                } else if row == dialog_area.y + dialog_area.height - 1 {
                    if col == dialog_area.x {
                        '└'
                    } else if col == dialog_area.x + dialog_area.width - 1 {
                        '┘'
                    } else {
                        '─'
                    }
                } else if col == dialog_area.x || col == dialog_area.x + dialog_area.width - 1 {
                    '│'
                } else {
                    ' '
                };
                buf.set_string(col, row, ch.to_string(), border_style);
            }
        }

        // Draw title
        let title = format!(" {} ", self.plugin_name);
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, &title, title_style);

        let content_width = dialog_area.width.saturating_sub(4) as usize;
        let label_width = 16;
        let input_width = content_width.saturating_sub(label_width + 2);

        // Draw fields
        for (i, (field, value)) in self.fields.iter().zip(self.values.iter()).enumerate() {
            let row_y = dialog_area.y + 2 + i as u16;
            if row_y >= dialog_area.y + dialog_area.height - 3 {
                break; // Don't overflow
            }

            // Label
            let label = format!("{:width$}", field.label, width = label_width);
            buf.set_string(dialog_area.x + 2, row_y, &label, label_style);

            let input_x = dialog_area.x + 2 + label_width as u16;
            let is_focused = self.focus == i;

            match &field.field_type {
                DialogFieldType::Text | DialogFieldType::Number | DialogFieldType::TextArea | DialogFieldType::FilePath => {
                    let style = if is_focused {
                        if self.input_selected { selected_style } else { input_focused_style }
                    } else {
                        input_style
                    };

                    let display_value = if value.len() > input_width {
                        &value[value.len() - input_width..]
                    } else {
                        value
                    };
                    let padded = format!("{:width$}", display_value, width = input_width);
                    buf.set_string(input_x, row_y, &padded, style);
                }
                DialogFieldType::Password => {
                    let style = if is_focused {
                        if self.input_selected { selected_style } else { input_focused_style }
                    } else {
                        input_style
                    };

                    let masked: String = "*".repeat(value.len().min(input_width));
                    let padded = format!("{:width$}", masked, width = input_width);
                    buf.set_string(input_x, row_y, &padded, style);
                }
                DialogFieldType::Checkbox => {
                    let checked = value == "true";
                    let checkbox = if checked { "[X]" } else { "[ ]" };
                    let style = if is_focused {
                        button_focused_style
                    } else {
                        checkbox_style
                    };
                    buf.set_string(input_x, row_y, checkbox, style);
                }
                DialogFieldType::Select { options } => {
                    let style = if is_focused { input_focused_style } else { input_style };
                    let display = options
                        .iter()
                        .find(|(v, _)| v == value)
                        .map(|(_, label)| label.as_str())
                        .unwrap_or(value.as_str());
                    let padded = format!("{:width$}", display, width = input_width);
                    buf.set_string(input_x, row_y, &padded, style);
                }
            }
        }

        // Error message
        let error_y = dialog_area.y + dialog_area.height - 4;
        if let Some(err) = self.error {
            let err_display = if err.len() > content_width {
                &err[..content_width]
            } else {
                err
            };
            buf.set_string(dialog_area.x + 2, error_y, err_display, error_style);
        }

        // Buttons
        let button_y = dialog_area.y + dialog_area.height - 3;
        let connect_text = "[ Connect ]";
        let save_text = "[ Save ]";
        let cancel_text = "[ Cancel ]";
        let buttons_width = connect_text.len() + save_text.len() + cancel_text.len() + 4;
        let button_x = dialog_area.x + (dialog_area.width.saturating_sub(buttons_width as u16)) / 2;

        let connect_style = if self.focus == num_fields { button_focused_style } else { button_unfocused_style };
        let save_style = if self.focus == num_fields + 1 { button_focused_style } else { button_unfocused_style };
        let cancel_style = if self.focus == num_fields + 2 { button_focused_style } else { button_unfocused_style };

        buf.set_string(button_x, button_y, connect_text, connect_style);
        buf.set_string(button_x + connect_text.len() as u16 + 2, button_y, save_text, save_style);
        buf.set_string(button_x + connect_text.len() as u16 + save_text.len() as u16 + 4, button_y, cancel_text, cancel_style);

        // Help text
        let help = "Tab=Navigate  Space=Toggle  Enter=OK  Esc=Cancel";
        let help_display: &str = if help.len() > content_width {
            &help[..content_width]
        } else {
            help
        };
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help_display.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + dialog_area.height - 2, help_display, help_style);
    }
}

/// Calculate cursor position for the plugin connect dialog input fields
pub fn plugin_connect_cursor_position(
    area: Rect,
    fields: &[crate::plugins::provider_api::DialogField],
    values: &[String],
    cursors: &[usize],
    focus: usize,
) -> Option<(u16, u16)> {
    use crate::plugins::provider_api::DialogFieldType;

    // Only show cursor for text input fields
    if focus >= fields.len() {
        return None;
    }

    let field = &fields[focus];
    let is_text_field = matches!(
        field.field_type,
        DialogFieldType::Text | DialogFieldType::Password | DialogFieldType::Number | DialogFieldType::TextArea | DialogFieldType::FilePath
    );

    if !is_text_field {
        return None;
    }

    let num_fields = fields.len();
    let dialog_height = (num_fields + 6).min(area.height as usize) as u16;
    let dialog_width = 65.min(area.width.saturating_sub(4));

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let label_width = 16;
    let input_x = x + 2 + label_width as u16;
    let content_width = dialog_width.saturating_sub(4) as usize;
    let input_width = content_width.saturating_sub(label_width + 2);

    let row_y = y + 2 + focus as u16;
    let value = values.get(focus).map(|s| s.as_str()).unwrap_or("");
    let cursor = cursors.get(focus).copied().unwrap_or(0);
    let cursor_pos = cursor.min(value.len()).min(input_width.saturating_sub(1));

    Some((input_x + cursor_pos as u16, row_y))
}

/// Overwrite confirmation dialog
pub struct OverwriteConfirmDialog<'a> {
    filename: &'a str,
    current: usize,
    total: usize,
    focus: usize,
    theme: &'a Theme,
}

impl<'a> OverwriteConfirmDialog<'a> {
    pub fn new(filename: &'a str, current: usize, total: usize, focus: usize, theme: &'a Theme) -> Self {
        Self { filename, current, total, focus, theme }
    }
}

impl Widget for OverwriteConfirmDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = self.theme;

        let dialog_width = 56u16.min(area.width.saturating_sub(4));
        let dialog_height = 8u16;

        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
        let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

        let dialog_bg = Style::default()
            .bg(theme.dialog_delete_bg)
            .fg(theme.dialog_text);
        let title_style = Style::default()
            .fg(theme.dialog_title)
            .bg(theme.dialog_delete_bg)
            .add_modifier(Modifier::BOLD);
        let button_style = Style::default()
            .fg(theme.dialog_button_unfocused)
            .bg(theme.dialog_delete_bg);
        let button_focused_style = Style::default()
            .fg(theme.dialog_button_focused_fg)
            .bg(theme.dialog_button_focused_bg)
            .add_modifier(Modifier::BOLD);
        let help_style = Style::default()
            .fg(theme.dialog_help)
            .bg(theme.dialog_delete_bg);
        let border_style = Style::default()
            .fg(theme.dialog_delete_border)
            .bg(theme.dialog_delete_bg);

        // Clear area
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_style(dialog_bg);
                buf[(col, row)].set_char(' ');
            }
        }

        // Border
        buf[(dialog_area.x, dialog_area.y)].set_char('╭').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('╮').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }
        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('╰').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('╯').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }
        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = " Overwrite ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        // Filename (truncated to fit)
        let inner_width = (dialog_width - 4) as usize;
        let display_name: String = self.filename.chars().take(inner_width).collect();
        let name_x = dialog_area.x + (dialog_area.width.saturating_sub(display_name.len() as u16)) / 2;
        buf.set_string(name_x, dialog_area.y + 2, &display_name, dialog_bg);

        // Conflict count
        let count_str = format!("({} of {})", self.current + 1, self.total);
        let count_x = dialog_area.x + (dialog_area.width.saturating_sub(count_str.len() as u16)) / 2;
        buf.set_string(count_x, dialog_area.y + 3, &count_str, dialog_bg);

        // Buttons: [Yes] [All] [Skip] [Skip All] [Cancel]
        let buttons = ["[Yes]", "[All]", "[Skip]", "[Skip All]", "[Cancel]"];
        let total_btn_width: usize = buttons.iter().map(|b| b.len()).sum::<usize>() + (buttons.len() - 1);
        let btn_x = dialog_area.x + (dialog_area.width.saturating_sub(total_btn_width as u16)) / 2;
        let mut cx = btn_x;
        for (i, btn) in buttons.iter().enumerate() {
            let style = if i == self.focus { button_focused_style } else { button_style };
            buf.set_string(cx, dialog_area.y + 5, btn, style);
            cx += btn.len() as u16 + 1;
        }

        // Help text
        let help = "Y/A/S/N/C, Tab, Enter, Esc";
        if dialog_width > help.len() as u16 + 4 {
            let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help.len() as u16)) / 2;
            buf.set_string(help_x, dialog_area.y + dialog_area.height - 1, help, help_style);
        }
    }
}

/// Permissions editing dialog (Unix only)
#[cfg(not(windows))]
pub struct EditPermissionsDialog<'a> {
    paths: &'a [PathBuf],
    owner: &'a str,
    group: &'a str,
    perms: [bool; 9],
    apply_recursive: bool,
    has_dirs: bool,
    focus: usize,
    theme: &'a Theme,
}

#[cfg(not(windows))]
impl<'a> EditPermissionsDialog<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        paths: &'a [PathBuf],
        owner: &'a str,
        group: &'a str,
        perms: [bool; 9],
        apply_recursive: bool,
        has_dirs: bool,
        focus: usize,
        theme: &'a Theme,
    ) -> Self {
        Self { paths, owner, group, perms, apply_recursive, has_dirs, focus, theme }
    }
}

#[cfg(not(windows))]
impl Widget for EditPermissionsDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog_width: u16 = 42;
        let dialog_height: u16 = if self.has_dirs { 13 } else { 11 };

        if area.width < 30 || area.height < dialog_height {
            return;
        }

        let dialog_area = Rect {
            x: area.x + (area.width.saturating_sub(dialog_width)) / 2,
            y: area.y + (area.height.saturating_sub(dialog_height)) / 2,
            width: dialog_width.min(area.width.saturating_sub(4)),
            height: dialog_height,
        };

        let bg_color = self.theme.dialog_move_bg;
        let border_color = self.theme.dialog_move_border;

        let border_style = Style::default().fg(border_color);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let label_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let checkbox_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let checkbox_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let button_focused = Style::default().fg(self.theme.dialog_button_focused_fg).bg(self.theme.dialog_button_focused_bg).add_modifier(Modifier::BOLD);
        let button_unfocused = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help);
        let bg_style = Style::default().bg(bg_color);

        // Background
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Border
        buf[(dialog_area.x, dialog_area.y)].set_char('┌').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('┐').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }
        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('└').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('┘').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }
        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = " Permissions ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        let cx = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;

        // File info (line 2)
        let first_name = self.paths.first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let info = format!("{} ({}/{})", first_name, self.owner, self.group);
        let display_info = if info.len() > content_width {
            format!("{}…", &info[..content_width.saturating_sub(1)])
        } else {
            info
        };
        buf.set_string(cx, dialog_area.y + 2, &display_info, label_style);

        // Extra files count
        if self.paths.len() > 1 {
            let extra = format!("(+ {} more files)", self.paths.len() - 1);
            buf.set_string(cx, dialog_area.y + 3, &extra, label_style);
        }

        // Permission rows start at y+4
        let perm_y = dialog_area.y + 4;
        let labels = ["Owner:", "Group:", "Other:"];
        let bits = ["r", "w", "x"];

        for (row_idx, row_label) in labels.iter().enumerate() {
            let y = perm_y + row_idx as u16;
            buf.set_string(cx, y, row_label, label_style);

            for col_idx in 0..3 {
                let perm_idx = row_idx * 3 + col_idx;
                let checked = self.perms[perm_idx];
                let focused = self.focus == perm_idx;
                let style = if focused { checkbox_focused } else { checkbox_unfocused };
                let checkbox = if checked { "[x]" } else { "[ ]" };
                let text = format!("{}{}", checkbox, bits[col_idx]);
                let x = cx + 9 + (col_idx as u16 * 6);
                buf.set_string(x, y, &text, style);
            }
        }

        // Recursive checkbox (only if has_dirs)
        let mut next_y = perm_y + 2; // last perm row (Other)
        if self.has_dirs {
            next_y += 2; // 1 blank line after Other row
            let focused = self.focus == 9;
            let style = if focused { checkbox_focused } else { checkbox_unfocused };
            let checkbox = if self.apply_recursive { "[x]" } else { "[ ]" };
            buf.set_string(cx, next_y, &format!("{} Apply recursively", checkbox), style);
        }

        // Buttons (1 blank line after last content)
        let ok_focus = if self.has_dirs { 10 } else { 9 };
        let cancel_focus = ok_focus + 1;
        let button_y = next_y + 2;
        let ok_style = if self.focus == ok_focus { button_focused } else { button_unfocused };
        let cancel_style = if self.focus == cancel_focus { button_focused } else { button_unfocused };
        let btn_x = dialog_area.x + (dialog_area.width.saturating_sub(20)) / 2;
        buf.set_string(btn_x, button_y, "[ OK ]", ok_style);
        buf.set_string(btn_x + 10, button_y, "[ Cancel ]", cancel_style);

        // Help text on bottom border
        let help = " Tab=Switch Space=Toggle ";
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + dialog_area.height - 1, help, help_style);
    }
}

/// Owner/Group editing dialog (Unix only)
#[cfg(not(windows))]
pub struct EditOwnerDialog<'a> {
    paths: &'a [PathBuf],
    current_owner: &'a str,
    current_group: &'a str,
    users: &'a [String],
    groups: &'a [String],
    user_selected: usize,
    user_scroll: usize,
    group_selected: usize,
    group_scroll: usize,
    apply_recursive: bool,
    has_dirs: bool,
    focus: usize,
    theme: &'a Theme,
}

#[cfg(not(windows))]
impl<'a> EditOwnerDialog<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        paths: &'a [PathBuf],
        current_owner: &'a str,
        current_group: &'a str,
        users: &'a [String],
        groups: &'a [String],
        user_selected: usize,
        user_scroll: usize,
        group_selected: usize,
        group_scroll: usize,
        apply_recursive: bool,
        has_dirs: bool,
        focus: usize,
        theme: &'a Theme,
    ) -> Self {
        Self {
            paths, current_owner, current_group,
            users, groups,
            user_selected, user_scroll,
            group_selected, group_scroll,
            apply_recursive, has_dirs, focus, theme,
        }
    }
}

#[cfg(not(windows))]
impl Widget for EditOwnerDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let list_height: u16 = 6;
        let dialog_width: u16 = 50;
        let dialog_height: u16 = if self.has_dirs { 10 + list_height } else { 8 + list_height };

        if area.width < 40 || area.height < dialog_height {
            return;
        }

        let dialog_area = Rect {
            x: area.x + (area.width.saturating_sub(dialog_width)) / 2,
            y: area.y + (area.height.saturating_sub(dialog_height)) / 2,
            width: dialog_width.min(area.width.saturating_sub(4)),
            height: dialog_height,
        };

        let bg_color = self.theme.dialog_move_bg;
        let border_color = self.theme.dialog_move_border;

        let border_style = Style::default().fg(border_color);
        let title_style = Style::default().bg(bg_color).fg(self.theme.dialog_title).add_modifier(Modifier::BOLD);
        let label_style = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let list_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let list_selected = Style::default().bg(self.theme.dialog_button_focused_bg).fg(self.theme.dialog_button_focused_fg).add_modifier(Modifier::BOLD);
        let list_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let checkbox_focused = Style::default().bg(self.theme.dialog_input_focused_bg).fg(self.theme.dialog_input_focused_fg);
        let checkbox_unfocused = Style::default().bg(bg_color).fg(self.theme.dialog_text);
        let button_focused = Style::default().fg(self.theme.dialog_button_focused_fg).bg(self.theme.dialog_button_focused_bg).add_modifier(Modifier::BOLD);
        let button_unfocused = Style::default().fg(self.theme.dialog_button_unfocused).bg(bg_color);
        let help_style = Style::default().bg(bg_color).fg(self.theme.dialog_help);
        let bg_style = Style::default().bg(bg_color);

        // Background
        for row in dialog_area.y..dialog_area.y + dialog_area.height {
            for col in dialog_area.x..dialog_area.x + dialog_area.width {
                buf[(col, row)].set_char(' ').set_style(bg_style);
            }
        }

        // Border
        buf[(dialog_area.x, dialog_area.y)].set_char('┌').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y)].set_char('┐').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y)].set_char('─').set_style(border_style);
        }
        buf[(dialog_area.x, dialog_area.y + dialog_area.height - 1)].set_char('└').set_style(border_style);
        buf[(dialog_area.x + dialog_area.width - 1, dialog_area.y + dialog_area.height - 1)].set_char('┘').set_style(border_style);
        for col in dialog_area.x + 1..dialog_area.x + dialog_area.width - 1 {
            buf[(col, dialog_area.y + dialog_area.height - 1)].set_char('─').set_style(border_style);
        }
        for row in dialog_area.y + 1..dialog_area.y + dialog_area.height - 1 {
            buf[(dialog_area.x, row)].set_char('│').set_style(border_style);
            buf[(dialog_area.x + dialog_area.width - 1, row)].set_char('│').set_style(border_style);
        }

        // Title
        let title = " Owner / Group ";
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, title, title_style);

        let cx = dialog_area.x + 2;
        let content_width = dialog_area.width.saturating_sub(4) as usize;

        // File info (line 2)
        let first_name = self.paths.first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let info = format!("{} ({}/{})", first_name, self.current_owner, self.current_group);
        let display_info = if info.len() > content_width {
            format!("{}…", &info[..content_width.saturating_sub(1)])
        } else {
            info
        };
        buf.set_string(cx, dialog_area.y + 2, &display_info, label_style);

        // Extra files count
        let mut y = dialog_area.y + 3;
        if self.paths.len() > 1 {
            let extra = format!("(+ {} more files)", self.paths.len() - 1);
            buf.set_string(cx, y, &extra, label_style);
            y += 1;
        }

        // Column headers
        let col1_x = cx;
        let col2_x = cx + (content_width as u16) / 2;
        let col1_width = ((content_width as u16) / 2).saturating_sub(1) as usize;
        let col2_width = col1_width;
        buf.set_string(col1_x, y, "Owner:", label_style);
        buf.set_string(col2_x, y, "Group:", label_style);
        y += 1;

        // Render user list
        for i in 0..list_height as usize {
            let idx = self.user_scroll + i;
            let row_y = y + i as u16;
            if idx < self.users.len() {
                let name = &self.users[idx];
                let is_selected = idx == self.user_selected;
                let style = if is_selected && self.focus == 0 {
                    list_selected
                } else if is_selected {
                    list_focused
                } else if self.focus == 0 {
                    list_focused.bg(bg_color)
                } else {
                    list_unfocused
                };
                let marker = if is_selected { "▸" } else { " " };
                let display = format!("{}{}", marker, name);
                let display = if display.len() > col1_width {
                    format!("{}…", &display[..col1_width.saturating_sub(1)])
                } else {
                    format!("{:<width$}", display, width = col1_width)
                };
                buf.set_string(col1_x, row_y, &display, style);
            }
        }

        // Render group list
        for i in 0..list_height as usize {
            let idx = self.group_scroll + i;
            let row_y = y + i as u16;
            if idx < self.groups.len() {
                let name = &self.groups[idx];
                let is_selected = idx == self.group_selected;
                let style = if is_selected && self.focus == 1 {
                    list_selected
                } else if is_selected {
                    list_focused
                } else if self.focus == 1 {
                    list_focused.bg(bg_color)
                } else {
                    list_unfocused
                };
                let marker = if is_selected { "▸" } else { " " };
                let display = format!("{}{}", marker, name);
                let display = if display.len() > col2_width {
                    format!("{}…", &display[..col2_width.saturating_sub(1)])
                } else {
                    format!("{:<width$}", display, width = col2_width)
                };
                buf.set_string(col2_x, row_y, &display, style);
            }
        }

        y += list_height;

        // Recursive checkbox (only if has_dirs)
        if self.has_dirs {
            y += 1; // 1 blank line after lists
            let focused = self.focus == 2;
            let style = if focused { checkbox_focused } else { checkbox_unfocused };
            let checkbox = if self.apply_recursive { "[x]" } else { "[ ]" };
            buf.set_string(cx, y, &format!("{} Apply recursively", checkbox), style);
            y += 1;
        }

        // Buttons (1 blank line after last content)
        let ok_focus = if self.has_dirs { 3 } else { 2 };
        let cancel_focus = ok_focus + 1;
        let button_y = y + 1;
        let ok_style = if self.focus == ok_focus { button_focused } else { button_unfocused };
        let cancel_style = if self.focus == cancel_focus { button_focused } else { button_unfocused };
        let btn_x = dialog_area.x + (dialog_area.width.saturating_sub(20)) / 2;
        buf.set_string(btn_x, button_y, "[ OK ]", ok_style);
        buf.set_string(btn_x + 10, button_y, "[ Cancel ]", cancel_style);

        // Help text on bottom border
        let help = " Tab=Switch ↑↓=Select ";
        let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help.len() as u16)) / 2;
        buf.set_string(help_x, dialog_area.y + dialog_area.height - 1, help, help_style);
    }
}

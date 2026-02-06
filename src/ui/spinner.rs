//! Spinner widget for background operations

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use super::Theme;

/// Spinner animation frames (Braille dots pattern)
const SPINNER_FRAMES_UNICODE: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// ASCII-safe spinner frames for Windows 10 (console font may lack Braille)
#[allow(dead_code)]
const SPINNER_FRAMES_ASCII: &[&str] = &["|", "/", "-", "\\", "|", "/", "-", "\\"];

/// Pick the right spinner frames for the current platform.
fn spinner_frames() -> &'static [&'static str] {
    #[cfg(windows)]
    if crate::persistent_shell::is_windows_10_or_older() {
        return SPINNER_FRAMES_ASCII;
    }
    SPINNER_FRAMES_UNICODE
}

/// A spinner widget that shows an animated indicator with a message
#[allow(dead_code)]
pub struct Spinner<'a> {
    /// Current animation frame (0-9)
    frame: usize,
    /// Message to display next to spinner
    message: &'a str,
    /// Style for the spinner character
    spinner_style: Style,
    /// Style for the message text
    message_style: Style,
}

#[allow(dead_code)]
impl<'a> Spinner<'a> {
    pub fn new(frame: usize, message: &'a str) -> Self {
        Self {
            frame: frame % spinner_frames().len(),
            message,
            spinner_style: Style::default(),
            message_style: Style::default(),
        }
    }

    pub fn spinner_style(mut self, style: Style) -> Self {
        self.spinner_style = style;
        self
    }

    pub fn message_style(mut self, style: Style) -> Self {
        self.message_style = style;
        self
    }
}

impl Widget for Spinner<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 3 || area.height < 1 {
            return;
        }

        // Get spinner character for current frame
        let spinner_char = spinner_frames()[self.frame];

        // Render spinner
        buf.set_string(area.x, area.y, spinner_char, self.spinner_style);

        // Render message with a space after spinner
        if area.width > 2 && !self.message.is_empty() {
            let msg_area_width = (area.width - 2) as usize;
            let display_msg: String = self.message.chars().take(msg_area_width).collect();
            buf.set_string(area.x + 2, area.y, &display_msg, self.message_style);
        }
    }
}

/// A centered spinner overlay dialog
pub struct SpinnerDialog<'a> {
    /// Current animation frame
    frame: usize,
    /// Title of the dialog
    title: &'a str,
    /// Message to display
    message: &'a str,
    /// Border style
    border_style: Style,
    /// Content style
    content_style: Style,
}

impl<'a> SpinnerDialog<'a> {
    pub fn new(frame: usize, title: &'a str, message: &'a str) -> Self {
        Self {
            frame: frame % spinner_frames().len(),
            title,
            message,
            border_style: Style::default(),
            content_style: Style::default(),
        }
    }

    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    pub fn content_style(mut self, style: Style) -> Self {
        self.content_style = style;
        self
    }
}

impl Widget for SpinnerDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Split message into lines
        let lines: Vec<&str> = self.message.lines().collect();
        let msg_lines = lines.len().max(1);

        // Calculate dialog size
        let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let title_len = self.title.chars().count();
        let content_width = max_line_len.max(title_len) + 4; // +4 for spinner + padding
        let dialog_width = (content_width + 4).min(area.width as usize) as u16; // +4 for borders
        // border + padding + spinner line + extra lines + help line + border
        let dialog_height = (3 + msg_lines as u16 + 2).max(5);

        if area.width < dialog_width || area.height < dialog_height {
            return;
        }

        // Center the dialog
        let x = area.x + (area.width - dialog_width) / 2;
        let y = area.y + (area.height - dialog_height) / 2;

        // Fill background
        for row in y..y + dialog_height {
            for col in x..x + dialog_width {
                buf[(col, row)].set_char(' ').set_style(self.content_style);
            }
        }

        // Draw border
        // Top
        buf[(x, y)].set_char('┌').set_style(self.border_style);
        buf[(x + dialog_width - 1, y)].set_char('┐').set_style(self.border_style);
        for col in x + 1..x + dialog_width - 1 {
            buf[(col, y)].set_char('─').set_style(self.border_style);
        }

        // Bottom
        buf[(x, y + dialog_height - 1)].set_char('└').set_style(self.border_style);
        buf[(x + dialog_width - 1, y + dialog_height - 1)].set_char('┘').set_style(self.border_style);
        for col in x + 1..x + dialog_width - 1 {
            buf[(col, y + dialog_height - 1)].set_char('─').set_style(self.border_style);
        }

        // Sides
        for row in y + 1..y + dialog_height - 1 {
            buf[(x, row)].set_char('│').set_style(self.border_style);
            buf[(x + dialog_width - 1, row)].set_char('│').set_style(self.border_style);
        }

        // Title centered on top border
        if !self.title.is_empty() {
            let title_with_padding = format!(" {} ", self.title);
            let title_x = x + (dialog_width - title_with_padding.len() as u16) / 2;
            buf.set_string(title_x, y, &title_with_padding, self.border_style);
        }

        // Spinner and first line centered
        let spinner_char = spinner_frames()[self.frame];
        let first_line = lines.first().copied().unwrap_or("");
        let content = format!("{} {}", spinner_char, first_line);
        let content_x = x + (dialog_width.saturating_sub(content.chars().count() as u16)) / 2;
        let content_y = y + 2;
        buf.set_string(content_x, content_y, &content, self.content_style);

        // Additional lines (e.g., elapsed time)
        for (i, line) in lines.iter().skip(1).enumerate() {
            let line_x = x + (dialog_width.saturating_sub(line.chars().count() as u16)) / 2;
            let line_y = content_y + 1 + i as u16;
            if line_y < y + dialog_height - 1 {
                buf.set_string(line_x, line_y, line, self.content_style);
            }
        }

        // "Esc = Cancel" on bottom border
        let help = "Esc = Cancel";
        if dialog_width > help.len() as u16 + 4 {
            let help_x = x + (dialog_width.saturating_sub(help.len() as u16)) / 2;
            buf.set_string(help_x, y + dialog_height - 1, help, self.border_style);
        }
    }
}

/// Format bytes into human-readable size
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// File operation progress dialog with spinner, progress bar, and file info
pub struct FileOpProgressDialog<'a> {
    frame: usize,
    title: &'a str,
    current_file: &'a str,
    bytes_done: u64,
    bytes_total: u64,
    files_done: usize,
    files_total: usize,
    theme: &'a Theme,
}

impl<'a> FileOpProgressDialog<'a> {
    pub fn new(
        frame: usize,
        title: &'a str,
        current_file: &'a str,
        bytes_done: u64,
        bytes_total: u64,
        files_done: usize,
        files_total: usize,
        theme: &'a Theme,
    ) -> Self {
        Self {
            frame: frame % spinner_frames().len(),
            title,
            current_file,
            bytes_done,
            bytes_total,
            files_done,
            files_total,
            theme,
        }
    }
}

impl Widget for FileOpProgressDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = self.theme;
        let dialog_width = 52u16.min(area.width.saturating_sub(4));
        let dialog_height = 9u16;

        let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
        let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

        let dialog_bg = Style::default()
            .bg(theme.dialog_copy_bg)
            .fg(theme.dialog_text);
        let title_style = Style::default()
            .fg(theme.dialog_title)
            .bg(theme.dialog_copy_bg)
            .add_modifier(Modifier::BOLD);
        let border_style = Style::default()
            .fg(theme.dialog_copy_border)
            .bg(theme.dialog_copy_bg);
        let help_style = Style::default()
            .fg(theme.dialog_help)
            .bg(theme.dialog_copy_bg);
        let bar_filled = Style::default()
            .fg(theme.dialog_copy_bg)
            .bg(theme.panel_border_active);
        let bar_empty = Style::default()
            .fg(theme.dialog_text)
            .bg(theme.dialog_copy_bg);

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
        let title = format!(" {} {} ", spinner_frames()[self.frame], self.title);
        let title_x = dialog_area.x + (dialog_area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, dialog_area.y, &title, title_style);

        // Current file (truncated)
        let inner_width = (dialog_width - 4) as usize;
        let display_name: String = self.current_file.chars().take(inner_width).collect();
        let name_x = dialog_area.x + 2;
        buf.set_string(name_x, dialog_area.y + 2, &display_name, dialog_bg);

        // Progress bar: [████░░░░░░] 45%
        let bar_area_width = inner_width.saturating_sub(6); // room for " 100%"
        let pct = if self.bytes_total > 0 {
            ((self.bytes_done as f64 / self.bytes_total as f64) * 100.0).min(100.0)
        } else {
            0.0
        };
        let filled = ((pct / 100.0) * bar_area_width as f64) as usize;
        let empty = bar_area_width.saturating_sub(filled);

        let bar_x = dialog_area.x + 2;
        let bar_y = dialog_area.y + 4;

        // Render filled portion
        for i in 0..filled {
            if bar_x + i as u16 >= dialog_area.x + dialog_area.width - 1 { break; }
            buf[(bar_x + i as u16, bar_y)].set_char('█').set_style(bar_filled);
        }
        // Render empty portion
        for i in 0..empty {
            let col = bar_x + (filled + i) as u16;
            if col >= dialog_area.x + dialog_area.width - 1 { break; }
            buf[(col, bar_y)].set_char('░').set_style(bar_empty);
        }
        // Percentage
        let pct_str = format!(" {:3.0}%", pct);
        let pct_x = bar_x + bar_area_width as u16;
        buf.set_string(pct_x, bar_y, &pct_str, dialog_bg);

        // Size info: "12.3 MB / 27.1 MB"
        let size_str = format!("{} / {}", format_bytes(self.bytes_done), format_bytes(self.bytes_total));
        let size_x = dialog_area.x + (dialog_area.width.saturating_sub(size_str.len() as u16)) / 2;
        buf.set_string(size_x, dialog_area.y + 5, &size_str, dialog_bg);

        // File count: "File 3 of 7"
        let count_str = format!("File {} of {}", self.files_done + 1, self.files_total);
        let count_x = dialog_area.x + (dialog_area.width.saturating_sub(count_str.len() as u16)) / 2;
        buf.set_string(count_x, dialog_area.y + 6, &count_str, dialog_bg);

        // Help text
        let help = "Esc = Cancel";
        if dialog_width > help.len() as u16 + 4 {
            let help_x = dialog_area.x + (dialog_area.width.saturating_sub(help.len() as u16)) / 2;
            buf.set_string(help_x, dialog_area.y + dialog_area.height - 1, help, help_style);
        }
    }
}

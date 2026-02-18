//! Status bar and function key bar widgets

use std::time::SystemTime;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use crate::state::panel::Panel;
use crate::fs::FileEntry;
use crate::git::GitStatus;
use super::Theme;

/// Status bar showing selected file information
pub struct StatusBar<'a> {
    panel: &'a Panel,
    git_status: Option<&'a GitStatus>,
    python_env: Option<&'a str>,
    quick_search: Option<&'a str>,
    plugin_status: Option<&'a [(String, String)]>,
    theme: &'a Theme,
}

impl<'a> StatusBar<'a> {
    pub fn new(panel: &'a Panel, theme: &'a Theme) -> Self {
        Self { panel, git_status: None, python_env: None, quick_search: None, plugin_status: None, theme }
    }

    pub fn with_git(mut self, git_status: Option<&'a GitStatus>) -> Self {
        self.git_status = git_status;
        self
    }

    pub fn with_python_env(mut self, python_env: Option<&'a String>) -> Self {
        self.python_env = python_env.map(|s| s.as_str());
        self
    }

    #[allow(dead_code)]
    pub fn with_quick_search(mut self, search: Option<&'a String>) -> Self {
        self.quick_search = search.map(|s| s.as_str());
        self
    }

    pub fn with_plugin_status(mut self, status: Option<&'a [(String, String)]>) -> Self {
        self.plugin_status = status;
        self
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        let style = Style::default().bg(self.theme.status_bg).fg(self.theme.status_fg);

        // Clear the line with background
        for x in area.x..area.x + area.width {
            buf[(x, area.y)].set_char(' ').set_style(style);
        }

        let mut x_offset: u16 = 0;

        // Render quick search at the very beginning if active
        if let Some(search) = self.quick_search {
            let search_style = Style::default()
                .bg(self.theme.cursor_bg)
                .fg(self.theme.cursor_fg)
                .add_modifier(Modifier::BOLD);
            let display = format!(" Search: {}_ ", search);
            buf.set_string(area.x, area.y, &display, search_style);
            x_offset = display.len() as u16;
        }

        // Render git status (after quick search if present)
        let mut has_left_info = false;
        if let Some(git) = self.git_status {
            let git_str = git.format();
            if !git_str.is_empty() {
                let git_style = Style::default()
                    .bg(self.theme.status_bg)
                    .fg(self.theme.git_clean)
                    .add_modifier(Modifier::BOLD);
                let dirty_style = Style::default()
                    .bg(self.theme.status_bg)
                    .fg(self.theme.git_dirty)
                    .add_modifier(Modifier::BOLD);

                // Use dirty style if there are changes
                let actual_style = if git.dirty || git.staged || git.untracked {
                    dirty_style
                } else {
                    git_style
                };

                let display = format!(" {} ", git_str);
                buf.set_string(area.x + x_offset, area.y, &display, actual_style);
                x_offset += display.len() as u16;
                has_left_info = true;
            }
        }

        // Render Python env (after git status, separated by bullet)
        if let Some(env_name) = self.python_env {
            let py_style = Style::default()
                .bg(self.theme.status_bg)
                .fg(self.theme.status_python_env)
                .add_modifier(Modifier::BOLD);

            let display = if has_left_info {
                format!("\u{2022} env: {} ", env_name)
            } else {
                format!(" env: {} ", env_name)
            };
            buf.set_string(area.x + x_offset, area.y, &display, py_style);
            x_offset += display.chars().count() as u16;
        }

        // Calculate plugin width (will be rendered on the right)
        let mut plugin_total_width: u16 = 0;
        let mut plugin_strings: Vec<String> = Vec::new();
        if let Some(plugin_outputs) = self.plugin_status {
            for (_name, text) in plugin_outputs {
                if !text.is_empty() {
                    let display = format!("│ {} ", text);
                    plugin_total_width += display.chars().count() as u16;
                    plugin_strings.push(display);
                }
            }
        }

        // Show error if present
        if let Some(ref error) = self.panel.error {
            let error_style = Style::default().bg(self.theme.status_error_bg).fg(self.theme.status_error_fg);
            let msg = format!(" {} ", error);
            let max_width = (area.width as usize).saturating_sub(x_offset as usize).saturating_sub(plugin_total_width as usize);
            let truncated = truncate_str(&msg, max_width);
            buf.set_string(area.x + x_offset, area.y, truncated, error_style);
            // Still render plugins on the right
            render_plugins_right(area, buf, &plugin_strings, style);
            return;
        }

        // Show selected file info
        let Some(entry) = self.panel.selected() else {
            // Still render plugins on the right even if no file selected
            render_plugins_right(area, buf, &plugin_strings, style);
            return;
        };

        // Adjust area to account for git status at the beginning and plugins on the right
        // Ensure at least 40 characters for path
        let available_width = area.width.saturating_sub(x_offset).saturating_sub(plugin_total_width);
        let info_area = Rect {
            x: area.x + x_offset,
            y: area.y,
            width: available_width,
            height: area.height,
        };

        render_file_info(entry, info_area, buf, style);

        // Render plugins on the right side
        render_plugins_right(area, buf, &plugin_strings, style);
    }
}

/// Render plugin status outputs on the right side of the status bar
fn render_plugins_right(area: Rect, buf: &mut Buffer, plugin_strings: &[String], style: Style) {
    if plugin_strings.is_empty() {
        return;
    }

    // Calculate total width
    let total_width: u16 = plugin_strings.iter()
        .map(|s| s.chars().count() as u16)
        .sum();

    // Start position for plugins (right-aligned)
    let start_x = area.x + area.width.saturating_sub(total_width);
    let mut x = start_x;

    for display in plugin_strings {
        buf.set_string(x, area.y, display, style);
        x += display.chars().count() as u16;
    }
}

/// Column widths for status bar fields
const DATE_WIDTH: usize = 16;      // YYYY-MM-DD HH:MM
const SEPARATOR: &str = " │ ";     // Field separator

#[cfg(unix)]
const OWNER_WIDTH: usize = 17;     // username:group

/// Render file information with fixed columns into the buffer
/// Unix:    owner:group | date | path
/// Windows: date | path
fn render_file_info(entry: &FileEntry, area: Rect, buf: &mut Buffer, style: Style) {
    let width = area.width as usize;
    if width < 20 {
        return;
    }

    let sep_len = SEPARATOR.chars().count();

    #[cfg(unix)]
    let fixed_width = 1 + OWNER_WIDTH + sep_len + DATE_WIDTH + sep_len;
    #[cfg(windows)]
    let fixed_width = 1 + DATE_WIDTH + sep_len;

    // Path gets remaining space
    let path_width = width.saturating_sub(fixed_width);

    // If not enough space for even a short path, just show truncated path
    if path_width < 10 {
        let path_str = entry.path.to_string_lossy();
        let truncated = truncate_path(&path_str, width.saturating_sub(1));
        buf.set_string(area.x + 1, area.y, &truncated, style);
        return;
    }

    let mut x = area.x;

    #[cfg(unix)]
    {
        // Owner:group (left-aligned)
        let owner_str = if entry.owner.is_empty() && entry.group.is_empty() {
            "-".to_string()
        } else {
            format!("{}:{}", entry.owner, entry.group)
        };
        let owner_display = format!(" {:<width$}", truncate_str(&owner_str, OWNER_WIDTH - 1), width = OWNER_WIDTH - 1);
        buf.set_string(x, area.y, &owner_display, style);
        x += OWNER_WIDTH as u16;

        // Separator
        buf.set_string(x, area.y, SEPARATOR, style);
        x += sep_len as u16;
    }

    #[cfg(windows)]
    {
        // Just a leading space
        buf.set_string(x, area.y, " ", style);
        x += 1;
    }

    // Date (fixed width)
    let date_str = format_date(entry.modified);
    buf.set_string(x, area.y, &date_str, style);
    x += DATE_WIDTH as u16;

    // Separator
    buf.set_string(x, area.y, SEPARATOR, style);
    x += sep_len as u16;

    // Path (takes remaining space, truncated showing end)
    let path_str = entry.path.to_string_lossy();
    let path_display = truncate_path(&path_str, path_width);
    buf.set_string(x, area.y, &path_display, style);
}

/// Truncate a string to max_width (keeps the start)
fn truncate_str(s: &str, max_width: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_width {
        s.to_string()
    } else if max_width <= 1 {
        "…".to_string()
    } else {
        let mut result: String = s.chars().take(max_width - 1).collect();
        result.push('…');
        result
    }
}

/// Truncate a path to max_width (keeps the end, showing filename)
fn truncate_path(s: &str, max_width: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_width {
        s.to_string()
    } else if max_width <= 1 {
        "…".to_string()
    } else {
        let skip = char_count - max_width + 1;
        let mut result = String::from("…");
        result.extend(s.chars().skip(skip));
        result
    }
}

/// Format a date for display
fn format_date(time: Option<SystemTime>) -> String {
    let Some(time) = time else {
        return "---------- --:--".to_string();
    };

    let Ok(duration) = time.duration_since(SystemTime::UNIX_EPOCH) else {
        return "---------- --:--".to_string();
    };

    let secs = duration.as_secs();

    const SECS_PER_MIN: u64 = 60;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_DAY: u64 = 86400;

    let days_since_epoch = secs / SECS_PER_DAY;
    let time_of_day = secs % SECS_PER_DAY;
    let hours = time_of_day / SECS_PER_HOUR;
    let minutes = (time_of_day % SECS_PER_HOUR) / SECS_PER_MIN;

    let mut year = 1970i64;
    let mut remaining_days = days_since_epoch as i64;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months: [i64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0;
    for (i, &days) in days_in_months.iter().enumerate() {
        if remaining_days < days {
            month = i + 1;
            break;
        }
        remaining_days -= days;
    }
    let day = remaining_days + 1;

    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hours, minutes)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

//! Panel widget for displaying file listings

use std::path::Path;
use std::time::SystemTime;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, StatefulWidget, Widget},
};

#[cfg(windows)]
use crate::utils::get_drive_letter;
use crate::state::panel::{Panel, ViewMode};
use crate::fs::FileEntry;
use super::Theme;

/// Get free space for the filesystem containing the given path
#[cfg(unix)]
fn get_free_space(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path_cstr = CString::new(path.as_os_str().as_bytes()).ok()?;

    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path_cstr.as_ptr(), &mut stat) == 0 {
            // Free space = free blocks * block size
            Some(stat.f_bavail as u64 * stat.f_frsize as u64)
        } else {
            None
        }
    }
}

#[cfg(windows)]
fn get_free_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;

    // Use the path directly - Windows will resolve to the correct drive
    let path_str: Vec<u16> = path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut free_bytes: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free_bytes: u64 = 0;

    unsafe {
        unsafe extern "system" {
            fn GetDiskFreeSpaceExW(
                lpDirectoryName: *const u16,
                lpFreeBytesAvailableToCaller: *mut u64,
                lpTotalNumberOfBytes: *mut u64,
                lpTotalNumberOfFreeBytes: *mut u64,
            ) -> i32;
        }

        if GetDiskFreeSpaceExW(
            path_str.as_ptr(),
            &mut free_bytes,
            &mut total_bytes,
            &mut total_free_bytes,
        ) != 0 {
            Some(free_bytes)
        } else {
            None
        }
    }
}

/// Height of the drive line at the top of each panel
const DRIVE_LINE_HEIGHT: u16 = 1;

/// Height of the footer area (separator line + footer text line)
const FOOTER_HEIGHT: u16 = 2;

/// Widget for rendering a file panel
pub struct PanelWidget<'a> {
    is_active: bool,
    theme: &'a Theme,
    dir_sizes: Option<&'a std::collections::HashMap<std::path::PathBuf, u64>>,
}

impl<'a> PanelWidget<'a> {
    pub fn new(is_active: bool, theme: &'a Theme) -> Self {
        Self { is_active, theme, dir_sizes: None }
    }

    pub fn with_dir_sizes(mut self, sizes: &'a std::collections::HashMap<std::path::PathBuf, u64>) -> Self {
        self.dir_sizes = Some(sizes);
        self
    }

    /// Format the panel path (replacing $HOME with ~)
    fn format_path(panel: &Panel) -> String {
        let path_str = panel.path.to_string_lossy();

        // Replace home directory with ~
        let display_path = if let Ok(home) = std::env::var("HOME") {
            if path_str.starts_with(&home) {
                format!("~{}", &path_str[home.len()..])
            } else {
                path_str.to_string()
            }
        } else {
            path_str.to_string()
        };

        format!(" {} ", display_path)
    }

    /// Format the sorting indicator
    fn format_sort(panel: &Panel) -> String {
        use crate::state::panel::{SortDirection, SortField};

        let sort_indicator = match panel.sort_config.field {
            SortField::Name => "Name",
            SortField::Extension => "Ext",
            SortField::Size => "Size",
            SortField::Modified => "Date",
            SortField::Unsorted => "---",
        };
        let dir_char = match panel.sort_config.direction {
            SortDirection::Ascending => '↑',
            SortDirection::Descending => '↓',
        };
        format!(" [{}{}] ", sort_indicator, dir_char)
    }

    /// Format the footer left side (free space + file counts or selection info)
    fn footer_left(panel: &Panel) -> String {
        // Get free space for current mount
        let free_space = get_free_space(&panel.path)
            .map(|s| format!("{} free", format_size(s)))
            .unwrap_or_default();

        let selected_count = panel.selected_count();
        if selected_count > 0 {
            let selected_size = format_size(panel.selected_size());
            if free_space.is_empty() {
                format!(" {} selected  {} ", selected_count, selected_size)
            } else {
                format!(" {}  {} selected  {} ", free_space, selected_count, selected_size)
            }
        } else {
            let files = panel.file_count();
            let dirs = panel.dir_count();
            let size = format_size(panel.total_size());
            if free_space.is_empty() {
                format!(" {} files, {} dirs  {} ", files, dirs, size)
            } else {
                format!(" {}  {} files, {} dirs  {} ", free_space, files, dirs, size)
            }
        }
    }

    /// Format the footer right side (current file's size and attributes)
    fn footer_right(panel: &Panel, dir_sizes: Option<&std::collections::HashMap<std::path::PathBuf, u64>>) -> String {
        let Some(entry) = panel.selected() else {
            return String::new();
        };

        // Size - check for computed dir size first
        let size_str = if entry.is_dir {
            if let Some(sizes) = dir_sizes {
                if let Some(&size) = sizes.get(&entry.path) {
                    format_size(size)
                } else {
                    "<DIR>".to_string()
                }
            } else {
                "<DIR>".to_string()
            }
        } else {
            format_size(entry.size)
        };

        // Permissions/attributes
        let perm_str = format_permissions(entry.permissions, entry.is_dir);

        format!(" {}  {} ", size_str, perm_str)
    }

    /// Render the drive line at the top of the panel
    fn render_drive_line(panel: &Panel, theme: &Theme, panel_bg: Color, area: Rect, buf: &mut Buffer) {
        if area.width < 4 {
            return;
        }

        let style = Style::default()
            .fg(theme.file_normal)
            .bg(panel_bg);

        // Clear the line
        for x in area.x..area.x + area.width {
            buf[(x, area.y)].set_char(' ').set_style(style);
        }

        // Show "TEMP" if in temp mode
        if panel.is_temp_mode() {
            let temp_style = Style::default()
                .fg(theme.cursor_fg)
                .bg(theme.cursor_bg)
                .add_modifier(Modifier::BOLD);
            buf.set_string(area.x, area.y, " TEMP ", temp_style);
            return;
        }

        // Show short label for archives (e.g., "[ZIP]") with border colors
        if let Some(label) = panel.provider_short_label() {
            let label_str = format!(" {} ", label);
            buf.set_string(area.x, area.y, &label_str, style);
            return;
        }

        // Show remote connection name for SCP/WebDAV panels
        if panel.is_remote() {
            let remote_style = Style::default()
                .fg(theme.cursor_fg)
                .bg(theme.cursor_bg)
                .add_modifier(Modifier::BOLD);
            let name = panel.provider_name();
            // Truncate name to fit
            let display_name = if name.len() + 2 > area.width as usize {
                format!(" {}… ", &name[..area.width as usize - 4])
            } else {
                format!(" {} ", name)
            };
            buf.set_string(area.x, area.y, &display_name, remote_style);
            return;
        }

        // On Windows, show the drive letter; on other platforms, leave empty
        #[cfg(windows)]
        {
            if let Some(drive) = get_drive_letter(&panel.path) {
                let drive_str = format!(" {} ", drive);
                buf.set_string(area.x, area.y, &drive_str, style);
            }
        }

        #[cfg(not(windows))]
        {
            let _ = panel; // Suppress unused warning
        }
    }

    /// Render in Brief mode (two columns)
    fn render_brief(panel: &Panel, is_active: bool, theme: &Theme, area: Rect, buf: &mut Buffer) {
        if area.height < 1 || area.width < 4 {
            return;
        }

        let col_width = area.width / 2;
        let rows = area.height as usize;
        let total_entries = panel.entry_count();

        // Calculate visible range based on scroll
        let start = panel.scroll_offset;
        let visible_count = rows * 2; // Two columns
        let end = (start + visible_count).min(total_entries);

        for (i, entry_idx) in (start..end).enumerate() {
            let Some(entry) = panel.entry_at(entry_idx) else {
                continue;
            };

            // Calculate position: fill left column first, then right
            let col = i / rows;
            let row = i % rows;

            if col > 1 {
                break; // Only two columns
            }

            // Second column starts after the separator
            let x = area.x + (col as u16 * col_width) + if col > 0 { 1 } else { 0 };
            let y = area.y + row as u16;

            // Determine style and decorations
            let is_cursor = entry_idx == panel.cursor;
            let is_marked = panel.is_selected(&entry.path);
            let (style, prefix, suffix) = entry_style_and_decorations(entry, is_cursor, is_active, is_marked, theme);

            // Format the name with prefix/suffix
            // In temp mode, show full path instead of just name
            let mut name = String::new();
            if entry.is_dir {
                name.push(std::path::MAIN_SEPARATOR);
            }
            if let Some(p) = prefix {
                name.push_str(p);
            }
            if panel.is_temp_mode() {
                // Show full path in temp mode
                name.push_str(&entry.path.to_string_lossy());
            } else {
                name.push_str(&entry.name);
            }
            if let Some(s) = suffix {
                name.push_str(s);
            }

            // Truncate to fit column
            let max_width = col_width.saturating_sub(1) as usize;
            // In temp mode, truncate from left to keep filename visible
            let display_name = if panel.is_temp_mode() {
                truncate_path_right(&name, max_width)
            } else {
                truncate_name(&name, max_width)
            };

            // Render
            let span = Span::styled(format!("{:<width$}", display_name, width = max_width), style);
            buf.set_span(x, y, &span, col_width);
        }

        // Draw column separator if there's room
        if col_width > 0 && area.width > col_width {
            let sep_x = area.x + col_width;
            for row in 0..area.height {
                if sep_x < area.x + area.width {
                    buf[(sep_x, area.y + row)]
                        .set_char('│')
                        .set_style(Style::default().fg(theme.panel_column_separator));
                }
            }
        }
    }

    /// Render in Full mode (single column with details)
    fn render_full(panel: &Panel, is_active: bool, theme: &Theme, area: Rect, buf: &mut Buffer) {
        if area.height < 2 || area.width < 20 {
            return;
        }

        let total_entries = panel.entry_count();

        // Reserve first row for header
        let header_y = area.y;
        let content_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(1),
        };
        let rows = content_area.height as usize;

        // Column widths (adaptive based on available space)
        // Name takes remaining space, other columns are fixed
        let size_width: u16 = 9;   // "1023.9 MB"
        let date_width: u16 = 12;  // "Jan 15 10:23"
        let perm_width: u16 = 10;  // "drwxr-xr-x"
        let fixed_width = size_width + date_width + perm_width + 3; // +3 for spaces
        let name_width = area.width.saturating_sub(fixed_width).max(10);

        // Render header
        let header_style = Style::default()
            .fg(theme.panel_header)
            .add_modifier(Modifier::BOLD);

        let header = format!(
            "{:<name_w$} {:>size_w$} {:>date_w$} {:<perm_w$}",
            "Name",
            "Size",
            "Modified",
            "Perms",
            name_w = name_width as usize,
            size_w = size_width as usize,
            date_w = date_width as usize,
            perm_w = perm_width as usize,
        );
        buf.set_string(area.x, header_y, &header, header_style);

        // Calculate visible range based on scroll
        let start = panel.scroll_offset;
        let end = (start + rows).min(total_entries);

        for (i, entry_idx) in (start..end).enumerate() {
            let Some(entry) = panel.entry_at(entry_idx) else {
                continue;
            };

            let y = content_area.y + i as u16;
            let is_cursor = entry_idx == panel.cursor;
            let is_marked = panel.is_selected(&entry.path);
            let (style, prefix, suffix) = entry_style_and_decorations(entry, is_cursor, is_active, is_marked, theme);

            // Format name with prefix/suffix
            // In temp mode, show full path instead of just name
            let mut name = String::new();
            if entry.is_dir {
                name.push(std::path::MAIN_SEPARATOR);
            }
            if let Some(p) = prefix {
                name.push_str(p);
            }
            if panel.is_temp_mode() {
                // Show full path in temp mode
                name.push_str(&entry.path.to_string_lossy());
            } else {
                name.push_str(&entry.name);
            }
            if let Some(s) = suffix {
                name.push_str(s);
            }
            // In temp mode, truncate from left to keep filename visible
            let display_name = if panel.is_temp_mode() {
                truncate_path_right(&name, name_width as usize)
            } else {
                truncate_name(&name, name_width as usize)
            };

            // Format size
            let size_str = if entry.is_dir {
                "<DIR>".to_string()
            } else {
                format_size_short(entry.size)
            };

            // Format date
            let date_str = format_date(entry.modified);

            // Format permissions
            let perm_str = format_permissions(entry.permissions, entry.is_dir);

            // Build the line
            let line = format!(
                "{:<name_w$} {:>size_w$} {:>date_w$} {:<perm_w$}",
                display_name,
                size_str,
                date_str,
                perm_str,
                name_w = name_width as usize,
                size_w = size_width as usize,
                date_w = date_width as usize,
                perm_w = perm_width as usize,
            );

            buf.set_string(area.x, y, &line, style);
        }
    }
}

impl StatefulWidget for PanelWidget<'_> {
    type State = Panel;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Use different background color for temp mode and remote panels
        let panel_bg = if state.is_temp_mode() {
            self.theme.temp_panel_background
        } else if state.is_remote() {
            self.theme.remote_panel_background
        } else {
            self.theme.panel_background
        };

        // Create the block with border
        let border_style = Style::default()
            .fg(self.theme.panel_border_inactive)
            .bg(panel_bg);

        let path_str = Self::format_path(state);
        let sort_str = Self::format_sort(state);
        let footer_left = Self::footer_left(state);
        let footer_right = Self::footer_right(state, self.dir_sizes);

        // Path style: header_bg when active, panel_background when inactive
        let path_style = if self.is_active {
            Style::default()
                .fg(Color::White)
                .bg(self.theme.panel_header_bg)
        } else {
            Style::default()
                .fg(self.theme.file_normal)
                .bg(panel_bg)
        };

        // Sort indicator always uses panel background
        let sort_style = Style::default()
            .fg(self.theme.panel_header)
            .bg(panel_bg);

        let title_line = Line::from(vec![
            Span::styled(path_str, path_style),
            Span::styled(sort_str, sort_style),
        ]);

        let block = Block::default()
            .title(title_line)
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(Style::default().bg(panel_bg));

        // Get inner area (inside borders)
        let inner = block.inner(area);

        // Reserve space for drive line at top
        let drive_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: DRIVE_LINE_HEIGHT.min(inner.height),
        };

        // Content area is below the drive line and above the footer
        let content_area = Rect {
            x: inner.x,
            y: inner.y + DRIVE_LINE_HEIGHT,
            width: inner.width,
            height: inner.height.saturating_sub(DRIVE_LINE_HEIGHT + FOOTER_HEIGHT),
        };

        // Footer area (separator line + footer text)
        let separator_y = inner.y + inner.height.saturating_sub(FOOTER_HEIGHT);
        let footer_text_y = separator_y + 1;

        // Determine effective view mode - temp panels always use Full mode
        let effective_view_mode = if state.is_temp_mode() {
            ViewMode::Full
        } else {
            state.view_mode
        };

        // Update panel's visible height for navigation calculations
        // For Full mode, subtract 1 for the header row
        state.visible_height = match effective_view_mode {
            ViewMode::Brief => content_area.height as usize,
            ViewMode::Full => content_area.height.saturating_sub(1) as usize,
        };

        // Render the block
        block.render(area, buf);

        // Fill inner area with background color
        let bg_style = Style::default().bg(panel_bg);
        for y in inner.y..inner.y + inner.height {
            for x in inner.x..inner.x + inner.width {
                buf[(x, y)].set_style(bg_style);
            }
        }

        // Render drive line at top
        Self::render_drive_line(state, self.theme, panel_bg, drive_area, buf);

        // Render content based on effective view mode
        match effective_view_mode {
            ViewMode::Brief => Self::render_brief(state, self.is_active, self.theme, content_area, buf),
            ViewMode::Full => Self::render_full(state, self.is_active, self.theme, content_area, buf),
        }

        // Render separator line (solid ─)
        let separator_style = Style::default()
            .fg(self.theme.panel_border_inactive)
            .bg(panel_bg);
        for x in inner.x..inner.x + inner.width {
            buf[(x, separator_y)].set_char('─').set_style(separator_style);
        }

        // Render footer text
        let footer_style = Style::default()
            .fg(self.theme.file_normal)
            .bg(panel_bg);

        let right_len = footer_right.chars().count();

        // Clear footer line
        for x in inner.x..inner.x + inner.width {
            buf[(x, footer_text_y)].set_char(' ').set_style(footer_style);
        }

        // Draw left part
        buf.set_string(inner.x, footer_text_y, &footer_left, footer_style);

        // Draw right part
        let right_x = inner.x + inner.width - right_len as u16;
        buf.set_string(right_x, footer_text_y, &footer_right, footer_style);
    }
}

/// Get the style for an entry based on type, cursor, and marked state
/// Get style and display decorations for a file entry
/// Returns (style, prefix, suffix)
fn entry_style_and_decorations<'a>(
    entry: &FileEntry,
    is_cursor: bool,
    is_active: bool,
    is_marked: bool,
    theme: &'a Theme,
) -> (Style, Option<&'a str>, Option<&'a str>) {
    // Default: no prefix/suffix
    let mut prefix: Option<&str> = None;
    let mut suffix: Option<&str> = None;

    // Determine base color and decorations
    let fg_color = if entry.is_dir {
        theme.file_directory
    } else if let Some((color, pfx, sfx)) = theme.find_highlight(
        &entry.name,
        entry.is_executable(),
        entry.is_symlink,
    ) {
        prefix = pfx;
        suffix = sfx;
        color
    } else {
        theme.file_normal
    };

    // Build style based on cursor/marked state
    let mut style = if is_marked {
        if is_cursor && is_active {
            Style::default()
                .bg(theme.cursor_bg)
                .fg(theme.file_selected)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.file_selected)
                .add_modifier(Modifier::BOLD)
        }
    } else if is_cursor && is_active {
        Style::default()
            .bg(theme.cursor_bg)
            .fg(theme.cursor_fg)
    } else {
        Style::default().fg(fg_color)
    };

    // Make directories bold
    if entry.is_dir {
        style = style.add_modifier(Modifier::BOLD);
    }

    (style, prefix, suffix)
}

/// Truncate a filename to fit within max_width (keeps the beginning)
fn truncate_name(name: &str, max_width: usize) -> String {
    if name.chars().count() <= max_width {
        name.to_string()
    } else if max_width <= 3 {
        name.chars().take(max_width).collect()
    } else {
        let mut result: String = name.chars().take(max_width - 1).collect();
        result.push('…');
        result
    }
}

/// Truncate a path to fit within max_width (keeps the end with filename, shows "..." at start)
fn truncate_path_right(path: &str, max_width: usize) -> String {
    let char_count = path.chars().count();
    if char_count <= max_width {
        path.to_string()
    } else if max_width <= 3 {
        path.chars().skip(char_count - max_width).collect()
    } else {
        // Show "..." at the beginning, keep the right side
        let keep_chars = max_width - 3; // 3 for "..."
        let skip = char_count - keep_chars;
        let mut result = String::from("...");
        result.extend(path.chars().skip(skip));
        result
    }
}

/// Format a file size for display (full version for footer)
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

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

/// Format a file size for display (short version for columns)
fn format_size_short(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1}T", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

/// Format a date for display
fn format_date(time: Option<SystemTime>) -> String {
    let Some(time) = time else {
        return "------------".to_string();
    };

    let Ok(duration) = time.duration_since(SystemTime::UNIX_EPOCH) else {
        return "------------".to_string();
    };

    let secs = duration.as_secs();

    // Simple date formatting without external crates
    // Convert to approximate date components
    const SECS_PER_MIN: u64 = 60;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_DAY: u64 = 86400;

    let days_since_epoch = secs / SECS_PER_DAY;
    let time_of_day = secs % SECS_PER_DAY;
    let hours = time_of_day / SECS_PER_HOUR;
    let minutes = (time_of_day % SECS_PER_HOUR) / SECS_PER_MIN;

    // Calculate year, month, day (simplified, doesn't account for leap seconds)
    let mut year = 1970;
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
            month = i;
            break;
        }
        remaining_days -= days;
    }
    let day = remaining_days + 1;

    let month_names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                       "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

    format!("{} {:2} {:02}:{:02}", month_names[month], day, hours, minutes)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Format Unix permissions
fn format_permissions(mode: u32, is_dir: bool) -> String {
    if mode == 0 {
        // Windows or unavailable
        return "----------".to_string();
    }

    let file_type = if is_dir { 'd' } else { '-' };

    let user_r = if mode & 0o400 != 0 { 'r' } else { '-' };
    let user_w = if mode & 0o200 != 0 { 'w' } else { '-' };
    let user_x = if mode & 0o100 != 0 { 'x' } else { '-' };

    let group_r = if mode & 0o040 != 0 { 'r' } else { '-' };
    let group_w = if mode & 0o020 != 0 { 'w' } else { '-' };
    let group_x = if mode & 0o010 != 0 { 'x' } else { '-' };

    let other_r = if mode & 0o004 != 0 { 'r' } else { '-' };
    let other_w = if mode & 0o002 != 0 { 'w' } else { '-' };
    let other_x = if mode & 0o001 != 0 { 'x' } else { '-' };

    format!("{}{}{}{}{}{}{}{}{}{}",
        file_type,
        user_r, user_w, user_x,
        group_r, group_w, group_x,
        other_r, other_w, other_x)
}

//! Plugin viewer widget

use std::path::Path;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::Widget,
};

use super::Theme;

/// Plugin viewer widget - displays content rendered by a plugin
pub struct PluginViewer<'a> {
    plugin_name: &'a str,
    path: &'a Path,
    lines: &'a [String],
    scroll: usize,
    total_lines: usize,
    theme: &'a Theme,
}

impl<'a> PluginViewer<'a> {
    pub fn new(
        plugin_name: &'a str,
        path: &'a Path,
        lines: &'a [String],
        scroll: usize,
        total_lines: usize,
        theme: &'a Theme,
    ) -> Self {
        Self {
            plugin_name,
            path,
            lines,
            scroll,
            total_lines,
            theme,
        }
    }

    /// Calculate the visible height (content area, excluding header and footer)
    pub fn content_height(area: Rect) -> usize {
        area.height.saturating_sub(2) as usize // -1 header, -1 footer
    }
}

impl Widget for PluginViewer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 10 {
            return;
        }

        let header_style = Style::default().bg(self.theme.viewer_header_bg).fg(self.theme.viewer_header_fg);
        let content_style = Style::default().bg(self.theme.viewer_content_bg).fg(self.theme.viewer_content_fg);
        let footer_style = Style::default().bg(self.theme.viewer_footer_bg).fg(self.theme.viewer_footer_fg);

        // Header row - show path and plugin name
        let path_str = self.path.to_string_lossy();
        let header = format!(" {} [{}] ", path_str, self.plugin_name);
        for x in area.x..area.x + area.width {
            buf[(x, area.y)].set_char(' ').set_style(header_style);
        }
        let header_truncated = if header.len() > area.width as usize {
            format!("…{}", &path_str[path_str.len().saturating_sub(area.width as usize - 2)..])
        } else {
            header
        };
        buf.set_string(area.x, area.y, &header_truncated, header_style);

        // Content area
        let content_start_y = area.y + 1;
        let content_height = area.height.saturating_sub(2) as usize;
        let content_width = area.width as usize;

        // Clear content area
        for y in content_start_y..content_start_y + content_height as u16 {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_char(' ').set_style(content_style);
            }
        }

        // Render visible lines
        for (i, line) in self.lines.iter().enumerate() {
            if i >= content_height {
                break;
            }
            let y = content_start_y + i as u16;
            let display_line: String = line.chars().take(content_width).collect();
            buf.set_string(area.x, y, &display_line, content_style);
        }

        // Footer row
        let footer_y = area.y + area.height - 1;
        for x in area.x..area.x + area.width {
            buf[(x, footer_y)].set_char(' ').set_style(footer_style);
        }

        // Footer content: position info and help
        let visible_end = (self.scroll + content_height).min(self.total_lines);
        let percent = if self.total_lines > 0 {
            ((visible_end as f64 / self.total_lines as f64) * 100.0) as usize
        } else {
            100
        };
        let position_info = format!(
            " Lines {}-{} of {} ({}%) ",
            self.scroll + 1,
            visible_end,
            self.total_lines,
            percent
        );
        let help_text = " ESC/q/F3:Exit  Up/Down:Scroll ";

        buf.set_string(area.x, footer_y, &position_info, footer_style);

        // Right-align help text
        let help_x = (area.x + area.width).saturating_sub(help_text.len() as u16);
        if help_x > area.x + position_info.len() as u16 {
            buf.set_string(help_x, footer_y, help_text, footer_style);
        }
    }
}

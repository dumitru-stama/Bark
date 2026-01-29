//! Help viewer widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use super::Theme;

/// Help viewer widget
pub struct HelpViewer<'a> {
    content: &'static str,
    scroll: usize,
    theme: &'a Theme,
}

impl<'a> HelpViewer<'a> {
    pub fn new(content: &'static str, scroll: usize, theme: &'a Theme) -> Self {
        Self { content, scroll, theme }
    }

    /// Calculate the visible height (content area, excluding header and footer)
    pub fn content_height(area: Rect) -> usize {
        area.height.saturating_sub(2) as usize // -1 header, -1 footer
    }
}

impl Widget for HelpViewer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 10 {
            return;
        }

        let header_style = Style::default().bg(self.theme.help_header_bg).fg(self.theme.help_header_fg).add_modifier(Modifier::BOLD);
        let content_style = Style::default().bg(self.theme.help_content_bg).fg(self.theme.help_content_fg);
        let highlight_style = Style::default().bg(self.theme.help_content_bg).fg(self.theme.help_highlight);
        let footer_style = Style::default().bg(self.theme.help_footer_bg).fg(self.theme.help_footer_fg);

        // Header row
        for x in area.x..area.x + area.width {
            buf[(x, area.y)].set_char(' ').set_style(header_style);
        }
        let title = " Bark Help ";
        let title_x = area.x + (area.width.saturating_sub(title.len() as u16)) / 2;
        buf.set_string(title_x, area.y, title, header_style);

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
        let lines: Vec<&str> = self.content.lines().collect();
        for (i, line) in lines.iter().skip(self.scroll).take(content_height).enumerate() {
            let y = content_start_y + i as u16;

            // Check if this is a section header (all caps or ends with ===)
            let style = if line.contains("===") || (line.chars().all(|c| c.is_uppercase() || c.is_whitespace()) && !line.is_empty()) {
                highlight_style
            } else {
                content_style
            };

            let display_line: String = line.chars().take(content_width).collect();
            buf.set_string(area.x + 1, y, &display_line, style);
        }

        // Footer row
        let footer_y = area.y + area.height - 1;
        for x in area.x..area.x + area.width {
            buf[(x, footer_y)].set_char(' ').set_style(footer_style);
        }

        // Footer content
        let total_lines = lines.len();
        let visible_end = (self.scroll + content_height).min(total_lines);
        let percent = if total_lines > 0 {
            ((visible_end as f64 / total_lines as f64) * 100.0) as usize
        } else {
            100
        };
        let position_info = format!(
            " Lines {}-{} of {} ({}%) ",
            self.scroll + 1,
            visible_end,
            total_lines,
            percent
        );
        let help_text = " q/Esc/F1: Close  j/k: Scroll  PgUp/PgDn: Page ";

        buf.set_string(area.x, footer_y, &position_info, footer_style);

        // Right-align help text
        let help_x = (area.x + area.width).saturating_sub(help_text.len() as u16);
        if help_x > area.x + position_info.len() as u16 {
            buf.set_string(help_x, footer_y, help_text, footer_style);
        }
    }
}

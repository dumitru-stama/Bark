//! File viewer widget

use std::path::Path;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::ui::viewer_utils::{format_hex_dump_range, format_cp437_range, cp437_line_count};
use crate::state::mode::{ViewContent, BinaryViewMode};
use super::Theme;

/// File viewer widget
pub struct FileViewer<'a> {
    content: &'a ViewContent,
    scroll: usize,
    path: &'a Path,
    theme: &'a Theme,
    binary_mode: BinaryViewMode,
    /// Search matches: (byte_offset, length)
    search_matches: &'a [(usize, usize)],
    /// Current match index (for different highlight)
    current_match: Option<usize>,
}

impl<'a> FileViewer<'a> {
    pub fn new(content: &'a ViewContent, scroll: usize, path: &'a Path, theme: &'a Theme, binary_mode: BinaryViewMode) -> Self {
        Self { content, scroll, path, theme, binary_mode, search_matches: &[], current_match: None }
    }

    /// Set search matches to highlight
    pub fn with_search(mut self, matches: &'a [(usize, usize)], current: Option<usize>) -> Self {
        self.search_matches = matches;
        self.current_match = current;
        self
    }

    /// Calculate byte offset for a given text line number
    fn byte_offset_for_text_line(&self, bytes: &[u8], line_num: usize) -> usize {
        let mut offset = 0;
        let mut current_line = 0;
        for (i, &b) in bytes.iter().enumerate() {
            if current_line >= line_num {
                return offset;
            }
            if b == b'\n' {
                current_line += 1;
                offset = i + 1;
            }
        }
        if current_line >= line_num {
            offset
        } else {
            bytes.len()
        }
    }

    /// Get the appropriate style for a byte offset based on search matches
    fn get_highlight_style(&self, byte_offset: usize, default: Style, highlight: Style, current: Style) -> Style {
        for (i, &(start, len)) in self.search_matches.iter().enumerate() {
            if byte_offset >= start && byte_offset < start + len {
                // This byte is in a match
                if self.current_match == Some(i) {
                    return current;
                } else {
                    return highlight;
                }
            }
        }
        default
    }

    /// Calculate the visible height (content area, excluding header and footer)
    pub fn content_height(area: Rect) -> usize {
        area.height.saturating_sub(2) as usize // -1 header, -1 footer
    }

    /// Calculate line count for the content
    pub fn line_count(content: &ViewContent, term_width: usize, binary_mode: BinaryViewMode) -> usize {
        match content {
            ViewContent::Text(text) => {
                match binary_mode {
                    BinaryViewMode::Cp437 => text.lines().count(),  // Normal text view
                    BinaryViewMode::Hex => {
                        // Hex view of text content
                        let bytes = text.as_bytes();
                        if bytes.is_empty() {
                            return 0;
                        }
                        let calc_bytes = if term_width > 20 {
                            ((term_width.saturating_sub(12)) * 8) / 33
                        } else {
                            8
                        };
                        let bytes_per_line = (calc_bytes / 8 * 8).clamp(8, 64);
                        bytes.len().div_ceil(bytes_per_line)
                    }
                }
            }
            ViewContent::Binary(bytes) => {
                if bytes.is_empty() {
                    return 0;
                }
                match binary_mode {
                    BinaryViewMode::Hex => {
                        // Same calculation as format_hex_dump
                        let calc_bytes = if term_width > 20 {
                            ((term_width.saturating_sub(12)) * 8) / 33
                        } else {
                            8
                        };
                        let bytes_per_line = (calc_bytes / 8 * 8).clamp(8, 64);
                        bytes.len().div_ceil(bytes_per_line)
                    }
                    BinaryViewMode::Cp437 => {
                        cp437_line_count(bytes.len(), term_width)
                    }
                }
            }
            ViewContent::MappedFile { mmap, is_text, line_offsets } => {
                let bytes: &[u8] = mmap;
                if bytes.is_empty() {
                    return 0;
                }
                match (*is_text, binary_mode) {
                    (true, BinaryViewMode::Cp437) => {
                        // Text view - use precomputed line offsets
                        line_offsets.len()
                    }
                    (_, BinaryViewMode::Hex) | (false, BinaryViewMode::Cp437) => {
                        // Hex view or binary CP437 view
                        if binary_mode == BinaryViewMode::Hex {
                            let calc_bytes = if term_width > 20 {
                                ((term_width.saturating_sub(12)) * 8) / 33
                            } else {
                                8
                            };
                            let bytes_per_line = (calc_bytes / 8 * 8).clamp(8, 64);
                            bytes.len().div_ceil(bytes_per_line)
                        } else {
                            cp437_line_count(bytes.len(), term_width)
                        }
                    }
                }
            }
        }
    }
}

impl Widget for FileViewer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 10 {
            return;
        }

        let header_style = Style::default().bg(self.theme.viewer_header_bg).fg(self.theme.viewer_header_fg);
        let content_style = Style::default().bg(self.theme.viewer_content_bg).fg(self.theme.viewer_content_fg);
        let footer_style = Style::default().bg(self.theme.viewer_footer_bg).fg(self.theme.viewer_footer_fg);
        let line_num_style = Style::default().bg(self.theme.viewer_content_bg).fg(self.theme.viewer_line_number);

        // Header row
        let path_str = self.path.to_string_lossy();
        let is_binary = match self.content {
            ViewContent::Binary(_) => true,
            ViewContent::MappedFile { is_text, .. } => !is_text,
            ViewContent::Text(_) => false,
        };
        let header = match (is_binary, self.binary_mode) {
            (true, BinaryViewMode::Hex) => format!(" {} [HEX] ", path_str),
            (true, BinaryViewMode::Cp437) => format!(" {} [CP437] ", path_str),
            (false, BinaryViewMode::Hex) => format!(" {} [HEX] ", path_str),
            (false, BinaryViewMode::Cp437) => format!(" {} [TEXT] ", path_str),
        };
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

        // Get lines to render based on content type
        let (lines, total_lines, show_line_numbers) = match self.content {
            ViewContent::Text(text) => {
                match self.binary_mode {
                    BinaryViewMode::Cp437 => {
                        // Normal text view
                        let all_lines: Vec<&str> = text.lines().collect();
                        let total = all_lines.len();
                        let visible: Vec<String> = all_lines
                            .into_iter()
                            .skip(self.scroll)
                            .take(content_height)
                            .map(|s| s.to_string())
                            .collect();
                        (visible, total, true)
                    }
                    BinaryViewMode::Hex => {
                        // Hex view of text content
                        let bytes = text.as_bytes();
                        let calc_bytes = if content_width > 20 {
                            ((content_width.saturating_sub(12)) * 8) / 33
                        } else {
                            8
                        };
                        let bytes_per_line = (calc_bytes / 8 * 8).clamp(8, 64);
                        let total = if bytes.is_empty() {
                            0
                        } else {
                            bytes.len().div_ceil(bytes_per_line)
                        };
                        let start_byte = self.scroll * bytes_per_line;
                        let end_byte = ((self.scroll + content_height) * bytes_per_line).min(bytes.len());
                        let visible_lines = if start_byte < bytes.len() {
                            format_hex_dump_range(bytes, content_width, start_byte, end_byte)
                        } else {
                            Vec::new()
                        };
                        (visible_lines, total, false)
                    }
                }
            }
            ViewContent::Binary(bytes) => {
                match self.binary_mode {
                    BinaryViewMode::Hex => {
                        // Calculate bytes per line (same formula as format_hex_dump)
                        let calc_bytes = if content_width > 20 {
                            ((content_width.saturating_sub(12)) * 8) / 33
                        } else {
                            8
                        };
                        let bytes_per_line = (calc_bytes / 8 * 8).clamp(8, 64);

                        // Calculate total lines
                        let total = if bytes.is_empty() {
                            0
                        } else {
                            bytes.len().div_ceil(bytes_per_line)
                        };

                        // Only format visible portion
                        let start_byte = self.scroll * bytes_per_line;
                        let end_byte = ((self.scroll + content_height) * bytes_per_line).min(bytes.len());

                        let visible_lines = if start_byte < bytes.len() {
                            format_hex_dump_range(bytes, content_width, start_byte, end_byte)
                        } else {
                            Vec::new()
                        };

                        (visible_lines, total, false)
                    }
                    BinaryViewMode::Cp437 => {
                        let total = cp437_line_count(bytes.len(), content_width);
                        let visible_lines = format_cp437_range(bytes, content_width, self.scroll, content_height);
                        (visible_lines, total, false)
                    }
                }
            }
            ViewContent::MappedFile { mmap, is_text, line_offsets } => {
                let bytes: &[u8] = mmap;
                match (*is_text, self.binary_mode) {
                    (true, BinaryViewMode::Cp437) => {
                        // Text view using precomputed line offsets
                        let total = line_offsets.len();
                        let visible: Vec<String> = (self.scroll..self.scroll + content_height)
                            .filter_map(|line_idx| {
                                if line_idx >= line_offsets.len() {
                                    return None;
                                }
                                let start = line_offsets[line_idx];
                                let end = if line_idx + 1 < line_offsets.len() {
                                    // End is start of next line minus the newline char
                                    let next_start = line_offsets[line_idx + 1];
                                    if next_start > 0 && bytes.get(next_start - 1) == Some(&b'\n') {
                                        next_start - 1
                                    } else {
                                        next_start
                                    }
                                } else {
                                    bytes.len()
                                };
                                // Convert bytes to string, handling any invalid UTF-8 gracefully
                                Some(String::from_utf8_lossy(&bytes[start..end]).into_owned())
                            })
                            .collect();
                        (visible, total, true)
                    }
                    (_, BinaryViewMode::Hex) => {
                        // Hex view
                        let calc_bytes = if content_width > 20 {
                            ((content_width.saturating_sub(12)) * 8) / 33
                        } else {
                            8
                        };
                        let bytes_per_line = (calc_bytes / 8 * 8).clamp(8, 64);
                        let total = if bytes.is_empty() {
                            0
                        } else {
                            bytes.len().div_ceil(bytes_per_line)
                        };
                        let start_byte = self.scroll * bytes_per_line;
                        let end_byte = ((self.scroll + content_height) * bytes_per_line).min(bytes.len());
                        let visible_lines = if start_byte < bytes.len() {
                            format_hex_dump_range(bytes, content_width, start_byte, end_byte)
                        } else {
                            Vec::new()
                        };
                        (visible_lines, total, false)
                    }
                    (false, BinaryViewMode::Cp437) => {
                        // Binary CP437 view
                        let total = cp437_line_count(bytes.len(), content_width);
                        let visible_lines = format_cp437_range(bytes, content_width, self.scroll, content_height);
                        (visible_lines, total, false)
                    }
                }
            }
        };

        // Line number width (based on total lines) - only for text
        let line_num_width = if show_line_numbers && total_lines > 0 {
            ((total_lines as f64).log10().floor() as usize + 1).max(4)
        } else {
            0
        };

        // Highlight style for search matches
        let highlight_style = Style::default().bg(Color::Rgb(180, 160, 60)).fg(Color::Black); // Darker for inactive
        let current_highlight_style = Style::default().bg(Color::Yellow).fg(Color::Black);

        // Get the raw bytes for highlighting calculation
        let bytes: &[u8] = match self.content {
            ViewContent::Text(text) => text.as_bytes(),
            ViewContent::Binary(data) => data,
            ViewContent::MappedFile { mmap, .. } => mmap,
        };

        // Calculate bytes per line for hex mode
        let bytes_per_line = if content_width > 20 {
            let calc_bytes = ((content_width.saturating_sub(12)) * 8) / 33;
            (calc_bytes / 8 * 8).clamp(8, 64)
        } else {
            8
        };

        // Render visible lines (already filtered to visible portion)
        for (i, line) in lines.iter().enumerate() {
            let y = content_start_y + i as u16;

            if show_line_numbers {
                // Text mode with line numbers
                let line_num = self.scroll + i + 1;
                // Line number
                let num_str = format!("{:>width$} ", line_num, width = line_num_width);
                buf.set_string(area.x, y, &num_str, line_num_style);

                // Line content with highlighting
                let text_start = area.x + line_num_width as u16 + 1;
                let text_width = content_width.saturating_sub(line_num_width + 1);

                // Calculate byte offset for this line
                let line_byte_start = self.byte_offset_for_text_line(bytes, self.scroll + i);
                let line_byte_end = self.byte_offset_for_text_line(bytes, self.scroll + i + 1);

                // Render each character, applying highlight if needed
                // Track actual byte offset (not character index) for proper UTF-8 handling
                let mut byte_offset = line_byte_start;
                for (col, ch) in line.chars().take(text_width).enumerate() {
                    let x = text_start + col as u16;

                    let style = if byte_offset < line_byte_end {
                        self.get_highlight_style(byte_offset, content_style, highlight_style, current_highlight_style)
                    } else {
                        content_style
                    };

                    // Replace control characters (like TAB) with spaces for display
                    // TAB and other control chars aren't rendered properly by terminals
                    let display_char = if ch.is_control() { ' ' } else { ch };
                    buf[(x, y)].set_char(display_char).set_style(style);
                    byte_offset += ch.len_utf8(); // Advance by actual byte length
                }
            } else {
                // Hex dump or CP437 view - no line numbers
                let display_line: String = line.chars().take(content_width).collect();

                match self.binary_mode {
                    BinaryViewMode::Hex => {
                        // Hex view - highlight both hex and ASCII portions
                        let line_byte_start = (self.scroll + i) * bytes_per_line;

                        // Calculate layout parameters
                        // Format: "00000000  XX XX XX XX XX XX XX XX  XX XX XX XX XX XX XX XX  |AAAAAAAAAAAAAAAA|"
                        let offset_width = 10; // "00000000  " (8 hex + 2 spaces)
                        let group_size = 8;
                        let num_groups = bytes_per_line / group_size;
                        // Extra spaces between groups (one less than num_groups)
                        let extra_spaces = num_groups.saturating_sub(1);
                        let hex_section_width = bytes_per_line * 3 + extra_spaces; // "XX " per byte + group separators
                        let separator_start = offset_width + hex_section_width;
                        let ascii_start = separator_start + 2; // " |"

                        // Render the line character by character
                        for (col, ch) in display_line.chars().enumerate() {
                            let x = area.x + col as u16;

                            let style = if col < offset_width {
                                // Offset area - no highlight
                                content_style
                            } else if col < separator_start {
                                // Hex area - need to account for group separators
                                let hex_col = col - offset_width;

                                // Calculate which byte this column belongs to
                                // Each group of 8 bytes takes 8*3 + 1 = 25 chars (except last group: 24 chars)
                                let chars_per_group_with_sep = group_size * 3 + 1;
                                let chars_per_group_no_sep = group_size * 3;

                                let mut byte_idx = None;
                                let mut col_remaining = hex_col;

                                for group in 0..num_groups {
                                    let group_width = if group < num_groups - 1 {
                                        chars_per_group_with_sep
                                    } else {
                                        chars_per_group_no_sep
                                    };

                                    if col_remaining < group_width {
                                        // We're in this group
                                        let pos_in_group = col_remaining;
                                        if pos_in_group < group_size * 3 {
                                            // In hex bytes area (not in trailing space)
                                            let byte_in_group = pos_in_group / 3;
                                            byte_idx = Some(group * group_size + byte_in_group);
                                        }
                                        // else: in the group separator space, no byte
                                        break;
                                    }
                                    col_remaining -= group_width;
                                }

                                if let Some(idx) = byte_idx {
                                    let byte_offset = line_byte_start + idx;
                                    if byte_offset < bytes.len() {
                                        self.get_highlight_style(byte_offset, content_style, highlight_style, current_highlight_style)
                                    } else {
                                        content_style
                                    }
                                } else {
                                    content_style
                                }
                            } else if col < ascii_start {
                                // Separator " |" - no highlight
                                content_style
                            } else {
                                // ASCII area (including closing "|")
                                let ascii_col = col - ascii_start;
                                if ascii_col < bytes_per_line {
                                    let byte_offset = line_byte_start + ascii_col;
                                    if byte_offset < bytes.len() {
                                        self.get_highlight_style(byte_offset, content_style, highlight_style, current_highlight_style)
                                    } else {
                                        content_style
                                    }
                                } else {
                                    // Closing "|" or beyond
                                    content_style
                                }
                            };

                            buf[(x, y)].set_char(ch).set_style(style);
                        }
                    }
                    BinaryViewMode::Cp437 => {
                        // CP437 view - each char is one byte
                        let line_byte_start = (self.scroll + i) * content_width;

                        for (col, ch) in display_line.chars().enumerate() {
                            let x = area.x + col as u16;
                            let byte_offset = line_byte_start + col;

                            let style = if byte_offset < bytes.len() {
                                self.get_highlight_style(byte_offset, content_style, highlight_style, current_highlight_style)
                            } else {
                                content_style
                            };

                            buf[(x, y)].set_char(ch).set_style(style);
                        }
                    }
                }
            }
        }

        // Footer row
        let footer_y = area.y + area.height - 1;
        for x in area.x..area.x + area.width {
            buf[(x, footer_y)].set_char(' ').set_style(footer_style);
        }

        // Footer content: line info and help
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
        let help_text = if is_binary {
            " TAB:Toggle HEX/CP437  ESC/q:Exit  Up/Down:Scroll "
        } else {
            " TAB:Toggle TEXT/HEX  ESC/q:Exit  Up/Down:Scroll "
        };

        buf.set_string(area.x, footer_y, &position_info, footer_style);

        // Right-align help text
        let help_x = (area.x + area.width).saturating_sub(help_text.len() as u16);
        if help_x > area.x + position_info.len() as u16 {
            buf.set_string(help_x, footer_y, help_text, footer_style);
        }
    }
}

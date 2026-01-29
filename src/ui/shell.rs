//! Shell area widget - shows command history and prompt

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Parse ANSI escape codes and render styled text to buffer
fn render_ansi_string(x: u16, y: u16, s: &str, max_width: usize, buf: &mut Buffer) {
    let mut current_x = x;
    let end_x = x + max_width as u16;
    let mut style = Style::default().bg(Color::Reset).fg(Color::Reset);

    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if current_x >= end_x {
            break;
        }

        if c == '\x1b' {
            // Start of escape sequence
            match chars.peek() {
                Some(&'[') => {
                    // CSI sequence: ESC[ params letter
                    chars.next(); // consume '['
                    let mut params = String::new();

                    // Read params until we hit a final byte (letter or ~)
                    while let Some(&next) = chars.peek() {
                        if next.is_ascii_alphabetic() || next == '~' {
                            break;
                        }
                        params.push(chars.next().unwrap());
                    }

                    // Get the command character
                    if let Some(cmd) = chars.next()
                        && cmd == 'm' {
                            // SGR (Select Graphic Rendition)
                            style = parse_sgr(&params, style);
                        }
                        // Ignore other CSI sequences (cursor movement, etc.)
                }
                Some(&']') => {
                    // OSC sequence: ESC] ... BEL or ESC\
                    chars.next(); // consume ']'
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' { break; } // BEL
                        if ch == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next(); // consume '\'
                                break;
                            }
                        }
                    }
                }
                Some(&'P') => {
                    // DCS sequence: ESCP ... ST (ESC\)
                    chars.next(); // consume 'P'
                    while let Some(ch) = chars.next() {
                        if ch == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                                break;
                            }
                        }
                    }
                }
                Some(&c2) if c2 == '(' || c2 == ')' || c2 == '*' || c2 == '+' => {
                    // Charset selection: ESC( X, ESC) X, etc. — skip 2 chars
                    chars.next();
                    chars.next();
                }
                Some(&c2) if c2.is_ascii_alphabetic() => {
                    // Two-char escape: ESC letter (e.g., ESCc for RIS)
                    chars.next();
                }
                _ => {
                    // Unknown escape, skip
                }
            }
        } else if c >= ' ' {
            // Printable character
            buf[(current_x, y)].set_char(c).set_style(style);
            current_x += 1;
        }
    }
}

/// Parse SGR (color/style) parameters
fn parse_sgr(params: &str, mut style: Style) -> Style {
    if params.is_empty() {
        return Style::default().bg(Color::Reset).fg(Color::Reset);
    }

    let mut iter = params.split(';').peekable();

    while let Some(param) = iter.next() {
        match param {
            "0" => style = Style::default().bg(Color::Reset).fg(Color::Reset),
            "1" => style = style.add_modifier(Modifier::BOLD),
            "2" => style = style.add_modifier(Modifier::DIM),
            "3" => style = style.add_modifier(Modifier::ITALIC),
            "4" => style = style.add_modifier(Modifier::UNDERLINED),
            "7" => style = style.add_modifier(Modifier::REVERSED),
            "22" => style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            "23" => style = style.remove_modifier(Modifier::ITALIC),
            "24" => style = style.remove_modifier(Modifier::UNDERLINED),
            "27" => style = style.remove_modifier(Modifier::REVERSED),
            // Foreground colors
            "30" => style = style.fg(Color::Black),
            "31" => style = style.fg(Color::Red),
            "32" => style = style.fg(Color::Green),
            "33" => style = style.fg(Color::Yellow),
            "34" => style = style.fg(Color::Blue),
            "35" => style = style.fg(Color::Magenta),
            "36" => style = style.fg(Color::Cyan),
            "37" => style = style.fg(Color::White),
            "39" => style = style.fg(Color::Reset),
            // Bright foreground colors
            "90" => style = style.fg(Color::DarkGray),
            "91" => style = style.fg(Color::LightRed),
            "92" => style = style.fg(Color::LightGreen),
            "93" => style = style.fg(Color::LightYellow),
            "94" => style = style.fg(Color::LightBlue),
            "95" => style = style.fg(Color::LightMagenta),
            "96" => style = style.fg(Color::LightCyan),
            "97" => style = style.fg(Color::White),
            // Background colors
            "40" => style = style.bg(Color::Black),
            "41" => style = style.bg(Color::Red),
            "42" => style = style.bg(Color::Green),
            "43" => style = style.bg(Color::Yellow),
            "44" => style = style.bg(Color::Blue),
            "45" => style = style.bg(Color::Magenta),
            "46" => style = style.bg(Color::Cyan),
            "47" => style = style.bg(Color::White),
            "49" => style = style.bg(Color::Reset),
            // Bright background colors
            "100" => style = style.bg(Color::DarkGray),
            "101" => style = style.bg(Color::LightRed),
            "102" => style = style.bg(Color::LightGreen),
            "103" => style = style.bg(Color::LightYellow),
            "104" => style = style.bg(Color::LightBlue),
            "105" => style = style.bg(Color::LightMagenta),
            "106" => style = style.bg(Color::LightCyan),
            "107" => style = style.bg(Color::White),
            // 256-color mode: 38;5;N or 48;5;N
            "38" => {
                if iter.peek() == Some(&"5") {
                    iter.next(); // consume "5"
                    if let Some(n) = iter.next()
                        && let Ok(color) = n.parse::<u8>() {
                            style = style.fg(Color::Indexed(color));
                        }
                } else if iter.peek() == Some(&"2") {
                    iter.next(); // consume "2"
                    // RGB mode: 38;2;R;G;B
                    let r = iter.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let g = iter.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let b = iter.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    style = style.fg(Color::Rgb(r, g, b));
                }
            }
            "48" => {
                if iter.peek() == Some(&"5") {
                    iter.next(); // consume "5"
                    if let Some(n) = iter.next()
                        && let Ok(color) = n.parse::<u8>() {
                            style = style.bg(Color::Indexed(color));
                        }
                } else if iter.peek() == Some(&"2") {
                    iter.next(); // consume "2"
                    // RGB mode: 48;2;R;G;B
                    let r = iter.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let g = iter.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let b = iter.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    style = style.bg(Color::Rgb(r, g, b));
                }
            }
            _ => {}
        }
    }

    style
}

/// Shell area widget showing history lines and command prompt
pub struct ShellArea<'a> {
    /// Command output history
    history: &'a [String],
    /// Current command input
    input: &'a str,
    /// Command prompt (e.g., "/path/to/dir> ")
    prompt: &'a str,
}

impl<'a> ShellArea<'a> {
    pub fn new(history: &'a [String], input: &'a str, prompt: &'a str) -> Self {
        Self { history, input, prompt }
    }
}

impl Widget for ShellArea<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        // Use terminal default colors
        let style = Style::default().bg(Color::Reset).fg(Color::Reset);

        // Clear the entire area
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_char(' ').set_style(style);
            }
        }

        let height = area.height as usize;
        let width = area.width as usize;

        // Last line is for the command prompt
        let history_lines = height.saturating_sub(1);

        // Render command prompt on the last line
        let prompt_y = area.y + area.height - 1;

        // Render history lines (show the most recent ones that fit, bottom-aligned above prompt)
        if history_lines > 0 && !self.history.is_empty() {
            let visible_count = self.history.len().min(history_lines);
            let start = self.history.len().saturating_sub(history_lines);
            // Position history lines just above the prompt
            let history_start_y = prompt_y - visible_count as u16;
            for (i, line) in self.history.iter().skip(start).enumerate() {
                let y = history_start_y + i as u16;
                // Render with ANSI color support
                render_ansi_string(area.x, y, line, width, buf);
            }
        }
        buf.set_string(area.x, prompt_y, self.prompt, style);

        // Render input after prompt
        let input_x = area.x + self.prompt.len() as u16;
        let max_input_width = width.saturating_sub(self.prompt.len() + 1);

        // Show end of input if it's too long
        let display_input = if self.input.len() > max_input_width {
            &self.input[self.input.len() - max_input_width..]
        } else {
            self.input
        };

        buf.set_string(input_x, prompt_y, display_input, style);
    }
}

//! Bark ASCII Table Plugin — Scrollable ASCII character reference.

use std::io::{self, BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--plugin-info") {
        println!(
            r#"{{"name":"ASCII Table","version":"1.0.0","type":"overlay","description":"ASCII character reference","width":62,"height":22}}"#
        );
        return;
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut state = AsciiState::new();

    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let command = extract_str(line, "command").unwrap_or_default();

        match command.as_str() {
            "init" => {
                state.width = extract_int(line, "width").unwrap_or(62) as u16;
                state.height = extract_int(line, "height").unwrap_or(22) as u16;
                let response = state.render();
                let _ = writeln!(writer, "{}", response);
                let _ = writer.flush();
            }
            "key" => {
                let key = extract_str(line, "key").unwrap_or_default();
                let response = state.handle_key(&key);
                let _ = writeln!(writer, "{}", response);
                let _ = writer.flush();
            }
            "close" => break,
            _ => {
                let _ = writeln!(writer, r#"{{"close":true}}"#);
                let _ = writer.flush();
                break;
            }
        }
    }
}

const ASCII_NAMES: [&str; 128] = [
    "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL",
    "BS",  "HT",  "LF",  "VT",  "FF",  "CR",  "SO",  "SI",
    "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB",
    "CAN", "EM",  "SUB", "ESC", "FS",  "GS",  "RS",  "US",
    "Space", "!",  "\"", "#",  "$",  "%",  "&",  "'",
    "(",  ")",  "*",  "+",  ",",  "-",  ".",  "/",
    "0",  "1",  "2",  "3",  "4",  "5",  "6",  "7",
    "8",  "9",  ":",  ";",  "<",  "=",  ">",  "?",
    "@",  "A",  "B",  "C",  "D",  "E",  "F",  "G",
    "H",  "I",  "J",  "K",  "L",  "M",  "N",  "O",
    "P",  "Q",  "R",  "S",  "T",  "U",  "V",  "W",
    "X",  "Y",  "Z",  "[",  "\\", "]",  "^",  "_",
    "`",  "a",  "b",  "c",  "d",  "e",  "f",  "g",
    "h",  "i",  "j",  "k",  "l",  "m",  "n",  "o",
    "p",  "q",  "r",  "s",  "t",  "u",  "v",  "w",
    "x",  "y",  "z",  "{",  "|",  "}",  "~",  "DEL",
];

const ASCII_DESCRIPTIONS: [&str; 128] = [
    "Null", "Start of Heading", "Start of Text", "End of Text",
    "End of Transmission", "Enquiry", "Acknowledge", "Bell",
    "Backspace", "Horizontal Tab", "Line Feed", "Vertical Tab",
    "Form Feed", "Carriage Return", "Shift Out", "Shift In",
    "Data Link Escape", "Device Control 1", "Device Control 2", "Device Control 3",
    "Device Control 4", "Negative Ack", "Synchronous Idle", "End of Block",
    "Cancel", "End of Medium", "Substitute", "Escape",
    "File Separator", "Group Separator", "Record Separator", "Unit Separator",
    "Space", "Exclamation", "Double Quote", "Hash",
    "Dollar", "Percent", "Ampersand", "Single Quote",
    "Left Paren", "Right Paren", "Asterisk", "Plus",
    "Comma", "Hyphen", "Period", "Slash",
    "Digit 0", "Digit 1", "Digit 2", "Digit 3",
    "Digit 4", "Digit 5", "Digit 6", "Digit 7",
    "Digit 8", "Digit 9", "Colon", "Semicolon",
    "Less Than", "Equals", "Greater Than", "Question Mark",
    "At Sign", "Latin A", "Latin B", "Latin C",
    "Latin D", "Latin E", "Latin F", "Latin G",
    "Latin H", "Latin I", "Latin J", "Latin K",
    "Latin L", "Latin M", "Latin N", "Latin O",
    "Latin P", "Latin Q", "Latin R", "Latin S",
    "Latin T", "Latin U", "Latin V", "Latin W",
    "Latin X", "Latin Y", "Latin Z", "Left Bracket",
    "Backslash", "Right Bracket", "Caret", "Underscore",
    "Backtick", "Latin a", "Latin b", "Latin c",
    "Latin d", "Latin e", "Latin f", "Latin g",
    "Latin h", "Latin i", "Latin j", "Latin k",
    "Latin l", "Latin m", "Latin n", "Latin o",
    "Latin p", "Latin q", "Latin r", "Latin s",
    "Latin t", "Latin u", "Latin v", "Latin w",
    "Latin x", "Latin y", "Latin z", "Left Brace",
    "Pipe", "Right Brace", "Tilde", "Delete",
];

struct AsciiState {
    scroll: usize,
    width: u16,
    height: u16,
}

impl AsciiState {
    fn new() -> Self {
        Self { scroll: 0, width: 62, height: 22 }
    }

    fn visible_rows(&self) -> usize {
        // header(1) + separator(1) + help(1) + borders(2) => 5 lines overhead
        self.height.saturating_sub(5) as usize
    }

    fn handle_key(&mut self, key: &str) -> String {
        let visible = self.visible_rows();
        match key {
            "Escape" => return r#"{"close":true}"#.to_string(),
            "Up" => { if self.scroll > 0 { self.scroll -= 1; } }
            "Down" => { if self.scroll + visible < 128 { self.scroll += 1; } }
            "PageUp" => { self.scroll = self.scroll.saturating_sub(visible); }
            "PageDown" => {
                self.scroll = (self.scroll + visible).min(128usize.saturating_sub(visible));
            }
            "Home" => { self.scroll = 0; }
            "End" => { self.scroll = 128usize.saturating_sub(visible); }
            _ => {
                // Type a printable char to jump to it
                if key.len() == 1 {
                    let ch = key.as_bytes()[0] as usize;
                    if ch < 128 {
                        self.scroll = ch.min(128usize.saturating_sub(visible));
                    }
                }
            }
        }
        self.render()
    }

    fn render(&self) -> String {
        let inner_w = self.width.saturating_sub(2) as usize;
        let visible = self.visible_rows();
        let mut lines: Vec<String> = Vec::new();

        // Header
        let header = format!(" {:>3}  {:>4}  {:>5}  {}", "Dec", "Hex", "Char", "Description");
        lines.push(truncate(&header, inner_w));

        // Separator
        lines.push("\u{2500}".repeat(inner_w));

        // Table rows
        for i in self.scroll..(self.scroll + visible).min(128) {
            let char_display = if i < 32 || i == 127 {
                format!("{:>5}", ASCII_NAMES[i])
            } else {
                format!("    {}", char::from(i as u8))
            };
            let row = format!(" {:>3}  0x{:02X}  {}  {}",
                i, i, char_display, ASCII_DESCRIPTIONS[i]);
            lines.push(truncate(&row, inner_w));
        }

        // Pad remaining space
        while lines.len() < visible + 2 {
            lines.push(String::new());
        }

        // Help
        let pos = format!("{}-{}/128", self.scroll + 1, (self.scroll + visible).min(128));
        let help = format!("\u{2191}\u{2193}=Scroll  PgUp/Dn  Home/End  Type char to jump  {}", pos);
        lines.push(truncate(&help, inner_w));

        // Ensure exact content height
        let content_height = self.height.saturating_sub(2) as usize;
        lines.truncate(content_height);
        while lines.len() < content_height {
            lines.push(String::new());
        }

        let lines_json: Vec<String> = lines.iter()
            .map(|l| format!("\"{}\"", escape_json(l)))
            .collect();

        format!(
            r#"{{"title":" ASCII Table ","width":{},"height":{},"close":false,"lines":[{}]}}"#,
            self.width, self.height, lines_json.join(",")
        )
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { s[..max].to_string() }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn extract_str(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();
    if !rest.starts_with('"') { return None; }
    let rest = &rest[1..];
    let mut result = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => { if let Some(&n) = chars.peek() { chars.next(); match n {
                'n' => result.push('\n'), '"' => result.push('"'),
                '\\' => result.push('\\'), 't' => result.push('\t'),
                _ => { result.push('\\'); result.push(n); }
            }}}
            _ => result.push(c),
        }
    }
    Some(result)
}

fn extract_int(json: &str, key: &str) -> Option<i64> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit() && c != '-').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

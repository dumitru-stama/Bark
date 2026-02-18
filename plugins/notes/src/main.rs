//! Bark Notes Plugin — Persistent scratchpad overlay.
//!
//! Saves to ~/.config/bark/notes.txt

use std::io::{self, BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--plugin-info") {
        println!(
            r#"{{"name":"Notes","version":"1.0.0","type":"overlay","description":"Persistent scratchpad","width":62,"height":22}}"#
        );
        return;
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut state = NotesState::new();

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
                state.load();
                let response = state.render();
                let _ = writeln!(writer, "{}", response);
                let _ = writer.flush();
            }
            "key" => {
                let key = extract_str(line, "key").unwrap_or_default();
                let mods = extract_str_array(line, "modifiers");
                let response = state.handle_key(&key, &mods);
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

fn notes_path() -> std::path::PathBuf {
    let config_dir = std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".config").join("bark"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    config_dir.join("notes.txt")
}

struct NotesState {
    lines: Vec<String>,
    cursor_line: usize,
    cursor_col: usize,
    scroll: usize,
    width: u16,
    height: u16,
    status: Option<String>,
    dirty: bool,
}

impl NotesState {
    fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
            scroll: 0,
            width: 62,
            height: 22,
            status: None,
            dirty: false,
        }
    }

    fn load(&mut self) {
        let path = notes_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            self.lines = content.lines().map(|l| l.to_string()).collect();
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
        }
    }

    fn save(&mut self) {
        let path = notes_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content = self.lines.join("\n");
        match std::fs::write(&path, &content) {
            Ok(()) => {
                self.status = Some("Saved!".to_string());
                self.dirty = false;
            }
            Err(e) => {
                self.status = Some(format!("Save error: {}", e));
            }
        }
    }

    fn visible_rows(&self) -> usize {
        // status(1) + separator(1) + help(1) + borders(2) => 5 overhead
        self.height.saturating_sub(5) as usize
    }

    fn ensure_visible(&mut self) {
        let visible = self.visible_rows();
        if self.cursor_line < self.scroll {
            self.scroll = self.cursor_line;
        } else if self.cursor_line >= self.scroll + visible {
            self.scroll = self.cursor_line - visible + 1;
        }
    }

    fn handle_key(&mut self, key: &str, modifiers: &[String]) -> String {
        let ctrl = modifiers.iter().any(|m| m == "ctrl");

        // Clear status on any key
        self.status = None;

        match key {
            "Escape" => {
                self.save();
                return r#"{"close":true}"#.to_string();
            }
            "Enter" if ctrl => {
                if self.dirty {
                    self.save();
                }
                return r#"{"close":true}"#.to_string();
            }
            "s" | "S" if ctrl => {
                self.save();
            }
            "Enter" => {
                // Split current line at cursor
                let rest = self.lines[self.cursor_line][self.cursor_col..].to_string();
                self.lines[self.cursor_line].truncate(self.cursor_col);
                self.cursor_line += 1;
                self.lines.insert(self.cursor_line, rest);
                self.cursor_col = 0;
                self.dirty = true;
                self.ensure_visible();
            }
            "Backspace" => {
                if self.cursor_col > 0 {
                    self.lines[self.cursor_line].remove(self.cursor_col - 1);
                    self.cursor_col -= 1;
                    self.dirty = true;
                } else if self.cursor_line > 0 {
                    // Merge with previous line
                    let current = self.lines.remove(self.cursor_line);
                    self.cursor_line -= 1;
                    self.cursor_col = self.lines[self.cursor_line].len();
                    self.lines[self.cursor_line].push_str(&current);
                    self.dirty = true;
                    self.ensure_visible();
                }
            }
            "Delete" => {
                if self.cursor_col < self.lines[self.cursor_line].len() {
                    self.lines[self.cursor_line].remove(self.cursor_col);
                    self.dirty = true;
                } else if self.cursor_line + 1 < self.lines.len() {
                    // Merge next line into current
                    let next = self.lines.remove(self.cursor_line + 1);
                    self.lines[self.cursor_line].push_str(&next);
                    self.dirty = true;
                }
            }
            "Up" => {
                if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                    self.cursor_col = self.cursor_col.min(self.lines[self.cursor_line].len());
                    self.ensure_visible();
                }
            }
            "Down" => {
                if self.cursor_line + 1 < self.lines.len() {
                    self.cursor_line += 1;
                    self.cursor_col = self.cursor_col.min(self.lines[self.cursor_line].len());
                    self.ensure_visible();
                }
            }
            "Left" => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                } else if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                    self.cursor_col = self.lines[self.cursor_line].len();
                    self.ensure_visible();
                }
            }
            "Right" => {
                if self.cursor_col < self.lines[self.cursor_line].len() {
                    self.cursor_col += 1;
                } else if self.cursor_line + 1 < self.lines.len() {
                    self.cursor_line += 1;
                    self.cursor_col = 0;
                    self.ensure_visible();
                }
            }
            "Home" => { self.cursor_col = 0; }
            "End" => { self.cursor_col = self.lines[self.cursor_line].len(); }
            "PageUp" => {
                let vis = self.visible_rows();
                self.cursor_line = self.cursor_line.saturating_sub(vis);
                self.cursor_col = self.cursor_col.min(self.lines[self.cursor_line].len());
                self.ensure_visible();
            }
            "PageDown" => {
                let vis = self.visible_rows();
                self.cursor_line = (self.cursor_line + vis).min(self.lines.len() - 1);
                self.cursor_col = self.cursor_col.min(self.lines[self.cursor_line].len());
                self.ensure_visible();
            }
            _ if key.len() == 1 && !ctrl => {
                let ch = key.chars().next().unwrap();
                self.lines[self.cursor_line].insert(self.cursor_col, ch);
                self.cursor_col += 1;
                self.dirty = true;
            }
            "Tab" if !ctrl => {
                // Insert 4 spaces
                for _ in 0..4 {
                    self.lines[self.cursor_line].insert(self.cursor_col, ' ');
                    self.cursor_col += 1;
                }
                self.dirty = true;
            }
            _ => {}
        }

        self.render()
    }

    fn render(&self) -> String {
        let inner_w = self.width.saturating_sub(2) as usize;
        let visible = self.visible_rows();
        let mut out: Vec<String> = Vec::new();

        // Status line
        let status = if let Some(ref s) = self.status {
            s.clone()
        } else {
            let modified = if self.dirty { " [modified]" } else { "" };
            format!("Line {}, Col {}  ({} lines){}", self.cursor_line + 1, self.cursor_col + 1, self.lines.len(), modified)
        };
        out.push(truncate(&status, inner_w));

        // Separator
        out.push("\u{2500}".repeat(inner_w));

        // Content lines
        for i in self.scroll..(self.scroll + visible).min(self.lines.len()) {
            let line = &self.lines[i];
            let max_col = inner_w.saturating_sub(1);

            if i == self.cursor_line {
                // Show cursor as |
                let col = self.cursor_col.min(line.len());
                let before: String = line.chars().take(col).collect();
                let after: String = line.chars().skip(col).collect();
                let display = format!("{}\u{2502}{}", before, after);
                out.push(truncate(&display, inner_w));
            } else {
                out.push(truncate(line, max_col));
            }
        }

        // Pad empty lines
        while out.len() < visible + 2 {
            out.push(String::new());
        }

        // Help
        let help = "Ctrl+S=Save  Esc=Save&Close  Arrows/PgUp/Dn=Navigate";
        out.push(truncate(help, inner_w));

        let content_height = self.height.saturating_sub(2) as usize;
        out.truncate(content_height);
        while out.len() < content_height {
            out.push(String::new());
        }

        let title = if self.dirty { " Notes [*] " } else { " Notes " };

        let lines_json: Vec<String> = out.iter()
            .map(|l| format!("\"{}\"", escape_json(l)))
            .collect();

        format!(
            r#"{{"title":"{}","width":{},"height":{},"close":false,"lines":[{}]}}"#,
            escape_json(title), self.width, self.height, lines_json.join(",")
        )
    }
}

fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max { s.to_string() } else { chars[..max].iter().collect() }
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

fn extract_str_array(json: &str, key: &str) -> Vec<String> {
    let pattern = format!("\"{}\":", key);
    let start = match json.find(&pattern) {
        Some(s) => s + pattern.len(),
        None => return Vec::new(),
    };
    let rest = json[start..].trim_start();
    if !rest.starts_with('[') { return Vec::new(); }
    let mut depth = 0;
    let mut end = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => { depth -= 1; if depth == 0 { end = i; break; } }
            _ => {}
        }
    }
    if end == 0 { return Vec::new(); }
    let inner = &rest[1..end];
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escape = false;
    for c in inner.chars() {
        if escape { current.push(c); escape = false; continue; }
        match c {
            '\\' if in_string => escape = true,
            '"' => { if in_string { result.push(current.clone()); current.clear(); } in_string = !in_string; }
            _ if in_string => current.push(c),
            _ => {}
        }
    }
    result
}

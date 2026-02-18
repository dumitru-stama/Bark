//! Bark Stopwatch Plugin — Timer with lap support.
//!
//! Uses the overlay tick mechanism for live updates while running.

use std::io::{self, BufRead, Write};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--plugin-info") {
        println!(
            r#"{{"name":"Stopwatch","version":"1.0.0","type":"overlay","description":"Timer with lap support","width":42,"height":18}}"#
        );
        return;
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut state = StopwatchState::new();

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
                state.width = extract_int(line, "width").unwrap_or(42) as u16;
                state.height = extract_int(line, "height").unwrap_or(18) as u16;
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
            "tick" => {
                // Periodic update — just re-render with current time
                let response = state.render();
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

struct StopwatchState {
    running: bool,
    /// When the stopwatch was started (or last resumed)
    start_time: Option<Instant>,
    /// Accumulated time from previous runs (before last pause)
    accumulated: f64,
    /// Lap times (elapsed at moment of lap)
    laps: Vec<f64>,
    width: u16,
    height: u16,
}

impl StopwatchState {
    fn new() -> Self {
        Self {
            running: false,
            start_time: None,
            accumulated: 0.0,
            laps: Vec::new(),
            width: 42,
            height: 18,
        }
    }

    fn elapsed(&self) -> f64 {
        let current = if let Some(start) = self.start_time {
            start.elapsed().as_secs_f64()
        } else {
            0.0
        };
        self.accumulated + current
    }

    fn handle_key(&mut self, key: &str, modifiers: &[String]) -> String {
        let ctrl = modifiers.iter().any(|m| m == "ctrl");

        match key {
            "Escape" => return r#"{"close":true}"#.to_string(),
            "Enter" if ctrl => return r#"{"close":true}"#.to_string(),

            // Space or Enter = Start/Stop
            " " | "Enter" => {
                if self.running {
                    // Pause
                    self.accumulated = self.elapsed();
                    self.start_time = None;
                    self.running = false;
                } else {
                    // Start/Resume
                    self.start_time = Some(Instant::now());
                    self.running = true;
                }
            }

            // R = Reset
            "r" | "R" if !ctrl => {
                self.running = false;
                self.start_time = None;
                self.accumulated = 0.0;
                self.laps.clear();
            }

            // L = Lap
            "l" | "L" if !ctrl => {
                if self.running || self.accumulated > 0.0 {
                    self.laps.push(self.elapsed());
                }
            }

            _ => {}
        }

        self.render()
    }

    fn render(&self) -> String {
        let inner_w = self.width.saturating_sub(2) as usize;
        let elapsed = self.elapsed();
        let mut lines: Vec<String> = Vec::new();

        // Status
        let status = if self.running { "RUNNING" } else if self.accumulated > 0.0 { "PAUSED" } else { "STOPPED" };
        lines.push(center(&format!("[{}]", status), inner_w));

        // Big time display
        lines.push(String::new());
        lines.push(center(&format_time(elapsed), inner_w));
        lines.push(String::new());

        // Separator
        lines.push("\u{2500}".repeat(inner_w));

        // Laps
        if self.laps.is_empty() {
            lines.push(center("No laps recorded", inner_w));
        } else {
            let visible_laps = self.height.saturating_sub(10) as usize;
            let start = self.laps.len().saturating_sub(visible_laps);
            for (i, &lap_time) in self.laps[start..].iter().enumerate() {
                let lap_num = start + i + 1;
                let split = if lap_num > 1 {
                    let prev = self.laps[start + i - 1];
                    format!("  (+{})", format_time(lap_time - prev))
                } else {
                    format!("  (+{})", format_time(lap_time))
                };
                let line = format!("  Lap {:>2}: {}{}", lap_num, format_time(lap_time), split);
                lines.push(truncate(&line, inner_w));
            }
        }

        // Pad
        let content_height = self.height.saturating_sub(2) as usize;
        while lines.len() < content_height.saturating_sub(1) {
            lines.push(String::new());
        }

        // Help
        let help = "Space=Start/Stop  R=Reset  L=Lap  Esc=Close";
        lines.push(truncate(help, inner_w));

        lines.truncate(content_height);
        while lines.len() < content_height {
            lines.push(String::new());
        }

        let title = format!(" Stopwatch [{}] ", if self.running { "Running" } else { "Stopped" });

        let lines_json: Vec<String> = lines.iter()
            .map(|l| format!("\"{}\"", escape_json(l)))
            .collect();

        // Request ticks only when running (live updates)
        let tick = self.running;

        format!(
            r#"{{"title":"{}","width":{},"height":{},"close":false,"tick":{},"lines":[{}]}}"#,
            escape_json(&title), self.width, self.height, tick, lines_json.join(",")
        )
    }
}

fn format_time(seconds: f64) -> String {
    let total_ms = (seconds * 1000.0) as u64;
    let hrs = total_ms / 3_600_000;
    let mins = (total_ms % 3_600_000) / 60_000;
    let secs = (total_ms % 60_000) / 1_000;
    let ms = total_ms % 1_000;

    if hrs > 0 {
        format!("{:02}:{:02}:{:02}.{:03}", hrs, mins, secs, ms)
    } else {
        format!("{:02}:{:02}.{:03}", mins, secs, ms)
    }
}

fn center(s: &str, width: usize) -> String {
    if s.len() >= width {
        return s[..width].to_string();
    }
    let pad = (width - s.len()) / 2;
    format!("{:>width$}", s, width = s.len() + pad)
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

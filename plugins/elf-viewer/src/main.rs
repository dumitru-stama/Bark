//! ELF Viewer plugin executable
//!
//! This is an external plugin that communicates with Bark via JSON over stdin/stdout.
//! Protocol:
//! - `--plugin-info`: Print plugin metadata as JSON
//! - stdin/stdout: JSON commands and responses

use std::io::{self, BufRead, Write};

mod elf;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Handle --plugin-info
    if args.len() > 1 && args[1] == "--plugin-info" {
        print_plugin_info();
        return;
    }

    // Handle JSON commands on stdin
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let response = handle_command(&line);
        writeln!(stdout, "{}", response).ok();
        stdout.flush().ok();
    }
}

fn print_plugin_info() {
    println!(
        r#"{{"name":"ELF Viewer","version":"1.0.0","type":"viewer","description":"Displays ELF binary file headers","icon":"🔧","extensions":["elf","so","o","a","ko"],"magic":"7f454c46"}}"#
    );
}

fn handle_command(json: &str) -> String {
    let command = extract_string(json, "command").unwrap_or_default();

    match command.as_str() {
        "viewer_can_handle" => handle_can_handle(json),
        "viewer_render" => handle_render(json),
        _ => format!(r#"{{"error":"Unknown command: {}"}}"#, escape_json(&command)),
    }
}

fn handle_can_handle(json: &str) -> String {
    let path = extract_string(json, "path").unwrap_or_default();

    // Try to read magic bytes
    let magic = match std::fs::File::open(&path) {
        Ok(mut f) => {
            use std::io::Read;
            let mut buf = [0u8; 4];
            if f.read_exact(&mut buf).is_ok() {
                buf
            } else {
                return r#"{"can_handle":false,"priority":0}"#.to_string();
            }
        }
        Err(_) => return r#"{"can_handle":false,"priority":0}"#.to_string(),
    };

    if elf::is_elf(&magic) {
        r#"{"can_handle":true,"priority":10}"#.to_string()
    } else {
        r#"{"can_handle":false,"priority":0}"#.to_string()
    }
}

fn handle_render(json: &str) -> String {
    let path = extract_string(json, "path").unwrap_or_default();
    let scroll: usize = extract_int(json, "scroll").unwrap_or(0) as usize;
    let height: usize = extract_int(json, "height").unwrap_or(24) as usize;

    match elf::parse_elf(&path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();

            // Apply scrolling and height limit
            let visible: Vec<&str> = lines.into_iter().skip(scroll).take(height).collect();

            // Build JSON array of lines
            let lines_json: Vec<String> = visible
                .iter()
                .map(|l| format!("\"{}\"", escape_json(l)))
                .collect();

            format!(
                r#"{{"lines":[{}],"total_lines":{}}}"#,
                lines_json.join(","),
                total_lines
            )
        }
        Err(e) => {
            format!(r#"{{"error":"{}"}}"#, escape_json(&e))
        }
    }
}

// === JSON helpers ===

fn extract_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();

    if rest.starts_with('"') {
        let rest = &rest[1..];
        let mut result = String::new();
        let mut chars = rest.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => {
                    if let Some(&next) = chars.peek() {
                        chars.next();
                        match next {
                            'n' => result.push('\n'),
                            'r' => result.push('\r'),
                            't' => result.push('\t'),
                            '"' => result.push('"'),
                            '\\' => result.push('\\'),
                            _ => {
                                result.push('\\');
                                result.push(next);
                            }
                        }
                    }
                }
                _ => result.push(c),
            }
        }

        Some(result)
    } else {
        // Non-string value
        let end = rest.find(|c: char| c == ',' || c == '}' || c == ']')?;
        Some(rest[..end].trim().to_string())
    }
}

fn extract_int(json: &str, key: &str) -> Option<i64> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();

    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '-')
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

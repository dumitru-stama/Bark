//! PDF Viewer plugin executable
//!
//! This is an external plugin that communicates with Bark via JSON over stdin/stdout.
//! Protocol:
//! - `--plugin-info`: Print plugin metadata as JSON
//! - stdin/stdout: JSON commands and responses

use std::io::{self, BufRead, Write};

mod pdf_info;

/// Cached render output for a single file.  Metadata lines are populated on
/// first access; text content lines are appended lazily when the visible
/// window scrolls past the metadata.
struct RenderCache {
    path: String,
    /// Pre-split lines (metadata first, then text content once extracted).
    lines: Vec<String>,
    /// Number of lines that belong to the metadata sections.
    metadata_lines: usize,
    /// Whether text extraction has already been performed.
    text_extracted: bool,
}

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
    let mut cache: Option<RenderCache> = None;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let response = handle_command(&line, &mut cache);
        writeln!(stdout, "{}", response).ok();
        stdout.flush().ok();
    }
}

fn print_plugin_info() {
    println!(
        r#"{{"name":"PDF Viewer","version":"1.0.0","type":"viewer","description":"Displays PDF document metadata, structure, and text content","icon":"","extensions":["pdf"],"magic":"255044462d"}}"#
    );
}

fn handle_command(json: &str, cache: &mut Option<RenderCache>) -> String {
    let command = extract_string(json, "command").unwrap_or_default();

    match command.as_str() {
        "viewer_can_handle" => handle_can_handle(json),
        "viewer_render" => handle_render(json, cache),
        _ => format!(r#"{{"error":"Unknown command: {}"}}"#, escape_json(&command)),
    }
}

fn handle_can_handle(json: &str) -> String {
    let path = extract_string(json, "path").unwrap_or_default();

    // Try to read magic bytes: %PDF- (25 50 44 46 2d)
    let magic = match std::fs::File::open(&path) {
        Ok(mut f) => {
            use std::io::Read;
            let mut buf = [0u8; 5];
            if f.read_exact(&mut buf).is_ok() {
                buf
            } else {
                return r#"{"can_handle":false,"priority":0}"#.to_string();
            }
        }
        Err(_) => return r#"{"can_handle":false,"priority":0}"#.to_string(),
    };

    if &magic == b"%PDF-" {
        r#"{"can_handle":true,"priority":10}"#.to_string()
    } else {
        r#"{"can_handle":false,"priority":0}"#.to_string()
    }
}

fn handle_render(json: &str, cache: &mut Option<RenderCache>) -> String {
    let path = extract_string(json, "path").unwrap_or_default();
    let scroll: usize = extract_int(json, "scroll").unwrap_or(0) as usize;
    let height: usize = extract_int(json, "height").unwrap_or(24) as usize;

    // Invalidate cache if viewing a different file.
    let needs_invalidate = match cache {
        Some(c) => c.path != path,
        None => false,
    };
    if needs_invalidate {
        *cache = None;
    }

    // Populate metadata on first access.
    if cache.is_none() {
        match pdf_info::parse_pdf_metadata(&path) {
            Ok(metadata) => {
                let lines: Vec<String> = metadata.lines().map(|l| l.to_string()).collect();
                let count = lines.len();
                *cache = Some(RenderCache {
                    path: path.clone(),
                    lines,
                    metadata_lines: count,
                    text_extracted: false,
                });
            }
            Err(e) => {
                return format!(r#"{{"error":"{}"}}"#, escape_json(&e));
            }
        }
    }

    let c = cache.as_mut().unwrap();

    // Lazily extract text when the visible window reaches past metadata.
    if !c.text_extracted && scroll.saturating_add(height) >= c.metadata_lines {
        c.text_extracted = true;
        let text = pdf_info::parse_pdf_text(&c.path);
        if !text.is_empty() {
            for line in text.lines() {
                c.lines.push(line.to_string());
            }
        }
    }

    let total_lines = c.lines.len();
    let visible: Vec<&str> = c.lines.iter()
        .skip(scroll)
        .take(height)
        .map(|s| s.as_str())
        .collect();

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

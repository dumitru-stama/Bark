//! Hex Editor launcher plugin for Bark
//!
//! This is a viewer plugin that launches an external hex editor (default: jinx)
//! for any file. It has the lowest priority (1) so all other viewer plugins
//! take precedence.
//!
//! The editor command is configurable via `editor.hex_editor` in Bark's
//! config.toml (default: "jinx"). Bark passes the config to the plugin
//! in the `viewer_render` JSON command.
//!
//! On Unix, the editor is spawned with /dev/tty so it gets direct terminal
//! access even though the plugin's own stdin/stdout are piped by Bark.
//! On Windows, the editor is spawned with CONIN$/CONOUT$ handles (the
//! Windows equivalent of /dev/tty) for the same reason.

use std::io::{self, BufRead, Write};
use std::process::Command;

const DEFAULT_EDITOR: &str = "jinx";

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "--plugin-info" {
        print_plugin_info();
        return;
    }

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
        r#"{{"name":"HexEditor","version":"1.0.0","type":"viewer","description":"Launches an external hex editor (configurable via editor.hex_editor in config.toml, default: jinx)","icon":"H","extensions":["*"],"needs_terminal":true}}"#
    );
}

/// Extract the hex editor command from the config object in the JSON,
/// falling back to DEFAULT_EDITOR.
fn get_editor(json: &str) -> String {
    // The config is a nested object: "config":{"editor.hex_editor":"jinx",...}
    // First, find the config object, then extract the key from within it.
    if let Some(config_str) = extract_object(json, "config") {
        if let Some(editor) = extract_string(&config_str, "editor.hex_editor") {
            if !editor.is_empty() {
                return editor;
            }
        }
    }
    DEFAULT_EDITOR.to_string()
}

fn handle_command(json: &str) -> String {
    let command = extract_string(json, "command").unwrap_or_default();

    match command.as_str() {
        "viewer_can_handle" => handle_can_handle(),
        "viewer_render" => handle_render(json),
        _ => format!(r#"{{"error":"Unknown command: {}"}}"#, escape_json(&command)),
    }
}

fn handle_can_handle() -> String {
    // Accept any file, but with lowest priority so other viewers win
    r#"{"can_handle":true,"priority":1}"#.to_string()
}

fn handle_render(json: &str) -> String {
    let path = extract_string(json, "path").unwrap_or_default();
    let editor = get_editor(json);

    launch_editor(&editor, &path);

    // Return without "lines" so Bark's render() returns None and falls
    // through to the built-in viewer, which re-reads the (possibly modified)
    // file and auto-detects text vs binary mode.
    r#"{"launched":true}"#.to_string()
}

#[cfg(unix)]
fn launch_editor(editor: &str, path: &str) -> bool {
    // Open /dev/tty for direct terminal access.
    // We need separate file handles because Stdio::from() takes ownership.
    let tty_in = match std::fs::File::open("/dev/tty") {
        Ok(f) => f,
        Err(_) => return false,
    };
    let tty_out = match std::fs::OpenOptions::new().write(true).open("/dev/tty") {
        Ok(f) => f,
        Err(_) => return false,
    };
    let tty_err = match std::fs::OpenOptions::new().write(true).open("/dev/tty") {
        Ok(f) => f,
        Err(_) => return false,
    };

    Command::new(editor)
        .arg(path)
        .stdin(tty_in)
        .stdout(tty_out)
        .stderr(tty_err)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn launch_editor(editor: &str, path: &str) -> bool {
    // CONIN$ and CONOUT$ are the Windows equivalents of /dev/tty — they
    // always refer to the attached console regardless of handle redirection.
    // Without this, the child inherits our piped stdin/stdout and can't
    // display anything in the console (the "black window" problem on Win10).
    let con_in = match std::fs::File::open("CONIN$") {
        Ok(f) => f,
        Err(_) => return false,
    };
    let con_out = match std::fs::OpenOptions::new().write(true).open("CONOUT$") {
        Ok(f) => f,
        Err(_) => return false,
    };
    let con_err = match std::fs::OpenOptions::new().write(true).open("CONOUT$") {
        Ok(f) => f,
        Err(_) => return false,
    };

    Command::new(editor)
        .arg(path)
        .stdin(con_in)
        .stdout(con_out)
        .stderr(con_err)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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
        let end = rest.find(|c: char| c == ',' || c == '}' || c == ']')?;
        Some(rest[..end].trim().to_string())
    }
}

/// Extract a nested JSON object as a raw string (including braces).
fn extract_object(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();

    if !rest.starts_with('{') {
        return None;
    }

    let mut depth = 0;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in rest.char_indices() {
        if esc {
            esc = false;
            continue;
        }
        match c {
            '\\' if in_str => esc = true,
            '"' => in_str = !in_str,
            '{' if !in_str => depth += 1,
            '}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return Some(rest[..i + c.len_utf8()].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

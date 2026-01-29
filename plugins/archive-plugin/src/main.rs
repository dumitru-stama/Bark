//! Archive provider plugin executable
//!
//! Extension-based provider plugin for browsing archive files
//! (zip, tar, tar.gz, tar.bz2, tar.xz, 7z, xz, gz, bz2).
//!
//! Protocol:
//! - `--plugin-info`: Print plugin metadata (type=provider, extensions=[...])
//! - stdin/stdout: JSON commands and responses

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;

mod archive;
use archive::{ArchiveSession, ArchiveType};

/// Global session storage
static SESSION: Mutex<Option<ArchiveSession>> = Mutex::new(None);

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
    let extensions = ArchiveType::all_extensions();
    let ext_json: Vec<String> = extensions.iter().map(|e| format!("\"{}\"", e)).collect();

    println!(
        r#"{{"name":"Archive Provider","version":"0.1","type":"provider","extensions":[{}],"description":"Browse archive files (zip, tar, 7z, xz, gz, bz2)","icon":"{}"}}"#,
        ext_json.join(","),
        '\u{1F4E6}' // 📦
    );
}

fn handle_command(json: &str) -> String {
    let command = extract_string(json, "command").unwrap_or_default();

    match command.as_str() {
        "get_dialog_fields" => r#"{"fields":[]}"#.to_string(),
        "validate_config" => r#"{"valid":true}"#.to_string(),
        "connect" => handle_connect(json),
        "disconnect" => handle_disconnect(),
        "list_directory" => handle_list_directory(json),
        "read_file" => handle_read_file(json),
        "write_file" => r#"{"error":"Archives are read-only","error_type":"permission"}"#.to_string(),
        "delete" => r#"{"error":"Archives are read-only","error_type":"permission"}"#.to_string(),
        "mkdir" => r#"{"error":"Archives are read-only","error_type":"permission"}"#.to_string(),
        "rename" => r#"{"error":"Archives are read-only","error_type":"permission"}"#.to_string(),
        "copy_file" => r#"{"error":"Archives are read-only","error_type":"permission"}"#.to_string(),
        _ => format!(r#"{{"error":"Unknown command: {}"}}"#, escape_json(&command)),
    }
}

fn handle_connect(json: &str) -> String {
    // Extract the archive path from config
    let path = extract_config_value(json, "path").unwrap_or_default();

    if path.is_empty() {
        return r#"{"success":false,"error":"No archive path provided"}"#.to_string();
    }

    match ArchiveSession::open(PathBuf::from(&path)) {
        Ok(session) => {
            let label = session.short_label();
            let mut guard = SESSION.lock().unwrap();
            *guard = Some(session);
            format!(r#"{{"success":true,"session_id":"default","short_label":"{}"}}"#, escape_json(&label))
        }
        Err(e) => format!(r#"{{"success":false,"error":"{}"}}"#, escape_json(&e)),
    }
}

fn handle_disconnect() -> String {
    let mut guard = SESSION.lock().unwrap();
    *guard = None;
    r#"{"success":true}"#.to_string()
}

fn handle_list_directory(json: &str) -> String {
    let path = extract_string(json, "path").unwrap_or_else(|| "/".to_string());

    let guard = SESSION.lock().unwrap();
    let session = match guard.as_ref() {
        Some(s) => s,
        None => return r#"{"error":"Not connected"}"#.to_string(),
    };

    let entries = session.list_directory(&path);

    let entries_json: Vec<String> = entries
        .iter()
        .map(|e| {
            let modified_ts = e.modified
                .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            format!(
                r#"{{"name":"{}","path":"{}","is_dir":{},"size":{},"is_hidden":{},"permissions":{},"is_symlink":false,"modified":{}}}"#,
                escape_json(&e.name),
                escape_json(&e.path.to_string_lossy()),
                e.is_dir,
                e.size,
                e.is_hidden,
                e.permissions,
                modified_ts
            )
        })
        .collect();

    format!(r#"{{"entries":[{}]}}"#, entries_json.join(","))
}

fn handle_read_file(json: &str) -> String {
    let path = extract_string(json, "path").unwrap_or_default();

    let guard = SESSION.lock().unwrap();
    let session = match guard.as_ref() {
        Some(s) => s,
        None => return r#"{"error":"Not connected"}"#.to_string(),
    };

    match session.read_file(&path) {
        Ok(data) => {
            let b64 = base64_encode(&data);
            format!(r#"{{"data":"{}"}}"#, b64)
        }
        Err(e) => format!(r#"{{"error":"{}","error_type":"not_found"}}"#, escape_json(&e)),
    }
}

// === JSON helpers ===

fn extract_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = &json[start..];
    let rest = rest.trim_start();

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
    } else if rest.starts_with('{') {
        let mut depth = 0;
        let mut end = 0;
        for (i, c) in rest.chars().enumerate() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        Some(rest[..end].to_string())
    } else {
        let end = rest.find(|c: char| c == ',' || c == '}' || c == ']')?;
        Some(rest[..end].trim().to_string())
    }
}

fn extract_config_value(json: &str, key: &str) -> Option<String> {
    let config_json = extract_string(json, "config")?;
    extract_string(&config_json, key)
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }

    result
}

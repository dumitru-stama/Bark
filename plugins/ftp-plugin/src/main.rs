//! FTP/FTPS provider plugin executable
//!
//! This is an external plugin that communicates with Bark via JSON over stdin/stdout.
//! Protocol:
//! - `--plugin-info`: Print plugin metadata as JSON
//! - stdin/stdout: JSON-RPC style commands and responses

use std::io::{self, BufRead, Write};
use std::sync::Mutex;

mod ftp;
use ftp::{FtpProviderPlugin, FtpProviderSession};

use bark_plugin_api::{ProviderConfig, ProviderPlugin, ProviderSession};

/// Global session storage (simple single-session for now)
static SESSION: Mutex<Option<FtpProviderSession>> = Mutex::new(None);

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
    let plugin = FtpProviderPlugin::new();
    let info = plugin.info();

    println!(
        r#"{{"name":"{}","version":"{}","type":"provider","schemes":{},"description":"{}","icon":"{}"}}"#,
        escape_json(&info.name),
        escape_json(&info.version),
        format!(
            "[{}]",
            info.schemes
                .iter()
                .map(|s| format!("\"{}\"", escape_json(s)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        escape_json(&info.description),
        info.icon.unwrap_or(' ')
    );
}

fn handle_command(json: &str) -> String {
    let command = extract_string(json, "command").unwrap_or_default();

    match command.as_str() {
        "get_dialog_fields" => handle_get_dialog_fields(),
        "validate_config" => handle_validate_config(json),
        "connect" => handle_connect(json),
        "disconnect" => handle_disconnect(),
        "list_directory" => handle_list_directory(json),
        "read_file" => handle_read_file(json),
        "write_file" => handle_write_file(json),
        "delete" => handle_delete(json),
        "mkdir" => handle_mkdir(json),
        "rename" => handle_rename(json),
        "copy_file" => handle_copy_file(json),
        _ => format!(r#"{{"error":"Unknown command: {}"}}"#, escape_json(&command)),
    }
}

fn handle_get_dialog_fields() -> String {
    let plugin = FtpProviderPlugin::new();
    let fields = plugin.get_dialog_fields();

    let fields_json: Vec<String> = fields
        .iter()
        .map(|f| {
            let field_type = match &f.field_type {
                bark_plugin_api::DialogFieldType::Text => "text",
                bark_plugin_api::DialogFieldType::Password => "password",
                bark_plugin_api::DialogFieldType::Number => "number",
                bark_plugin_api::DialogFieldType::Checkbox => "checkbox",
                bark_plugin_api::DialogFieldType::Select { .. } => "select",
                bark_plugin_api::DialogFieldType::TextArea => "textarea",
                bark_plugin_api::DialogFieldType::FilePath => "filepath",
            };

            format!(
                r#"{{"id":"{}","label":"{}","type":"{}","required":{},"default":{}}}"#,
                escape_json(&f.id),
                escape_json(&f.label),
                field_type,
                f.required,
                f.default_value
                    .as_ref()
                    .map(|v| format!("\"{}\"", escape_json(v)))
                    .unwrap_or_else(|| "null".to_string())
            )
        })
        .collect();

    format!(r#"{{"fields":[{}]}}"#, fields_json.join(","))
}

fn handle_validate_config(json: &str) -> String {
    let config = parse_config(json);
    let plugin = FtpProviderPlugin::new();

    match plugin.validate_config(&config) {
        Ok(()) => r#"{"valid":true}"#.to_string(),
        Err(e) => format!(r#"{{"valid":false,"error":"{}"}}"#, escape_json(&e.to_string())),
    }
}

fn handle_connect(json: &str) -> String {
    let config = parse_config(json);
    let plugin = FtpProviderPlugin::new();

    match plugin.connect(&config) {
        Ok(session) => {
            // Downcast to FtpProviderSession
            // Since we control both sides, we know the type
            let ftp_session = unsafe {
                // This is safe because we know FtpProviderPlugin::connect returns FtpProviderSession
                let raw = Box::into_raw(session);
                Box::from_raw(raw as *mut FtpProviderSession)
            };

            let mut guard = SESSION.lock().unwrap();
            *guard = Some(*ftp_session);

            r#"{"success":true,"session_id":"default"}"#.to_string()
        }
        Err(e) => format!(r#"{{"success":false,"error":"{}"}}"#, escape_json(&e.to_string())),
    }
}

fn handle_disconnect() -> String {
    let mut guard = SESSION.lock().unwrap();
    if let Some(ref mut session) = *guard {
        session.disconnect();
    }
    *guard = None;
    r#"{"success":true}"#.to_string()
}

fn handle_list_directory(json: &str) -> String {
    let path = extract_string(json, "path").unwrap_or_else(|| "/".to_string());

    let mut guard = SESSION.lock().unwrap();
    let session = match guard.as_mut() {
        Some(s) => s,
        None => return r#"{"error":"Not connected"}"#.to_string(),
    };

    match session.list_directory(&path) {
        Ok(entries) => {
            let entries_json: Vec<String> = entries
                .iter()
                .map(|e| {
                    format!(
                        r#"{{"name":"{}","path":"{}","is_dir":{},"size":{},"is_hidden":{},"permissions":{},"is_symlink":{}}}"#,
                        escape_json(&e.name),
                        escape_json(&e.path.to_string_lossy()),
                        e.is_dir,
                        e.size,
                        e.is_hidden,
                        e.permissions,
                        e.is_symlink
                    )
                })
                .collect();

            format!(r#"{{"entries":[{}]}}"#, entries_json.join(","))
        }
        Err(e) => format!(r#"{{"error":"{}"}}"#, escape_json(&e.to_string())),
    }
}

fn handle_read_file(json: &str) -> String {
    let path = extract_string(json, "path").unwrap_or_default();

    let mut guard = SESSION.lock().unwrap();
    let session = match guard.as_mut() {
        Some(s) => s,
        None => return r#"{"error":"Not connected"}"#.to_string(),
    };

    match session.read_file(&path) {
        Ok(data) => {
            let b64 = base64_encode(&data);
            format!(r#"{{"data":"{}"}}"#, b64)
        }
        Err(e) => format!(r#"{{"error":"{}"}}"#, escape_json(&e.to_string())),
    }
}

fn handle_write_file(json: &str) -> String {
    let path = extract_string(json, "path").unwrap_or_default();
    let data_b64 = extract_string(json, "data").unwrap_or_default();

    let data = match base64_decode(&data_b64) {
        Ok(d) => d,
        Err(e) => return format!(r#"{{"error":"Invalid base64: {}"}}"#, e),
    };

    let mut guard = SESSION.lock().unwrap();
    let session = match guard.as_mut() {
        Some(s) => s,
        None => return r#"{"error":"Not connected"}"#.to_string(),
    };

    match session.write_file(&path, &data) {
        Ok(()) => r#"{"success":true}"#.to_string(),
        Err(e) => format!(r#"{{"error":"{}"}}"#, escape_json(&e.to_string())),
    }
}

fn handle_delete(json: &str) -> String {
    let path = extract_string(json, "path").unwrap_or_default();

    let mut guard = SESSION.lock().unwrap();
    let session = match guard.as_mut() {
        Some(s) => s,
        None => return r#"{"error":"Not connected"}"#.to_string(),
    };

    match session.delete(&path) {
        Ok(()) => r#"{"success":true}"#.to_string(),
        Err(e) => format!(r#"{{"error":"{}"}}"#, escape_json(&e.to_string())),
    }
}

fn handle_mkdir(json: &str) -> String {
    let path = extract_string(json, "path").unwrap_or_default();

    let mut guard = SESSION.lock().unwrap();
    let session = match guard.as_mut() {
        Some(s) => s,
        None => return r#"{"error":"Not connected"}"#.to_string(),
    };

    match session.mkdir(&path) {
        Ok(()) => r#"{"success":true}"#.to_string(),
        Err(e) => format!(r#"{{"error":"{}"}}"#, escape_json(&e.to_string())),
    }
}

fn handle_rename(json: &str) -> String {
    let from = extract_string(json, "from").unwrap_or_default();
    let to = extract_string(json, "to").unwrap_or_default();

    let mut guard = SESSION.lock().unwrap();
    let session = match guard.as_mut() {
        Some(s) => s,
        None => return r#"{"error":"Not connected"}"#.to_string(),
    };

    match session.rename(&from, &to) {
        Ok(()) => r#"{"success":true}"#.to_string(),
        Err(e) => format!(r#"{{"error":"{}"}}"#, escape_json(&e.to_string())),
    }
}

fn handle_copy_file(json: &str) -> String {
    let from = extract_string(json, "from").unwrap_or_default();
    let to = extract_string(json, "to").unwrap_or_default();

    let mut guard = SESSION.lock().unwrap();
    let session = match guard.as_mut() {
        Some(s) => s,
        None => return r#"{"error":"Not connected"}"#.to_string(),
    };

    match session.copy_file(&from, &to) {
        Ok(()) => r#"{"success":true}"#.to_string(),
        Err(e) => format!(r#"{{"error":"{}"}}"#, escape_json(&e.to_string())),
    }
}

// === JSON helpers (simple, no dependencies) ===

fn extract_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = &json[start..];
    let rest = rest.trim_start();

    if rest.starts_with('"') {
        // String value
        let rest = &rest[1..];
        let end = rest.find('"')?;
        Some(unescape_json(&rest[..end]))
    } else if rest.starts_with('{') {
        // Object value - find matching brace
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
        // Other value (number, bool, null)
        let end = rest.find(|c: char| c == ',' || c == '}' || c == ']')?;
        Some(rest[..end].trim().to_string())
    }
}

fn parse_config(json: &str) -> ProviderConfig {
    let mut config = ProviderConfig::new();

    // Extract config object
    if let Some(config_json) = extract_string(json, "config") {
        // Parse simple key-value pairs from the config object
        let keys = [
            "name", "host", "port", "user", "password", "path", "passive_mode", "use_tls",
        ];
        for key in keys {
            if let Some(value) = extract_string(&config_json, key) {
                config.set(key, value);
            }
        }
    }

    config
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn unescape_json(s: &str) -> String {
    s.replace("\\\"", "\"")
        .replace("\\\\", "\\")
        .replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t")
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let chunks = data.chunks(3);

    for chunk in chunks {
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

fn base64_decode(s: &str) -> Result<Vec<u8>, &'static str> {
    const DECODE: [i8; 128] = [
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 62, -1, -1,
        -1, 63, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, -1, -1, -1, -1, -1, -1, -1, 0, 1, 2, 3, 4,
        5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, -1, -1, -1,
        -1, -1, -1, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
        46, 47, 48, 49, 50, 51, -1, -1, -1, -1, -1,
    ];

    let s = s.trim_end_matches('=');
    let mut result = Vec::with_capacity((s.len() * 3) / 4);
    let bytes: Vec<u8> = s.bytes().collect();

    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            break;
        }

        let b0 = DECODE.get(chunk[0] as usize).copied().unwrap_or(-1);
        let b1 = DECODE.get(chunk[1] as usize).copied().unwrap_or(-1);
        let b2 = chunk.get(2).and_then(|&c| DECODE.get(c as usize)).copied().unwrap_or(0);
        let b3 = chunk.get(3).and_then(|&c| DECODE.get(c as usize)).copied().unwrap_or(0);

        if b0 < 0 || b1 < 0 {
            return Err("Invalid base64");
        }

        result.push(((b0 << 2) | (b1 >> 4)) as u8);
        if chunk.len() > 2 {
            result.push((((b1 & 0x0f) << 4) | (b2 >> 2)) as u8);
        }
        if chunk.len() > 3 {
            result.push((((b2 & 0x03) << 6) | b3) as u8);
        }
    }

    Ok(result)
}

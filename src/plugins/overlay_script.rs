//! Script-based overlay plugins
//!
//! Overlay plugins are persistent child processes that render interactive
//! dialog overlays. Communication is JSON over stdin/stdout.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use crate::plugins::api::{OverlayPluginInfo, OverlayRenderResult};

/// A script-based overlay plugin (loaded from executable)
pub struct ScriptOverlayPlugin {
    pub info: OverlayPluginInfo,
    pub executable: PathBuf,
}

impl ScriptOverlayPlugin {
    /// Load an overlay plugin from an executable path
    pub fn load(path: &Path) -> Result<Self, String> {
        let output = Command::new(path)
            .arg("--plugin-info")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("Failed to execute plugin: {}", e))?;

        if !output.status.success() {
            return Err(format!("Plugin exited with error: {:?}", output.status));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let info = Self::parse_info(&stdout, path)?;

        Ok(ScriptOverlayPlugin {
            info,
            executable: path.to_path_buf(),
        })
    }

    fn parse_info(output: &str, path: &Path) -> Result<OverlayPluginInfo, String> {
        let mut name = "Unknown Overlay".to_string();
        let mut description = String::new();
        let mut width: u16 = 46;
        let mut height: u16 = 18;

        for line in output.lines() {
            let line = line.trim();
            if line.starts_with('{') {
                if let Some(n) = extract_json_string(line, "name") {
                    name = n;
                }
                if let Some(d) = extract_json_string(line, "description") {
                    description = d;
                }
                if let Some(w) = extract_json_int(line, "width") {
                    width = w as u16;
                }
                if let Some(h) = extract_json_int(line, "height") {
                    height = h as u16;
                }

                // Verify plugin type
                if let Some(t) = extract_json_string(line, "type")
                    && t != "overlay"
                {
                    return Err(format!("Not an overlay plugin (type: {})", t));
                }
                break;
            }
        }

        Ok(OverlayPluginInfo {
            name,
            description,
            width,
            height,
            source: path.to_path_buf(),
        })
    }

    /// Start a new overlay session (spawns child process)
    pub fn start_session(&self) -> Result<ScriptOverlaySession, String> {
        let mut child = Command::new(&self.executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn overlay plugin: {}", e))?;

        let child_stdin = child.stdin.take()
            .ok_or_else(|| "Failed to open plugin stdin".to_string())?;
        let child_stdout = child.stdout.take()
            .ok_or_else(|| "Failed to open plugin stdout".to_string())?;

        Ok(ScriptOverlaySession {
            child: Some(child),
            stdin: Mutex::new(child_stdin),
            stdout: Mutex::new(std::io::BufReader::new(child_stdout)),
        })
    }
}

/// An active overlay session (persistent child process)
pub struct ScriptOverlaySession {
    child: Option<Child>,
    stdin: Mutex<std::process::ChildStdin>,
    stdout: Mutex<std::io::BufReader<std::process::ChildStdout>>,
}

// Safety: the child process handles are accessed through Mutex locks
unsafe impl Send for ScriptOverlaySession {}

impl Drop for ScriptOverlaySession {
    fn drop(&mut self) {
        let _ = self.send_raw("{\"command\":\"close\"}\n");
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl ScriptOverlaySession {
    fn send_raw(&self, request: &str) -> Result<String, String> {
        {
            let mut stdin = self.stdin.lock().unwrap();
            stdin.write_all(request.as_bytes())
                .map_err(|e| format!("Failed to write to overlay plugin: {}", e))?;
            stdin.flush()
                .map_err(|e| format!("Failed to flush overlay plugin stdin: {}", e))?;
        }

        let response = {
            let mut stdout = self.stdout.lock().unwrap();
            let mut line = String::new();
            stdout.read_line(&mut line)
                .map_err(|e| format!("Failed to read from overlay plugin: {}", e))?;
            line
        };

        let response = response.trim().to_string();
        if response.is_empty() {
            return Err("Empty response from overlay plugin".to_string());
        }
        Ok(response)
    }

    fn parse_render_result(response: &str) -> Result<OverlayRenderResult, String> {
        let title = extract_json_string(response, "title").unwrap_or_default();
        let width = extract_json_int(response, "width").unwrap_or(46) as u16;
        let height = extract_json_int(response, "height").unwrap_or(18) as u16;
        let close = extract_json_bool(response, "close").unwrap_or(false);
        let tick = extract_json_bool(response, "tick").unwrap_or(false);
        let lines = extract_json_string_array(response, "lines").unwrap_or_default();

        Ok(OverlayRenderResult { lines, title, width, height, close, tick })
    }

    /// Send init command and get initial render
    pub fn init(&self, width: u16, height: u16) -> Result<OverlayRenderResult, String> {
        let request = format!("{{\"command\":\"init\",\"width\":{},\"height\":{}}}\n", width, height);
        let response = self.send_raw(&request)?;
        Self::parse_render_result(&response)
    }

    /// Send a key event and get updated render
    pub fn send_key(&self, key: &str, modifiers: &[&str]) -> Result<OverlayRenderResult, String> {
        let mods_json: Vec<String> = modifiers.iter().map(|m| format!("\"{}\"", m)).collect();
        let request = format!(
            "{{\"command\":\"key\",\"key\":\"{}\",\"modifiers\":[{}]}}\n",
            escape_json(key),
            mods_json.join(",")
        );
        let response = self.send_raw(&request)?;
        Self::parse_render_result(&response)
    }

    /// Send a tick command (for live-updating overlays like stopwatch)
    pub fn tick(&self) -> Result<OverlayRenderResult, String> {
        let response = self.send_raw("{\"command\":\"tick\"}\n")?;
        Self::parse_render_result(&response)
    }

    /// Close the session
    pub fn close(&mut self) {
        let _ = self.send_raw("{\"command\":\"close\"}\n");
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Check if the child process is still alive
    #[allow(dead_code)]
    pub fn is_alive(&mut self) -> bool {
        if let Some(child) = &mut self.child {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }
}

// ============================================================================
// JSON Helpers (same minimal approach as provider_script.rs)
// ============================================================================

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();

    if !rest.starts_with('"') {
        return None;
    }

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
}

fn extract_json_bool(json: &str, key: &str) -> Option<bool> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();

    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn extract_json_int(json: &str, key: &str) -> Option<i64> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();

    let end = rest.find(|c: char| !c.is_ascii_digit() && c != '-').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn extract_json_string_array(json: &str, key: &str) -> Option<Vec<String>> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();

    if !rest.starts_with('[') {
        return None;
    }

    let mut depth = 0;
    let mut end = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = i + c.len_utf8();
                    break;
                }
            }
            _ => {}
        }
    }

    if end == 0 {
        return None;
    }

    let array_str = &rest['['.len_utf8()..end - ']'.len_utf8()];
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escape = false;

    for c in array_str.chars() {
        if escape {
            match c {
                'n' => current.push('\n'),
                'r' => current.push('\r'),
                't' => current.push('\t'),
                '"' => current.push('"'),
                '\\' => current.push('\\'),
                _ => {
                    current.push('\\');
                    current.push(c);
                }
            }
            escape = false;
            continue;
        }

        match c {
            '\\' if in_string => escape = true,
            '"' => {
                if in_string {
                    result.push(current.clone());
                    current.clear();
                }
                in_string = !in_string;
            }
            ',' if !in_string => {}
            _ if in_string => current.push(c),
            _ => {}
        }
    }

    Some(result)
}

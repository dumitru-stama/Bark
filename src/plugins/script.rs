//! Script plugin support (external executables via stdin/stdout JSON protocol)

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::plugins::api::*;

/// A script plugin (external executable)
pub struct ScriptPlugin {
    info: PluginInfo,
    executable: PathBuf,
    /// Cached process for persistent plugins (optional optimization)
    process: Option<Child>,
}

impl ScriptPlugin {
    /// Load a script plugin from an executable
    pub fn load(path: &Path) -> Result<Self, String> {
        // Query the plugin for its info
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

        Ok(ScriptPlugin {
            info,
            executable: path.to_path_buf(),
            process: None,
        })
    }

    fn parse_info(output: &str, path: &Path) -> Result<PluginInfo, String> {
        // Parse JSON output: {"name": "...", "version": "...", "type": "status|viewer"}
        let mut name = "Unknown".to_string();
        let mut version = "0.0".to_string();
        let mut plugin_type = PluginType::StatusBar;

        for line in output.lines() {
            let line = line.trim();
            if line.starts_with('{') {
                // Simple JSON parsing without external crates
                if let Some(n) = extract_json_string(line, "name") {
                    name = n;
                }
                if let Some(v) = extract_json_string(line, "version") {
                    version = v;
                }
                if let Some(t) = extract_json_string(line, "type") {
                    plugin_type = match t.as_str() {
                        "status" | "statusbar" | "status_bar" => PluginType::StatusBar,
                        "viewer" | "view" => PluginType::Viewer,
                        _ => PluginType::StatusBar,
                    };
                }
                break;
            }
        }

        Ok(PluginInfo {
            name,
            version,
            plugin_type,
            source: PluginSource::Script(path.to_path_buf()),
        })
    }

    fn execute_command(&self, command: &str, args: &str) -> Option<String> {
        // Build JSON request
        let request = if args.is_empty() {
            format!("{{\"command\":\"{}\"}}", command)
        } else {
            format!("{{\"command\":\"{}\",{}}}", command, args)
        };

        // Execute plugin with request on stdin
        let mut child = Command::new(&self.executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        // Write request
        if let Some(mut stdin) = child.stdin.take() {
            let _ = writeln!(stdin, "{}", request);
        }

        // Read response
        let output = child.wait_with_output().ok()?;
        if !output.status.success() {
            return None;
        }

        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn info(&self) -> &PluginInfo {
        &self.info
    }
}

impl StatusBarPlugin for ScriptPlugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }

    fn render(&self, context: &StatusContext) -> Option<StatusResult> {
        let args = format!(
            "\"path\":\"{}\",\"selected_file\":{},\"is_dir\":{},\"file_size\":{},\"selected_count\":{}",
            escape_json(&context.path.to_string_lossy()),
            context.selected_file.as_ref()
                .map(|s| format!("\"{}\"", escape_json(s)))
                .unwrap_or_else(|| "null".to_string()),
            context.is_dir,
            context.file_size,
            context.selected_count
        );

        let response = self.execute_command("status_render", &args)?;

        // Parse response: {"text": "..."}
        let text = extract_json_string(&response, "text")?;
        Some(StatusResult { text })
    }
}

impl ViewerPlugin for ScriptPlugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }

    fn can_handle(&self, path: &Path) -> ViewerCanHandleResult {
        let args = format!("\"path\":\"{}\"", escape_json(&path.to_string_lossy()));

        let response = match self.execute_command("viewer_can_handle", &args) {
            Some(r) => r,
            None => return ViewerCanHandleResult { can_handle: false, priority: 0 },
        };

        // Parse response: {"can_handle": true, "priority": 10}
        let can_handle = extract_json_bool(&response, "can_handle").unwrap_or(false);
        let priority = extract_json_int(&response, "priority").unwrap_or(0);

        ViewerCanHandleResult { can_handle, priority }
    }

    fn render(&self, context: &ViewerContext) -> Option<ViewerRenderResult> {
        let args = format!(
            "\"path\":\"{}\",\"width\":{},\"height\":{},\"scroll\":{}",
            escape_json(&context.path.to_string_lossy()),
            context.width,
            context.height,
            context.scroll
        );

        let response = self.execute_command("viewer_render", &args)?;

        // Parse response: {"lines": ["line1", "line2", ...], "total_lines": 100}
        let lines = extract_json_string_array(&response, "lines")?;
        let total_lines = extract_json_int(&response, "total_lines")
            .map(|i| i as usize)
            .unwrap_or(lines.len());

        Some(ViewerRenderResult { lines, total_lines })
    }
}

// Simple JSON parsing helpers (no external crates)

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

fn extract_json_int(json: &str, key: &str) -> Option<i32> {
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

    // Find matching ] — must skip brackets inside JSON strings
    let mut depth = 0;
    let mut end = 0;
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
            '[' if !in_str => depth += 1,
            ']' if !in_str => {
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
            current.push(c);
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

impl Drop for ScriptPlugin {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
        }
    }
}

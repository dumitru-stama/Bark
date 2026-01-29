#![allow(dead_code)]
//! Script-based provider plugins
//!
//! Provider plugins that communicate via JSON over stdin/stdout.
//! The plugin protocol is:
//!
//! ## Plugin Discovery (--plugin-info)
//! ```json
//! {"name": "S3 Provider", "version": "1.0", "type": "provider",
//!  "schemes": ["s3", "minio"], "description": "AWS S3 compatible storage"}
//! ```
//!
//! ## Commands (JSON-RPC style on stdin, response on stdout)
//!
//! ### get_dialog_fields
//! Request: `{"command": "get_dialog_fields"}`
//! Response: `{"fields": [{"id": "bucket", "label": "Bucket", "type": "text", "required": true}, ...]}`
//!
//! ### validate_config
//! Request: `{"command": "validate_config", "config": {"bucket": "my-bucket", ...}}`
//! Response: `{"valid": true}` or `{"valid": false, "error": "Bucket is required"}`
//!
//! ### connect
//! Request: `{"command": "connect", "config": {"bucket": "my-bucket", ...}}`
//! Response: `{"success": true, "session_id": "abc123"}` or `{"success": false, "error": "..."}`
//!
//! ### list_directory
//! Request: `{"command": "list_directory", "session_id": "abc123", "path": "/folder"}`
//! Response: `{"entries": [{"name": "file.txt", "is_dir": false, "size": 1234, ...}, ...]}`
//!
//! ### read_file
//! Request: `{"command": "read_file", "session_id": "abc123", "path": "/file.txt"}`
//! Response: `{"data": "<base64 encoded content>"}` or `{"error": "..."}`
//!
//! ### write_file
//! Request: `{"command": "write_file", "session_id": "abc123", "path": "/file.txt", "data": "<base64>"}`
//! Response: `{"success": true}` or `{"error": "..."}`
//!
//! ### delete / mkdir / rename / copy_file
//! Similar pattern with appropriate parameters

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::SystemTime;

use bark_plugin_api::FileEntry;
use crate::plugins::provider_api::*;

/// A script-based provider plugin
pub struct ScriptProviderPlugin {
    info: ProviderPluginInfo,
    executable: PathBuf,
    /// Cached dialog fields
    dialog_fields: Vec<DialogField>,
}

impl ScriptProviderPlugin {
    /// Load a provider plugin from an executable
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

        // Pre-fetch dialog fields (only for scheme-based plugins that show connection dialogs)
        let dialog_fields = if !info.schemes.is_empty() {
            Self::fetch_dialog_fields(path)?
        } else {
            Vec::new()
        };

        Ok(ScriptProviderPlugin {
            info,
            executable: path.to_path_buf(),
            dialog_fields,
        })
    }

    fn parse_info(output: &str, path: &Path) -> Result<ProviderPluginInfo, String> {
        let mut name = "Unknown Provider".to_string();
        let mut version = "0.0".to_string();
        let mut description = String::new();
        let mut schemes = Vec::new();
        let mut extensions = Vec::new();
        let mut icon = None;

        for line in output.lines() {
            let line = line.trim();
            if line.starts_with('{') {
                if let Some(n) = extract_json_string(line, "name") {
                    name = n;
                }
                if let Some(v) = extract_json_string(line, "version") {
                    version = v;
                }
                if let Some(d) = extract_json_string(line, "description") {
                    description = d;
                }
                if let Some(s) = extract_json_string_array(line, "schemes") {
                    schemes = s;
                }
                if let Some(e) = extract_json_string_array(line, "extensions") {
                    extensions = e;
                }
                if let Some(i) = extract_json_string(line, "icon") {
                    icon = i.chars().next();
                }

                // Check plugin type
                if let Some(t) = extract_json_string(line, "type")
                    && t != "provider"
                {
                    return Err(format!("Not a provider plugin (type: {})", t));
                }
                break;
            }
        }

        // Provider plugins must specify at least one scheme OR one extension
        if schemes.is_empty() && extensions.is_empty() {
            return Err("Provider plugin must specify at least one URI scheme or file extension".to_string());
        }

        let mut info = ProviderPluginInfo::provider(name, version, schemes)
            .with_description(description)
            .with_extensions(extensions);
        info.source = path.to_path_buf();
        if let Some(i) = icon {
            info = info.with_icon(i);
        }
        Ok(info)
    }

    fn fetch_dialog_fields(path: &Path) -> Result<Vec<DialogField>, String> {
        let request = r#"{"command":"get_dialog_fields"}"#;

        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn plugin: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = writeln!(stdin, "{}", request);
        }

        let output = child.wait_with_output()
            .map_err(|e| format!("Failed to read plugin output: {}", e))?;

        if !output.status.success() {
            return Ok(Vec::new()); // Return empty if command not supported
        }

        let response = String::from_utf8_lossy(&output.stdout);
        Self::parse_dialog_fields(&response)
    }

    fn parse_dialog_fields(response: &str) -> Result<Vec<DialogField>, String> {
        let mut fields = Vec::new();

        // Find fields array
        let fields_json = extract_json_array(response, "fields")
            .unwrap_or_default();

        for field_json in fields_json {
            let id = extract_json_string(&field_json, "id").unwrap_or_default();
            let label = extract_json_string(&field_json, "label").unwrap_or_else(|| id.clone());
            let field_type_str = extract_json_string(&field_json, "type").unwrap_or_else(|| "text".to_string());
            let default_value = extract_json_string(&field_json, "default");
            let placeholder = extract_json_string(&field_json, "placeholder");
            let required = extract_json_bool(&field_json, "required").unwrap_or(false);
            let help_text = extract_json_string(&field_json, "help");

            let field_type = match field_type_str.as_str() {
                "text" => DialogFieldType::Text,
                "password" => DialogFieldType::Password,
                "number" => DialogFieldType::Number,
                "checkbox" => DialogFieldType::Checkbox,
                "textarea" => DialogFieldType::TextArea,
                "file" | "filepath" => DialogFieldType::FilePath,
                "select" => {
                    let options = extract_json_select_options(&field_json);
                    DialogFieldType::Select { options }
                }
                _ => DialogFieldType::Text,
            };

            if !id.is_empty() {
                fields.push(DialogField {
                    id,
                    label,
                    field_type,
                    default_value,
                    placeholder,
                    required,
                    help_text,
                });
            }
        }

        Ok(fields)
    }

    fn execute_command(&self, command: &str, args: &str) -> Result<String, String> {
        let request = if args.is_empty() {
            format!("{{\"command\":\"{}\"}}", command)
        } else {
            format!("{{\"command\":\"{}\",{}}}", command, args)
        };

        let mut child = Command::new(&self.executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn plugin: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = writeln!(stdin, "{}", request);
        }

        let output = child.wait_with_output()
            .map_err(|e| format!("Failed to read plugin output: {}", e))?;

        if !output.status.success() {
            return Err("Plugin command failed".to_string());
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl ProviderPlugin for ScriptProviderPlugin {
    fn info(&self) -> &ProviderPluginInfo {
        &self.info
    }

    fn get_dialog_fields(&self) -> Vec<DialogField> {
        self.dialog_fields.clone()
    }

    fn validate_config(&self, config: &ProviderConfig) -> ProviderPluginResult<()> {
        let config_json = config_to_json(config);
        let args = format!("\"config\":{}", config_json);

        let response = self.execute_command("validate_config", &args)
            .map_err(ProviderPluginError::PluginError)?;

        if extract_json_bool(&response, "valid").unwrap_or(false) {
            Ok(())
        } else {
            let error = extract_json_string(&response, "error")
                .unwrap_or_else(|| "Validation failed".to_string());
            Err(ProviderPluginError::ConfigError(error))
        }
    }

    fn connect(&self, config: &ProviderConfig) -> ProviderPluginResult<Box<dyn ProviderSession>> {
        // Spawn a persistent child process for this session
        let mut child = Command::new(&self.executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| ProviderPluginError::Connection(format!("Failed to spawn plugin: {}", e)))?;

        let child_stdin = child.stdin.take()
            .ok_or_else(|| ProviderPluginError::Connection("Failed to open plugin stdin".to_string()))?;
        let child_stdout = child.stdout.take()
            .ok_or_else(|| ProviderPluginError::Connection("Failed to open plugin stdout".to_string()))?;

        let mut session = ScriptProviderSession {
            child: Some(child),
            stdin: Mutex::new(child_stdin),
            stdout: Mutex::new(std::io::BufReader::new(child_stdout)),
            session_id: uuid_v4(),
            display_name: config.name.clone(),
            short_label: None,
            home_path: config.get("path").unwrap_or("/").to_string(),
            connected: Mutex::new(true),
        };

        // Send the connect command to the persistent process
        let config_json = config_to_json(config);
        let args = format!("\"config\":{}", config_json);
        let response = session.execute_command("connect", &args)
            .map_err(|e| ProviderPluginError::Connection(format!("Connect failed: {}", e)))?;

        if extract_json_bool(&response, "success").unwrap_or(false) {
            if let Some(sid) = extract_json_string(&response, "session_id") {
                session.session_id = sid;
            }
            session.short_label = extract_json_string(&response, "short_label");
            Ok(Box::new(session))
        } else {
            let error = extract_json_string(&response, "error")
                .unwrap_or_else(|| "Connection failed".to_string());
            Err(ProviderPluginError::Connection(error))
        }
    }
}

/// An active session with a script provider plugin.
/// Keeps the plugin child process alive for the duration of the session,
/// sending commands via stdin and reading responses from stdout.
pub struct ScriptProviderSession {
    child: Option<std::process::Child>,
    stdin: Mutex<std::process::ChildStdin>,
    stdout: Mutex<std::io::BufReader<std::process::ChildStdout>>,
    session_id: String,
    display_name: String,
    short_label: Option<String>,
    home_path: String,
    connected: Mutex<bool>,
}

// Safety: the child process handles are accessed through Mutex locks
unsafe impl Send for ScriptProviderSession {}

impl Drop for ScriptProviderSession {
    fn drop(&mut self) {
        // Try to send disconnect, then kill the child
        let _ = self.execute_command("disconnect", "");
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl ScriptProviderSession {
    fn execute_command(&self, command: &str, args: &str) -> Result<String, ProviderPluginError> {
        let session_arg = format!("\"session_id\":\"{}\"", escape_json(&self.session_id));
        let full_args = if args.is_empty() {
            session_arg
        } else {
            format!("{},{}", session_arg, args)
        };

        let request = format!("{{\"command\":\"{}\",{}}}\n", command, full_args);

        // Write request to stdin
        {
            let mut stdin = self.stdin.lock().unwrap();
            use std::io::Write;
            stdin.write_all(request.as_bytes())
                .map_err(|e| ProviderPluginError::PluginError(format!("Failed to write to plugin: {}", e)))?;
            stdin.flush()
                .map_err(|e| ProviderPluginError::PluginError(format!("Failed to flush plugin stdin: {}", e)))?;
        }

        // Read one line of response from stdout
        let response = {
            let mut stdout = self.stdout.lock().unwrap();
            let mut line = String::new();
            use std::io::BufRead;
            stdout.read_line(&mut line)
                .map_err(|e| ProviderPluginError::PluginError(format!("Failed to read from plugin: {}", e)))?;
            line
        };

        let response = response.trim().to_string();

        if response.is_empty() {
            return Err(ProviderPluginError::PluginError("Empty response from plugin".to_string()));
        }

        // Check for error in response
        if let Some(error) = extract_json_string(&response, "error") {
            if let Some(error_type) = extract_json_string(&response, "error_type") {
                return Err(match error_type.as_str() {
                    "auth" => ProviderPluginError::Auth(error),
                    "not_found" => ProviderPluginError::NotFound(error),
                    "permission" => ProviderPluginError::PermissionDenied(error),
                    "connection" => ProviderPluginError::Connection(error),
                    _ => ProviderPluginError::Other(error),
                });
            }
            return Err(ProviderPluginError::Other(error));
        }

        Ok(response)
    }
}

impl ProviderSession for ScriptProviderSession {
    fn display_name(&self) -> String {
        self.display_name.clone()
    }

    fn short_label(&self) -> Option<String> {
        self.short_label.clone()
    }

    fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }

    fn disconnect(&mut self) {
        *self.connected.lock().unwrap() = false;
        // Actual disconnect + child kill happens in Drop
    }

    fn list_directory(&mut self, path: &str) -> ProviderPluginResult<Vec<FileEntry>> {
        let args = format!("\"path\":\"{}\"", escape_json(path));
        let response = self.execute_command("list_directory", &args)?;

        let entries_json = extract_json_array(&response, "entries")
            .unwrap_or_default();

        let mut entries = Vec::new();
        let base_path = if path == "/" { "" } else { path };

        for entry_json in entries_json {
            let name = extract_json_string(&entry_json, "name").unwrap_or_default();
            if name.is_empty() || name == "." {
                continue;
            }

            let is_dir = extract_json_bool(&entry_json, "is_dir").unwrap_or(false);
            let size = extract_json_int(&entry_json, "size").unwrap_or(0) as u64;
            let is_hidden = name.starts_with('.') ||
                extract_json_bool(&entry_json, "is_hidden").unwrap_or(false);
            let is_symlink = extract_json_bool(&entry_json, "is_symlink").unwrap_or(false);

            let modified = extract_json_int(&entry_json, "modified")
                .and_then(|ts| SystemTime::UNIX_EPOCH.checked_add(
                    std::time::Duration::from_secs(ts as u64)
                ));

            let full_path = if base_path.is_empty() {
                format!("/{}", name)
            } else {
                format!("{}/{}", base_path, name)
            };

            entries.push(FileEntry {
                name,
                path: PathBuf::from(&full_path),
                is_dir,
                size,
                modified,
                is_hidden,
                permissions: extract_json_int(&entry_json, "permissions").unwrap_or(0) as u32,
                is_symlink,
                symlink_target: extract_json_string(&entry_json, "symlink_target")
                    .map(PathBuf::from),
                owner: extract_json_string(&entry_json, "owner").unwrap_or_default(),
                group: extract_json_string(&entry_json, "group").unwrap_or_default(),
            });
        }

        Ok(entries)
    }

    fn read_file(&mut self, path: &str) -> ProviderPluginResult<Vec<u8>> {
        let args = format!("\"path\":\"{}\"", escape_json(path));
        let response = self.execute_command("read_file", &args)?;

        // Data is base64 encoded
        let data_b64 = extract_json_string(&response, "data")
            .ok_or_else(|| ProviderPluginError::Other("No data in response".to_string()))?;

        base64_decode(&data_b64)
            .map_err(|e| ProviderPluginError::Other(format!("Failed to decode data: {}", e)))
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> ProviderPluginResult<()> {
        let data_b64 = base64_encode(data);
        let args = format!("\"path\":\"{}\",\"data\":\"{}\"", escape_json(path), data_b64);
        let response = self.execute_command("write_file", &args)?;

        if extract_json_bool(&response, "success").unwrap_or(false) {
            Ok(())
        } else {
            Err(ProviderPluginError::Other("Write failed".to_string()))
        }
    }

    fn delete(&mut self, path: &str) -> ProviderPluginResult<()> {
        let args = format!("\"path\":\"{}\"", escape_json(path));
        let response = self.execute_command("delete", &args)?;

        if extract_json_bool(&response, "success").unwrap_or(false) {
            Ok(())
        } else {
            Err(ProviderPluginError::Other("Delete failed".to_string()))
        }
    }

    fn delete_recursive(&mut self, path: &str) -> ProviderPluginResult<()> {
        let args = format!("\"path\":\"{}\",\"recursive\":true", escape_json(path));
        let response = self.execute_command("delete", &args)?;

        if extract_json_bool(&response, "success").unwrap_or(false) {
            Ok(())
        } else {
            Err(ProviderPluginError::Other("Delete failed".to_string()))
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> ProviderPluginResult<()> {
        let args = format!("\"from\":\"{}\",\"to\":\"{}\"", escape_json(from), escape_json(to));
        let response = self.execute_command("rename", &args)?;

        if extract_json_bool(&response, "success").unwrap_or(false) {
            Ok(())
        } else {
            Err(ProviderPluginError::Other("Rename failed".to_string()))
        }
    }

    fn mkdir(&mut self, path: &str) -> ProviderPluginResult<()> {
        let args = format!("\"path\":\"{}\"", escape_json(path));
        let response = self.execute_command("mkdir", &args)?;

        if extract_json_bool(&response, "success").unwrap_or(false) {
            Ok(())
        } else {
            Err(ProviderPluginError::Other("Mkdir failed".to_string()))
        }
    }

    fn copy_file(&mut self, from: &str, to: &str) -> ProviderPluginResult<()> {
        let args = format!("\"from\":\"{}\",\"to\":\"{}\"", escape_json(from), escape_json(to));
        let response = self.execute_command("copy", &args)?;

        if extract_json_bool(&response, "success").unwrap_or(false) {
            Ok(())
        } else {
            Err(ProviderPluginError::Other("Copy failed".to_string()))
        }
    }

    fn set_attributes(
        &mut self,
        path: &str,
        modified: Option<SystemTime>,
        permissions: u32,
    ) -> ProviderPluginResult<()> {
        use std::time::UNIX_EPOCH;

        let mtime_arg = modified
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| format!(",\"modified\":{}", d.as_secs()))
            .unwrap_or_default();
        let perm_arg = if permissions != 0 {
            format!(",\"permissions\":{}", permissions)
        } else {
            String::new()
        };

        // Only send if there's something to set
        if mtime_arg.is_empty() && perm_arg.is_empty() {
            return Ok(());
        }

        let args = format!("\"path\":\"{}\"{}{}",
            escape_json(path), mtime_arg, perm_arg);

        // Best-effort: plugins may not support this command, so ignore errors
        let _ = self.execute_command("set_attributes", &args);
        Ok(())
    }

    fn home_path(&self) -> String {
        self.home_path.clone()
    }
}

// ============================================================================
// JSON Helpers (minimal, no external crates)
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
    for (i, c) in rest.chars().enumerate() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }

    if end == 0 {
        return None;
    }

    let array_str = &rest[1..end - 1];
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

fn extract_json_array(json: &str, key: &str) -> Option<Vec<String>> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();

    if !rest.starts_with('[') {
        return None;
    }

    let mut depth = 0;
    let mut end = 0;
    for (i, c) in rest.chars().enumerate() {
        match c {
            '[' | '{' => depth += 1,
            ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }

    if end == 0 {
        return None;
    }

    // Parse array of objects
    let array_content = &rest[1..end - 1];
    let mut objects = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for c in array_content.chars() {
        if escape {
            current.push(c);
            escape = false;
            continue;
        }

        match c {
            '\\' => {
                escape = true;
                current.push(c);
            }
            '"' => {
                in_string = !in_string;
                current.push(c);
            }
            '{' if !in_string => {
                depth += 1;
                current.push(c);
            }
            '}' if !in_string => {
                depth -= 1;
                current.push(c);
                if depth == 0 {
                    objects.push(current.trim().to_string());
                    current.clear();
                }
            }
            ',' if depth == 0 && !in_string => {
                // Skip commas between objects
            }
            _ => {
                if depth > 0 || !c.is_whitespace() {
                    current.push(c);
                }
            }
        }
    }

    if !current.trim().is_empty() && current.contains('{') {
        objects.push(current.trim().to_string());
    }

    Some(objects)
}

fn extract_json_select_options(json: &str) -> Vec<(String, String)> {
    let options_json = extract_json_array(json, "options").unwrap_or_default();
    let mut options = Vec::new();

    for opt_json in options_json {
        let value = extract_json_string(&opt_json, "value").unwrap_or_default();
        let label = extract_json_string(&opt_json, "label").unwrap_or_else(|| value.clone());
        if !value.is_empty() {
            options.push((value, label));
        }
    }

    options
}

fn config_to_json(config: &ProviderConfig) -> String {
    let mut parts = Vec::new();
    parts.push(format!("\"name\":\"{}\"", escape_json(&config.name)));

    for (key, value) in &config.values {
        parts.push(format!("\"{}\":\"{}\"", escape_json(key), escape_json(value)));
    }

    format!("{{{}}}", parts.join(","))
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:032x}", time)
}

// Simple base64 encode/decode (no external crates)
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in data.chunks(3) {
        let mut n = (chunk[0] as u32) << 16;
        if chunk.len() > 1 { n |= (chunk[1] as u32) << 8; }
        if chunk.len() > 2 { n |= chunk[2] as u32; }

        result.push(CHARS[(n >> 18 & 0x3F) as usize] as char);
        result.push(CHARS[(n >> 12 & 0x3F) as usize] as char);
        result.push(if chunk.len() > 1 { CHARS[(n >> 6 & 0x3F) as usize] as char } else { '=' });
        result.push(if chunk.len() > 2 { CHARS[(n & 0x3F) as usize] as char } else { '=' });
    }

    result
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    const DECODE: [i8; 128] = [
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,62,-1,-1,-1,63,
        52,53,54,55,56,57,58,59,60,61,-1,-1,-1,-1,-1,-1,
        -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14,
        15,16,17,18,19,20,21,22,23,24,25,-1,-1,-1,-1,-1,
        -1,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,
        41,42,43,44,45,46,47,48,49,50,51,-1,-1,-1,-1,-1,
    ];

    let bytes: Vec<u8> = s.bytes()
        .filter(|&b| b != b'=' && b != b'\n' && b != b'\r')
        .collect();

    let mut result = Vec::new();

    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            break;
        }

        let mut n = 0u32;
        for (i, &b) in chunk.iter().enumerate() {
            if b > 127 || DECODE[b as usize] < 0 {
                return Err(format!("Invalid base64 character: {}", b as char));
            }
            n |= (DECODE[b as usize] as u32) << (18 - i * 6);
        }

        result.push((n >> 16) as u8);
        if chunk.len() > 2 {
            result.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            result.push(n as u8);
        }
    }

    Ok(result)
}

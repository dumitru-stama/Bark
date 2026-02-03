//! WebDAV/WebDAVS provider plugin for Bark file manager
//!
//! This plugin provides WebDAV and WebDAVS remote filesystem access.

use std::io::Read;
use std::path::PathBuf;
use std::time::SystemTime;

use bark_plugin_api::*;

/// WebDAV provider plugin
pub struct WebdavProviderPlugin {
    info: ProviderPluginInfo,
}

impl WebdavProviderPlugin {
    /// Create a new WebDAV provider plugin
    pub fn new() -> Self {
        Self {
            info: ProviderPluginInfo::provider(
                "WebDAV Provider",
                "1.0.0",
                vec!["webdav".to_string(), "webdavs".to_string()],
            )
            .with_description("WebDAV/WebDAVS remote file access")
            .with_icon('\u{2601}'), // cloud emoji ☁
        }
    }
}

impl Default for WebdavProviderPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderPlugin for WebdavProviderPlugin {
    fn info(&self) -> &ProviderPluginInfo {
        &self.info
    }

    fn get_dialog_fields(&self) -> Vec<DialogField> {
        vec![
            DialogField {
                id: "name".to_string(),
                label: "Connection Name".to_string(),
                field_type: DialogFieldType::Text,
                default_value: None,
                placeholder: Some("My WebDAV Server".to_string()),
                required: false,
                help_text: Some("Optional name for this connection".to_string()),
            },
            DialogField {
                id: "url".to_string(),
                label: "URL".to_string(),
                field_type: DialogFieldType::Text,
                default_value: Some("https://".to_string()),
                placeholder: Some("https://example.com/dav/".to_string()),
                required: true,
                help_text: Some("WebDAV server URL".to_string()),
            },
            DialogField {
                id: "user".to_string(),
                label: "Username".to_string(),
                field_type: DialogFieldType::Text,
                default_value: None,
                placeholder: None,
                required: false,
                help_text: None,
            },
            DialogField {
                id: "password".to_string(),
                label: "Password".to_string(),
                field_type: DialogFieldType::Password,
                default_value: None,
                placeholder: None,
                required: false,
                help_text: None,
            },
            DialogField {
                id: "verify_ssl".to_string(),
                label: "Verify SSL".to_string(),
                field_type: DialogFieldType::Checkbox,
                default_value: Some("true".to_string()),
                placeholder: None,
                required: false,
                help_text: Some("Verify SSL certificates".to_string()),
            },
            DialogField {
                id: "path".to_string(),
                label: "Initial Path".to_string(),
                field_type: DialogFieldType::Text,
                default_value: Some("/".to_string()),
                placeholder: Some("/".to_string()),
                required: false,
                help_text: Some("Path relative to base URL".to_string()),
            },
        ]
    }

    fn validate_config(&self, config: &ProviderConfig) -> ProviderResult<()> {
        if config.get("url").map(|s| s.is_empty()).unwrap_or(true) {
            return Err(ProviderError::ConfigError("URL is required".to_string()));
        }
        Ok(())
    }

    fn connect(&self, config: &ProviderConfig) -> ProviderResult<Box<dyn ProviderSession>> {
        let url = config
            .get("url")
            .ok_or_else(|| ProviderError::ConfigError("URL is required".to_string()))?
            .to_string();
        let username = config.get("user").unwrap_or("").to_string();
        let password = config.get("password").unwrap_or("").to_string();
        let verify_ssl = config.get("verify_ssl").map(|v| matches!(v, "true" | "1" | "yes" | "on")).unwrap_or(true);
        let initial_path = config.get("path").map(|s| s.to_string());

        // Build HTTP client — no global timeout; per-request timeouts are set
        // on each call instead (short for metadata ops, scaled for transfers).
        // Disable automatic redirects: reqwest's default policy downgrades
        // PUT/MKCOL to GET on 301/302, which causes 405 on WebDAV servers.
        // We handle redirects manually in send_following_redirects().
        let client = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(!verify_ssl)
            .connect_timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| ProviderError::Connection(format!("Failed to create HTTP client: {}", e)))?;

        // Test connection with OPTIONS request, following redirects to find
        // the real base URL (some servers redirect http→https or add a trailing path).
        let mut base_url = url.trim_end_matches('/').to_string();
        let test_url = format!("{}/", base_url);
        let mut req = client.request(reqwest::Method::OPTIONS, &test_url);
        if !username.is_empty() {
            req = req.basic_auth(&username, Some(&password));
        }
        let mut response = req.send()
            .map_err(|e| ProviderError::Connection(format!("Failed to connect: {}", e)))?;

        // Follow redirects, updating base_url to the final destination
        for _ in 0..5 {
            if !response.status().is_redirection() {
                break;
            }
            if let Some(location) = response.headers().get("location").and_then(|v| v.to_str().ok()) {
                let new_url = location.trim_end_matches('/').to_string();
                let mut req = client.request(reqwest::Method::OPTIONS, &format!("{}/", new_url));
                if !username.is_empty() {
                    req = req.basic_auth(&username, Some(&password));
                }
                response = req.send()
                    .map_err(|e| ProviderError::Connection(format!("Failed to connect (redirect): {}", e)))?;
                base_url = new_url;
            } else {
                break;
            }
        }

        if response.status().is_client_error() {
            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Err(ProviderError::Auth("Invalid username or password".to_string()));
            }
            return Err(ProviderError::Connection(format!(
                "Server returned error: {}",
                response.status()
            )));
        }

        // Build display name
        let display = extract_host(&url)
            .map(|host| {
                if username.is_empty() {
                    host.to_string()
                } else {
                    format!("{}@{}", username, host)
                }
            })
            .unwrap_or_else(|| url.clone());
        let display_name = format!("webdav://{}", display);

        Ok(Box::new(WebdavProviderSession {
            client,
            base_url,
            username,
            password,
            display_name,
            home_path: initial_path.unwrap_or_else(|| "/".to_string()),
        }))
    }
}

/// Active WebDAV session
pub struct WebdavProviderSession {
    client: reqwest::blocking::Client,
    base_url: String,
    username: String,
    password: String,
    display_name: String,
    home_path: String,
}

impl WebdavProviderSession {
    /// Build the full URL for a path, percent-encoding each segment
    fn build_url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            format!("{}/", base)
        } else {
            let encoded: Vec<String> = path.split('/').map(urlencode_segment).collect();
            format!("{}/{}", base, encoded.join("/"))
        }
    }

    /// Create a request builder with basic auth and a 30s metadata timeout.
    fn request(&self, method: reqwest::Method, url: &str) -> reqwest::blocking::RequestBuilder {
        let mut req = self.client.request(method, url)
            .timeout(std::time::Duration::from_secs(30));
        if !self.username.is_empty() {
            req = req.basic_auth(&self.username, Some(&self.password));
        }
        req
    }

    /// Create a request builder with a timeout scaled for data transfer size.
    /// Minimum 60s, plus 60s per 10 MB.
    fn transfer_request(&self, method: reqwest::Method, url: &str, data_len: usize) -> reqwest::blocking::RequestBuilder {
        let secs = 60 + (data_len as u64 / (10 * 1024 * 1024)) * 60;
        let mut req = self.client.request(method, url)
            .timeout(std::time::Duration::from_secs(secs));
        if !self.username.is_empty() {
            req = req.basic_auth(&self.username, Some(&self.password));
        }
        req
    }

    /// Ensure all parent directories exist for a given path by issuing MKCOL requests.
    /// Silently ignores errors (directory may already exist).
    fn ensure_parent_dirs(&self, path: &str) {
        let path = path.trim_start_matches('/');
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() <= 1 {
            return; // File is at the root level, no parents to create
        }

        let mut current = String::new();
        for part in &parts[..parts.len() - 1] {
            if current.is_empty() {
                current = part.to_string();
            } else {
                current = format!("{}/{}", current, part);
            }
            let url = self.build_url(&current);
            let _ = self.request(
                reqwest::Method::from_bytes(b"MKCOL").unwrap(),
                &url,
            ).send();
        }
    }

    /// Parse PROPFIND XML response to get file entries
    fn parse_propfind_response(&self, xml: &str, base_path: &str) -> ProviderResult<Vec<FileEntry>> {
        let doc = roxmltree::Document::parse(xml)
            .map_err(|e| ProviderError::Other(format!("Failed to parse XML: {}", e)))?;

        let mut entries = Vec::new();
        let base_path_normalized = self.normalize_path(base_path);

        for response in doc.descendants().filter(|n| n.tag_name().name() == "response") {
            let href = response
                .descendants()
                .find(|n| n.tag_name().name() == "href")
                .and_then(|n| n.text())
                .unwrap_or("");

            let href_decoded = urlencoding_decode(href);
            let path = extract_path_from_href(&href_decoded, &self.base_url);
            let path_normalized = self.normalize_path(&path);

            // Skip the directory itself
            if path_normalized == base_path_normalized {
                continue;
            }

            let propstat = response
                .descendants()
                .find(|n| n.tag_name().name() == "propstat");

            let prop = propstat.and_then(|ps| {
                ps.descendants().find(|n| n.tag_name().name() == "prop")
            });

            let is_dir = prop
                .map(|p| {
                    p.descendants().any(|n| n.tag_name().name() == "collection")
                })
                .unwrap_or(false);

            let size = prop
                .and_then(|p| {
                    p.descendants()
                        .find(|n| n.tag_name().name() == "getcontentlength")
                        .and_then(|n| n.text())
                        .and_then(|s| s.parse::<u64>().ok())
                })
                .unwrap_or(0);

            let modified = prop.and_then(|p| {
                p.descendants()
                    .find(|n| n.tag_name().name() == "getlastmodified")
                    .and_then(|n| n.text())
                    .and_then(parse_http_date)
            });

            let name = path_normalized
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("")
                .to_string();

            if name.is_empty() || name == ".." || name == "." {
                continue;
            }

            let is_hidden = name.starts_with('.');

            entries.push(
                FileEntry::new(name, PathBuf::from(&path_normalized), is_dir, if is_dir { 0 } else { size })
                    .with_modified(modified)
                    .with_hidden(is_hidden)
                    .with_permissions(if is_dir { 0o755 } else { 0o644 }),
            );
        }

        Ok(entries)
    }
}

impl ProviderSession for WebdavProviderSession {
    fn display_name(&self) -> String {
        self.display_name.clone()
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn disconnect(&mut self) {
        // HTTP is stateless, nothing to disconnect
    }

    fn list_directory(&mut self, path: &str) -> ProviderResult<Vec<FileEntry>> {
        let path = if path.is_empty() { "/" } else { path };
        let normalized_path = self.normalize_path(path);

        let parent_path = if normalized_path != "/" {
            self.parent_path(&normalized_path)
        } else {
            None
        };

        let url = self.build_url(&normalized_path);

        let response = self.request(
            reqwest::Method::from_bytes(b"PROPFIND").unwrap(),
            &url,
        )
            .header("Depth", "1")
            .header("Content-Type", "application/xml")
            .body(r#"<?xml version="1.0" encoding="utf-8"?>
<propfind xmlns="DAV:">
  <prop>
    <resourcetype/>
    <getcontentlength/>
    <getlastmodified/>
    <displayname/>
  </prop>
</propfind>"#)
            .send()
            .map_err(|e| ProviderError::Connection(format!("PROPFIND failed: {}", e)))?;

        if response.status().is_client_error() {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(ProviderError::NotFound(path.to_string()));
            }
            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Err(ProviderError::Auth("Unauthorized".to_string()));
            }
            return Err(ProviderError::Other(format!(
                "Server returned error: {}",
                response.status()
            )));
        }

        let xml = response.text()
            .map_err(|e| ProviderError::Other(format!("Failed to read response: {}", e)))?;

        let mut entries = self.parse_propfind_response(&xml, &normalized_path)?;

        // Add parent directory entry if not at root
        if let Some(parent) = parent_path {
            entries.insert(0, FileEntry::parent(PathBuf::from(&parent)));
        }

        Ok(entries)
    }

    fn read_file(&mut self, path: &str) -> ProviderResult<Vec<u8>> {
        let url = self.build_url(path);

        // Use a generous timeout for downloads (size unknown upfront)
        let mut response = self.transfer_request(reqwest::Method::GET, &url, 100 * 1024 * 1024)
            .send()
            .map_err(|e| ProviderError::Connection(format!("GET failed for {}: {}", path, e)))?;

        if response.status().is_client_error() {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(ProviderError::NotFound(path.to_string()));
            }
            return Err(ProviderError::Other(format!(
                "Server returned error: {}",
                response.status()
            )));
        }

        let mut contents = Vec::new();
        response.read_to_end(&mut contents)
            .map_err(|e| ProviderError::Other(format!("Failed to read response: {}", e)))?;

        Ok(contents)
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> ProviderResult<()> {
        // Ensure parent directories exist before uploading
        self.ensure_parent_dirs(path);

        let url = self.build_url(path);

        let response = self.transfer_request(reqwest::Method::PUT, &url, data.len())
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", data.len().to_string())
            .body(data.to_vec())
            .send()
            .map_err(|e| ProviderError::Connection(format!("PUT {}: {}", url, e)))?;

        if response.status().is_client_error() || response.status().is_server_error() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            let reason = extract_error_reason(&body);
            return Err(ProviderError::Other(format!(
                "PUT {} -> {}{}",
                url, status,
                if reason.is_empty() { String::new() } else { format!(" ({})", reason) }
            )));
        }

        Ok(())
    }

    fn delete(&mut self, path: &str) -> ProviderResult<()> {
        let url = self.build_url(path);

        let response = self.request(reqwest::Method::DELETE, &url)
            .send()
            .map_err(|e| ProviderError::Connection(format!("DELETE failed: {}", e)))?;

        if response.status().is_client_error() || response.status().is_server_error() {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(ProviderError::NotFound(path.to_string()));
            }
            return Err(ProviderError::Other(format!(
                "Failed to delete: {}",
                response.status()
            )));
        }

        Ok(())
    }

    fn delete_recursive(&mut self, path: &str) -> ProviderResult<()> {
        // WebDAV DELETE on a collection is automatically recursive
        self.delete(path)
    }

    fn rename(&mut self, from: &str, to: &str) -> ProviderResult<()> {
        let from_url = self.build_url(from);
        let to_url = self.build_url(to);

        let response = self.request(
            reqwest::Method::from_bytes(b"MOVE").unwrap(),
            &from_url,
        )
            .header("Destination", &to_url)
            .header("Overwrite", "F")
            .send()
            .map_err(|e| ProviderError::Connection(format!("MOVE failed: {}", e)))?;

        if response.status().is_client_error() || response.status().is_server_error() {
            return Err(ProviderError::Other(format!(
                "Failed to move: {}",
                response.status()
            )));
        }

        Ok(())
    }

    fn mkdir(&mut self, path: &str) -> ProviderResult<()> {
        let url = self.build_url(path);

        let response = self.request(
            reqwest::Method::from_bytes(b"MKCOL").unwrap(),
            &url,
        )
            .send()
            .map_err(|e| ProviderError::Connection(format!("MKCOL failed: {}", e)))?;

        if response.status().is_client_error() || response.status().is_server_error() {
            return Err(ProviderError::Other(format!(
                "Failed to create directory: {}",
                response.status()
            )));
        }

        Ok(())
    }

    fn copy_file(&mut self, from: &str, to: &str) -> ProviderResult<()> {
        let from_url = self.build_url(from);
        let to_url = self.build_url(to);

        let response = self.request(
            reqwest::Method::from_bytes(b"COPY").unwrap(),
            &from_url,
        )
            .header("Destination", &to_url)
            .header("Overwrite", "F")
            .send()
            .map_err(|e| ProviderError::Connection(format!("COPY failed: {}", e)))?;

        if response.status().is_client_error() || response.status().is_server_error() {
            return Err(ProviderError::Other(format!(
                "Failed to copy: {}",
                response.status()
            )));
        }

        Ok(())
    }

    fn home_path(&self) -> String {
        self.home_path.clone()
    }
}

impl Drop for WebdavProviderSession {
    fn drop(&mut self) {
        self.disconnect();
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Extract a human-readable reason from a WebDAV error response body.
/// Strips HTML tags and collapses whitespace. Returns at most 120 chars.
fn extract_error_reason(body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return String::new();
    }

    // Strip HTML tags
    let mut result = String::with_capacity(body.len());
    let mut in_tag = false;
    for c in body.chars() {
        match c {
            '<' => in_tag = true,
            '>' => { in_tag = false; result.push(' '); }
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }

    // Collapse whitespace and trim
    let collapsed: String = result.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() > 120 {
        format!("{}...", &collapsed[..120])
    } else {
        collapsed
    }
}

/// Extract host from URL
fn extract_host(url: &str) -> Option<&str> {
    let url = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let end = url.find('/').unwrap_or(url.len());
    Some(&url[..end])
}

/// Percent-encode a single URL path segment (RFC 3986 unreserved chars are kept as-is)
fn urlencode_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len() * 2);
    for byte in segment.bytes() {
        match byte {
            // unreserved: ALPHA / DIGIT / "-" / "." / "_" / "~"
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

/// Simple URL decoding (handles %XX sequences)
fn urlencoding_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Extract the path portion from an href, removing the base URL
fn extract_path_from_href(href: &str, base_url: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        if let Some(path_start) = href.find("://").and_then(|i| href[i+3..].find('/').map(|j| i + 3 + j)) {
            return href[path_start..].to_string();
        }
    }

    let base_path = extract_url_path(base_url);
    if href.starts_with(&base_path) {
        return href.to_string();
    }

    href.to_string()
}

/// Extract just the path portion from a URL
fn extract_url_path(url: &str) -> String {
    if let Some(path_start) = url.find("://").and_then(|i| url[i+3..].find('/').map(|j| i + 3 + j)) {
        url[path_start..].to_string()
    } else {
        "/".to_string()
    }
}

/// Parse HTTP date format (RFC 7231)
fn parse_http_date(s: &str) -> Option<SystemTime> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }

    let day: u32 = parts[1].parse().ok()?;
    let month = match parts[2].to_lowercase().as_str() {
        "jan" => 1, "feb" => 2, "mar" => 3, "apr" => 4,
        "may" => 5, "jun" => 6, "jul" => 7, "aug" => 8,
        "sep" => 9, "oct" => 10, "nov" => 11, "dec" => 12,
        _ => return None,
    };
    let year: i32 = parts[3].parse().ok()?;

    let time_parts: Vec<&str> = parts[4].split(':').collect();
    if time_parts.len() != 3 {
        return None;
    }

    let hour: u32 = time_parts[0].parse().ok()?;
    let minute: u32 = time_parts[1].parse().ok()?;
    let second: u32 = time_parts[2].parse().ok()?;

    let days_since_epoch = days_since_unix_epoch(year, month, day)?;
    let seconds = (days_since_epoch as u64) * 86400 + (hour as u64) * 3600 + (minute as u64) * 60 + (second as u64);

    Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
}

/// Calculate days since Unix epoch (1970-01-01)
fn days_since_unix_epoch(year: i32, month: u32, day: u32) -> Option<i64> {
    if year < 1970 {
        return None;
    }

    let month_days = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let is_leap = |y: i32| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);

    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }

    for m in 1..month {
        days += month_days[m as usize] as i64;
        if m == 2 && is_leap(year) {
            days += 1;
        }
    }

    days += (day - 1) as i64;

    Some(days)
}

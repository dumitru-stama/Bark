use std::path::Path;

/// Convert a glob pattern (with * and ?) to a regex pattern
pub fn glob_to_regex(pattern: &str, case_sensitive: bool) -> String {
    let mut regex = if case_sensitive {
        String::from("^") // Case-sensitive, anchor at start
    } else {
        String::from("(?i)^") // Case-insensitive, anchor at start
    };
    for c in pattern.chars() {
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            // Escape regex special characters
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' | '\\' => {
                regex.push('\\');
                regex.push(c);
            }
            _ => regex.push(c),
        }
    }
    regex.push('$'); // Anchor at end
    regex
}

/// Convert a wildcard pattern (* only) to regex (for search)
pub fn wildcard_to_regex(pattern: &str, case_sensitive: bool) -> String {
    let mut regex = if case_sensitive {
        String::new()
    } else {
        String::from("(?i)")
    };

    for c in pattern.chars() {
        match c {
            '*' => regex.push_str(".*"),
            '.' | '+' | '?' | '[' | ']' | '(' | ')' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                regex.push('\\');
                regex.push(c);
            }
            _ => regex.push(c),
        }
    }

    regex
}

/// Parse a hex string like "4D 5A" or "4D5A" or "4d5a" into bytes
pub fn parse_hex_string(s: &str) -> Option<Vec<u8>> {
    // Remove spaces and convert to uppercase for easier parsing
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();

    if clean.is_empty() {
        return Some(Vec::new());
    }

    // Must have even number of hex chars
    if !clean.len().is_multiple_of(2) {
        return None;
    }

    let mut bytes = Vec::with_capacity(clean.len() / 2);
    for i in (0..clean.len()).step_by(2) {
        let byte_str = &clean[i..i+2];
        match u8::from_str_radix(byte_str, 16) {
            Ok(b) => bytes.push(b),
            Err(_) => return None,
        }
    }

    Some(bytes)
}

/// Calculate bytes per line for hex view based on terminal width
pub fn calculate_hex_bytes_per_line(term_width: usize) -> usize {
    let calc_bytes = if term_width > 20 {
        ((term_width.saturating_sub(12)) * 8) / 33
    } else {
        8
    };
    (calc_bytes / 8 * 8).clamp(8, 64)
}

/// Get the drive letter from a path (Windows only, returns None on other platforms)
#[allow(dead_code)]
pub fn get_drive_letter(path: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        use std::path::Component;
        if let Some(Component::Prefix(prefix)) = path.components().next() {
            let prefix_str = prefix.as_os_str().to_string_lossy();
            // Extract just the drive letter part (e.g., "C:" from "C:")
            if prefix_str.len() >= 2 && prefix_str.chars().nth(1) == Some(':') {
                return Some(prefix_str[..2].to_uppercase());
            }
        }
        None
    }
    #[cfg(not(windows))] 
    {
        let _ = path;
        None
    }
}

/// Get list of available drives (Windows only, returns empty on other platforms)
#[allow(dead_code)]
pub fn get_available_drives() -> Vec<String> {
    #[cfg(windows)]
    {
        let mut drives = Vec::new();
        // Check drives A-Z
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:", letter as char);
            let path = std::path::Path::new(&drive);
            // Check if the drive exists by trying to get metadata
            if path.exists() || std::fs::read_dir(format!("{}\\", drive)).is_ok() {
                drives.push(drive);
            }
        }
        drives
    }
    #[cfg(not(windows))] 
    {
        Vec::new()
    }
}
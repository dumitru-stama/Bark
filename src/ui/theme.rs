//! Color theme system for Bark
//!
//! Provides built-in presets (classic, dark, light) and custom color configuration.

use ratatui::style::Color;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete theme definition with all UI colors
#[derive(Debug, Clone)]
pub struct Theme {
    // Panel colors
    pub panel_border_active: Color,
    pub panel_border_inactive: Color,
    pub panel_header: Color,
    pub panel_header_bg: Color,
    pub panel_column_separator: Color,
    pub panel_background: Color,
    pub temp_panel_background: Color,
    pub remote_panel_background: Color,  // Background for remote (SCP/SFTP) panels

    // File list colors
    pub file_normal: Color,
    pub file_directory: Color,
    pub file_selected: Color,
    pub cursor_bg: Color,
    pub cursor_fg: Color,

    // Status bar
    pub status_bg: Color,
    pub status_fg: Color,
    pub status_error_bg: Color,
    pub status_error_fg: Color,
    pub git_clean: Color,
    pub git_dirty: Color,

    // Viewer
    pub viewer_header_bg: Color,
    pub viewer_header_fg: Color,
    pub viewer_content_bg: Color,
    pub viewer_content_fg: Color,
    pub viewer_line_number: Color,
    pub viewer_footer_bg: Color,
    pub viewer_footer_fg: Color,

    // Help viewer
    pub help_header_bg: Color,
    pub help_header_fg: Color,
    pub help_content_bg: Color,
    pub help_content_fg: Color,
    pub help_highlight: Color,
    pub help_footer_bg: Color,
    pub help_footer_fg: Color,

    // Dialog - copy
    pub dialog_copy_bg: Color,
    pub dialog_copy_border: Color,
    // Dialog - move
    pub dialog_move_bg: Color,
    pub dialog_move_border: Color,
    // Dialog - delete
    pub dialog_delete_bg: Color,
    pub dialog_delete_border: Color,
    // Dialog - mkdir
    pub dialog_mkdir_bg: Color,
    pub dialog_mkdir_border: Color,
    // Dialog common
    pub dialog_title: Color,
    pub dialog_text: Color,
    pub dialog_warning: Color,
    pub dialog_input_focused_bg: Color,
    pub dialog_input_focused_fg: Color,
    pub dialog_input_selected_bg: Color,  // When text is selected (brighter)
    pub dialog_input_selected_fg: Color,
    pub dialog_input_unfocused_fg: Color,
    pub dialog_button_focused_bg: Color,
    pub dialog_button_focused_fg: Color,
    pub dialog_button_unfocused: Color,
    pub dialog_delete_button_focused_bg: Color,
    pub dialog_delete_button_focused_fg: Color,
    pub dialog_help: Color,

    // File highlighting rules
    pub highlights: Vec<CompiledHighlight>,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Dark theme (default) - modern dark colors matching screenshot
    pub fn dark() -> Self {
        // Color palette from screenshot
        let teal = Color::Rgb(0, 150, 136);           // Teal accent
        let gold = Color::Rgb(200, 170, 100);         // Muted gold for headers
        let light_gray = Color::Rgb(171, 178, 191);   // Light gray for files
        let dark_gray = Color::Rgb(76, 82, 99);       // Dark gray for inactive/separators
        let charcoal = Color::Rgb(58, 58, 58);        // Dark background #3a3a3a

        Self {
            // Panel
            panel_border_active: teal,
            panel_border_inactive: Color::Rgb(160, 160, 160),  // Light gray frame
            panel_header: gold,
            panel_header_bg: Color::Rgb(95, 135, 135),  // #5f8787
            panel_column_separator: Color::Rgb(120, 120, 120),  // Slightly darker than frame
            panel_background: charcoal,
            temp_panel_background: Color::Rgb(75, 70, 50),  // Warmer brownish for temp panel
            remote_panel_background: Color::Rgb(50, 58, 70),  // Slightly blue-tinted for remote panels

            // Files
            file_normal: Color::Rgb(220, 220, 220),  // #dcdcdc - light gray
            file_directory: Color::Rgb(171, 175, 135),  // #abaf87 - sage green
            file_selected: Color::Rgb(255, 220, 80),  // Bright gold/yellow for marked files
            cursor_bg: Color::Rgb(0, 95, 95),  // #005f5f
            cursor_fg: Color::Rgb(220, 220, 220),  // Same as file_normal

            // Status bar
            status_bg: Color::Rgb(45, 45, 45),  // Slightly darker than panel background
            status_fg: light_gray,
            status_error_bg: Color::Rgb(180, 60, 60),
            status_error_fg: Color::White,
            git_clean: Color::Rgb(152, 195, 121),     // Soft green
            git_dirty: gold,

            // Viewer
            viewer_header_bg: teal,
            viewer_header_fg: Color::Black,
            viewer_content_bg: charcoal,
            viewer_content_fg: light_gray,
            viewer_line_number: dark_gray,
            viewer_footer_bg: teal,
            viewer_footer_fg: Color::Black,

            // Help
            help_header_bg: teal,
            help_header_fg: Color::Black,
            help_content_bg: charcoal,
            help_content_fg: light_gray,
            help_highlight: gold,
            help_footer_bg: teal,
            help_footer_fg: Color::Black,

            // Dialog - copy (green tint)
            dialog_copy_bg: Color::Rgb(30, 50, 40),
            dialog_copy_border: Color::Rgb(152, 195, 121),
            // Dialog - move (blue tint)
            dialog_move_bg: Color::Rgb(30, 40, 55),
            dialog_move_border: Color::Rgb(97, 175, 239),
            // Dialog - delete (red tint)
            dialog_delete_bg: Color::Rgb(55, 35, 35),
            dialog_delete_border: Color::Rgb(224, 108, 117),
            // Dialog - mkdir (dark orange tint)
            dialog_mkdir_bg: Color::Rgb(60, 40, 25),
            dialog_mkdir_border: Color::Rgb(210, 140, 60),
            // Dialog common
            dialog_title: Color::White,
            dialog_text: light_gray,
            dialog_warning: gold,
            dialog_input_focused_bg: dark_gray,
            dialog_input_focused_fg: Color::White,
            dialog_input_selected_bg: Color::Rgb(0, 100, 150),  // Bright blue for selection
            dialog_input_selected_fg: Color::White,
            dialog_input_unfocused_fg: dark_gray,
            dialog_button_focused_bg: teal,
            dialog_button_focused_fg: Color::Black,
            dialog_button_unfocused: dark_gray,
            dialog_delete_button_focused_bg: Color::Rgb(224, 108, 117),
            dialog_delete_button_focused_fg: Color::White,
            dialog_help: dark_gray,

            highlights: Vec::new(),
        }
    }

    /// Classic Norton Commander blue theme
    pub fn classic() -> Self {
        Self {
            // Panel - classic NC uses blue background with cyan border
            panel_border_active: Color::LightCyan,
            panel_border_inactive: Color::Cyan,
            panel_header: Color::Yellow,
            panel_header_bg: Color::Cyan,
            panel_column_separator: Color::Cyan,
            panel_background: Color::Blue,
            temp_panel_background: Color::Rgb(80, 80, 128),  // Purple-ish blue for temp panel
            remote_panel_background: Color::Rgb(32, 48, 96),  // Slightly lighter blue for remote panels

            // Files
            file_normal: Color::LightCyan,
            file_directory: Color::White,
            file_selected: Color::Yellow,
            cursor_bg: Color::Cyan,
            cursor_fg: Color::Black,

            // Status bar
            status_bg: Color::Cyan,
            status_fg: Color::Black,
            status_error_bg: Color::Red,
            status_error_fg: Color::White,
            git_clean: Color::Green,
            git_dirty: Color::Yellow,

            // Viewer
            viewer_header_bg: Color::Cyan,
            viewer_header_fg: Color::Black,
            viewer_content_bg: Color::Blue,
            viewer_content_fg: Color::LightCyan,
            viewer_line_number: Color::Cyan,
            viewer_footer_bg: Color::Cyan,
            viewer_footer_fg: Color::Black,

            // Help
            help_header_bg: Color::Cyan,
            help_header_fg: Color::Black,
            help_content_bg: Color::Blue,
            help_content_fg: Color::LightCyan,
            help_highlight: Color::Yellow,
            help_footer_bg: Color::Cyan,
            help_footer_fg: Color::Black,

            // Dialog - copy
            dialog_copy_bg: Color::Blue,
            dialog_copy_border: Color::LightCyan,
            // Dialog - move
            dialog_move_bg: Color::Blue,
            dialog_move_border: Color::LightCyan,
            // Dialog - delete
            dialog_delete_bg: Color::Blue,
            dialog_delete_border: Color::Red,
            // Dialog - mkdir
            dialog_mkdir_bg: Color::Blue,
            dialog_mkdir_border: Color::Yellow,
            // Dialog common
            dialog_title: Color::White,
            dialog_text: Color::LightCyan,
            dialog_warning: Color::Yellow,
            dialog_input_focused_bg: Color::Cyan,
            dialog_input_focused_fg: Color::Black,
            dialog_input_selected_bg: Color::LightCyan,  // Brighter for selection
            dialog_input_selected_fg: Color::Black,
            dialog_input_unfocused_fg: Color::Cyan,
            dialog_button_focused_bg: Color::Cyan,
            dialog_button_focused_fg: Color::Black,
            dialog_button_unfocused: Color::LightCyan,
            dialog_delete_button_focused_bg: Color::Red,
            dialog_delete_button_focused_fg: Color::White,
            dialog_help: Color::Cyan,

            highlights: Vec::new(),
        }
    }

    /// Light theme - for light terminal backgrounds
    pub fn light() -> Self {
        Self {
            // Panel
            panel_border_active: Color::Blue,
            panel_border_inactive: Color::DarkGray,
            panel_header: Color::Blue,
            panel_header_bg: Color::Gray,
            panel_column_separator: Color::Gray,
            panel_background: Color::White,
            temp_panel_background: Color::Rgb(255, 255, 220),  // Light yellow for temp panel
            remote_panel_background: Color::Rgb(230, 240, 255),  // Light blue for remote panels

            // Files
            file_normal: Color::Black,
            file_directory: Color::Blue,
            file_selected: Color::Magenta,
            cursor_bg: Color::Blue,
            cursor_fg: Color::White,

            // Status bar
            status_bg: Color::Gray,
            status_fg: Color::Black,
            status_error_bg: Color::Red,
            status_error_fg: Color::White,
            git_clean: Color::Green,
            git_dirty: Color::Rgb(180, 100, 0), // Orange-ish for visibility

            // Viewer
            viewer_header_bg: Color::Blue,
            viewer_header_fg: Color::White,
            viewer_content_bg: Color::White,
            viewer_content_fg: Color::Black,
            viewer_line_number: Color::Gray,
            viewer_footer_bg: Color::Blue,
            viewer_footer_fg: Color::White,

            // Help
            help_header_bg: Color::Blue,
            help_header_fg: Color::White,
            help_content_bg: Color::White,
            help_content_fg: Color::Black,
            help_highlight: Color::Blue,
            help_footer_bg: Color::Blue,
            help_footer_fg: Color::White,

            // Dialog - copy
            dialog_copy_bg: Color::Rgb(220, 255, 220),
            dialog_copy_border: Color::Green,
            // Dialog - move
            dialog_move_bg: Color::Rgb(220, 220, 255),
            dialog_move_border: Color::Blue,
            // Dialog - delete
            dialog_delete_bg: Color::Rgb(255, 220, 220),
            dialog_delete_border: Color::Red,
            // Dialog - mkdir (light orange tint)
            dialog_mkdir_bg: Color::Rgb(255, 235, 200),
            dialog_mkdir_border: Color::Rgb(200, 120, 40),
            // Dialog common
            dialog_title: Color::Black,
            dialog_text: Color::Black,
            dialog_warning: Color::Red,
            dialog_input_focused_bg: Color::White,
            dialog_input_focused_fg: Color::Black,
            dialog_input_selected_bg: Color::Rgb(180, 210, 255),  // Light blue for selection
            dialog_input_selected_fg: Color::Black,
            dialog_input_unfocused_fg: Color::Gray,
            dialog_button_focused_bg: Color::Blue,
            dialog_button_focused_fg: Color::White,
            dialog_button_unfocused: Color::DarkGray,
            dialog_delete_button_focused_bg: Color::Red,
            dialog_delete_button_focused_fg: Color::White,
            dialog_help: Color::Gray,

            highlights: Vec::new(),
        }
    }

    /// Get a theme by name
    pub fn by_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "dark" => Some(Self::dark()),
            "classic" => Some(Self::classic()),
            "light" => Some(Self::light()),
            _ => None,
        }
    }

    /// Apply custom color overrides from config
    pub fn with_overrides(mut self, overrides: &HashMap<String, String>) -> Self {
        for (key, value) in overrides {
            if let Some(color) = parse_color(value) {
                match key.as_str() {
                    "panel_border_active" => self.panel_border_active = color,
                    "panel_border_inactive" => self.panel_border_inactive = color,
                    "panel_header" => self.panel_header = color,
                    "panel_header_bg" => self.panel_header_bg = color,
                    "panel_column_separator" => self.panel_column_separator = color,
                    "panel_background" => self.panel_background = color,
                    "temp_panel_background" => self.temp_panel_background = color,
                    "remote_panel_background" => self.remote_panel_background = color,
                    "file_normal" => self.file_normal = color,
                    "file_directory" => self.file_directory = color,
                    "file_selected" => self.file_selected = color,
                    "cursor_bg" => self.cursor_bg = color,
                    "cursor_fg" => self.cursor_fg = color,
                    "status_bg" => self.status_bg = color,
                    "status_fg" => self.status_fg = color,
                    "status_error_bg" => self.status_error_bg = color,
                    "status_error_fg" => self.status_error_fg = color,
                    "git_clean" => self.git_clean = color,
                    "git_dirty" => self.git_dirty = color,
                    "viewer_header_bg" => self.viewer_header_bg = color,
                    "viewer_header_fg" => self.viewer_header_fg = color,
                    "viewer_content_bg" => self.viewer_content_bg = color,
                    "viewer_content_fg" => self.viewer_content_fg = color,
                    "viewer_line_number" => self.viewer_line_number = color,
                    "viewer_footer_bg" => self.viewer_footer_bg = color,
                    "viewer_footer_fg" => self.viewer_footer_fg = color,
                    "help_header_bg" => self.help_header_bg = color,
                    "help_header_fg" => self.help_header_fg = color,
                    "help_content_bg" => self.help_content_bg = color,
                    "help_content_fg" => self.help_content_fg = color,
                    "help_highlight" => self.help_highlight = color,
                    "help_footer_bg" => self.help_footer_bg = color,
                    "help_footer_fg" => self.help_footer_fg = color,
                    "dialog_copy_bg" => self.dialog_copy_bg = color,
                    "dialog_copy_border" => self.dialog_copy_border = color,
                    "dialog_move_bg" => self.dialog_move_bg = color,
                    "dialog_move_border" => self.dialog_move_border = color,
                    "dialog_delete_bg" => self.dialog_delete_bg = color,
                    "dialog_delete_border" => self.dialog_delete_border = color,
                    "dialog_mkdir_bg" => self.dialog_mkdir_bg = color,
                    "dialog_mkdir_border" => self.dialog_mkdir_border = color,
                    "dialog_title" => self.dialog_title = color,
                    "dialog_text" => self.dialog_text = color,
                    "dialog_warning" => self.dialog_warning = color,
                    "dialog_input_focused_bg" => self.dialog_input_focused_bg = color,
                    "dialog_input_focused_fg" => self.dialog_input_focused_fg = color,
                    "dialog_input_selected_bg" => self.dialog_input_selected_bg = color,
                    "dialog_input_selected_fg" => self.dialog_input_selected_fg = color,
                    "dialog_input_unfocused_fg" => self.dialog_input_unfocused_fg = color,
                    "dialog_button_focused_bg" => self.dialog_button_focused_bg = color,
                    "dialog_button_focused_fg" => self.dialog_button_focused_fg = color,
                    "dialog_button_unfocused" => self.dialog_button_unfocused = color,
                    "dialog_delete_button_focused_bg" => self.dialog_delete_button_focused_bg = color,
                    "dialog_delete_button_focused_fg" => self.dialog_delete_button_focused_fg = color,
                    "dialog_help" => self.dialog_help = color,
                    _ => {} // Ignore unknown keys
                }
            }
        }
        self
    }

    /// Find matching highlight for a file entry
    /// Returns (color, prefix, suffix) if a match is found
    pub fn find_highlight(&self, name: &str, is_executable: bool, is_symlink: bool) -> Option<(Color, Option<&str>, Option<&str>)> {
        for h in &self.highlights {
            let matches = if let Some(special) = h.special {
                match special {
                    SpecialPattern::Executable => is_executable,
                    SpecialPattern::Symlink => is_symlink,
                }
            } else if let Some(ref regex) = h.regex {
                regex.is_match(name)
            } else {
                false
            };

            if matches {
                return Some((
                    h.color,
                    h.prefix.as_deref(),
                    h.suffix.as_deref(),
                ));
            }
        }
        None
    }
}

/// A user-defined custom theme
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CustomTheme {
    /// Base theme to inherit from: "dark", "classic", "light", or another custom theme name
    pub base: Option<String>,
    /// Color overrides
    #[serde(flatten)]
    pub colors: HashMap<String, String>,
}

/// File highlighting rule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHighlight {
    /// Pattern to match: regex for filename, or special values:
    /// - "executable" matches executable files
    /// - "symlink" matches symbolic links
    pub pattern: String,
    /// Color for matching files (name, hex, or rgb)
    pub color: String,
    /// Optional prefix to show before filename (e.g., "*" for executables)
    #[serde(default)]
    pub prefix: Option<String>,
    /// Optional suffix to show after filename
    #[serde(default)]
    pub suffix: Option<String>,
}

/// Compiled file highlighting rule (for runtime use)
#[derive(Debug, Clone)]
pub struct CompiledHighlight {
    /// Compiled regex (None for special patterns like "executable")
    pub regex: Option<Regex>,
    /// Special pattern type
    pub special: Option<SpecialPattern>,
    /// Parsed color
    pub color: Color,
    /// Prefix
    pub prefix: Option<String>,
    /// Suffix
    pub suffix: Option<String>,
}

/// Special pattern types that aren't regex-based
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpecialPattern {
    Executable,
    Symlink,
}

/// Theme configuration for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Active theme name: "dark", "classic", "light", or a custom theme name
    pub preset: String,
    /// Custom color overrides for the active theme (quick customization)
    #[serde(default)]
    pub colors: HashMap<String, String>,
    /// User-defined themes
    #[serde(default)]
    pub themes: HashMap<String, CustomTheme>,
    /// File highlighting rules (first match wins)
    #[serde(default = "default_highlights")]
    pub highlights: Vec<FileHighlight>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            preset: "dark".to_string(),
            colors: HashMap::new(),
            themes: HashMap::new(),
            highlights: default_highlights(),
        }
    }
}

/// Default file highlighting rules
fn default_highlights() -> Vec<FileHighlight> {
    vec![
        // Executables - green with asterisk prefix (like MC)
        FileHighlight {
            pattern: "executable".to_string(),
            color: "green".to_string(),
            prefix: Some("*".to_string()),
            suffix: None,
        },
        // Symbolic links - cyan
        FileHighlight {
            pattern: "symlink".to_string(),
            color: "magenta".to_string(),
            prefix: Some("@".to_string()),
            suffix: None,
        },
        // Archives - muted dark red
        FileHighlight {
            pattern: r"\.(tar|gz|bz2|xz|zip|rar|7z|tgz|tbz|txz|zst)$".to_string(),
            color: "#b05050".to_string(),
            prefix: None,
            suffix: None,
        },
        // Images - magenta
        FileHighlight {
            pattern: r"\.(jpg|jpeg|png|gif|bmp|svg|webp|ico|tiff?)$".to_string(),
            color: "magenta".to_string(),
            prefix: None,
            suffix: None,
        },
        // Audio/Video - cyan
        FileHighlight {
            pattern: r"\.(mp3|mp4|mkv|avi|mov|wav|flac|ogg|webm|m4a)$".to_string(),
            color: "cyan".to_string(),
            prefix: None,
            suffix: None,
        },
        // Documents - yellow
        FileHighlight {
            pattern: r"\.(pdf|doc|docx|odt|xls|xlsx|ppt|pptx)$".to_string(),
            color: "yellow".to_string(),
            prefix: None,
            suffix: None,
        },
        // Source code - light blue
        FileHighlight {
            pattern: r"\.(rs|py|js|ts|c|cpp|h|hpp|java|go|rb|sh|bash|zsh)$".to_string(),
            color: "lightblue".to_string(),
            prefix: None,
            suffix: None,
        },
    ]
}

impl ThemeConfig {
    /// Build a Theme from this config
    pub fn build_theme(&self) -> Theme {
        let mut theme = self.resolve_theme(&self.preset, &mut Vec::new())
            .with_overrides(&self.colors);

        // Compile and apply highlights
        theme.highlights = self.compile_highlights();
        theme
    }

    /// Compile file highlighting rules
    fn compile_highlights(&self) -> Vec<CompiledHighlight> {
        self.highlights.iter().filter_map(|h| {
            let color = parse_color(&h.color)?;

            // Check for special patterns
            let (regex, special) = match h.pattern.to_lowercase().as_str() {
                "executable" | "exec" => (None, Some(SpecialPattern::Executable)),
                "symlink" | "link" => (None, Some(SpecialPattern::Symlink)),
                _ => {
                    // Try to compile as regex (case insensitive)
                    match Regex::new(&format!("(?i){}", h.pattern)) {
                        Ok(re) => (Some(re), None),
                        Err(_) => return None, // Skip invalid patterns
                    }
                }
            };

            Some(CompiledHighlight {
                regex,
                special,
                color,
                prefix: h.prefix.clone(),
                suffix: h.suffix.clone(),
            })
        }).collect()
    }

    /// Resolve a theme by name, handling inheritance
    /// visited tracks already-seen themes to prevent infinite loops
    fn resolve_theme(&self, name: &str, visited: &mut Vec<String>) -> Theme {
        // Check for circular reference
        if visited.contains(&name.to_string()) {
            return Theme::default();
        }
        visited.push(name.to_string());

        // Check built-in themes first
        if let Some(theme) = Theme::by_name(name) {
            return theme;
        }

        // Check custom themes
        if let Some(custom) = self.themes.get(name) {
            // Get base theme (default to "dark" if not specified)
            let base_name = custom.base.as_deref().unwrap_or("dark");
            let base = self.resolve_theme(base_name, visited);
            return base.with_overrides(&custom.colors);
        }

        // Unknown theme, return default
        Theme::default()
    }

    /// Get a theme by name (for :theme command)
    pub fn get_theme(&self, name: &str) -> Option<Theme> {
        // Check built-in themes
        if Theme::by_name(name).is_some() {
            return Some(self.resolve_theme(name, &mut Vec::new()));
        }

        // Check custom themes
        if self.themes.contains_key(name) {
            return Some(self.resolve_theme(name, &mut Vec::new()));
        }

        None
    }

    /// List all available theme names (built-in + custom)
    pub fn available_themes(&self) -> Vec<String> {
        let mut themes = vec![
            "dark".to_string(),
            "classic".to_string(),
            "light".to_string(),
        ];
        themes.extend(self.themes.keys().cloned());
        themes
    }
}

/// Parse a color string into a ratatui Color
///
/// Supports:
/// - Named colors: "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "gray"
/// - Light variants: "light_red", "light_green", etc.
/// - RGB hex: "#RRGGBB" or "RRGGBB"
/// - RGB decimal: "rgb(R,G,B)"
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim().to_lowercase();

    // Named colors
    match s.as_str() {
        "black" => return Some(Color::Black),
        "red" => return Some(Color::Red),
        "green" => return Some(Color::Green),
        "yellow" => return Some(Color::Yellow),
        "blue" => return Some(Color::Blue),
        "magenta" => return Some(Color::Magenta),
        "cyan" => return Some(Color::Cyan),
        "white" => return Some(Color::White),
        "gray" | "grey" => return Some(Color::Gray),
        "dark_gray" | "dark_grey" | "darkgray" | "darkgrey" => return Some(Color::DarkGray),
        "light_red" | "lightred" => return Some(Color::LightRed),
        "light_green" | "lightgreen" => return Some(Color::LightGreen),
        "light_yellow" | "lightyellow" => return Some(Color::LightYellow),
        "light_blue" | "lightblue" => return Some(Color::LightBlue),
        "light_magenta" | "lightmagenta" => return Some(Color::LightMagenta),
        "light_cyan" | "lightcyan" => return Some(Color::LightCyan),
        "reset" => return Some(Color::Reset),
        _ => {}
    }

    // Hex color: #RRGGBB or RRGGBB
    let hex = s.strip_prefix('#').unwrap_or(&s);
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        return Some(Color::Rgb(r, g, b));
    }

    // RGB format: rgb(R,G,B)
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse().ok()?;
            let g = parts[1].trim().parse().ok()?;
            let b = parts[2].trim().parse().ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_named_colors() {
        assert_eq!(parse_color("red"), Some(Color::Red));
        assert_eq!(parse_color("BLUE"), Some(Color::Blue));
        assert_eq!(parse_color("dark_gray"), Some(Color::DarkGray));
        assert_eq!(parse_color("lightcyan"), Some(Color::LightCyan));
    }

    #[test]
    fn test_parse_hex_colors() {
        assert_eq!(parse_color("#ff0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(parse_color("00ff00"), Some(Color::Rgb(0, 255, 0)));
        assert_eq!(parse_color("#1a2b3c"), Some(Color::Rgb(26, 43, 60)));
    }

    #[test]
    fn test_parse_rgb_colors() {
        assert_eq!(parse_color("rgb(255,0,0)"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(parse_color("rgb(0, 128, 255)"), Some(Color::Rgb(0, 128, 255)));
    }

    #[test]
    fn test_theme_presets() {
        assert!(Theme::by_name("dark").is_some());
        assert!(Theme::by_name("classic").is_some());
        assert!(Theme::by_name("light").is_some());
        assert!(Theme::by_name("nonexistent").is_none());
    }

    #[test]
    fn test_default_highlights() {
        let config = ThemeConfig::default();
        assert!(!config.highlights.is_empty(), "Default highlights should not be empty");

        // Build theme and verify highlights are compiled
        let theme = config.build_theme();
        assert!(!theme.highlights.is_empty(), "Compiled highlights should not be empty");

        // Test executable pattern match
        let result = theme.find_highlight("test.exe", true, false);
        assert!(result.is_some(), "Should match executable files");
        let (_, prefix, _) = result.unwrap();
        assert_eq!(prefix, Some("*"), "Executable should have * prefix");

        // Test symlink pattern match
        let result = theme.find_highlight("link", false, true);
        assert!(result.is_some(), "Should match symlinks");

        // Test archive pattern match
        let result = theme.find_highlight("file.tar.gz", false, false);
        assert!(result.is_some(), "Should match archive files");
    }

    #[test]
    fn test_highlights_from_toml() {
        // Simulate loading config without highlights section
        let toml_str = r#"
            preset = "dark"
        "#;
        let config: ThemeConfig = toml_edit::de::from_str(toml_str).unwrap();
        assert!(!config.highlights.is_empty(), "Should use default highlights when not in TOML");
    }
}

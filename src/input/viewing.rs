//! Viewing mode handlers (file viewer, plugin viewer, help)

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::state::app::App;
use crate::state::mode::{BinaryViewMode, Mode};
use crate::ui::FileViewer;

pub fn handle_viewing_mode(app: &mut App, key: KeyEvent, visible_height: usize) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let term_width = app.ui.terminal_width as usize;

    let Mode::Viewing { content, scroll, binary_mode, .. } = &mut app.mode else {
        return;
    };

    let line_count = FileViewer::line_count(content, term_width, *binary_mode);
    let max_scroll = line_count.saturating_sub(visible_height);
    let half_page = visible_height / 2;

    // Handle 'g' prefix commands (gg, ge)
    if app.ui.viewer_pending_g {
        app.ui.viewer_pending_g = false;
        match key.code {
            // gg - go to top (helix/vim)
            KeyCode::Char('g') => {
                *scroll = 0;
                return;
            }
            // ge - go to end (helix)
            KeyCode::Char('e') => {
                *scroll = max_scroll;
                return;
            }
            _ => {
                // Invalid g-command, fall through to normal handling
            }
        }
    }

    match key.code {
        // Exit viewer
        KeyCode::Esc | KeyCode::F(3) | KeyCode::Char('q') | KeyCode::F(10) => {
            app.ui.viewer_pending_g = false;
            app.mode = Mode::Normal;
            app.refresh_panels();
        }

        // Show plugin menu (F2)
        KeyCode::F(2) => {
            app.show_viewer_plugin_menu();
        }

        // Open search dialog (/)
        KeyCode::Char('/') => {
            app.show_viewer_search();
        }

        // Next search match (n)
        KeyCode::Char('n') if !ctrl => {
            app.viewer_next_match();
        }

        // Previous search match (N)
        KeyCode::Char('N') => {
            app.viewer_prev_match();
        }

        // 'g' prefix - start g-command
        KeyCode::Char('g') => {
            app.ui.viewer_pending_g = true;
        }

        // G - go to end (vim)
        KeyCode::Char('G') => {
            *scroll = max_scroll;
        }

        // Toggle view mode (hex/text for text files, hex/cp437 for binary)
        KeyCode::Tab => {
            *binary_mode = match *binary_mode {
                BinaryViewMode::Hex => BinaryViewMode::Cp437,
                BinaryViewMode::Cp437 => BinaryViewMode::Hex,
            };
            // Recalculate max_scroll for new mode and clamp scroll
            let new_line_count = FileViewer::line_count(content, term_width, *binary_mode);
            let new_max_scroll = new_line_count.saturating_sub(visible_height);
            if *scroll > new_max_scroll {
                *scroll = new_max_scroll;
            }
        }

        // Scroll up - k or Up arrow
        KeyCode::Up | KeyCode::Char('k') => {
            *scroll = scroll.saturating_sub(1);
        }

        // Scroll down - j or Down arrow
        KeyCode::Down | KeyCode::Char('j') => {
            if *scroll < max_scroll {
                *scroll += 1;
            }
        }

        // Scroll left (not typically used in viewer, but for consistency)
        KeyCode::Left | KeyCode::Char('h') => {
            *scroll = scroll.saturating_sub(1);
        }

        // Scroll right (not typically used in viewer, but for consistency)
        KeyCode::Right | KeyCode::Char('l') => {
            if *scroll < max_scroll {
                *scroll += 1;
            }
        }

        // Half page up - Ctrl+u (vim)
        KeyCode::Char('u') if ctrl => {
            *scroll = scroll.saturating_sub(half_page);
        }

        // Half page down - Ctrl+d (vim)
        KeyCode::Char('d') if ctrl => {
            *scroll = (*scroll + half_page).min(max_scroll);
        }

        // Page up - PageUp or Ctrl+b (vim)
        KeyCode::PageUp | KeyCode::Char('b') if key.code == KeyCode::PageUp || ctrl => {
            *scroll = scroll.saturating_sub(visible_height);
        }

        // Page down - PageDown or Ctrl+f (vim)
        KeyCode::PageDown | KeyCode::Char('f') if key.code == KeyCode::PageDown || ctrl => {
            *scroll = (*scroll + visible_height).min(max_scroll);
        }

        // Home - go to start
        KeyCode::Home => {
            *scroll = 0;
        }

        // End - go to end
        KeyCode::End => {
            *scroll = max_scroll;
        }

        _ => {}
    }
}

pub fn handle_plugin_viewing_mode(app: &mut App, key: KeyEvent, visible_height: usize) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Check configurable keybindings before borrowing app.mode mutably.
    if app.key_matches("viewer_save", &key) {
        app.save_plugin_viewer_output();
        return;
    }

    let Mode::ViewingPlugin { scroll, total_lines, status_message, .. } = &mut app.mode else {
        return;
    };

    // Clear any status message on the next keypress.
    *status_message = None;

    let max_scroll = total_lines.saturating_sub(visible_height);
    let half_page = visible_height / 2;

    // All lines are loaded up front, so scrolling just changes the offset —
    // no plugin round-trip needed.  The widget slices from `scroll`.

    // Handle 'g' prefix commands (gg, ge)
    if app.ui.viewer_pending_g {
        app.ui.viewer_pending_g = false;
        match key.code {
            // gg - go to top (helix/vim)
            KeyCode::Char('g') => {
                *scroll = 0;
            }
            // ge - go to end (helix)
            KeyCode::Char('e') => {
                *scroll = max_scroll;
            }
            _ => {}
        }
        return;
    }

    match key.code {
        // Exit viewer
        KeyCode::Esc | KeyCode::F(3) | KeyCode::Char('q') | KeyCode::F(10) => {
            app.ui.viewer_pending_g = false;
            app.mode = Mode::Normal;
            app.refresh_panels();
        }

        // 'g' prefix - start g-command
        KeyCode::Char('g') => {
            app.ui.viewer_pending_g = true;
        }

        // G - go to end (vim)
        KeyCode::Char('G') => {
            *scroll = max_scroll;
        }

        // Switch to built-in viewer
        KeyCode::Tab => {
            app.switch_plugin_to_builtin_viewer();
        }

        // Show plugin menu (F2)
        KeyCode::F(2) => {
            app.show_viewer_plugin_menu();
        }

        // Scroll up - k or Up arrow
        KeyCode::Up | KeyCode::Char('k') => {
            *scroll = scroll.saturating_sub(1);
        }

        // Scroll down - j or Down arrow
        KeyCode::Down | KeyCode::Char('j') => {
            if *scroll < max_scroll {
                *scroll += 1;
            }
        }

        // Half page up - Ctrl+u (vim)
        KeyCode::Char('u') if ctrl => {
            *scroll = scroll.saturating_sub(half_page);
        }

        // Half page down - Ctrl+d (vim)
        KeyCode::Char('d') if ctrl => {
            *scroll = (*scroll + half_page).min(max_scroll);
        }

        // Page up - PageUp or Ctrl+b (vim)
        KeyCode::PageUp | KeyCode::Char('b') if key.code == KeyCode::PageUp || ctrl => {
            *scroll = scroll.saturating_sub(visible_height);
        }

        // Page down - PageDown or Ctrl+f (vim)
        KeyCode::PageDown | KeyCode::Char('f') if key.code == KeyCode::PageDown || ctrl => {
            *scroll = (*scroll + visible_height).min(max_scroll);
        }

        // Home - go to start
        KeyCode::Home => {
            *scroll = 0;
        }

        // End - go to end
        KeyCode::End => {
            *scroll = max_scroll;
        }

        _ => {}
    }
}

pub fn handle_viewer_plugin_menu(app: &mut App, key: KeyEvent) {
    let Mode::ViewerPluginMenu { plugins, selected, .. } = &mut app.mode else {
        return;
    };

    // Total items: built-in viewer + plugins
    let total_items = 1 + plugins.len();

    match key.code {
        // Cancel - return to built-in viewer
        KeyCode::Esc | KeyCode::F(2) => {
            app.cancel_viewer_plugin_menu();
        }

        // Select current item
        KeyCode::Enter => {
            let idx = *selected;
            app.select_viewer_plugin(idx);
        }

        // Navigate up
        KeyCode::Up | KeyCode::Char('k') => {
            if *selected > 0 {
                *selected -= 1;
            }
        }

        // Navigate down
        KeyCode::Down | KeyCode::Char('j') => {
            if *selected + 1 < total_items {
                *selected += 1;
            }
        }

        // Home
        KeyCode::Home => {
            *selected = 0;
        }

        // End
        KeyCode::End => {
            *selected = total_items.saturating_sub(1);
        }

        _ => {}
    }
}

pub fn handle_help_mode(app: &mut App, key: KeyEvent, visible_height: usize) {
    let Mode::Help { scroll } = &mut app.mode else {
        return;
    };

    let help_text = get_help_text();
    let line_count = help_text.lines().count();
    let max_scroll = line_count.saturating_sub(visible_height);

    match key.code {
        // Exit help
        KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('q') => {
            app.mode = Mode::Normal;
        }

        // Scroll up
        KeyCode::Up | KeyCode::Char('k') => {
            *scroll = scroll.saturating_sub(1);
        }

        // Scroll down
        KeyCode::Down | KeyCode::Char('j') => {
            if *scroll < max_scroll {
                *scroll += 1;
            }
        }

        // Page up
        KeyCode::PageUp => {
            *scroll = scroll.saturating_sub(visible_height);
        }

        // Page down
        KeyCode::PageDown => {
            *scroll = (*scroll + visible_height).min(max_scroll);
        }

        // Home
        KeyCode::Home | KeyCode::Char('g') => {
            *scroll = 0;
        }

        // End
        KeyCode::End | KeyCode::Char('G') => {
            *scroll = max_scroll;
        }

        _ => {}
    }
}

pub fn get_help_text() -> &'static str {
    r##"Bark - Help

NAVIGATION (Visual Mode)
========================
  h, Left      Move left (Brief mode: other column)
  j, Down      Move down
  k, Up        Move up
  l, Right     Move right (Brief mode: other column)
  g, Home      Go to first entry
  G, End       Go to last entry
  Ctrl+B       Page up
  Ctrl+F       Page down
  PageUp       Page up
  PageDown     Page down
  Enter        Enter directory / view file
  Backspace    Go to parent directory
  Tab          Switch active panel
  Insert       Select/deselect file (for copy/move)

INSERT INTO COMMAND LINE (while typing)
=======================================
  Ctrl+F           Insert file name
  Ctrl+P           Insert current folder path
  Alt+Enter        Insert full path (folder + file)

RESIZING
========
  Shift+Up     Expand shell area (show more history)
  Shift+Down   Shrink shell area
  Shift+Left   Shrink left panel / grow right panel
  Shift+Right  Grow left panel / shrink right panel

SHELL AREA SCROLLING
====================
  Ctrl+Up/Down      Scroll shell output by one line
  Ctrl+PgUp/PgDn    Scroll shell output by 10 lines
  Alt+Up/Down       Same (macOS: Ctrl+Up triggers Mission Control)
  Alt+PgUp/PgDn     Same

COMMANDS
========
  :            Enter command mode (type shell commands)
  Ctrl+O       Toggle shell mode (interactive shell)
               Press Ctrl+O again to return to TUI
               Partial input is preserved

FUNCTION KEYS
=============
  F1           Show this help
  F3           View file contents (built-in viewer)
  F4           Edit file with external editor
  F5           Copy selected files to other panel
  F6           Move selected files to other panel
  F7           Create new directory
  F8           Delete selected files
  F10          Quit
  Alt+F1/Ctrl+F1  Source selector for left panel (drives/connections)
  Alt+F2/Ctrl+F2  Source selector for right panel (drives/connections)
  Alt+/        Find files (search with * and ? patterns)
               Results appear in TEMP panel (other panel)
  Alt+M        Toggle view mode (Brief/Full)
  Ctrl+D       Add current directory to favorites

TEMP PANEL (for search results, etc.)
=====================================
  Alt+T        Add current file to TEMP panel (other panel)
  Delete       Remove entry from TEMP panel (doesn't delete file)
  Esc          Exit TEMP mode, return to original directory

FILE SELECTION
==============
  Insert       Toggle selection of current file
  Ctrl+A       Select files by pattern (opens dialog)
  Alt+A        Select files by pattern (opens dialog)
  Ctrl+U       Unmark all selected files

SORTING
=======
  Ctrl+N       Sort by name (toggle asc/desc)
  Ctrl+T       Sort by modification time
  Ctrl+S       Sort by size
  Ctrl+F3-F7   Alternative: Name/Ext/Time/Size/Unsorted

COMMAND HISTORY
===============
  Ctrl+E       Recall command from history (press repeatedly to cycle)
  Alt+H / F9   Open command history panel

DISPLAY
=======
  Ctrl+H       Toggle hidden files
  Ctrl+R       Refresh all panels

COMMAND MODE (after pressing :)
===============================
  Enter        Execute command
  Esc          Cancel
  Tab          Complete command (cycle through matches)
  Up           Previous command from history
  Down         Next command from history
  Backspace    Delete character
  Ctrl+E       Previous command from history
  Ctrl+U       Clear line
  Ctrl+K       Clear line
  Ctrl+O       Toggle shell mode

  Regular commands show output in shell area.
  TUI programs (vi, htop, etc.) auto-detected
  and given full terminal access.

BUILT-IN COMMANDS (type after :)
================================
  config-save       Save configuration to file
  config-reload     Reload configuration from file
  config-edit       Open config file in editor
  config-upgrade    Add missing options with defaults
  config-reset      Reset config to default with full docs
  show-hidden       Toggle hidden files
  show-settings     Show current settings
  set <opt>=<val>   Change a setting at runtime:
                      hidden=true/false
                      shell_height=<n>
                      panel_ratio=<n>  (10-90, left panel %)
                      view=brief/full  (both panels)
                      left_view=brief/full
                      right_view=brief/full
                      git=true/false
                      dirs_first=true/false
                      uppercase_first=true/false
                      sort=name/ext/size/modified
                      theme=<name>
                      remember_path=true/false
  sort_name_asc     Sort by name ascending
  sort_name_desc    Sort by name descending
  sort_ext_asc      Sort by extension ascending
  sort_ext_desc     Sort by extension descending
  sort_time_asc     Sort by time ascending
  sort_time_desc    Sort by time descending
  sort_size_asc     Sort by size ascending
  sort_size_desc    Sort by size descending
  theme <name>      Switch color scheme (built-in or custom)
  themes            List all available themes
  q, quit, exit     Quit
  help, ?           Show built-in commands

CUSTOM THEMES
=============
  Define custom themes in config.toml under [theme.themes.<name>]:

    [theme.themes.mytheme]
    base = "dark"                    # Inherit from dark/classic/light
    panel_border_active = "#ff5500"  # Override specific colors
    file_directory = "cyan"
    cursor_bg = "rgb(80, 40, 120)"

  Then use:  :theme mytheme

FILE HIGHLIGHTING
=================
  Color files based on patterns in config.toml:

    [[theme.highlights]]
    pattern = "\.pdf$"        # Regex pattern
    color = "lightblue"       # Color (name, hex, or rgb)
    prefix = ">"              # Optional prefix before filename
    suffix = ""               # Optional suffix after filename

  Special patterns:
    "executable" - files with execute permission (shown with *)
    "symlink"    - symbolic links (shown with @)

  Default rules highlight executables, archives, images,
  audio/video, documents, and source code. First match wins.

  Any other command is executed in the shell.
  Use config-save after 'set' to persist changes.

SHELL MODE (Ctrl+O)
===================
  Type commands and press Enter to execute
  Ctrl+C       Clear current line
  Ctrl+O       Return to TUI (preserves partial input)

FILE VIEWER (F3)
================
  j, Down      Scroll down
  k, Up        Scroll up
  gg           Go to start (vim/helix)
  ge           Go to end (helix)
  G            Go to end (vim)
  Home         Go to start
  End          Go to end
  Ctrl+U       Half page up
  Ctrl+D       Half page down
  Ctrl+B       Page up (vim)
  Ctrl+F       Page down (vim)
  PageUp       Page up
  PageDown     Page down
  TAB          Toggle HEX/CP437 mode (built-in viewer)
               Switch to built-in viewer (plugin viewer)
  F2           Select viewer plugin
  /            Search text
  n / N        Next / previous match
  q, Esc, F3   Exit viewer

VIEWER PLUGINS
==============
  Press F2 in the viewer to select from available plugins.
  All navigation keys (gg, ge, G, Ctrl+U/D/B/F, etc.)
  work in both built-in and plugin viewers.
  TAB in a plugin viewer switches to the built-in viewer.
  F2 opens the plugin menu from either viewer.
  Ctrl+S saves plugin viewer output to a .bark_plugin.txt file.

OTHER
=====
  q            Quit (in visual mode)
  Ctrl+C       Quit

Press q, Esc, or F1 to close this help."##
}

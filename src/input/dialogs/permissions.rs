//! Permissions editing dialog handler (Unix only)

#[cfg(not(windows))]
use crossterm::event::{KeyCode, KeyEvent};
#[cfg(not(windows))]
use crate::state::app::App;
#[cfg(not(windows))]
use crate::state::mode::Mode;

#[cfg(not(windows))]
pub fn handle_permissions_mode(app: &mut App, key: KeyEvent) {
    let Mode::EditingPermissions {
        paths,
        user_read, user_write, user_execute,
        group_read, group_write, group_execute,
        other_read, other_write, other_execute,
        apply_recursive, has_dirs, focus,
        ..
    } = &mut app.mode
    else {
        return;
    };

    // Focus layout:
    // 0-8: permission checkboxes (ur, uw, ux, gr, gw, gx, or, ow, ox)
    // 9: recursive checkbox (only if has_dirs)
    // then OK, Cancel
    let ok_focus = if *has_dirs { 10 } else { 9 };
    let cancel_focus = ok_focus + 1;
    let max_focus = cancel_focus;

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }

        KeyCode::Tab => {
            *focus = if *focus >= max_focus { 0 } else { *focus + 1 };
            // Skip recursive checkbox if no dirs
            if !*has_dirs && *focus == 9 {
                *focus = ok_focus;
            }
        }

        KeyCode::BackTab => {
            if *focus == 0 {
                *focus = max_focus;
            } else {
                *focus -= 1;
                // Skip recursive checkbox if no dirs
                if !*has_dirs && *focus == 9 {
                    *focus = 8;
                }
            }
        }

        KeyCode::Char(' ') => {
            match *focus {
                0 => *user_read = !*user_read,
                1 => *user_write = !*user_write,
                2 => *user_execute = !*user_execute,
                3 => *group_read = !*group_read,
                4 => *group_write = !*group_write,
                5 => *group_execute = !*group_execute,
                6 => *other_read = !*other_read,
                7 => *other_write = !*other_write,
                8 => *other_execute = !*other_execute,
                f if f == 9 && *has_dirs => *apply_recursive = !*apply_recursive,
                _ => {}
            }
        }

        KeyCode::Enter => {
            if *focus == cancel_focus {
                app.mode = Mode::Normal;
            } else {
                // Treat Enter on any non-cancel element as OK
                let mode_bits = compose_mode(
                    *user_read, *user_write, *user_execute,
                    *group_read, *group_write, *group_execute,
                    *other_read, *other_write, *other_execute,
                );
                let recursive = *apply_recursive;
                let file_paths = paths.clone();
                app.mode = Mode::Normal;
                app.apply_permissions(&file_paths, mode_bits, recursive);
            }
        }

        // Arrow keys for navigating the grid + buttons
        KeyCode::Left => {
            if *focus <= 8 && *focus % 3 > 0 {
                *focus -= 1;
            } else if *focus == cancel_focus {
                *focus = ok_focus;
            }
        }
        KeyCode::Right => {
            if *focus <= 8 && *focus % 3 < 2 {
                *focus += 1;
            } else if *focus == ok_focus {
                *focus = cancel_focus;
            }
        }
        KeyCode::Up => {
            if *focus >= 3 && *focus <= 8 {
                *focus -= 3;
            } else if *focus == ok_focus || *focus == cancel_focus {
                // Go up from buttons to recursive checkbox or last permission row
                if *has_dirs {
                    *focus = 9; // recursive checkbox
                } else {
                    *focus = 7; // middle of Other row
                }
            } else if *focus == 9 {
                // recursive checkbox -> Other row middle
                *focus = 7;
            }
        }
        KeyCode::Down => {
            if *focus <= 5 {
                *focus += 3;
            } else if *focus >= 6 && *focus <= 8 {
                // Other row -> recursive checkbox or OK button
                if *has_dirs {
                    *focus = 9;
                } else {
                    *focus = ok_focus;
                }
            } else if *focus == 9 {
                *focus = ok_focus;
            }
        }

        _ => {}
    }
}

#[cfg(not(windows))]
fn compose_mode(
    ur: bool, uw: bool, ux: bool,
    gr: bool, gw: bool, gx: bool,
    or: bool, ow: bool, ox: bool,
) -> u32 {
    let mut mode: u32 = 0;
    if ur { mode |= 0o400; }
    if uw { mode |= 0o200; }
    if ux { mode |= 0o100; }
    if gr { mode |= 0o040; }
    if gw { mode |= 0o020; }
    if gx { mode |= 0o010; }
    if or { mode |= 0o004; }
    if ow { mode |= 0o002; }
    if ox { mode |= 0o001; }
    mode
}

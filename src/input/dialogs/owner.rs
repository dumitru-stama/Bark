//! Owner/Group editing dialog handler (Unix only)

#[cfg(not(windows))]
use crossterm::event::{KeyCode, KeyEvent};
#[cfg(not(windows))]
use crate::state::app::App;
#[cfg(not(windows))]
use crate::state::mode::Mode;

#[cfg(not(windows))]
pub fn handle_owner_mode(app: &mut App, key: KeyEvent) {
    let Mode::EditingOwner {
        paths,
        users, groups,
        user_selected, user_scroll,
        group_selected, group_scroll,
        apply_recursive, has_dirs, focus,
        ..
    } = &mut app.mode
    else {
        return;
    };

    // Focus layout:
    // 0 = user list, 1 = group list
    // 2 = recursive checkbox (only if has_dirs)
    // then OK, Cancel
    let ok_focus = if *has_dirs { 3 } else { 2 };
    let cancel_focus = ok_focus + 1;
    let max_focus = cancel_focus;

    // Visible list height for scrolling
    let list_height: usize = 6;

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }

        KeyCode::Tab => {
            *focus = if *focus >= max_focus { 0 } else { *focus + 1 };
            if !*has_dirs && *focus == 2 && ok_focus == 2 {
                // Skip recursive if no dirs and focus would land on ok_focus
                // Actually ok_focus==2 means recursive doesn't exist, focus 2 IS ok
            }
        }

        KeyCode::BackTab => {
            if *focus == 0 {
                *focus = max_focus;
            } else {
                *focus -= 1;
            }
        }

        KeyCode::Up => {
            match *focus {
                0 => {
                    if *user_selected > 0 {
                        *user_selected -= 1;
                        if *user_selected < *user_scroll {
                            *user_scroll = *user_selected;
                        }
                    }
                }
                1 => {
                    if *group_selected > 0 {
                        *group_selected -= 1;
                        if *group_selected < *group_scroll {
                            *group_scroll = *group_selected;
                        }
                    }
                }
                _ => {}
            }
        }

        KeyCode::Down => {
            match *focus {
                0 => {
                    if *user_selected + 1 < users.len() {
                        *user_selected += 1;
                        if *user_selected >= *user_scroll + list_height {
                            *user_scroll = user_selected.saturating_sub(list_height - 1);
                        }
                    }
                }
                1 => {
                    if *group_selected + 1 < groups.len() {
                        *group_selected += 1;
                        if *group_selected >= *group_scroll + list_height {
                            *group_scroll = group_selected.saturating_sub(list_height - 1);
                        }
                    }
                }
                _ => {}
            }
        }

        KeyCode::PageUp => {
            match *focus {
                0 => {
                    *user_selected = user_selected.saturating_sub(list_height);
                    if *user_selected < *user_scroll {
                        *user_scroll = *user_selected;
                    }
                }
                1 => {
                    *group_selected = group_selected.saturating_sub(list_height);
                    if *group_selected < *group_scroll {
                        *group_scroll = *group_selected;
                    }
                }
                _ => {}
            }
        }

        KeyCode::PageDown => {
            match *focus {
                0 => {
                    *user_selected = (*user_selected + list_height).min(users.len().saturating_sub(1));
                    if *user_selected >= *user_scroll + list_height {
                        *user_scroll = user_selected.saturating_sub(list_height - 1);
                    }
                }
                1 => {
                    *group_selected = (*group_selected + list_height).min(groups.len().saturating_sub(1));
                    if *group_selected >= *group_scroll + list_height {
                        *group_scroll = group_selected.saturating_sub(list_height - 1);
                    }
                }
                _ => {}
            }
        }

        KeyCode::Char(' ') => {
            if *has_dirs && *focus == 2 {
                *apply_recursive = !*apply_recursive;
            }
        }

        KeyCode::Enter => {
            if *focus == cancel_focus {
                app.mode = Mode::Normal;
            } else {
                // Apply chown
                let selected_user = users.get(*user_selected).cloned().unwrap_or_default();
                let selected_group = groups.get(*group_selected).cloned().unwrap_or_default();
                let recursive = *apply_recursive;
                let file_paths = paths.clone();
                app.mode = Mode::Normal;
                app.apply_chown(&file_paths, &selected_user, &selected_group, recursive);
            }
        }

        _ => {}
    }
}

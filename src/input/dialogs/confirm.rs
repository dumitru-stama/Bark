//! Confirmation dialog handlers

use std::path::PathBuf;
use crossterm::event::{KeyCode, KeyEvent};
use crate::input::TextField;
use crate::state::app::App;
use crate::state::mode::Mode;

pub fn handle_confirming_mode(app: &mut App, key: KeyEvent) {
    let Mode::Confirming { operation, sources, dest_input, cursor_pos, focus, apply_all } = &mut app.mode else {
        return;
    };

    let is_delete = matches!(operation, crate::state::mode::FileOperation::Delete);
    // When deleting a single directory, checkbox is a focusable element:
    //   focus 1 = checkbox, 2 = Delete, 3 = Cancel
    // Otherwise for delete: focus 1 = Delete, 2 = Cancel
    // For copy/move: focus 0 = input, 1 = OK, 2 = Cancel
    let show_checkbox = is_delete && sources.len() == 1 && sources[0].is_dir();
    let max_focus = if show_checkbox { 3 } else { 2 };
    let min_focus = if is_delete { 1 } else { 0 };
    let delete_button = if show_checkbox { 2 } else { 1 };
    let cancel_button = if show_checkbox { 3 } else { 2 };

    match key.code {
        KeyCode::Esc => {
            app.ui.input_selected = false;
            app.mode = Mode::Normal;
        }

        // Space toggles checkbox when it's focused
        KeyCode::Char(' ') if show_checkbox && *focus == 1 => {
            *apply_all = !*apply_all;
        }

        KeyCode::Tab => {
            *focus += 1;
            if *focus > max_focus {
                *focus = min_focus;
            }
            // Select text when entering text field (focus 0) with content
            app.ui.input_selected = *focus == 0 && !dest_input.is_empty();
        }

        KeyCode::BackTab => {
            if *focus <= min_focus {
                *focus = max_focus;
            } else {
                *focus -= 1;
            }
            app.ui.input_selected = *focus == 0 && !dest_input.is_empty();
        }

        KeyCode::Enter => {
            app.ui.input_selected = false;

            // Enter on checkbox toggles it
            if show_checkbox && *focus == 1 {
                *apply_all = !*apply_all;
                return;
            }

            if *focus == 0 || *focus == delete_button {
                // Enter in text field or Delete/OK button executes the operation
                let operation = operation.clone();
                let sources = sources.clone();
                let dest = PathBuf::from(dest_input.as_str());
                let apply = *apply_all;
                app.mode = Mode::Normal;

                // For delete of a single directory without apply_all,
                // enumerate contents and confirm each item individually
                if matches!(operation, crate::state::mode::FileOperation::Delete)
                    && !apply
                    && sources.len() == 1
                    && sources[0].is_dir()
                {
                    let dir_path = sources[0].clone();
                    app.start_iterative_delete(dir_path);
                } else {
                    app.start_file_operation(operation, sources, dest);
                }
            } else if *focus == cancel_button {
                app.mode = Mode::Normal;
            }
        }

        KeyCode::Backspace if *focus == 0 => {
            if app.ui.input_selected && !dest_input.is_empty() {
                dest_input.clear();
                *cursor_pos = 0;
            } else {
                TextField::backspace(dest_input, cursor_pos);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 0 => {
            if app.ui.input_selected && !dest_input.is_empty() {
                dest_input.clear();
                *cursor_pos = 0;
            } else {
                TextField::delete(dest_input, *cursor_pos);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::left(cursor_pos);
        }
        KeyCode::Right if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::right(dest_input, cursor_pos);
        }
        KeyCode::Home if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::home(cursor_pos);
        }
        KeyCode::End if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::end(dest_input, cursor_pos);
        }
        KeyCode::Char(c) if *focus == 0 => {
            if app.ui.input_selected && !dest_input.is_empty() {
                dest_input.clear();
                *cursor_pos = 0;
            }
            app.ui.input_selected = false;
            TextField::insert_char(dest_input, cursor_pos, c);
        }

        KeyCode::Left | KeyCode::Up if *focus > min_focus => {
            *focus -= 1;
        }

        KeyCode::Right | KeyCode::Down if *focus < max_focus && *focus >= 1 => {
            *focus += 1;
        }

        _ => {}
    }
}

pub fn handle_delete_iterative_mode(app: &mut App, key: KeyEvent) {
    use crate::fs::utils::delete_path;

    let Mode::DeleteIterative {
        items, parent_dir, current, deleted_count, errors, apply_all, focus
    } = &mut app.mode else {
        return;
    };

    // focus 0 = checkbox, 1 = Delete, 2 = Skip, 3 = Cancel
    let max_focus = 3;

    match key.code {
        KeyCode::Esc => {
            // Cancel: finish with what we've done so far
            let count = *deleted_count;
            let errs = errors.clone();
            app.mode = Mode::Normal;
            // Don't try to remove parent since user cancelled
            app.active_panel_mut().selected.clear();
            app.left_panel.refresh();
            app.right_panel.refresh();
            app.refresh_git_status();
            if count > 0 {
                app.add_shell_output(format!("Deleted {} item(s) (cancelled)", count));
            }
            if !errs.is_empty() {
                app.active_panel_mut().error = Some(format!(
                    "{} errors: {}", errs.len(), errs.first().unwrap_or(&String::new())
                ));
            }
        }

        // Space toggles checkbox when focused on it
        KeyCode::Char(' ') if *focus == 0 => {
            *apply_all = !*apply_all;
        }

        KeyCode::Tab | KeyCode::Right | KeyCode::Down => {
            *focus = (*focus + 1) % (max_focus + 1);
        }

        KeyCode::BackTab | KeyCode::Left | KeyCode::Up => {
            *focus = if *focus == 0 { max_focus } else { *focus - 1 };
        }

        KeyCode::Enter => {
            match *focus {
                0 => {
                    // Toggle checkbox
                    *apply_all = !*apply_all;
                }
                1 => {
                    // Delete this item
                    let item = items[*current].clone();
                    let result = delete_path(&item);
                    match result {
                        Ok(()) => *deleted_count += 1,
                        Err(e) => errors.push(format!("{}: {}", item.display(), e)),
                    }
                    *current += 1;

                    if *apply_all {
                        // Delete all remaining items without asking
                        while *current < items.len() {
                            let item = items[*current].clone();
                            let result = delete_path(&item);
                            match result {
                                Ok(()) => *deleted_count += 1,
                                Err(e) => errors.push(format!("{}: {}", item.display(), e)),
                            }
                            *current += 1;
                        }
                    }

                    if *current >= items.len() {
                        let parent = parent_dir.clone();
                        let count = *deleted_count;
                        let errs = errors.clone();
                        app.mode = Mode::Normal;
                        app.finish_iterative_delete(parent, count, errs);
                    }
                }
                2 => {
                    // Skip this item
                    *current += 1;
                    if *current >= items.len() {
                        let parent = parent_dir.clone();
                        let count = *deleted_count;
                        let errs = errors.clone();
                        app.mode = Mode::Normal;
                        app.finish_iterative_delete(parent, count, errs);
                    }
                }
                3 => {
                    // Cancel
                    let count = *deleted_count;
                    let errs = errors.clone();
                    app.mode = Mode::Normal;
                    app.active_panel_mut().selected.clear();
                    app.left_panel.refresh();
                    app.right_panel.refresh();
                    app.refresh_git_status();
                    if count > 0 {
                        app.add_shell_output(format!("Deleted {} item(s) (cancelled)", count));
                    }
                    if !errs.is_empty() {
                        app.active_panel_mut().error = Some(format!(
                            "{} errors: {}", errs.len(), errs.first().unwrap_or(&String::new())
                        ));
                    }
                }
                _ => {}
            }
        }

        _ => {}
    }
}

pub fn handle_overwrite_confirm_mode(app: &mut App, key: KeyEvent) {
    let Mode::OverwriteConfirm {
        operation, all_sources, dest, conflicts, current_conflict,
        skip_set, overwrite_all, focus,
    } = &mut app.mode else {
        return;
    };

    let button_count = 5; // Yes, All, Skip, SkipAll, Cancel

    match key.code {
        KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('C') => {
            app.mode = Mode::Normal;
        }

        KeyCode::Char('y') | KeyCode::Char('Y') => {
            // Overwrite this one — advance to next conflict
            *current_conflict += 1;
            if *current_conflict >= conflicts.len() || *overwrite_all {
                let op = operation.clone();
                let srcs: Vec<PathBuf> = all_sources.iter()
                    .filter(|s| !skip_set.contains(*s))
                    .cloned().collect();
                let d = dest.clone();
                app.mode = Mode::Normal;
                app.execute_file_operation(op, srcs, d);
            }
        }

        KeyCode::Char('a') | KeyCode::Char('A') => {
            // Overwrite all remaining
            let op = operation.clone();
            let srcs: Vec<PathBuf> = all_sources.iter()
                .filter(|s| !skip_set.contains(*s))
                .cloned().collect();
            let d = dest.clone();
            app.mode = Mode::Normal;
            app.execute_file_operation(op, srcs, d);
        }

        KeyCode::Char('s') | KeyCode::Char('S') => {
            // Skip this one
            if let Some(conflict_src) = conflicts.get(*current_conflict).cloned() {
                skip_set.insert(conflict_src);
            }
            *current_conflict += 1;
            if *current_conflict >= conflicts.len() {
                let op = operation.clone();
                let srcs: Vec<PathBuf> = all_sources.iter()
                    .filter(|s| !skip_set.contains(*s))
                    .cloned().collect();
                let d = dest.clone();
                app.mode = Mode::Normal;
                if !srcs.is_empty() {
                    app.execute_file_operation(op, srcs, d);
                }
            }
        }

        KeyCode::Char('n') | KeyCode::Char('N') => {
            // Skip All remaining
            for i in *current_conflict..conflicts.len() {
                if let Some(c) = conflicts.get(i).cloned() {
                    skip_set.insert(c);
                }
            }
            let op = operation.clone();
            let srcs: Vec<PathBuf> = all_sources.iter()
                .filter(|s| !skip_set.contains(*s))
                .cloned().collect();
            let d = dest.clone();
            app.mode = Mode::Normal;
            if !srcs.is_empty() {
                app.execute_file_operation(op, srcs, d);
            }
        }

        KeyCode::Tab | KeyCode::Right | KeyCode::Down => {
            *focus = (*focus + 1) % button_count;
        }

        KeyCode::BackTab | KeyCode::Left | KeyCode::Up => {
            *focus = if *focus == 0 { button_count - 1 } else { *focus - 1 };
        }

        KeyCode::Enter => {
            // Dispatch based on focused button
            match *focus {
                0 => { // Yes
                    *current_conflict += 1;
                    if *current_conflict >= conflicts.len() || *overwrite_all {
                        let op = operation.clone();
                        let srcs: Vec<PathBuf> = all_sources.iter()
                            .filter(|s| !skip_set.contains(*s))
                            .cloned().collect();
                        let d = dest.clone();
                        app.mode = Mode::Normal;
                        app.execute_file_operation(op, srcs, d);
                    }
                }
                1 => { // All
                    let op = operation.clone();
                    let srcs: Vec<PathBuf> = all_sources.iter()
                        .filter(|s| !skip_set.contains(*s))
                        .cloned().collect();
                    let d = dest.clone();
                    app.mode = Mode::Normal;
                    app.execute_file_operation(op, srcs, d);
                }
                2 => { // Skip
                    if let Some(conflict_src) = conflicts.get(*current_conflict).cloned() {
                        skip_set.insert(conflict_src);
                    }
                    *current_conflict += 1;
                    if *current_conflict >= conflicts.len() {
                        let op = operation.clone();
                        let srcs: Vec<PathBuf> = all_sources.iter()
                            .filter(|s| !skip_set.contains(*s))
                            .cloned().collect();
                        let d = dest.clone();
                        app.mode = Mode::Normal;
                        if !srcs.is_empty() {
                            app.execute_file_operation(op, srcs, d);
                        }
                    }
                }
                3 => { // Skip All
                    for i in *current_conflict..conflicts.len() {
                        if let Some(c) = conflicts.get(i).cloned() {
                            skip_set.insert(c);
                        }
                    }
                    let op = operation.clone();
                    let srcs: Vec<PathBuf> = all_sources.iter()
                        .filter(|s| !skip_set.contains(*s))
                        .cloned().collect();
                    let d = dest.clone();
                    app.mode = Mode::Normal;
                    if !srcs.is_empty() {
                        app.execute_file_operation(op, srcs, d);
                    }
                }
                4 => { // Cancel
                    app.mode = Mode::Normal;
                }
                _ => {}
            }
        }

        _ => {}
    }
}

pub fn handle_file_op_error_mode(app: &mut App, key: KeyEvent) {
    use crate::state::background::FileOpErrorResponse;

    let Mode::FileOpErrorDialog { focus, .. } = &mut app.mode else {
        return;
    };

    let button_count = 4; // Retry, Skip, SkipAll, Abort

    match key.code {
        KeyCode::Esc | KeyCode::Char('a') | KeyCode::Char('A') => {
            app.respond_to_file_op_error(FileOpErrorResponse::Abort);
        }

        KeyCode::Char('r') | KeyCode::Char('R') => {
            app.respond_to_file_op_error(FileOpErrorResponse::Retry);
        }

        KeyCode::Char('s') | KeyCode::Char('S') => {
            app.respond_to_file_op_error(FileOpErrorResponse::Skip);
        }

        KeyCode::Char('k') | KeyCode::Char('K') => {
            app.respond_to_file_op_error(FileOpErrorResponse::SkipAll);
        }

        KeyCode::Tab | KeyCode::Right | KeyCode::Down => {
            *focus = (*focus + 1) % button_count;
        }

        KeyCode::BackTab | KeyCode::Left | KeyCode::Up => {
            *focus = if *focus == 0 { button_count - 1 } else { *focus - 1 };
        }

        KeyCode::Enter => {
            let response = match *focus {
                0 => FileOpErrorResponse::Retry,
                1 => FileOpErrorResponse::Skip,
                2 => FileOpErrorResponse::SkipAll,
                3 => FileOpErrorResponse::Abort,
                _ => return,
            };
            app.respond_to_file_op_error(response);
        }

        _ => {}
    }
}

pub fn handle_simple_confirm_mode(app: &mut App, key: KeyEvent) {
    let Mode::SimpleConfirm { action, focus, .. } = &mut app.mode else {
        return;
    };

    match key.code {
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
            app.mode = Mode::Normal;
        }

        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let action = action.clone();
            app.mode = Mode::Normal;
            app.execute_simple_confirm_action(action);
        }

        KeyCode::Enter => {
            if *focus == 0 {
                let action = action.clone();
                app.mode = Mode::Normal;
                app.execute_simple_confirm_action(action);
            } else {
                app.mode = Mode::Normal;
            }
        }

        KeyCode::Tab | KeyCode::Left | KeyCode::Right | KeyCode::BackTab => {
            *focus = if *focus == 0 { 1 } else { 0 };
        }

        _ => {}
    }
}

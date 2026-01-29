//! Confirmation dialog handlers

use std::path::PathBuf;
use crossterm::event::{KeyCode, KeyEvent};
use crate::input::TextField;
use crate::state::app::App;
use crate::state::mode::Mode;

pub fn handle_confirming_mode(app: &mut App, key: KeyEvent) {
    let Mode::Confirming { operation, sources, dest_input, cursor_pos, focus } = &mut app.mode else {
        return;
    };

    let is_delete = matches!(operation, crate::state::mode::FileOperation::Delete);
    let max_focus = 2;
    let min_focus = if is_delete { 1 } else { 0 };

    match key.code {
        KeyCode::Esc => {
            app.ui.input_selected = false;
            app.mode = Mode::Normal;
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
            match *focus {
                // Enter in text field or OK button executes the operation
                0 | 1 => {
                    let operation = operation.clone();
                    let sources = sources.clone();
                    let dest = PathBuf::from(dest_input.as_str());
                    app.mode = Mode::Normal;
                    app.start_file_operation(operation, sources, dest);
                }
                2 => {
                    app.mode = Mode::Normal;
                }
                _ => {}
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

//! Text field input handling utilities.
//!
//! Provides common operations for text input fields used in dialogs.

/// Helper for handling text field input operations.
///
/// This eliminates duplicate code across dialog handlers for common
/// text editing operations like backspace, delete, cursor movement, etc.
pub struct TextField;

impl TextField {
    /// Handle backspace key - delete character before cursor
    #[inline]
    pub fn backspace(input: &mut String, cursor: &mut usize) {
        if *cursor > 0 {
            input.remove(*cursor - 1);
            *cursor -= 1;
        }
    }

    /// Handle delete key - delete character at cursor
    #[inline]
    pub fn delete(input: &mut String, cursor: usize) {
        if cursor < input.len() {
            input.remove(cursor);
        }
    }

    /// Handle left arrow - move cursor left
    #[inline]
    pub fn left(cursor: &mut usize) {
        if *cursor > 0 {
            *cursor -= 1;
        }
    }

    /// Handle right arrow - move cursor right
    #[inline]
    pub fn right(input: &str, cursor: &mut usize) {
        if *cursor < input.len() {
            *cursor += 1;
        }
    }

    /// Handle home key - move cursor to start
    #[inline]
    pub fn home(cursor: &mut usize) {
        *cursor = 0;
    }

    /// Handle end key - move cursor to end
    #[inline]
    pub fn end(input: &str, cursor: &mut usize) {
        *cursor = input.len();
    }

    /// Handle character input - insert at cursor
    #[inline]
    pub fn insert_char(input: &mut String, cursor: &mut usize, c: char) {
        input.insert(*cursor, c);
        *cursor += 1;
    }

    /// Handle character input with filter (e.g., digits only)
    #[inline]
    pub fn insert_char_if<F>(input: &mut String, cursor: &mut usize, c: char, predicate: F)
    where
        F: FnOnce(char) -> bool,
    {
        if predicate(c) {
            Self::insert_char(input, cursor, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backspace() {
        let mut input = "hello".to_string();
        let mut cursor = 3;
        TextField::backspace(&mut input, &mut cursor);
        assert_eq!(input, "helo");
        assert_eq!(cursor, 2);
    }

    #[test]
    fn test_backspace_at_start() {
        let mut input = "hello".to_string();
        let mut cursor = 0;
        TextField::backspace(&mut input, &mut cursor);
        assert_eq!(input, "hello");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn test_delete() {
        let mut input = "hello".to_string();
        TextField::delete(&mut input, 2);
        assert_eq!(input, "helo");
    }

    #[test]
    fn test_insert_char() {
        let mut input = "hllo".to_string();
        let mut cursor = 1;
        TextField::insert_char(&mut input, &mut cursor, 'e');
        assert_eq!(input, "hello");
        assert_eq!(cursor, 2);
    }
}

/// Windows console input helpers for shell mode.
///
/// Reads INPUT_RECORD events via ReadConsoleInputW and converts key-down events
/// to raw bytes / VT escape sequences suitable for writing to a PTY.
/// This bypasses crossterm entirely, avoiding double-key and freeze issues
/// when interacting with ConPTY.

#[cfg(windows)]
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
#[cfg(windows)]
use windows_sys::Win32::System::Console::*;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::WaitForSingleObject;

/// Result of polling Windows console input.
#[cfg(windows)]
pub enum ConsoleInput {
    /// Regular bytes to forward to the PTY.
    Data(Vec<u8>),
    /// User pressed Ctrl+O — return to Bark.
    CtrlO,
    /// No input ready within the timeout.
    None,
}

// ---------- Console mode helpers ----------

/// Save the current console input mode and switch to raw-ish mode
/// (only window input events, no line editing or echo).
/// Returns the original mode for later restoration.
#[cfg(windows)]
pub fn save_and_set_raw_console_mode() -> u32 {
    unsafe {
        let h = GetStdHandle(STD_INPUT_HANDLE);
        let mut mode: u32 = 0;
        GetConsoleMode(h, &mut mode);
        SetConsoleMode(h, ENABLE_WINDOW_INPUT);
        mode
    }
}

/// Restore a previously saved console input mode.
#[cfg(windows)]
pub fn restore_console_mode(mode: u32) {
    unsafe {
        let h = GetStdHandle(STD_INPUT_HANDLE);
        SetConsoleMode(h, mode);
    }
}

/// Flush all pending events from the console input buffer.
/// Call this after returning from an interactive shell to prevent stale
/// key events from being read by crossterm.
#[cfg(windows)]
pub fn flush_console_input() {
    unsafe {
        let h = GetStdHandle(STD_INPUT_HANDLE);
        FlushConsoleInputBuffer(h);
    }
}

/// Ensure the output console has VT processing enabled.
/// ConPTY teardown can disable it, which breaks crossterm's alternate screen
/// escape sequences.
#[cfg(windows)]
pub fn ensure_vt_processing() {
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode: u32 = 0;
        GetConsoleMode(h, &mut mode);
        let desired = mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
        if mode != desired {
            SetConsoleMode(h, desired);
        }
    }
}

// No-op stubs for non-Windows
#[cfg(not(windows))]
#[allow(dead_code)]
pub fn flush_console_input() {}
#[cfg(not(windows))]
#[allow(dead_code)]
pub fn ensure_vt_processing() {}

/// Poll the console for key events, with a timeout in milliseconds.
/// Returns `ConsoleInput::Data` with the bytes to write to the PTY,
/// `ConsoleInput::CtrlO` if the user wants to exit, or
/// `ConsoleInput::None` if there was no input within the timeout.
#[cfg(windows)]
pub fn poll_console_input(timeout_ms: u32) -> ConsoleInput {
    let stdin_handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };

    let wait = unsafe { WaitForSingleObject(stdin_handle, timeout_ms) };
    if wait != WAIT_OBJECT_0 {
        return ConsoleInput::None;
    }

    let mut records: [INPUT_RECORD; 32] = unsafe { std::mem::zeroed() };
    let mut count: u32 = 0;
    let ok = unsafe { ReadConsoleInputW(stdin_handle, records.as_mut_ptr(), 32, &mut count) };
    if ok == 0 {
        return ConsoleInput::None;
    }

    let mut out = Vec::new();

    for i in 0..count as usize {
        let event_type = records[i].EventType as u32;
        if event_type != KEY_EVENT {
            continue;
        }
        let key = unsafe { records[i].Event.KeyEvent };
        let ctrl = (key.dwControlKeyState & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED)) != 0;
        let ch = unsafe { key.uChar.UnicodeChar };

        if key.bKeyDown == 0 {
            continue; // skip key-up
        }

        // Ctrl+O = return to Bark
        if ctrl && (ch == 0x0F || ch == b'o' as u16 || ch == b'O' as u16) {
            return ConsoleInput::CtrlO;
        }

        if ch != 0 {
            // Regular character — encode UTF-16 to UTF-8
            if let Some(c) = char::from_u32(ch as u32) {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                out.extend_from_slice(s.as_bytes());
            }
        } else {
            // Special key — emit VT escape sequence
            // Virtual key codes: https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes
            let seq: &[u8] = match key.wVirtualKeyCode {
                0x25 => b"\x1b[D",    // VK_LEFT
                0x26 => b"\x1b[A",    // VK_UP
                0x27 => b"\x1b[C",    // VK_RIGHT
                0x28 => b"\x1b[B",    // VK_DOWN
                0x24 => b"\x1b[H",    // VK_HOME
                0x23 => b"\x1b[F",    // VK_END
                0x2D => b"\x1b[2~",   // VK_INSERT
                0x2E => b"\x1b[3~",   // VK_DELETE
                0x21 => b"\x1b[5~",   // VK_PRIOR (Page Up)
                0x22 => b"\x1b[6~",   // VK_NEXT (Page Down)
                0x70 => b"\x1bOP",    // VK_F1
                0x71 => b"\x1bOQ",    // VK_F2
                0x72 => b"\x1bOR",    // VK_F3
                0x73 => b"\x1bOS",    // VK_F4
                0x74 => b"\x1b[15~",  // VK_F5
                0x75 => b"\x1b[17~",  // VK_F6
                0x76 => b"\x1b[18~",  // VK_F7
                0x77 => b"\x1b[19~",  // VK_F8
                0x78 => b"\x1b[20~",  // VK_F9
                0x79 => b"\x1b[21~",  // VK_F10
                0x7A => b"\x1b[23~",  // VK_F11
                0x7B => b"\x1b[24~",  // VK_F12
                _ => b""
            };
            out.extend_from_slice(seq);
        }
    }

    if out.is_empty() {
        ConsoleInput::None
    } else {
        ConsoleInput::Data(out)
    }
}

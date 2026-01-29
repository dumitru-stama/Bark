use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};

use crate::state::app::App;
use crate::terminal::{setup_terminal, restore_terminal};

/// Check if buffer contains alternate screen buffer escape sequence
#[allow(dead_code)]
pub fn has_alternate_screen(buf: &[u8]) -> bool {
    // Look for \x1b[?1049h or \x1b[?47h or \x1b[?1047h
    for i in 0..buf.len().saturating_sub(4) {
        if buf[i] == 0x1b && buf.get(i + 1) == Some(&b'[') && buf.get(i + 2) == Some(&b'?') {
            let rest = &buf[i + 3..];
            // Check for 1049h
            if rest.starts_with(b"1049h") {
                return true;
            }
            // Check for 1047h
            if rest.starts_with(b"1047h") {
                return true;
            }
            // Check for 47h
            if rest.starts_with(b"47h") {
                return true;
            }
        }
    }
    false
}

/// Strip ANSI escape sequences from text
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    chars.next();
                    // Skip until we hit a letter or ~
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch.is_ascii_alphabetic() || ch == '~' {
                            break;
                        }
                    }
                    continue;
                } else if next == ']' {
                    // OSC sequence - skip until BEL or ST
                    chars.next();
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                                break;
                            }
                        }
                    }
                    continue;
                }
            }
        } else if c == '\r' {
            // Skip carriage returns
            continue;
        }
        result.push(c);
    }

    result
}

/// Check if output looks like it came from a TUI program
/// TUI programs typically clear the screen or exit alternate buffer when done
pub fn is_tui_output(content: &str) -> bool {
    // Look for terminal reset/clear sequences anywhere in output
    // These indicate a full-screen TUI program was running

    // ESC[?1049l - exit alternate screen buffer (most common)
    if content.contains("\x1b[?1049l") {
        return true;
    }
    // ESC[?1049h - enter alternate screen buffer
    if content.contains("\x1b[?1049h") {
        return true;
    }
    // ESC[?47l / ESC[?47h - older alternate screen
    if content.contains("\x1b[?47l") || content.contains("\x1b[?47h") {
        return true;
    }
    // ESC[2J - clear entire screen
    if content.contains("\x1b[2J") {
        return true;
    }
    // ESC c - full terminal reset (RIS)
    if content.contains("\x1bc") {
        return true;
    }
    // ESC[H followed by ESC[J - home cursor + clear
    if content.contains("\x1b[H\x1b[J") {
        return true;
    }

    false
}

/// Run a command directly in terminal, capture output for shell area
pub fn run_command_with_pty_detection(
    command: &str,
    cwd: &Path,
    _force_interactive: bool,
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> io::Result<()> {
    let shell = resolve_shell(&app.config.general.shell);
    let shell_arg = shell_command_flag(&shell);

    // Leave alternate screen so user sees the terminal
    restore_terminal()?;

    // Echo the command so user sees what's being run (like a normal shell)
    println!("{}> {}", cwd.display(), command);

    // Use script command to capture output while still allowing interaction
    // Note: script -c syntax varies by platform and may not exist on BusyBox/minimal systems
    #[cfg(target_os = "linux")]
    let (capture_cmd, capture_file) = {
        let tmp = format!("/tmp/rc_capture_{}", std::process::id());
        // Check if script supports -c (util-linux vs BusyBox)
        let script_supports_c = std::process::Command::new("script")
            .arg("--help")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("-c"))
            .unwrap_or(false);
        if script_supports_c {
            // Use the user's shell with -ic so aliases (like ls --color) are loaded
            let inner = format!("{} -ic {}", shell_quote(&shell), shell_quote(command));
            (format!("script -q -c {} {}", shell_quote(&inner), &tmp), tmp)
        } else {
            // Fallback: use Python's pty module (available on virtually all Linux systems)
            let py_script = format!(
                r#"import pty,os,sys;f=open('{}','wb')
def r(fd):
 d=os.read(fd,1024);f.write(d);sys.stdout.buffer.write(d);sys.stdout.buffer.flush();return d
pty.spawn([{},'-ic',{}],r);f.close()"#,
                &tmp,
                python_quote(&shell),
                python_quote(command)
            );
            (format!("python3 -c {}", shell_quote(&py_script)), tmp)
        }
    };

    #[cfg(target_os = "macos")]
    let (capture_cmd, capture_file) = {
        let tmp = format!("/tmp/rc_capture_{}", std::process::id());
        // macOS BSD script: script -q <file> command [args...]
        // No -c flag on macOS. Use the user's shell with -ic so aliases are loaded.
        (format!("script -q {} {} -ic {}", &tmp, &shell, shell_quote(command)), tmp)
    };

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let (capture_cmd, capture_file) = (command.to_string(), String::new());

    // Run the command
    let cmd_to_run = if capture_file.is_empty() { command } else { &capture_cmd };

    let status = std::process::Command::new(&shell)
        .arg(shell_arg)
        .arg(cmd_to_run)
        .current_dir(cwd)
        .status();

    // Read captured output if available - store ALL lines
    if !capture_file.is_empty() {
        if let Ok(content) = std::fs::read_to_string(&capture_file) {
            // Check if this looks like TUI output (has screen clear/reset at end)
            // If so, discard all output from this command
            if !is_tui_output(&content) {
                for line in content.lines() {
                    // First strip trailing \r (handles \r\n line endings)
                    let line = line.trim_end_matches('\r');

                    // Handle carriage returns: keep only content after last \r
                    // This simulates terminal behavior for progress indicators
                    let line = if let Some(pos) = line.rfind('\r') {
                        &line[pos + 1..]
                    } else {
                        line
                    };

                    // Skip script command header/footer lines
                    let clean = strip_ansi(line);
                    if clean.starts_with("Script started on ") || clean.starts_with("Script done on ") {
                        continue;
                    }
                    // Skip empty lines (check stripped version)
                    if clean.is_empty() {
                        continue;
                    }
                    // Keep the original line with ANSI codes for colored output
                    app.add_shell_output(line.to_string());
                }
            }
        }
        let _ = std::fs::remove_file(&capture_file);
    }

    // Return to TUI
    *terminal = setup_terminal()?;

    if let Err(e) = status {
        app.add_shell_output(format!("Error: {}", e));
    }

    Ok(())
}

/// Resolve which shell to use. If `configured` is non-empty, use it directly.
/// Otherwise auto-detect: on Windows pwsh > powershell > cmd.exe, on Unix $SHELL > /bin/sh.
fn resolve_shell(configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }
    if cfg!(windows) {
        if std::process::Command::new("pwsh").arg("-Version").output().is_ok() {
            return "pwsh".to_string();
        }
        if std::process::Command::new("powershell").arg("-Version").output().is_ok() {
            return "powershell".to_string();
        }
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

/// Determine the right shell argument flag for running a command
fn shell_command_flag(shell: &str) -> &'static str {
    let lower = shell.to_lowercase();
    if lower.contains("powershell") || lower.contains("pwsh") {
        "-Command"
    } else if cfg!(windows) {
        "/C"
    } else {
        "-c"
    }
}

/// Quote a command for shell
#[cfg(unix)]
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace("'", "'\\''"))
}

/// Quote a string for embedding in a Python string literal
pub fn python_quote(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("'{}'", escaped)
}

/// Run an interactive shell with PTY support (for tab completion, etc.)
/// Returns captured output lines when user presses Ctrl+O
pub fn run_interactive_shell(cwd: &Path, shell_config: &str) -> io::Result<Vec<String>> {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));

    let pty_system = native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    let shell = resolve_shell(shell_config);

    let mut cmd = CommandBuilder::new(&shell);
    cmd.cwd(cwd);

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    let mut reader = pair.master.try_clone_reader()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    let mut writer = pair.master.take_writer()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    // Set raw mode via libc with VMIN=0/VTIME=0 for poll()-based reading.
    #[cfg(unix)]
    let orig_termios = unsafe {
        let mut orig: libc::termios = std::mem::zeroed();
        libc::tcgetattr(libc::STDIN_FILENO, &mut orig);
        let mut raw = orig;
        libc::cfmakeraw(&mut raw);
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &raw);
        orig
    };
    #[cfg(not(unix))]
    enable_raw_mode()?;

    // Reset terminal attributes so the shell doesn't inherit colors from Bark's UI
    let _ = io::stdout().write_all(b"\x1b[0m");
    let _ = io::stdout().flush();

    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);

    // Shared buffer to capture PTY output for shell history
    let captured = Arc::new(Mutex::new(Vec::<u8>::new()));
    let captured_clone = Arc::clone(&captured);

    let stdout_handle = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut stdout = io::stdout();
        while running_clone.load(Ordering::Relaxed) {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = stdout.write_all(&buf[..n]);
                    let _ = stdout.flush();
                    if let Ok(mut cap) = captured_clone.lock() {
                        cap.extend_from_slice(&buf[..n]);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    'shell_loop: loop {
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }

        #[cfg(unix)]
        {
            let mut pfd = libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            };
            let ret = unsafe { libc::poll(&mut pfd, 1, 50) };
            if ret > 0 && (pfd.revents & libc::POLLIN) != 0 {
                let mut buf = [0u8; 4096];
                let n = unsafe {
                    libc::read(
                        libc::STDIN_FILENO,
                        buf.as_mut_ptr() as *mut libc::c_void,
                        buf.len(),
                    )
                };
                if n > 0 {
                    let data = &buf[..n as usize];
                    if data.contains(&0x0F) || data.windows(8).any(|w| w == b"\x1b[111;5u") {
                        break 'shell_loop;
                    }
                    let _ = writer.write_all(data);
                    let _ = writer.flush();
                } else if n == 0 {
                    break;
                }
            }
        }

        #[cfg(not(unix))]
        {
            if crossterm::event::poll(Duration::from_millis(50))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    let ctrl = key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL);
                    if ctrl && matches!(key.code, crossterm::event::KeyCode::Char('o' | 'O')) {
                        break 'shell_loop;
                    }
                    let bytes: Vec<u8> = match key.code {
                        crossterm::event::KeyCode::Char(c) if ctrl => {
                            vec![(c.to_ascii_lowercase() as u8).wrapping_sub(b'a').wrapping_add(1)]
                        }
                        crossterm::event::KeyCode::Char(c) => {
                            let mut b = [0u8; 4];
                            let s = c.encode_utf8(&mut b);
                            s.as_bytes().to_vec()
                        }
                        crossterm::event::KeyCode::Enter => vec![b'\r'],
                        crossterm::event::KeyCode::Backspace => vec![127],
                        crossterm::event::KeyCode::Tab => vec![b'\t'],
                        crossterm::event::KeyCode::Esc => vec![27],
                        _ => vec![],
                    };
                    if !bytes.is_empty() {
                        let _ = writer.write_all(&bytes);
                        let _ = writer.flush();
                    }
                }
            }
        }
    }

    #[cfg(unix)]
    unsafe {
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &orig_termios);
    }
    #[cfg(not(unix))]
    disable_raw_mode()?;

    running.store(false, Ordering::Relaxed);
    let _ = child.kill();
    drop(writer);
    drop(pair.master);
    std::thread::sleep(Duration::from_millis(100));

    let _ = child.try_wait();

    // Wait for reader thread to finish so all output is captured
    let _ = stdout_handle.join();

    print!("\r\n");
    let _ = io::stdout().flush();

    // Convert captured bytes into lines for shell history.
    // No TUI filtering — interactive shells use screen clears etc. normally.
    let raw = captured.lock().unwrap_or_else(|e| e.into_inner());
    let content = String::from_utf8_lossy(&raw);
    let mut lines = Vec::new();

    for line in content.lines() {
        let line = line.trim_end_matches('\r');

        let line = if let Some(pos) = line.rfind('\r') {
            &line[pos + 1..]
        } else {
            line
        };

        let clean = strip_ansi(line);
        if clean.is_empty() {
            continue;
        }

        lines.push(line.to_string());
    }

    Ok(lines)
}

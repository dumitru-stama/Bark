//! Persistent PTY shell that lives for the entire application lifetime.
//!
//! The shell runs on the primary screen buffer while the TUI uses the
//! alternate screen buffer.  Ctrl+O toggles between them.  Commands
//! typed in the TUI are sent to the persistent shell's stdin and
//! output streams back via an mpsc channel.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};

/// Messages sent from the reader thread to the main thread.
pub enum ShellMessage {
    /// A line of output from the shell (already stripped of trailing CR/LF).
    OutputLine(String),
    /// A command line reconstructed from input tracking in write_bytes().
    /// Distinguished from OutputLine so the drain loop can always keep these.
    InputTracked(String),
    /// The shell process has exited.
    ShellExited,
}

/// A persistent shell that owns a PTY and communicates via channels.
#[allow(dead_code)]
pub struct PersistentShell {
    /// Writer handle for sending bytes to the shell's stdin.
    writer: Box<dyn Write + Send>,
    /// Master PTY handle (kept alive to prevent EOF on the slave side).
    #[allow(dead_code)]
    master: Box<dyn MasterPty + Send>,
    /// Child process handle.
    child: Box<dyn portable_pty::Child + Send>,
    /// Receiver for messages from the reader thread.
    pub receiver: Receiver<ShellMessage>,
    /// Sender clone for injecting input-echo lines into the channel.
    input_tx: Sender<ShellMessage>,
    /// Accumulates keystrokes typed during Ctrl+O forwarding so we can
    /// echo the command line into the shell area (fancy prompts use cursor
    /// addressing that prevents capturing commands from PTY output).
    input_buf: String,
    /// Tracks escape sequence state so arrow keys / function keys don't
    /// leak into `input_buf`.  0 = normal, 1 = saw ESC, 2 = in CSI body.
    esc_state: u8,
    /// Set when arrow up/down is detected (shell history navigation).
    used_history: bool,
    /// Signals the reader thread to stop.
    running: Arc<AtomicBool>,
    /// When true, the reader thread writes raw output to stdout
    /// (user is in Ctrl+O shell mode and sees live output).
    shell_visible: Arc<AtomicBool>,
    /// When true, the reader thread discards output instead of sending
    /// it to the channel.  Used during history injection to suppress
    /// prompt noise and command re-execution output.
    pub suppress_output: Arc<AtomicBool>,
    /// Reader thread join handle.
    reader_handle: Option<JoinHandle<()>>,
    /// The last CWD we sent a `cd` for, to avoid redundant cd commands.
    last_cwd: Option<PathBuf>,
    /// The shell executable name (for choosing correct cd/chaining syntax).
    shell_name: String,
}

impl PersistentShell {
    /// Spawn a new persistent shell in the given working directory.
    pub fn spawn(cwd: &Path, shell_config: &str) -> io::Result<Self> {
        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| io::Error::other(e.to_string()))?;

        let shell = resolve_shell(shell_config);
        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(cwd);

        // Fish 4.1+ sends a Device Attributes query that hangs in PTYs
        // that don't respond.  Disable it.
        if shell.to_lowercase().contains("fish") {
            cmd.env("fish_features", "no-query-term");
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| io::Error::other(e.to_string()))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| io::Error::other(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| io::Error::other(e.to_string()))?;

        let (tx, rx) = mpsc::channel();
        let input_tx = tx.clone();

        let running = Arc::new(AtomicBool::new(true));
        let shell_visible = Arc::new(AtomicBool::new(false));
        // Start suppressed so the shell's startup banner and prompt
        // don't appear in the TUI shell area.  Cleared on first Ctrl+O.
        let suppress_output = Arc::new(AtomicBool::new(true));

        let reader_handle = Self::start_reader_thread(
            reader,
            tx,
            Arc::clone(&running),
            Arc::clone(&shell_visible),
            Arc::clone(&suppress_output),
        );

        Ok(Self {
            writer,
            master: pair.master,
            child,
            receiver: rx,
            input_tx,
            input_buf: String::new(),
            esc_state: 0,
            used_history: false,
            running,
            shell_visible,
            suppress_output,
            reader_handle: Some(reader_handle),
            last_cwd: Some(cwd.to_path_buf()),
            shell_name: shell,
        })
    }

    /// Start the background reader thread that reads PTY output and
    /// sends it to the main thread via the channel.
    fn start_reader_thread(
        mut reader: Box<dyn Read + Send>,
        tx: Sender<ShellMessage>,
        running: Arc<AtomicBool>,
        shell_visible: Arc<AtomicBool>,
        suppress_output: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut line_buf = String::new();

            while running.load(Ordering::Relaxed) {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let chunk = &buf[..n];
                        let visible = shell_visible.load(Ordering::Relaxed);
                        let suppressing = suppress_output.load(Ordering::Relaxed);

                        // If the shell is visible (Ctrl+O mode), write raw
                        // bytes directly to stdout so the user sees live output.
                        if visible {
                            let _ = io::stdout().write_all(chunk);
                            let _ = io::stdout().flush();
                        }

                        // Keep sending to the channel during visible mode
                        // so command output from Ctrl+O sessions is captured
                        // for TUI history.  The drain loop in main.rs filters
                        // out rendering noise (cursor moves, prompts, etc.).
                        // Always skip when suppress_output is set (history
                        // injection).
                        let skip_channel = suppressing;
                        let text = String::from_utf8_lossy(chunk);
                        for ch in text.chars() {
                            if ch == '\n' {
                                if !skip_channel {
                                    let line = line_buf.trim_end_matches('\r').to_string();
                                    let clean = strip_ansi(&line);
                                    if !clean.is_empty() {
                                        let _ = tx.send(ShellMessage::OutputLine(line));
                                    }
                                }
                                line_buf.clear();
                            } else {
                                line_buf.push(ch);
                            }
                        }

                        // Windows ConPTY uses cursor-positioning sequences
                        // instead of newlines, so output accumulates in
                        // line_buf forever.  Flush whatever we have after
                        // each read() chunk — ConPTY batches output per
                        // logical update so this is a reasonable boundary.
                        #[cfg(windows)]
                        if !line_buf.is_empty() {
                            if !skip_channel {
                                // ConPTY uses cursor-positioning sequences
                                // (\x1b[row;colH) instead of newlines to
                                // move between screen rows.  Split on these
                                // so each row becomes a separate output line.
                                for part in split_on_cursor_pos(&line_buf) {
                                    let line = part.trim_end_matches('\r').to_string();
                                    let clean = strip_ansi(&line);
                                    if !clean.is_empty() {
                                        let _ = tx.send(ShellMessage::OutputLine(line));
                                    }
                                }
                            }
                            line_buf.clear();
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }

            // Flush any remaining partial line.
            if !line_buf.is_empty() {
                let line = line_buf.trim_end_matches('\r').to_string();
                let clean = strip_ansi(&line);
                if !clean.is_empty() {
                    let _ = tx.send(ShellMessage::OutputLine(line));
                }
            }

            let _ = tx.send(ShellMessage::ShellExited);
        })
    }

    /// Send a shell command (appends newline).
    #[allow(dead_code)]
    pub fn send_command(&mut self, cmd: &str) -> io::Result<()> {
        self.writer.write_all(cmd.as_bytes())?;
        // Windows ConPTY expects CR (\r) to trigger command execution,
        // matching what Enter generates via poll_console_input (0x0D).
        // LF alone (\n) is not recognised as Enter by cmd.exe or PowerShell
        // running inside ConPTY.
        if cfg!(windows) {
            self.writer.write_all(b"\r\n")?;
        } else {
            self.writer.write_all(b"\n")?;
        }
        self.writer.flush()
    }

    /// Send a command, automatically prepending a `cd` if the cwd changed.
    #[allow(dead_code)]
    pub fn send_command_in_dir(&mut self, cmd: &str, cwd: &Path) -> io::Result<()> {
        let needs_cd = self
            .last_cwd
            .as_ref()
            .map_or(true, |last| last != cwd);

        if needs_cd {
            // Combine cd + command on one line to avoid extra prompt output.
            if cfg!(windows) {
                let quoted = shell_quote_platform(&cwd.to_string_lossy());
                let lower = self.shell_name.to_lowercase();
                let line = if lower.contains("powershell") || lower.contains("pwsh") {
                    // PowerShell: `cd` is an alias for Set-Location; use `;` to chain.
                    format!("cd {}; {}", quoted, cmd)
                } else {
                    // cmd.exe: `cd /d` for cross-drive navigation, `&` to chain.
                    format!("cd /d {} & {}", quoted, cmd)
                };
                self.send_command(&line)?;
            } else {
                let line = format!("cd {} && {}", shell_quote_platform(&cwd.to_string_lossy()), cmd);
                self.send_command(&line)?;
            }
            self.last_cwd = Some(cwd.to_path_buf());
        } else {
            self.send_command(cmd)?;
        }
        Ok(())
    }

    /// Write raw bytes to the shell's stdin (for Ctrl+O keystroke forwarding).
    /// Also tracks typed characters so we can echo the command line into the
    /// shell area (fancy prompts use cursor addressing that prevents capturing
    /// commands from PTY output).
    pub fn write_bytes(&mut self, data: &[u8]) -> io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;

        // Track keystrokes to reconstruct the typed command.
        // Escape sequences (arrow keys, function keys) are skipped so they
        // don't pollute the input buffer.
        for &b in data {
            // Escape sequence state machine: skip ESC [ ... <letter> sequences.
            if self.esc_state == 1 {
                // Saw ESC, check what follows.
                self.esc_state = if b == b'[' || b == b'O' { 2 } else { 0 };
                continue;
            }
            if self.esc_state == 2 {
                // Inside CSI/SS3 body — consume until a letter terminates it.
                if b.is_ascii_alphabetic() || b == b'~' {
                    if b == b'A' || b == b'B' {
                        // Arrow up/down — shell history navigation.
                        self.used_history = true;
                        self.input_buf.clear();
                    }
                    self.esc_state = 0;
                }
                continue;
            }

            match b {
                b'\r' | b'\n' => {
                    // Enter pressed — emit the accumulated command.
                    let cmd = if !self.input_buf.is_empty() {
                        std::mem::take(&mut self.input_buf)
                    } else if self.used_history {
                        "(bark: command recalled from shell history)".to_string()
                    } else {
                        String::new()
                    };
                    self.used_history = false;
                    self.input_buf.clear();
                    if !cmd.is_empty() {
                        let prompt = if let Some(cwd) = &self.last_cwd {
                            format!("{}> {}", cwd.display(), cmd)
                        } else {
                            format!("> {}", cmd)
                        };
                        let _ = self.input_tx.send(ShellMessage::InputTracked(prompt));
                    }
                }
                0x7f | 0x08 => {
                    // Backspace / DEL — remove last char.
                    self.input_buf.pop();
                }
                0x15 => {
                    // Ctrl+U — clear line.
                    self.input_buf.clear();
                }
                0x17 => {
                    // Ctrl+W — delete last word.
                    let trimmed = self.input_buf.trim_end();
                    if let Some(pos) = trimmed.rfind(' ') {
                        self.input_buf.truncate(pos + 1);
                    } else {
                        self.input_buf.clear();
                    }
                }
                0x1b => {
                    // Start of escape sequence.
                    self.esc_state = 1;
                }
                b if b >= 0x20 => {
                    // Printable ASCII.
                    self.input_buf.push(b as char);
                }
                _ => {
                    // Control chars (Ctrl+C, Ctrl+D, etc.) — ignore for tracking.
                }
            }
        }

        Ok(())
    }

    /// Toggle whether the reader thread writes raw output to stdout.
    pub fn set_visible(&self, visible: bool) {
        self.shell_visible.store(visible, Ordering::Relaxed);
    }

    /// Return the shell executable name.
    #[allow(dead_code)]
    pub fn shell_name(&self) -> &str {
        &self.shell_name
    }

    /// Inject a TUI-executed command into the persistent shell's history
    /// so it can be recalled with Up arrow during Ctrl+O interactive mode.
    ///
    /// Uses shell-specific mechanisms:
    /// - bash: `history -s -- 'command'`
    /// - zsh:  `print -s -- 'command'`
    /// - fish: `builtin history add -- 'command'`
    /// - PowerShell/pwsh: `[Microsoft.PowerShell.PSConsoleReadLine]::AddToHistory('command')`
    /// - cmd.exe: re-executes the command (no silent injection available)
    ///
    /// Output is suppressed so the TUI shell area isn't polluted.
    pub fn inject_history(&mut self, command: &str, cwd: &Path) -> io::Result<()> {
        let lower = self.shell_name.to_lowercase();

        // Build the injection command based on shell type.
        let injection = if lower.contains("bash") {
            // bash: history -s adds to in-memory history without executing.
            let escaped = command.replace('\'', "'\\''");
            format!("history -s -- '{}'", escaped)
        } else if lower.contains("zsh") {
            let escaped = command.replace('\'', "'\\''");
            format!("print -s -- '{}'", escaped)
        } else if lower.contains("fish") {
            let escaped = command.replace('\'', "\\'");
            format!("builtin history add -- '{}'", escaped)
        } else if lower.contains("powershell") || lower.contains("pwsh") {
            // PSReadLine's AddToHistory doesn't reliably work inside ConPTY.
            // Use the same re-execution strategy as cmd.exe — the command runs
            // in the persistent shell (output suppressed) so it enters the
            // native history buffer and can be recalled with Up arrow.
            String::new()
        } else if cfg!(windows) {
            // cmd.exe: no silent history injection exists.  Re-execute
            // the command in the persistent shell (with cd if needed)
            // so it enters the doskey history buffer.  Output is
            // suppressed by the reader thread.
            String::new() // handled below as special case
        } else {
            // Unknown Unix shell — try bash-style.
            let escaped = command.replace('\'', "'\\''");
            format!("history -s -- '{}'", escaped)
        };

        // Suppress reader thread output during injection.
        self.suppress_output.store(true, Ordering::Relaxed);

        if injection.is_empty() {
            // cmd.exe special case: re-execute command in persistent shell.
            self.send_command_in_dir(command, cwd)?;
        } else {
            self.send_command(&injection)?;
        }

        // Clear suppress after a delay (spawns a background thread).
        let suppress = Arc::clone(&self.suppress_output);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            suppress.store(false, Ordering::Relaxed);
        });

        Ok(())
    }

    /// Check if the shell child process is still alive.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Resize the PTY to match the terminal dimensions.
    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    /// Shut down the persistent shell: kill child, signal reader, try to join.
    pub fn shutdown(mut self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.child.kill();
        // Drop writer first to unblock the reader thread (causes EOF).
        drop(self.writer);
        // On Windows, ConPTY cleanup can be slow — give the reader thread
        // a moment to notice the EOF.  If it's still stuck in a blocking
        // read after the timeout, just detach it (the process is exiting).
        std::thread::sleep(Duration::from_millis(200));
        // Don't join — the reader thread may be stuck in a blocking read
        // (especially on Windows with ConPTY).  Dropping the JoinHandle
        // detaches the thread; it will be cleaned up at process exit.
    }
}

// ---------------------------------------------------------------------------
// Helper functions (moved from main.rs)
// ---------------------------------------------------------------------------

/// Strip ANSI escape sequences from text.
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    chars.next();
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch.is_ascii_alphabetic() || ch == '~' {
                            break;
                        }
                    }
                    continue;
                } else if next == ']' {
                    // OSC sequence — skip until BEL or ST
                    chars.next();
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                    continue;
                }
            }
        } else if c == '\r' {
            continue;
        }
        result.push(c);
    }

    result
}

/// Split ConPTY output on cursor-position sequences (`\x1b[row;colH`).
/// ConPTY uses these instead of newlines to move between screen rows,
/// so we treat each one as a line break for the TUI shell area.
#[cfg(windows)]
fn split_on_cursor_pos(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut start = 0;
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            let csi_start = i;
            i += 2; // skip ESC [
            // Scan CSI parameter bytes: digits, semicolons, '?'
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b';' || bytes[i] == b'?') {
                i += 1;
            }
            if i < len && bytes[i] == b'H' {
                // Cursor position — emit text before this sequence
                if csi_start > start {
                    parts.push(&s[start..csi_start]);
                }
                i += 1; // skip 'H'
                start = i;
            } else if i < len {
                i += 1; // skip other CSI terminator
            }
        } else {
            i += 1;
        }
    }

    if start < len {
        parts.push(&s[start..]);
    }

    if parts.is_empty() && !s.is_empty() {
        parts.push(s);
    }

    parts
}

/// Resolve which shell to use.  If `configured` is non-empty, use it
/// directly.  Otherwise auto-detect: on Windows pwsh > powershell >
/// cmd.exe, on Unix $SHELL > /bin/sh.
pub fn resolve_shell(configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }
    if cfg!(windows) {
        if std::process::Command::new("pwsh")
            .arg("-Version")
            .output()
            .is_ok()
        {
            return "pwsh".to_string();
        }
        if std::process::Command::new("powershell")
            .arg("-Version")
            .output()
            .is_ok()
        {
            return "powershell".to_string();
        }
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

/// Determine the right shell argument flag for running a command.
#[allow(dead_code)]
pub fn shell_command_flag(shell: &str) -> &'static str {
    let lower = shell.to_lowercase();
    if lower.contains("powershell") || lower.contains("pwsh") {
        "-Command"
    } else if cfg!(windows) {
        "/C"
    } else {
        "-c"
    }
}

/// Quote a string for Unix shell (single-quote with escaping).
#[cfg(unix)]
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Check if output looks like it came from a TUI program
/// (alternate screen sequences, full clears, etc.).
pub fn is_tui_output(content: &str) -> bool {
    content.contains("\x1b[?1049l")
        || content.contains("\x1b[?1049h")
        || content.contains("\x1b[?47l")
        || content.contains("\x1b[?47h")
        || content.contains("\x1b[2J")
        || content.contains("\x1bc")
        || content.contains("\x1b[H\x1b[J")
}

/// Platform-appropriate quoting for shell arguments.
#[allow(dead_code)]
fn shell_quote_platform(s: &str) -> String {
    if cfg!(windows) {
        // On Windows cmd.exe, use double quotes
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        // On Unix, use single quotes with proper escaping
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Run the Ctrl+O forwarding loop: read stdin bytes and forward them
/// to the persistent shell.  Returns when the user presses Ctrl+O
/// again (or the shell dies).
pub fn run_forwarding_loop(shell: &mut PersistentShell) -> io::Result<()> {
    // Sync the PTY size with the real terminal before entering so that
    // full-screen programs (vi, htop, …) get correct dimensions.
    if let Ok((cols, rows)) = crossterm::terminal::size() {
        shell.resize(cols, rows);
    }
    #[cfg(unix)]
    let mut last_size = crossterm::terminal::size().unwrap_or((80, 24));

    // Set raw terminal mode for transparent byte forwarding
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
    #[cfg(windows)]
    let orig_console_mode = crate::win_console::save_and_set_raw_console_mode();

    'forward: loop {
        // Check if child has exited
        if !shell.is_alive() {
            break;
        }

        // Forward terminal resize events to the PTY so that
        // full-screen programs (vi, htop) update their layout.
        #[cfg(unix)]
        if let Ok(cur) = crossterm::terminal::size() {
            if cur != last_size {
                shell.resize(cur.0, cur.1);
                last_size = cur;
            }
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
                    // Ctrl+O (0x0F) or Kitty protocol ESC[111;5u
                    if data.contains(&0x0F)
                        || data.windows(8).any(|w| w == b"\x1b[111;5u")
                    {
                        break 'forward;
                    }
                    let _ = shell.write_bytes(data);
                } else if n == 0 {
                    break; // EOF
                }
            }
        }

        #[cfg(windows)]
        {
            match crate::win_console::poll_console_input(50) {
                crate::win_console::ConsoleInput::CtrlO => break 'forward,
                crate::win_console::ConsoleInput::Data(data) => {
                    let _ = shell.write_bytes(&data);
                }
                crate::win_console::ConsoleInput::None => {}
            }
        }
    }

    // Restore terminal settings
    #[cfg(unix)]
    unsafe {
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &orig_termios);
    }
    #[cfg(windows)]
    crate::win_console::restore_console_mode(orig_console_mode);

    Ok(())
}

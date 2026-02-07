use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct EmbeddedTerminal {
    pty_pair: PtyPair,
    parser: Arc<Mutex<vt100::Parser>>,
    writer: Box<dyn Write + Send>,
    running: Arc<Mutex<bool>>,
}

/// Escape a string for safe use in single-quoted shell arguments.
pub(crate) fn shell_escape(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '/' || c == '_' || c == '-')
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

impl EmbeddedTerminal {
    pub fn new(cols: u16, rows: u16) -> Result<Self> {
        let pty_system = native_pty_system();
        let pty_pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 1000)));
        let writer = pty_pair.master.take_writer()?;
        let running = Arc::new(Mutex::new(false));

        Ok(Self {
            pty_pair,
            parser,
            writer,
            running,
        })
    }

    /// Start the reader thread that processes PTY output
    fn start_reader_thread(&self) -> Result<()> {
        let mut reader = self.pty_pair.master.try_clone_reader()?;
        let parser = Arc::clone(&self.parser);
        let running = Arc::clone(&self.running);

        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(mut p) = parser.lock() {
                            p.process(&buf[..n]);
                        }
                    }
                    Err(_) => break,
                }
                if !*running.lock().unwrap() {
                    break;
                }
            }
            *running.lock().unwrap() = false;
        });

        Ok(())
    }

    /// Spawn nvim for viewing a log file with auto-reload on changes
    ///
    /// The nvim instance is configured with:
    /// - autoread: automatically reload when file changes
    /// - CursorHold autocmd: check for changes when cursor is idle
    /// - noswapfile: don't create swap files for temp logs
    pub fn spawn_log_viewer(&mut self, log_path: &str) -> Result<()> {
        let escaped_path = shell_escape(log_path);

        // nvim with autoread settings for live log viewing
        // - autoread: auto-reload when file changes externally
        // - noswapfile: don't clutter with .swp files
        // - CursorHold autocmd: check for updates when idle
        // - updatetime=1000: check every 1 second when idle
        let script = format!(
            r#"nvim -c "set autoread noswapfile updatetime=1000 | au CursorHold * checktime | normal G" {escaped_path}"#,
        );

        let mut cmd = CommandBuilder::new("bash");
        cmd.args(["-c", &script]);

        let child = self.pty_pair.slave.spawn_command(cmd)?;
        *self.running.lock().unwrap() = true;

        self.start_reader_thread()?;

        // Don't wait for child - let it run in background
        drop(child);

        Ok(())
    }

    /// Spawn a generic editor for a file
    #[allow(dead_code)]
    pub fn spawn_editor(&mut self, file_path: &str) -> Result<()> {
        if file_path.is_empty() {
            return Ok(());
        }

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nvim".to_string());
        let escaped_path = shell_escape(file_path);

        let mut cmd = CommandBuilder::new("bash");
        cmd.args(["-c", &format!("{editor} {escaped_path}")]);

        let child = self.pty_pair.slave.spawn_command(cmd)?;
        *self.running.lock().unwrap() = true;

        self.start_reader_thread()?;

        drop(child);

        Ok(())
    }

    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.pty_pair.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        if let Ok(mut p) = self.parser.lock() {
            p.set_size(rows, cols);
        }
        Ok(())
    }

    /// Get screen contents with styling for rendering
    #[allow(clippy::type_complexity)]
    pub fn get_screen_with_styles(
        &self,
    ) -> Option<Vec<Vec<(char, vt100::Color, vt100::Color, bool)>>> {
        self.parser.lock().ok().map(|p| {
            let screen = p.screen();
            (0..screen.size().0)
                .map(|row| {
                    (0..screen.size().1)
                        .map(|col| {
                            let cell = screen.cell(row, col).unwrap();
                            let ch = cell.contents().chars().next().unwrap_or(' ');
                            let fg = cell.fgcolor();
                            let bg = cell.bgcolor();
                            let bold = cell.bold();
                            (ch, fg, bg, bold)
                        })
                        .collect()
                })
                .collect()
        })
    }

    pub fn cursor_position(&self) -> Option<(u16, u16)> {
        self.parser
            .lock()
            .ok()
            .map(|p| p.screen().cursor_position())
    }

    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }

    pub fn stop(&mut self) {
        *self.running.lock().unwrap() = false;
        // Send Ctrl+C to terminate
        let _ = self.write(&[3]);
    }
}

impl Drop for EmbeddedTerminal {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escape_safe_strings() {
        // Alphanumeric and safe characters should pass through unchanged
        assert_eq!(shell_escape("file.txt"), "file.txt");
        assert_eq!(shell_escape("/path/to/file"), "/path/to/file");
        assert_eq!(shell_escape("my_file-name"), "my_file-name");
        assert_eq!(shell_escape("file123.txt"), "file123.txt");
        assert_eq!(shell_escape("a.b.c"), "a.b.c");
        assert_eq!(shell_escape("/usr/local/bin"), "/usr/local/bin");
        assert_eq!(shell_escape("test_file-2024.log"), "test_file-2024.log");
    }

    #[test]
    fn test_shell_escape_spaces() {
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("my file.txt"), "'my file.txt'");
        assert_eq!(shell_escape("path with spaces"), "'path with spaces'");
    }

    #[test]
    fn test_shell_escape_single_quotes() {
        // Single quote should be escaped as '\''
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape("don't"), "'don'\\''t'");
        assert_eq!(shell_escape("'quoted'"), "''\\''quoted'\\'''");
    }

    #[test]
    fn test_shell_escape_special_chars() {
        // Command injection attempts should be quoted
        assert_eq!(shell_escape("$(rm -rf /)"), "'$(rm -rf /)'");
        assert_eq!(shell_escape("`whoami`"), "'`whoami`'");
        assert_eq!(shell_escape("cmd; rm"), "'cmd; rm'");
        assert_eq!(shell_escape("cmd | grep"), "'cmd | grep'");
        assert_eq!(shell_escape("a&b"), "'a&b'");
        assert_eq!(shell_escape("a>b"), "'a>b'");
        assert_eq!(shell_escape("a<b"), "'a<b'");
        assert_eq!(shell_escape("a*b"), "'a*b'");
        assert_eq!(shell_escape("a?b"), "'a?b'");
        assert_eq!(shell_escape("a[b]c"), "'a[b]c'");
        assert_eq!(shell_escape("$VAR"), "'$VAR'");
    }

    #[test]
    fn test_shell_escape_newlines() {
        assert_eq!(shell_escape("line1\nline2"), "'line1\nline2'");
        assert_eq!(shell_escape("a\nb\nc"), "'a\nb\nc'");
    }

    #[test]
    fn test_shell_escape_edge_cases() {
        // Empty string
        assert_eq!(shell_escape(""), "");

        // Only special characters
        assert_eq!(shell_escape("!@#$%"), "'!@#$%'");
        assert_eq!(shell_escape(";;;"), "';;;'");

        // Mixed safe and unsafe
        assert_eq!(shell_escape("file name.txt"), "'file name.txt'");
        assert_eq!(shell_escape("test's_file.log"), "'test'\\''s_file.log'");
    }

    #[test]
    fn test_shell_escape_unicode() {
        // Unicode characters should be quoted
        assert_eq!(shell_escape("café"), "'café'");
        assert_eq!(shell_escape("文件.txt"), "'文件.txt'");
        assert_eq!(shell_escape("🚀"), "'🚀'");
    }

    #[test]
    fn test_shell_escape_complex_paths() {
        // Real-world path examples
        assert_eq!(
            shell_escape("/tmp/my log file.txt"),
            "'/tmp/my log file.txt'"
        );
        assert_eq!(
            shell_escape("/home/user/it's important.doc"),
            "'/home/user/it'\\''s important.doc'"
        );
        assert_eq!(
            shell_escape("C:\\Program Files\\App"),
            "'C:\\Program Files\\App'"
        );
    }
}

use std::fs::{remove_file, File};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;

pub const CONTROL_FIFO: &str = "/tmp/gnomescroll_control.fifo";
pub const PID_FILE: &str = "/tmp/gnomescroll.pid";
pub const STAMP_FILE: &str = "/tmp/gnomescroll_finish.stamp";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserAction {
    Finish,
    Cancel,
}

pub struct Controller {
    action_rx: Receiver<UserAction>,
    stop_flag: Arc<AtomicBool>,
    pill_child: Option<Child>,
}

impl Controller {
    /// Check if another instance is already running; if so, signal it to finish and exit
    pub fn handle_toggle_instance() -> bool {
        // 1. Debounce check: if another instance finished or toggled within the last 1.2s, ignore key repeats
        if let Ok(metadata) = std::fs::metadata(STAMP_FILE) {
            if let Ok(mtime) = metadata.modified() {
                if let Ok(elapsed) = mtime.elapsed() {
                    if elapsed < std::time::Duration::from_millis(1200) {
                        return true;
                    }
                }
            }
        }

        // 2. Check for active running instance via PID file
        if let Ok(content) = std::fs::read_to_string(PID_FILE) {
            if let Ok(pid) = content.trim().parse::<i32>() {
                if unsafe { libc::kill(pid, 0) } == 0 {
                    // Running instance found; signal it via FIFO
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .write(true)
                        .custom_flags(libc::O_NONBLOCK)
                        .open(CONTROL_FIFO)
                    {
                        let _ = writeln!(file, "DONE");
                        let _ = file.flush();
                    }
                    let _ = std::fs::write(STAMP_FILE, format!("{}", std::process::id()));
                    println!("🏁 Signaled running GNOMEscroll instance to finish.");
                    return true;
                }
            }
            let _ = remove_file(PID_FILE);
            let _ = remove_file(CONTROL_FIFO);
        }

        false
    }

    pub fn start(_x: i32, _y: i32, _width: u32, _height: u32) -> Self {
        let (tx, rx) = channel();
        let stop_flag = Arc::new(AtomicBool::new(false));

        // 1. Clean up old FIFO & PID file and create fresh ones
        let _ = remove_file(CONTROL_FIFO);
        let _ = Command::new("mkfifo").arg(CONTROL_FIFO).status();
        let _ = std::fs::write(PID_FILE, format!("{}", std::process::id()));

        // 2. FIFO listener thread (receives DONE / CANCEL from toggle shortcut or pill)
        let tx_fifo = tx.clone();
        let stop_flag_fifo = stop_flag.clone();
        thread::spawn(move || {
            while !stop_flag_fifo.load(Ordering::Relaxed) {
                if let Ok(file) = File::open(CONTROL_FIFO) {
                    let mut reader = BufReader::new(file);
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_ok() {
                        let trimmed = line.trim();
                        if trimmed == "DONE" {
                            let _ = tx_fifo.send(UserAction::Finish);
                            break;
                        } else if trimmed == "CANCEL" {
                            let _ = tx_fifo.send(UserAction::Cancel);
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
        });

        // 3. Terminal keyboard listener (if TTY)
        let tx_term = tx.clone();
        let stop_flag_term = stop_flag.clone();
        thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut handle = stdin.lock();
            let mut line = String::new();

            while !stop_flag_term.load(Ordering::Relaxed) {
                line.clear();
                match handle.read_line(&mut line) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed == "q" || trimmed.is_empty() {
                            let _ = tx_term.send(UserAction::Finish);
                            break;
                        } else if trimmed == "c" || trimmed == "cancel" {
                            let _ = tx_term.send(UserAction::Cancel);
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // 4. Send non-intrusive desktop notification (never overlay a widget in the capture area)
        let _ = Command::new("notify-send")
            .args([
                "-a", "GNOMEscroll",
                "-i", "camera-photo-symbolic",
                "-t", "3500",
                "📸 GNOMEscroll Active",
                "Scroll freely up or down. Press Super+Ctrl+S or [Enter] when finished."
            ])
            .spawn();

        Self {
            action_rx: rx,
            stop_flag,
            pill_child: None,
        }
    }

    pub fn poll_action(&self) -> Option<UserAction> {
        self.action_rx.try_recv().ok()
    }

    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(mut child) = self.pill_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::write(STAMP_FILE, format!("{}", std::process::id()));
        let _ = remove_file(PID_FILE);
        let _ = remove_file(CONTROL_FIFO);
    }
}

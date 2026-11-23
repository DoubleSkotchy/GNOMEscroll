use std::fs::{remove_file, File};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;

pub const CONTROL_FIFO: &str = "/tmp/gnomescroll_control.fifo";

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
        if Path::new(CONTROL_FIFO).exists() {
            // Attempt non-blocking write to signal active instance
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(CONTROL_FIFO)
            {
                let _ = writeln!(file, "DONE");
                let _ = file.flush();
                println!("🏁 Signaled running GNOMEscroll instance to finish.");
                return true;
            } else {
                // Stale FIFO without active reader; clean it up
                let _ = remove_file(CONTROL_FIFO);
            }
        }
        false
    }

    pub fn start(x: i32, y: i32, width: u32, height: u32) -> Self {
        let (tx, rx) = channel();
        let stop_flag = Arc::new(AtomicBool::new(false));

        // 1. Clean up old FIFO and create new one
        let _ = remove_file(CONTROL_FIFO);
        let _ = Command::new("mkfifo").arg(CONTROL_FIFO).status();

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

        // 4. Decide whether to spawn on-screen floating pill widget
        // If capture area is large (> 1200x700 or near fullscreen), DO NOT spawn the pill
        // so it never appears in the screenshot! User can simply press Super+Ctrl+S or Enter.
        let is_near_fullscreen = width >= 1200 || height >= 700;
        let mut pill_child = None;

        if is_near_fullscreen {
            // Send quick desktop notification so user knows capture is active
            let _ = Command::new("notify-send")
                .args([
                    "-a", "GNOMEscroll",
                    "-i", "camera-photo-symbolic",
                    "-t", "3000",
                    "Recording Started",
                    "Scroll down, then press Super+Ctrl+S (or Enter) to finish."
                ])
                .spawn();
        } else {
            // Spawn floating pill outside capture area if script exists
            if let Some(script_path) = find_pill_script() {
                if let Ok(child) = Command::new(script_path)
                    .arg(CONTROL_FIFO)
                    .arg(x.to_string())
                    .arg(y.to_string())
                    .arg(width.to_string())
                    .arg(height.to_string())
                    .spawn()
                {
                    pill_child = Some(child);
                }
            }
        }

        Self {
            action_rx: rx,
            stop_flag,
            pill_child,
        }
    }

    pub fn poll_action(&self) -> Option<UserAction> {
        self.action_rx.try_recv().ok()
    }

    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }
}

fn find_pill_script() -> Option<PathBuf> {
    let local = PathBuf::from("scripts/floating_pill.py");
    if local.exists() {
        return Some(local.canonicalize().unwrap_or(local));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("scripts/floating_pill.py");
            if candidate.exists() {
                return Some(candidate);
            }
            let candidate2 = dir.join("../share/gnomescroll/scripts/floating_pill.py");
            if candidate2.exists() {
                return Some(candidate2);
            }
        }
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/kalpit"));
    let user_share = home.join(".local/share/gnomescroll/floating_pill.py");
    if user_share.exists() {
        return Some(user_share);
    }

    None
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(mut child) = self.pill_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = remove_file(CONTROL_FIFO);
    }
}

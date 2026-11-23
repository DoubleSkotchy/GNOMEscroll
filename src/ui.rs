use std::fs::{remove_file, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserAction {
    Finish,
    Cancel,
}

pub struct Controller {
    action_rx: Receiver<UserAction>,
    stop_flag: Arc<AtomicBool>,
    fifo_path: Option<PathBuf>,
    pill_child: Option<Child>,
}

impl Controller {
    pub fn start() -> Self {
        let (tx, rx) = channel();
        let stop_flag = Arc::new(AtomicBool::new(false));

        // 1. Terminal keyboard listener (if TTY)
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

        // 2. Floating pill controller
        let pid = std::process::id();
        let fifo_path = PathBuf::from(format!("/tmp/gnomescroll_fifo_{}", pid));
        let _ = remove_file(&fifo_path);

        // Create FIFO using mkfifo
        let fifo_created = Command::new("mkfifo")
            .arg(&fifo_path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let mut pill_child = None;
        if fifo_created {
            // Find script path
            let script_path = find_pill_script();
            if let Some(path) = script_path {
                if let Ok(child) = Command::new(path).arg(&fifo_path).spawn() {
                    pill_child = Some(child);

                    let tx_fifo = tx.clone();
                    let f_path = fifo_path.clone();

                    thread::spawn(move || {
                        if let Ok(file) = File::open(&f_path) {
                            let mut reader = BufReader::new(file);
                            let mut buf = String::new();
                            if reader.read_line(&mut buf).is_ok() {
                                let action = buf.trim();
                                if action == "DONE" {
                                    let _ = tx_fifo.send(UserAction::Finish);
                                } else if action == "CANCEL" {
                                    let _ = tx_fifo.send(UserAction::Cancel);
                                }
                            }
                        }
                    });
                }
            }
        }

        Self {
            action_rx: rx,
            stop_flag,
            fifo_path: if fifo_created { Some(fifo_path) } else { None },
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
    // 1. Current dir scripts/
    let local = PathBuf::from("scripts/floating_pill.py");
    if local.exists() {
        return Some(local.canonicalize().unwrap_or(local));
    }

    // 2. Binary sibling directory
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

    // 3. User local share
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
        if let Some(path) = self.fifo_path.take() {
            let _ = remove_file(path);
        }
    }
}

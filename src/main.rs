mod export;
mod screencast;
mod stitcher;
mod ui;

use clap::Parser;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "gnomescroll",
    author = "Kalpit",
    version = "0.1.0",
    about = "High-performance scrolling screenshot utility for GNOME Wayland"
)]
struct Args {
    /// Target capture area in 'x,y,width,height' format (e.g. 100,200,800,600)
    #[arg(short, long)]
    area: Option<String>,

    /// Custom output file path (defaults to ~/Pictures/Screenshots/...)
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Do not copy the resulting screenshot to the system clipboard
    #[arg(long)]
    no_clipboard: bool,

    /// Frame rate for screencast capture (default: 30)
    #[arg(long, default_value_t = 30)]
    fps: u32,
}

fn parse_area(area_str: &str) -> Result<(i32, i32, u32, u32), String> {
    let parts: Vec<&str> = area_str.split(',').collect();
    if parts.len() != 4 {
        return Err("Area must be formatted as x,y,width,height (e.g. 100,100,800,600)".into());
    }

    let x = parts[0].trim().parse::<i32>().map_err(|e| e.to_string())?;
    let y = parts[1].trim().parse::<i32>().map_err(|e| e.to_string())?;
    let w = parts[2].trim().parse::<u32>().map_err(|e| e.to_string())?;
    let h = parts[3].trim().parse::<u32>().map_err(|e| e.to_string())?;

    if w == 0 || h == 0 {
        return Err("Width and height must be greater than zero".into());
    }

    Ok((x, y, w, h))
}

fn find_script(name: &str) -> Option<PathBuf> {
    let local = PathBuf::from(format!("scripts/{}", name));
    if local.exists() {
        return Some(local.canonicalize().unwrap_or(local));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(format!("scripts/{}", name));
            if candidate.exists() {
                return Some(candidate);
            }
            let candidate2 = dir.join(format!("../share/gnomescroll/{}", name));
            if candidate2.exists() {
                return Some(candidate2);
            }
        }
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/kalpit"));
    let user_share = home.join(format!(".local/share/gnomescroll/{}", name));
    if user_share.exists() {
        return Some(user_share);
    }

    None
}

fn select_area_interactive() -> Option<(i32, i32, u32, u32)> {
    if let Some(script_path) = find_script("select_area.py") {
        if let Ok(output) = std::process::Command::new(script_path).output() {
            if output.status.success() {
                let stdout_str = String::from_utf8_lossy(&output.stdout);
                let trimmed = stdout_str.trim();
                if let Ok(geom) = parse_area(trimmed) {
                    return Some(geom);
                }
            } else {
                return None; // User pressed Esc or cancelled
            }
        }
    }
    Some(get_primary_monitor_geometry())
}

fn get_primary_monitor_geometry() -> (i32, i32, u32, u32) {
    (0, 0, 1920, 1080)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let (x, y, width, height) = match args.area {
        Some(ref a) => match parse_area(a) {
            Ok(geom) => geom,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        None => {
            match select_area_interactive() {
                Some(geom) => geom,
                None => {
                    println!("🚫 Area selection cancelled.");
                    return Ok(());
                }
            }
        }
    };

    println!("======================================================");
    println!(" 📜 GNOMEscroll: Scrolling Screenshot Active");
    println!(" Capture Region : {}x{} at ({}, {})", width, height, x, y);
    println!(" 👉 Scroll down now using your mouse wheel / trackpad!");
    println!(" ⌨️  Press [Enter] or click 'Finish' when done.");
    println!(" ⌨️  Press Ctrl+C or click 'Cancel' to abort.");
    println!("======================================================");

    // 1. Start Mutter Screencast session
    let screencast = match screencast::ScreencastSession::new(x, y, width, height) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Failed to initialize GNOME Mutter ScreenCast session: {}", e);
            eprintln!("Ensure GNOME Shell is running and Wayland session is active.");
            std::process::exit(1);
        }
    };

    // 2. Start controller (terminal listener + floating GTK pill)
    let controller = ui::Controller::start();
    let stop_flag = controller.stop_flag();

    // 3. Start PipeWire frame ingestion stream
    let frame_rx = match screencast.start_stream(stop_flag.clone(), args.fps) {
        Ok(rx) => rx,
        Err(e) => {
            eprintln!("❌ Failed to start frame capture stream: {}", e);
            std::process::exit(1);
        }
    };

    // 4. Initialize frame stitcher
    let mut stitcher = stitcher::FrameStitcher::new(width, height);
    let mut frames_received = 0usize;

    // Wait briefly for initial frame ingestion
    for _ in 0..20 {
        if let Ok(frame) = frame_rx.recv_timeout(Duration::from_millis(100)) {
            frames_received += 1;
            if stitcher.feed_frame(&frame) {
                break;
            }
        }
    }

    // Loop until user signals Finish or Cancel
    loop {
        if let Some(action) = controller.poll_action() {
            match action {
                ui::UserAction::Finish => {
                    println!("\n🏁 Finishing capture and stitching...");
                    break;
                }
                ui::UserAction::Cancel => {
                    println!("\n🚫 Capture cancelled by user.");
                    return Ok(());
                }
            }
        }

        // Ingest pending frames
        while let Ok(frame) = frame_rx.try_recv() {
            frames_received += 1;
            if stitcher.feed_frame(&frame) {
                print!("\r📸 Stitched slice #{} (Frames analyzed: {})", stitcher.stitched_count, frames_received);
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
        }

        std::thread::sleep(Duration::from_millis(15));
    }

    // Stop screencast stream
    stop_flag.store(true, Ordering::Relaxed);
    screencast.stop();

    // Ingest any remaining frames from the channel buffer
    while let Ok(frame) = frame_rx.try_recv() {
        stitcher.feed_frame(&frame);
    }

    println!("\n🔄 Finalizing composite image buffer...");
    let (final_w, final_h, count, final_rgba) = stitcher.finish();

    if final_h == 0 || final_rgba.is_empty() {
        eprintln!("⚠️  No frames were captured.");
        return Ok(());
    }

    if final_h <= height && count <= 1 {
        println!("ℹ️  No scrolling movement detected. Single frame captured.");
    } else {
        println!("✨ Successfully stitched long screenshot: {}x{} px ({} slices)", final_w, final_h, count);
    }

    // Export to file, clipboard, and desktop notification
    let export_res = export::export_screenshot(
        final_w,
        final_h,
        final_rgba,
        args.out,
        !args.no_clipboard,
    )?;

    if let Some(path) = export_res.file_path {
        println!("💾 Saved to: {}", path.display());
    }
    if export_res.copied_to_clipboard {
        println!("📋 Copied to system clipboard (image/png).");
    }

    Ok(())
}

use chrono::Local;
use image::{ImageBuffer, Rgba};
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct ExportResult {
    pub file_path: Option<PathBuf>,
    pub copied_to_clipboard: bool,
}

pub fn export_screenshot(
    width: u32,
    height: u32,
    rgba_bytes: Vec<u8>,
    custom_output: Option<PathBuf>,
    copy_clipboard: bool,
) -> Result<ExportResult, Box<dyn std::error::Error>> {
    // 1. Construct RgbaImage
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = match ImageBuffer::from_raw(width, height, rgba_bytes) {
        Some(buffer) => buffer,
        None => return Err("Failed to create image buffer from raw RGBA bytes".into()),
    };

    // 2. Encode to PNG in memory
    let mut png_bytes = Vec::new();
    {
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        img.write_with_encoder(encoder)?;
    }

    // 3. Determine save path
    let out_path = if let Some(p) = custom_output {
        p
    } else {
        let pic_dir = dirs::picture_dir().unwrap_or_else(|| {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join("Pictures")
        });
        let ss_dir = pic_dir.join("Screenshots");
        if !ss_dir.exists() {
            let _ = create_dir_all(&ss_dir);
        }

        let now = Local::now();
        let timestamp = now.format("%Y-%m-%d %H-%M-%S").to_string();
        let filename = format!("Screenshot from {}-scrolling.png", timestamp);
        ss_dir.join(filename)
    };

    // 4. Save PNG to disk
    let mut file = File::create(&out_path)?;
    file.write_all(&png_bytes)?;
    file.flush()?;

    // 5. Copy to clipboard via wl-copy
    let mut copied = false;
    if copy_clipboard {
        if let Ok(mut child) = Command::new("wl-copy")
            .arg("-t")
            .arg("image/png")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(&png_bytes);
                let _ = stdin.flush();
            }
            if let Ok(status) = child.wait() {
                copied = status.success();
            }
        }
    }

    // 6. Play native screenshot shutter sound & send desktop notification
    play_camera_sound();
    send_notification(&out_path, copied);

    Ok(ExportResult {
        file_path: Some(out_path),
        copied_to_clipboard: copied,
    })
}

fn play_camera_sound() {
    let _ = Command::new("canberra-gtk-play")
        .args(["-i", "screen-capture"])
        .spawn();
}

fn send_notification(file_path: &Path, copied: bool) {
    let summary = "Scrolling Screenshot Captured";
    let body = if copied {
        format!(
            "Saved to {}\nCopied to clipboard.",
            file_path.file_name().unwrap_or_default().to_string_lossy()
        )
    } else {
        format!(
            "Saved to {}",
            file_path.file_name().unwrap_or_default().to_string_lossy()
        )
    };

    let _ = Command::new("notify-send")
        .args([
            "-a",
            "GNOMEscroll",
            "-i",
            "camera-photo-symbolic",
            summary,
            &body,
        ])
        .spawn();
}


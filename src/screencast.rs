use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, Value};

#[allow(dead_code)]
pub struct ScreencastSession {
    pub node_id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub session_path: OwnedObjectPath,
    conn: Connection,
}

impl ScreencastSession {
    /// Connect to GNOME Mutter ScreenCast service and start recording an area
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::session()?;

        // 1. Create ScreenCast session
        let empty_props: HashMap<String, Value> = HashMap::new();
        let session_reply = conn.call_method(
            Some("org.gnome.Mutter.ScreenCast"),
            "/org/gnome/Mutter/ScreenCast",
            Some("org.gnome.Mutter.ScreenCast"),
            "CreateSession",
            &(empty_props,),
        )?;
        let (session_path,): (OwnedObjectPath,) = session_reply.body().deserialize()?;

        // 2. Call RecordArea
        let area_props: HashMap<String, Value> = HashMap::new();
        let stream_reply = conn.call_method(
            Some("org.gnome.Mutter.ScreenCast"),
            session_path.as_str(),
            Some("org.gnome.Mutter.ScreenCast.Session"),
            "RecordArea",
            &(x, y, width as i32, height as i32, area_props),
        )?;
        let (stream_path,): (OwnedObjectPath,) = stream_reply.body().deserialize()?;

        // 3. Setup signal receiver for PipeWireStreamAdded
        let stream_proxy = zbus::blocking::Proxy::new(
            &conn,
            "org.gnome.Mutter.ScreenCast",
            stream_path.as_str(),
            "org.gnome.Mutter.ScreenCast.Stream",
        )?;

        let mut signal_iter = stream_proxy.receive_signal("PipeWireStreamAdded")?;

        // 4. Start the session
        let _ = conn.call_method(
            Some("org.gnome.Mutter.ScreenCast"),
            session_path.as_str(),
            Some("org.gnome.Mutter.ScreenCast.Session"),
            "Start",
            &(),
        )?;

        // 5. Wait for PipeWireStreamAdded signal
        let signal = signal_iter.next().ok_or("Failed to receive PipeWire stream signal")?;
        let (node_id,): (u32,) = signal.body().deserialize()?;

        Ok(Self {
            node_id,
            x,
            y,
            width,
            height,
            session_path,
            conn,
        })
    }

    /// Spawns a streaming pipeline that feeds RGBA frames into a channel
    pub fn start_stream(
        &self,
        stop_signal: Arc<AtomicBool>,
        _fps: u32,
    ) -> Result<Receiver<Vec<u8>>, Box<dyn std::error::Error>> {
        let (tx, rx) = channel();
        let node_id = self.node_id;
        let width = self.width;
        let height = self.height;
        let frame_size = (width * height * 4) as usize;

        thread::spawn(move || {
            let mut child: Child = match Command::new("gst-launch-1.0")
                .args(["-q"])
                .arg("pipewiresrc")
                .arg(format!("path={}", node_id))
                .arg("keepalive-time=100")
                .arg("always-copy=true")
                .arg("!")
                .arg("videoconvert")
                .arg("!")
                .arg("video/x-raw,format=RGBA")
                .arg("!")
                .arg("fdsink")
                .arg("fd=1")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    eprintln!("Failed to spawn gst-launch-1.0: {}", e);
                    return;
                }
            };

            let mut stdout = match child.stdout.take() {
                Some(s) => s,
                None => return,
            };

            let mut buffer = vec![0u8; frame_size];

            while !stop_signal.load(Ordering::Relaxed) {
                match stdout.read_exact(&mut buffer) {
                    Ok(()) => {
                        if tx.send(buffer.clone()).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        break;
                    }
                }
            }

            let _ = child.kill();
            let _ = child.wait();
        });

        Ok(rx)
    }

    /// Stop session cleanly
    pub fn stop(&self) {
        let _ = self.conn.call_method(
            Some("org.gnome.Mutter.ScreenCast"),
            self.session_path.as_str(),
            Some("org.gnome.Mutter.ScreenCast.Session"),
            "Stop",
            &(),
        );
    }
}

impl Drop for ScreencastSession {
    fn drop(&mut self) {
        self.stop();
    }
}

//! `FrameSource` dari webcam via subprocess ffmpeg (DirectShow/Windows).
//!
//! Uji lakmus arsitektur PRD: menambah sumber webcam TIDAK menyentuh engine
//! maupun renderer — cukup implementasi `FrameSource` baru yang membaca raw
//! `rgb24` dari pipe ffmpeg, sama seperti [`super::video::VideoSource`].

use std::io::{BufReader, Read};
use std::process::{Child, ChildStdout, Command, Stdio};

use anyhow::{anyhow, Context, Result};

use super::{Frame, FrameSource, SourceInfo};

/// Default mode capture (umum didukung sebagian besar webcam).
pub const DEFAULT_WIDTH: usize = 1280;
pub const DEFAULT_HEIGHT: usize = 720;
pub const DEFAULT_FPS: u32 = 30;

pub struct WebcamSource {
    child: Child,
    stdout: BufReader<ChildStdout>,
    info: SourceInfo,
    buf: Vec<u8>,
}

impl WebcamSource {
    /// Buka kamera `device` (nama DirectShow) pada `width x height @ fps`.
    pub fn open(device: &str, width: usize, height: usize, fps: u32) -> Result<Self> {
        let frame_bytes = width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(3))
            .ok_or_else(|| anyhow!("dimensi webcam terlalu besar"))?;

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-v")
            .arg("error")
            .arg("-f")
            .arg("dshow")
            .arg("-video_size")
            .arg(format!("{width}x{height}"))
            .arg("-framerate")
            .arg(fps.to_string())
            // Buffer besar agar frame tidak drop saat pipe sempat melambat.
            .arg("-rtbufsize")
            .arg("128M")
            .arg("-i")
            .arg(format!("video={device}"))
            .arg("-f")
            .arg("rawvideo")
            .arg("-pix_fmt")
            .arg("rgb24")
            .arg("-");
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null());

        let mut child = cmd
            .spawn()
            .context("gagal spawn ffmpeg dshow (pastikan ffmpeg ada di PATH)")?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("tidak bisa mengambil stdout ffmpeg webcam"))?;

        Ok(Self {
            child,
            stdout: BufReader::with_capacity(1 << 20, stdout),
            info: SourceInfo {
                width,
                height,
                fps: fps as f32,
            },
            buf: vec![0u8; frame_bytes],
        })
    }
}

impl FrameSource for WebcamSource {
    fn next_frame(&mut self) -> Option<Frame> {
        match self.stdout.read_exact(&mut self.buf) {
            Ok(()) => Some(Frame {
                data: self.buf.clone(),
                width: self.info.width,
                height: self.info.height,
            }),
            Err(_) => None,
        }
    }

    fn info(&self) -> SourceInfo {
        self.info
    }
}

impl Drop for WebcamSource {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Kembalikan nama device bila `spec` menunjuk webcam, atau `None` bila file.
///
/// `"webcam"`/`"cam"` -> `Some("")` (device default);
/// `"webcam:Nama"`/`"cam:Nama"` -> `Some("Nama")`.
pub fn parse_spec(spec: &str) -> Option<String> {
    let s = spec.trim();
    let lower = s.to_ascii_lowercase();
    if lower == "webcam" || lower == "cam" {
        return Some(String::new());
    }
    for prefix in ["webcam:", "cam:"] {
        if lower.starts_with(prefix) {
            return Some(s[prefix.len()..].trim().to_string());
        }
    }
    None
}

/// Device default: kamera fisik pertama. Lewati kamera virtual (NDI/OBS/EOS/
/// Virtual) yang sering tak punya feed aktif sehingga bikin capture menggantung.
pub fn default_device() -> Result<String> {
    let cams = list_devices()?;
    let is_virtual = |n: &str| {
        let l = n.to_ascii_lowercase();
        l.contains("ndi")
            || l.contains("obs")
            || l.contains("virtual")
            || l.contains("eos webcam")
            || l.contains("eoswebcam")
    };
    cams.iter()
        .find(|c| !is_virtual(c))
        .cloned()
        .or_else(|| cams.into_iter().next())
        .ok_or_else(|| anyhow!("tidak ada perangkat kamera terdeteksi"))
}

/// Daftar nama device video DirectShow via ffmpeg.
pub fn list_devices() -> Result<Vec<String>> {
    // ffmpeg menulis daftar device ke stderr dan keluar dengan status error
    // (karena input "dummy") — itu normal; kita hanya mem-parse stderr.
    let out = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-list_devices",
            "true",
            "-f",
            "dshow",
            "-i",
            "dummy",
        ])
        .stdin(Stdio::null())
        .output()
        .context("gagal menjalankan ffmpeg untuk daftar device")?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut devices = Vec::new();
    for line in stderr.lines() {
        // Baris video berformat: [dshow @ ..] "Nama Device" (video)
        if line.contains("(video)") {
            if let (Some(a), Some(b)) = (line.find('"'), line.rfind('"')) {
                if b > a {
                    devices.push(line[a + 1..b].to_string());
                }
            }
        }
    }
    Ok(devices)
}

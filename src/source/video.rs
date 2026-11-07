//! Implementasi `FrameSource` via subprocess ffmpeg (lihat ADR-001).
//!
//! Alur: `ffprobe` untuk metadata, lalu `ffmpeg` di-spawn menghasilkan raw
//! `rgb24` ke stdout; tiap `next_frame` membaca tepat satu frame dari pipe.

use std::io::{BufReader, Read};
use std::process::{Child, ChildStdout, Command, Stdio};

use anyhow::{anyhow, Context, Result};

use super::{Frame, FrameSource, SourceInfo};

pub struct VideoSource {
    child: Child,
    stdout: BufReader<ChildStdout>,
    info: SourceInfo,
    /// Buffer baca dipakai ulang antar frame (panjang = width * height * 3).
    buf: Vec<u8>,
}

impl VideoSource {
    /// Buka video.
    ///
    /// * `looping`  — bila true, ffmpeg mengulang input tanpa henti (`-stream_loop -1`).
    /// * `realtime` — bila true, ffmpeg memaksa kecepatan native (`-re`) sehingga
    ///   playback tidak lebih cepat dari fps sumber. Bila false, ffmpeg mendekode
    ///   secepat mungkin (dibatasi backpressure pipe) — berguna untuk mengukur
    ///   ceiling throughput.
    pub fn open(path: &str, looping: bool, realtime: bool) -> Result<Self> {
        let info = probe(path)?;
        let frame_bytes = info
            .width
            .checked_mul(info.height)
            .and_then(|n| n.checked_mul(3))
            .ok_or_else(|| anyhow!("dimensi video terlalu besar"))?;

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-v").arg("error");
        if realtime {
            cmd.arg("-re");
        }
        if looping {
            cmd.arg("-stream_loop").arg("-1");
        }
        cmd.arg("-i")
            .arg(path)
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
            .context("gagal spawn ffmpeg (pastikan ffmpeg ada di PATH)")?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("tidak bisa mengambil stdout ffmpeg"))?;

        Ok(Self {
            child,
            // buffer 1 MB agar pembacaan pipe efisien
            stdout: BufReader::with_capacity(1 << 20, stdout),
            info,
            buf: vec![0u8; frame_bytes],
        })
    }
}

impl FrameSource for VideoSource {
    fn next_frame(&mut self) -> Option<Frame> {
        // read_exact mengembalikan Err(UnexpectedEof) saat stream habis.
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

impl Drop for VideoSource {
    fn drop(&mut self) {
        // Pastikan subprocess ffmpeg dibunuh saat source di-drop.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Ambil metadata video lewat ffprobe (output CSV, tanpa dependensi JSON).
fn probe(path: &str) -> Result<SourceInfo> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .context("gagal menjalankan ffprobe (pastikan ffprobe ada di PATH)")?;

    if !out.status.success() {
        return Err(anyhow!(
            "ffprobe gagal untuk '{}': {}",
            path,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| anyhow!("output ffprobe kosong untuk '{}'", path))?;

    let parts: Vec<&str> = line.trim().split(',').collect();
    if parts.len() < 3 {
        return Err(anyhow!("format ffprobe tak terduga: '{}'", line));
    }

    let width: usize = parts[0].trim().parse().context("parse lebar video")?;
    let height: usize = parts[1].trim().parse().context("parse tinggi video")?;
    let fps = parse_fps(parts[2].trim());

    Ok(SourceInfo { width, height, fps })
}

/// Parse `r_frame_rate` berformat pecahan ("30/1", "30000/1001").
fn parse_fps(s: &str) -> f32 {
    if let Some((n, d)) = s.split_once('/') {
        let n: f32 = n.parse().unwrap_or(30.0);
        let d: f32 = d.parse().unwrap_or(1.0);
        if d != 0.0 {
            return n / d;
        }
    }
    s.parse().unwrap_or(30.0)
}

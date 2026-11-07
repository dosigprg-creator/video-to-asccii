//! Widget stats bar (1 baris) + pengukur FPS rolling.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

/// Pengukur FPS rolling: rata-rata jumlah frame pada jendela 0.5 detik.
pub struct FpsMeter {
    stamps: VecDeque<Instant>,
    window: Duration,
}

impl Default for FpsMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl FpsMeter {
    pub fn new() -> Self {
        Self {
            stamps: VecDeque::with_capacity(128),
            window: Duration::from_millis(500),
        }
    }

    /// Catat satu frame selesai dirender.
    pub fn tick(&mut self) {
        let now = Instant::now();
        self.stamps.push_back(now);
        while let Some(&front) = self.stamps.front() {
            if now.duration_since(front) > self.window {
                self.stamps.pop_front();
            } else {
                break;
            }
        }
    }

    /// FPS saat ini berdasarkan jendela rolling.
    pub fn fps(&self) -> f32 {
        if self.stamps.len() < 2 {
            return 0.0;
        }
        let span = self
            .stamps
            .back()
            .unwrap()
            .duration_since(*self.stamps.front().unwrap())
            .as_secs_f32();
        if span <= 0.0 {
            return 0.0;
        }
        (self.stamps.len() as f32 - 1.0) / span
    }
}

/// Snapshot stats untuk ditampilkan pada bar.
#[derive(Clone, Copy)]
pub struct StatsSnapshot {
    pub fps: f32,
    pub target_fps: f32,
    pub gpu_pct: Option<f32>,
    pub vram_used_mb: Option<f32>,
    pub vram_total_mb: Option<f32>,
    pub cpu_pct: Option<f32>,
    /// Backend engine aktif: "CPU" atau "GPU".
    pub backend: &'static str,
}

impl StatsSnapshot {
    /// Format teks bar: `FPS x/target | GPU% | VRAM used/total | CPU%`.
    pub fn format(&self) -> String {
        let gpu = self
            .gpu_pct
            .map(|v| format!("{v:.0}%"))
            .unwrap_or_else(|| "--".to_string());
        let vram = match self.vram_total_mb {
            Some(t) => {
                let used = self
                    .vram_used_mb
                    .map(|u| format!("{:.1}", u / 1024.0))
                    .unwrap_or_else(|| "-".to_string());
                format!("{used}/{:.1}GB", t / 1024.0)
            }
            None => "--".to_string(),
        };
        let cpu = self
            .cpu_pct
            .map(|v| format!("{v:.0}%"))
            .unwrap_or_else(|| "--".to_string());
        format!(
            " {} | FPS {:.0}/{:.0} | GPU {} | VRAM {} | CPU {} ",
            self.backend, self.fps, self.target_fps, gpu, vram, cpu
        )
    }
}

/// Gambar stats bar mengisi seluruh `area` (diasumsikan tinggi 1 baris).
pub fn render_stats_bar(stats: &StatsSnapshot, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let style = Style::default().fg(Color::Black).bg(Color::Gray);
    // Isi latar seluruh baris agar bar terlihat solid.
    for x in area.x..area.x.saturating_add(area.width) {
        if let Some(cell) = buf.cell_mut((x, area.y)) {
            cell.set_char(' ');
            cell.set_style(style);
        }
    }
    let text = stats.format();
    let max = area.width as usize;
    let shown = if text.len() > max { &text[..max] } else { &text };
    buf.set_string(area.x, area.y, shown, style);
}

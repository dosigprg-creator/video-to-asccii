//! Entry point. Default: jalankan aplikasi TUI (clean mode).
//! `--stdout`: mode demo headless (cetak frame ke stdout) untuk verifikasi cepat.

use anyhow::Result;
use clap::Parser;

use video_to_ascii::app::{self, AppOptions};
use video_to_ascii::engine::{compute_dims, EngineConfig};
use video_to_ascii::source::FrameSource;

#[derive(Parser)]
#[command(name = "video_to_ascii", about = "Terminal video-to-ASCII player")]
struct Cli {
    /// Path video (mp4/mkv/avi/mov/webm)
    #[arg(default_value = "assets/test_1080p.mp4")]
    path: String,

    /// Batas lebar output dalam kolom (default: ikuti terminal)
    #[arg(long)]
    cols: Option<usize>,

    /// Mode demo headless: cetak frame ASCII ke stdout lalu keluar
    #[arg(long)]
    stdout: bool,

    /// Jumlah frame yang dicetak pada mode --stdout
    #[arg(long, default_value_t = 1)]
    frames: usize,

    /// Langsung mainkan video (lewati layar startup)
    #[arg(long)]
    play: bool,

    /// Pakai webcam sebagai sumber. Nama device opsional; kosong = kamera pertama.
    /// Contoh: `--webcam` atau `--webcam "Integrated Camera"`.
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    webcam: Option<String>,

    /// Daftar perangkat webcam yang terdeteksi lalu keluar
    #[arg(long)]
    list_cams: bool,

    /// Cetak info hardware terdeteksi (GPU/VRAM/CPU) lalu keluar
    #[arg(long)]
    hwinfo: bool,

    /// Pakai engine GPU (wgpu compute); fallback CPU bila init gagal
    #[arg(long)]
    gpu: bool,

    /// Mode benchmark headless: ukur FPS pipeline selama N detik lalu keluar
    #[arg(long)]
    bench_fps: Option<f64>,

    /// Adu bukti compute CPU vs GPU (head-to-head) lalu keluar
    #[arg(long)]
    bench_gpu: bool,

    /// Ukuran backend (kolom x baris) untuk --bench-fps
    #[arg(long, default_value_t = 200)]
    bench_cols: u16,

    #[arg(long, default_value_t = 55)]
    bench_rows: u16,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.list_cams {
        match video_to_ascii::source::webcam::list_devices() {
            Ok(cams) if !cams.is_empty() => {
                eprintln!("Perangkat webcam terdeteksi:");
                for (i, c) in cams.iter().enumerate() {
                    eprintln!("  [{i}] {c}");
                }
            }
            Ok(_) => eprintln!("Tidak ada perangkat webcam terdeteksi."),
            Err(e) => eprintln!("Gagal mendaftar webcam: {e}"),
        }
        return Ok(());
    }

    if cli.hwinfo {
        let hw = video_to_ascii::hardware::HardwareInfo::detect();
        let stats = video_to_ascii::hardware::StatsProvider::spawn();
        eprintln!("CPU cores : {}", hw.cpu_cores);
        eprintln!(
            "GPU       : {} [{}]",
            hw.gpu_name.as_deref().unwrap_or("(tidak terdeteksi)"),
            hw.gpu_kind.as_deref().unwrap_or("-")
        );
        eprintln!(
            "VRAM total: {}",
            hw.vram_total_mb
                .map(|v| format!("{v} MB"))
                .unwrap_or_else(|| "(tidak terdeteksi)".to_string())
        );
        std::thread::sleep(std::time::Duration::from_millis(1100));
        eprintln!("CPU usage : {:.1}%", stats.cpu_pct());
        return Ok(());
    }

    if let Some(secs) = cli.bench_fps {
        return app::run_bench(&cli.path, secs, cli.bench_cols, cli.bench_rows);
    }

    // `--webcam [device]` menimpa path positional dan langsung memainkan.
    let (source_spec, autostart) = match &cli.webcam {
        Some(dev) if dev.is_empty() => ("webcam".to_string(), true),
        Some(dev) => (format!("webcam:{dev}"), true),
        None => (cli.path.clone(), cli.play),
    };

    if cli.bench_gpu {
        return app::run_compare(&source_spec, cli.bench_cols, cli.bench_rows, 200);
    }

    if cli.stdout {
        return run_stdout_demo(&source_spec, cli.cols, cli.frames, cli.gpu);
    }

    app::run(AppOptions {
        path: source_spec,
        autostart,
        use_gpu: cli.gpu,
    })
}

/// Mode demo headless (tanpa TTY): dekode + konversi + cetak ke stdout.
/// Menerima spec file video maupun webcam ("webcam[:Nama]").
fn run_stdout_demo(spec: &str, cols: Option<usize>, frames: usize, use_gpu: bool) -> Result<()> {
    let mut src = video_to_ascii::source::open(spec, false, false)?;
    let info = src.info();
    eprintln!("Sumber: {}x{} @ {:.2} fps", info.width, info.height, info.fps);

    let cfg = EngineConfig {
        colored: false,
        ..EngineConfig::default()
    };
    let char_aspect = cfg.char_aspect;
    let mut engine = video_to_ascii::engine::Engine::new(cfg, use_gpu);
    let cols = cols.unwrap_or(100);
    let (w, h) = compute_dims(info.width, info.height, cols, 1000, char_aspect);
    eprintln!("Output ASCII: {w}x{h}");

    let mut count = 0usize;
    while count < frames {
        let Some(frame) = src.next_frame() else { break };
        let out = engine.process(&frame, w, h);
        let mut s = String::with_capacity((w + 1) * h);
        for row in &out.rows {
            for run in row {
                s.push_str(&run.text);
            }
            s.push('\n');
        }
        print!("{s}");
        count += 1;
    }
    Ok(())
}

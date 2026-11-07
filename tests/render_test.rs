//! Verifikasi headless (tanpa TTY) integrasi engine -> renderer -> buffer,
//! memakai TestBackend ratatui.

use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;

use video_to_ascii::engine::{AsciiEngine, EngineConfig};
use video_to_ascii::render::RatatuiRenderer;
use video_to_ascii::source::Frame;
use video_to_ascii::ui::clean::draw_clean;
use video_to_ascii::ui::stats_bar::StatsSnapshot;

fn solid_frame(w: usize, h: usize, r: u8, g: u8, b: u8) -> Frame {
    let mut data = vec![0u8; w * h * 3];
    for px in data.chunks_exact_mut(3) {
        px[0] = r;
        px[1] = g;
        px[2] = b;
    }
    Frame {
        data,
        width: w,
        height: h,
    }
}

#[test]
fn clean_mode_menampilkan_stats_bar_dan_sel_berwarna() {
    let frame = solid_frame(64, 36, 20, 40, 200); // biru terang
    let mut engine = AsciiEngine::new(EngineConfig::default());
    let out = engine.process(&frame, 20, 8);

    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    let mut renderer = RatatuiRenderer::new(2);
    let stats = StatsSnapshot {
        fps: 30.0,
        target_fps: 30.0,
        gpu_pct: None,
        vram_used_mb: None,
        vram_total_mb: None,
        cpu_pct: None,
        backend: "CPU",
    };

    terminal
        .draw(|f| draw_clean(&out, &stats, f.area(), &mut renderer, f.buffer_mut()))
        .unwrap();

    let buf = terminal.backend().buffer();

    // Baris 0 = stats bar, harus memuat "FPS".
    let mut row0 = String::new();
    for x in 0..40 {
        row0.push_str(buf.cell((x, 0)).unwrap().symbol());
    }
    assert!(row0.contains("FPS"), "stats bar tidak tampil: {row0:?}");

    // Area ASCII (baris >=1) harus memuat sel dengan fg kebiruan.
    let mut found_blue = false;
    for y in 1..12 {
        for x in 0..40 {
            if let Some(cell) = buf.cell((x, y)) {
                if let Color::Rgb(_, _, b) = cell.fg {
                    if b > 120 {
                        found_blue = true;
                    }
                }
            }
        }
    }
    assert!(found_blue, "tidak ada sel ASCII berwarna biru di area render");
}

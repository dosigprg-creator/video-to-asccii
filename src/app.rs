//! State machine aplikasi + event loop utama.
//!
//! Mode: Startup (pilih video) -> Playing (clean mode) <-> Menu (paused).
//! Decode berjalan di thread terpisah (lihat `pipeline`), render selalu memakai
//! frame terbaru (drop-stale). Loop video ditangani ffmpeg (`-stream_loop -1`).

use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::{DefaultTerminal, Terminal};

use crate::config::{charset_chars, RESOLUTIONS};
use crate::engine::{compute_dims, AsciiEngine, Engine, EngineConfig};
use crate::hardware::{HardwareInfo, StatsProvider};
use crate::pipeline::DecodePipeline;
use crate::render::RatatuiRenderer;
use crate::source::{video::VideoSource, Frame, FrameSource, SourceInfo};
use crate::ui::clean::{ascii_area, draw_clean};
use crate::ui::menu::{draw_menu_sidebar, MenuAction, MenuState, MENU_WIDTH};
use crate::ui::startup::{draw_startup, StartupAction, StartupState};
use crate::ui::stats_bar::{FpsMeter, StatsSnapshot};

const CHANNEL_CAPACITY: usize = 3;
const CHAR_ASPECT: f32 = 0.5;
const QUANT_BITS: u8 = 2;

pub struct AppOptions {
    pub path: String,
    /// Langsung mainkan (lewati layar startup) bila true.
    pub autostart: bool,
    /// Pakai engine GPU (eksperimen); fallback CPU bila init gagal.
    pub use_gpu: bool,
}

enum Mode {
    Startup,
    Playing,
    Menu,
}

struct Playback {
    pipeline: DecodePipeline,
    info: SourceInfo,
    engine: Engine,
    last_frame: Option<Frame>,
}

struct App {
    mode: Mode,
    startup: StartupState,
    menu: MenuState,
    hw: HardwareInfo,
    stats: StatsProvider,
    renderer: RatatuiRenderer,
    fps: FpsMeter,
    res_idx: usize,
    charset_idx: usize,
    colored: bool,
    use_gpu: bool,
    playback: Option<Playback>,
    should_quit: bool,
}

impl App {
    fn new(default_path: String, use_gpu: bool) -> Self {
        Self {
            mode: Mode::Startup,
            startup: StartupState::new(default_path),
            menu: MenuState::new(),
            hw: HardwareInfo::detect(),
            stats: StatsProvider::spawn(),
            renderer: RatatuiRenderer::new(QUANT_BITS),
            fps: FpsMeter::new(),
            res_idx: 0,
            charset_idx: 0,
            colored: true,
            use_gpu,
            playback: None,
            should_quit: false,
        }
    }

    /// Bangun snapshot stats dari sampler CPU + info hardware statis.
    fn stats_snapshot(&self, fps: f32, target_fps: f32) -> StatsSnapshot {
        let backend = self
            .playback
            .as_ref()
            .map(|p| p.engine.backend_name())
            .unwrap_or("CPU");
        StatsSnapshot {
            fps,
            target_fps,
            gpu_pct: None, // utilisasi GPU butuh API vendor spesifik (di luar scope)
            vram_used_mb: None,
            vram_total_mb: self.hw.vram_total_mb_f32(),
            cpu_pct: Some(self.stats.cpu_pct()),
            backend,
        }
    }

    fn max_cols(&self, term_w: u16) -> usize {
        match RESOLUTIONS[self.res_idx] {
            None => term_w as usize,
            Some(c) => c.min(term_w as usize),
        }
    }

    /// Mulai playback dari pilihan pada startup state. Kegagalan (path kosong/
    /// tak valid) menampilkan pesan dan tetap di layar startup.
    fn start_playback(&mut self) -> Result<()> {
        let path = self.startup.path.trim().to_string();
        if path.is_empty() {
            self.startup.message = Some("Path video kosong.".to_string());
            return Ok(());
        }

        self.res_idx = self.startup.res_idx;
        self.charset_idx = self.startup.charset_idx;
        self.colored = self.startup.colored;

        // Factory memilih webcam ("webcam"/"cam[:Nama]") atau file video.
        let source = match crate::source::open(&path, true, true) {
            Ok(s) => s,
            Err(e) => {
                self.startup.message = Some(format!("Gagal buka sumber: {e}"));
                return Ok(());
            }
        };
        let info = source.info();
        let pipeline = DecodePipeline::spawn(source, CHANNEL_CAPACITY);

        let cfg = EngineConfig {
            charset: charset_chars(self.charset_idx),
            colored: self.colored,
            quant_bits: QUANT_BITS,
            char_aspect: CHAR_ASPECT,
        };
        let engine = Engine::new(cfg, self.use_gpu);

        self.renderer.set_quant_bits(QUANT_BITS);
        self.fps = FpsMeter::new();
        self.playback = Some(Playback {
            pipeline,
            info,
            engine,
            last_frame: None,
        });
        self.startup.message = None;
        self.mode = Mode::Playing;
        Ok(())
    }

    fn stop_playback(&mut self) {
        // Drop menghentikan decode thread + membunuh ffmpeg.
        self.playback = None;
    }

    fn open_menu(&mut self) {
        if let Some(pb) = &self.playback {
            pb.pipeline.set_paused(true);
        }
        self.menu = MenuState::new();
        self.mode = Mode::Menu;
    }

    fn resume(&mut self) {
        if let Some(pb) = &self.playback {
            pb.pipeline.set_paused(false);
        }
        self.mode = Mode::Playing;
    }

    fn cycle_resolution(&mut self) {
        self.res_idx = (self.res_idx + 1) % RESOLUTIONS.len();
    }

    fn cycle_charset(&mut self) {
        self.charset_idx = (self.charset_idx + 1) % crate::config::CHARSETS.len();
        let charset_idx = self.charset_idx;
        let colored = self.colored;
        if let Some(pb) = self.playback.as_mut() {
            pb.engine.set_config(EngineConfig {
                charset: charset_chars(charset_idx),
                colored,
                quant_bits: QUANT_BITS,
                char_aspect: CHAR_ASPECT,
            });
        }
    }

    fn main_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            match self.mode {
                Mode::Startup => self.tick_startup(terminal)?,
                Mode::Playing => self.tick_playing(terminal)?,
                Mode::Menu => self.tick_menu(terminal)?,
            }
        }
        Ok(())
    }

    fn tick_startup(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let startup = &self.startup;
        let hw = &self.hw;
        terminal.draw(|f| draw_startup(startup, hw, f.area(), f.buffer_mut()))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match self.startup.on_key(key) {
                        StartupAction::Submit => self.start_playback()?,
                        StartupAction::Quit => self.should_quit = true,
                        StartupAction::None => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn tick_playing(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        if event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc => {
                            self.should_quit = true;
                            return Ok(());
                        }
                        KeyCode::Char('q') => {
                            self.open_menu();
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }

        let (vw, vh, vfps) = match &self.playback {
            Some(pb) => (pb.info.width, pb.info.height, pb.info.fps),
            None => {
                self.mode = Mode::Startup;
                return Ok(());
            }
        };

        // Tarik frame terbaru (drop-stale) ke last_frame.
        if let Some(pb) = self.playback.as_mut() {
            if let Some(frame) = pb.pipeline.latest(Duration::from_millis(100)) {
                pb.last_frame = Some(frame);
            }
        }

        let size = terminal.size()?;
        let area = ascii_area(Rect::new(0, 0, size.width, size.height));
        let max_w = self.max_cols(area.width);
        let (w, h) = compute_dims(vw, vh, max_w, area.height as usize, CHAR_ASPECT);

        let out = {
            let pb = self.playback.as_mut().unwrap();
            let Some(frame) = pb.last_frame.as_ref() else {
                return Ok(());
            };
            pb.engine.process(frame, w, h)
        };

        self.fps.tick();
        let stats = self.stats_snapshot(self.fps.fps(), vfps);
        let renderer = &mut self.renderer;
        terminal.draw(|f| draw_clean(&out, &stats, f.area(), renderer, f.buffer_mut()))?;
        Ok(())
    }

    fn tick_menu(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let (vw, vh, vfps) = match &self.playback {
            Some(pb) => (pb.info.width, pb.info.height, pb.info.fps),
            None => {
                self.mode = Mode::Startup;
                return Ok(());
            }
        };

        let size = terminal.size()?;
        let area = ascii_area(Rect::new(0, 0, size.width, size.height));
        let max_w = self.max_cols(area.width);
        let (w, h) = compute_dims(vw, vh, max_w, area.height as usize, CHAR_ASPECT);

        let out_opt = {
            let pb = self.playback.as_mut().unwrap();
            pb.last_frame
                .as_ref()
                .map(|frame| pb.engine.process(frame, w, h))
        };

        let stats = self.stats_snapshot(0.0, vfps);
        let renderer = &mut self.renderer;
        let menu = &self.menu;
        terminal.draw(|f| {
            if let Some(out) = &out_opt {
                draw_clean(out, &stats, f.area(), renderer, f.buffer_mut());
            }
            let sidebar = Rect::new(0, 0, MENU_WIDTH.min(size.width), size.height);
            draw_menu_sidebar(menu, sidebar, f.buffer_mut());
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match self.menu.on_key(key) {
                        MenuAction::Resume => self.resume(),
                        MenuAction::OpenVideo => {
                            self.stop_playback();
                            self.mode = Mode::Startup;
                        }
                        MenuAction::CycleResolution => self.cycle_resolution(),
                        MenuAction::CycleCharset => self.cycle_charset(),
                        MenuAction::Quit => self.should_quit = true,
                        MenuAction::None => {}
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn run(opts: AppOptions) -> Result<()> {
    let mut app = App::new(opts.path, opts.use_gpu);
    if opts.autostart {
        app.start_playback()?;
    }

    let mut terminal = ratatui::init();
    let result = app.main_loop(&mut terminal);
    ratatui::restore();
    result
}

/// Mode benchmark headless (tanpa TTY): jalankan pipeline penuh selama `secs`
/// detik, lalu laporkan FPS. Mengukur ceiling CPU (decode+engine+diff); biaya
/// tulis terminal nyata terlihat di stats bar saat aplikasi interaktif.
pub fn run_bench(path: &str, secs: f64, cols: u16, rows: u16) -> Result<()> {
    let cfg = EngineConfig::default();
    let source = VideoSource::open(path, true, false)?; // uncapped
    let info = source.info();
    let pipeline = DecodePipeline::spawn(source, CHANNEL_CAPACITY);

    let mut engine = AsciiEngine::new(cfg);
    let mut renderer = RatatuiRenderer::new(QUANT_BITS);
    let mut terminal = Terminal::new(TestBackend::new(cols, rows))?;

    let ascii = ascii_area(Rect::new(0, 0, cols, rows));
    let (w, h) = compute_dims(
        info.width,
        info.height,
        ascii.width as usize,
        ascii.height as usize,
        CHAR_ASPECT,
    );
    eprintln!(
        "Bench: sumber {}x{}@{:.1}fps -> ASCII {}x{} pada backend {}x{}",
        info.width, info.height, info.fps, w, h, cols, rows
    );

    let mut frames = 0u64;
    let mut process_total = Duration::ZERO;
    let start = Instant::now();
    let deadline = start + Duration::from_secs_f64(secs);

    while Instant::now() < deadline {
        let Some(frame) = pipeline.latest(Duration::from_millis(200)) else {
            continue;
        };
        let t0 = Instant::now();
        let out = engine.process(&frame, w, h);
        process_total += t0.elapsed();

        let stats = StatsSnapshot {
            fps: 0.0,
            target_fps: info.fps,
            gpu_pct: None,
            vram_used_mb: None,
            vram_total_mb: None,
            cpu_pct: None,
            backend: "CPU",
        };
        terminal.draw(|f| draw_clean(&out, &stats, f.area(), &mut renderer, f.buffer_mut()))?;
        frames += 1;
    }

    let elapsed = start.elapsed().as_secs_f64();
    let fps = frames as f64 / elapsed;
    let avg_proc = if frames > 0 {
        process_total.as_secs_f64() * 1000.0 / frames as f64
    } else {
        0.0
    };
    drop(pipeline);
    eprintln!("Hasil: {frames} frame / {elapsed:.2}s = {fps:.1} FPS | avg process() = {avg_proc:.2} ms");
    Ok(())
}

/// Head-to-head compute murni: CPU vs GPU pada frame & ukuran yang sama, tanpa
/// beban decode/terminal. Mengisolasi biaya engine (termasuk upload+readback GPU).
pub fn run_compare(path: &str, cols: u16, rows: u16, iters: usize) -> Result<()> {
    let cfg = EngineConfig::default();

    // Ambil satu frame nyata sebagai input konstan.
    let mut source = crate::source::open(path, true, false)?;
    let info = source.info();
    let frame = source
        .next_frame()
        .ok_or_else(|| anyhow::anyhow!("gagal membaca frame dari sumber"))?;
    drop(source);

    let ascii = ascii_area(Rect::new(0, 0, cols, rows));
    let (w, h) = compute_dims(
        info.width,
        info.height,
        ascii.width as usize,
        ascii.height as usize,
        CHAR_ASPECT,
    );
    eprintln!(
        "Adu bukti: sumber {}x{} -> ASCII {}x{} ({} sel), {} iterasi\n",
        info.width,
        info.height,
        w,
        h,
        w * h,
        iters
    );

    let bench = |engine: &mut Engine| -> f64 {
        // warm-up
        for _ in 0..3 {
            let _ = engine.process(&frame, w, h);
        }
        let t0 = Instant::now();
        for _ in 0..iters {
            let out = engine.process(&frame, w, h);
            std::hint::black_box(&out);
        }
        t0.elapsed().as_secs_f64() * 1000.0 / iters as f64
    };

    let mut cpu = Engine::new(cfg.clone(), false);
    let cpu_ms = bench(&mut cpu);
    eprintln!("CPU : {cpu_ms:.2} ms/frame  (~{:.0} fps compute-only)", 1000.0 / cpu_ms);

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Engine::new(cfg, true)
    })) {
        Ok(mut gpu @ Engine::Gpu(_)) => {
            let gpu_ms = bench(&mut gpu);
            eprintln!("GPU : {gpu_ms:.2} ms/frame  (~{:.0} fps compute-only)", 1000.0 / gpu_ms);
            let ratio = cpu_ms / gpu_ms;
            if ratio >= 1.0 {
                eprintln!("\nGPU {ratio:.2}x lebih cepat dari CPU.");
            } else {
                eprintln!("\nGPU {:.2}x lebih LAMBAT dari CPU (overhead upload+readback).", 1.0 / ratio);
            }
        }
        _ => eprintln!("GPU tidak tersedia; hanya hasil CPU."),
    }
    Ok(())
}

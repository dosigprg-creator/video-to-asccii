//! Lapisan ENGINE: mengubah `Frame` (RGB) -> `EngineOutput` (ASCII agnostik).
//!
//! Engine TIDAK tahu apa-apa soal terminal/UI. Ini inti yang akan sering
//! dioptimasi. M2: warna + run-length + paralelisme per-baris (rayon).

pub mod charmap;
pub mod gpu;
pub mod quantize;
pub mod runlength;

use crate::source::Frame;
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use rayon::prelude::*;

pub use quantize::dequantize;

/// Penanda "tanpa warna" pada `Run.color_key`.
pub const NO_COLOR: u32 = u32::MAX;

/// Satu run: deretan karakter berdekatan dengan warna terkuantisasi sama.
pub struct Run {
    pub text: String,
    pub color_key: u32,
}

/// Hasil engine, siap dikonsumsi renderer apa pun.
pub struct EngineOutput {
    pub width: usize,
    pub height: usize,
    /// Per baris: daftar run.
    pub rows: Vec<Vec<Run>>,
    /// Hash tiap baris untuk row cache renderer.
    pub row_hashes: Vec<u64>,
}

/// Konfigurasi engine.
#[derive(Clone)]
pub struct EngineConfig {
    pub charset: Vec<char>,
    pub colored: bool,
    /// Jumlah bit per channel yang dibuang saat kuantisasi (default 2 -> 6-bit).
    pub quant_bits: u8,
    /// Rasio lebar/tinggi sel terminal (sel ~2x lebih tinggi dari lebar -> 0.5).
    pub char_aspect: f32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            charset: " .:-=+&#%@".chars().collect(),
            colored: true,
            quant_bits: 2,
            char_aspect: 0.5,
        }
    }
}

pub struct AsciiEngine {
    cfg: EngineConfig,
    lut: [u8; 256],
    resizer: Resizer,
    opts: ResizeOptions,
    /// Buffer RGB hasil resize, dipakai ulang antar frame.
    dst: Option<Image<'static>>,
    dst_dims: (usize, usize),
}

impl AsciiEngine {
    pub fn new(cfg: EngineConfig) -> Self {
        let lut = charmap::build_lut(cfg.charset.len());
        // Box filter ~ setara INTER_AREA untuk downscale: cepat & rata-rata area.
        let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Box));
        Self {
            cfg,
            lut,
            resizer: Resizer::new(),
            opts,
            dst: None,
            dst_dims: (0, 0),
        }
    }

    pub fn config(&self) -> &EngineConfig {
        &self.cfg
    }

    /// Ubah konfigurasi (mis. toggle warna dari menu). Membangun ulang LUT.
    pub fn set_config(&mut self, cfg: EngineConfig) {
        self.lut = charmap::build_lut(cfg.charset.len());
        self.cfg = cfg;
    }

    /// Resize `frame` ke `out_w x out_h` RGB24 ke buffer internal `self.dst`.
    fn resize_into(&mut self, frame: &Frame, out_w: usize, out_h: usize) {
        if self.dst.is_none() || self.dst_dims != (out_w, out_h) {
            self.dst = Some(Image::new(out_w as u32, out_h as u32, PixelType::U8x3));
            self.dst_dims = (out_w, out_h);
        }
        let dst = self.dst.as_mut().unwrap();

        let src = ImageRef::new(
            frame.width as u32,
            frame.height as u32,
            &frame.data,
            PixelType::U8x3,
        )
        .expect("dimensi frame tidak valid untuk ImageRef");

        self.resizer
            .resize(&src, dst, Some(&self.opts))
            .expect("resize gagal");
    }

    /// Pipeline utama (paralel via rayon): `Frame` -> `EngineOutput`.
    pub fn process(&mut self, frame: &Frame, out_w: usize, out_h: usize) -> EngineOutput {
        self.process_impl(frame, out_w, out_h, true)
    }

    /// Varian single-thread — hanya untuk benchmark pembanding.
    pub fn process_sequential(
        &mut self,
        frame: &Frame,
        out_w: usize,
        out_h: usize,
    ) -> EngineOutput {
        self.process_impl(frame, out_w, out_h, false)
    }

    fn process_impl(
        &mut self,
        frame: &Frame,
        out_w: usize,
        out_h: usize,
        parallel: bool,
    ) -> EngineOutput {
        self.resize_into(frame, out_w, out_h);
        let resized = self.dst.as_ref().unwrap().buffer();
        let lut = &self.lut;
        let charset = &self.cfg.charset;
        let colored = self.cfg.colored;
        let quant_bits = self.cfg.quant_bits;

        let built: Vec<(Vec<Run>, u64)> = if parallel {
            (0..out_h)
                .into_par_iter()
                .map(|y| runlength::build_row(y, out_w, resized, lut, charset, colored, quant_bits))
                .collect()
        } else {
            (0..out_h)
                .map(|y| runlength::build_row(y, out_w, resized, lut, charset, colored, quant_bits))
                .collect()
        };

        let mut rows = Vec::with_capacity(out_h);
        let mut row_hashes = Vec::with_capacity(out_h);
        for (runs, h) in built {
            rows.push(runs);
            row_hashes.push(h);
        }

        EngineOutput {
            width: out_w,
            height: out_h,
            rows,
            row_hashes,
        }
    }

    /// Hanya resize (untuk mengukur fase ini secara terpisah di benchmark).
    pub fn resize_frame(&mut self, frame: &Frame, out_w: usize, out_h: usize) -> &[u8] {
        self.resize_into(frame, out_w, out_h);
        self.dst.as_ref().unwrap().buffer()
    }
}

/// Pemilih backend engine: CPU (default) atau GPU (eksperimen, flag `--gpu`).
/// Keduanya menghasilkan `EngineOutput` identik sehingga renderer/app tak berubah.
pub enum Engine {
    Cpu(Box<AsciiEngine>),
    Gpu(Box<gpu::GpuEngine>),
}

impl Engine {
    /// Bangun engine. Bila `use_gpu` true tapi init GPU gagal, otomatis
    /// fallback ke CPU (dengan pesan ke stderr).
    pub fn new(cfg: EngineConfig, use_gpu: bool) -> Self {
        if use_gpu {
            match gpu::GpuEngine::new(cfg.clone()) {
                Ok(g) => {
                    eprintln!("Engine GPU aktif: {}", g.adapter_name());
                    return Engine::Gpu(Box::new(g));
                }
                Err(e) => eprintln!("Init GPU gagal ({e}); fallback ke CPU."),
            }
        }
        Engine::Cpu(Box::new(AsciiEngine::new(cfg)))
    }

    pub fn process(&mut self, frame: &Frame, out_w: usize, out_h: usize) -> EngineOutput {
        match self {
            Engine::Cpu(e) => e.process(frame, out_w, out_h),
            Engine::Gpu(g) => g.process(frame, out_w, out_h),
        }
    }

    pub fn set_config(&mut self, cfg: EngineConfig) {
        match self {
            Engine::Cpu(e) => e.set_config(cfg),
            Engine::Gpu(g) => g.set_config(cfg),
        }
    }

    pub fn backend_name(&self) -> &'static str {
        match self {
            Engine::Cpu(_) => "CPU",
            Engine::Gpu(_) => "GPU",
        }
    }
}

/// Hitung dimensi output ASCII yang mempertahankan aspect ratio video,
/// dikoreksi `char_aspect`, dan muat dalam `max_w x max_h`.
///
/// Sel terminal lebih tinggi dari lebar, jadi `out_w/out_h = aspect / char_aspect`.
pub fn compute_dims(
    src_w: usize,
    src_h: usize,
    max_w: usize,
    max_h: usize,
    char_aspect: f32,
) -> (usize, usize) {
    if src_h == 0 || src_w == 0 {
        return (max_w.max(1), max_h.max(1));
    }
    let aspect = src_w as f32 / src_h as f32;
    let target_ratio = aspect / char_aspect; // out_w / out_h

    let mut w = max_w;
    let mut h = (w as f32 / target_ratio).round() as usize;
    if h > max_h {
        h = max_h;
        w = (h as f32 * target_ratio).round() as usize;
    }
    (w.clamp(1, max_w), h.clamp(1, max_h))
}

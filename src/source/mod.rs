//! Lapisan SOURCE: menghasilkan frame RGB mentah, tak tahu apa-apa soal ASCII.
//!
//! Trait `FrameSource` adalah batas abstraksi: mengganti sumber (video -> webcam
//! -> GIF) cukup mengganti satu implementasi, tanpa menyentuh engine/renderer.

pub mod video;
pub mod webcam;

use anyhow::Result;

/// Satu frame gambar mentah dalam format RGB24 (row-major).
/// `data.len()` selalu sama dengan `width * height * 3`.
pub struct Frame {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

/// Metadata sumber: dimensi asli + frame rate.
#[derive(Clone, Copy, Debug)]
pub struct SourceInfo {
    pub width: usize,
    pub height: usize,
    pub fps: f32,
}

/// Sumber frame yang bisa dipindah antar-thread (dijalankan di decode thread).
pub trait FrameSource: Send {
    /// Ambil frame berikutnya. `None` = sumber habis.
    fn next_frame(&mut self) -> Option<Frame>;
    /// Metadata sumber (width, height, fps).
    fn info(&self) -> SourceInfo;
}

/// Agar `Box<dyn FrameSource>` sendiri bisa dipakai sebagai `FrameSource`
/// (mis. dikirim ke `DecodePipeline::spawn`). Ini yang membuat factory `open`
/// bisa mengembalikan satu tipe seragam untuk video maupun webcam.
impl FrameSource for Box<dyn FrameSource> {
    fn next_frame(&mut self) -> Option<Frame> {
        (**self).next_frame()
    }
    fn info(&self) -> SourceInfo {
        (**self).info()
    }
}

/// Factory sumber: pilih webcam atau file video dari sebuah `spec`.
///
/// - `"webcam"` / `"cam"` -> kamera default.
/// - `"webcam:Nama Device"` / `"cam:Nama Device"` -> kamera bernama.
/// - selain itu -> path file video (`looping`/`realtime` diteruskan ke ffmpeg).
pub fn open(spec: &str, looping: bool, realtime: bool) -> Result<Box<dyn FrameSource>> {
    if let Some(device) = webcam::parse_spec(spec) {
        let dev = if device.is_empty() {
            webcam::default_device()?
        } else {
            device
        };
        Ok(Box::new(webcam::WebcamSource::open(
            &dev,
            webcam::DEFAULT_WIDTH,
            webcam::DEFAULT_HEIGHT,
            webcam::DEFAULT_FPS,
        )?))
    } else {
        Ok(Box::new(video::VideoSource::open(spec, looping, realtime)?))
    }
}

//! Lapisan RENDERER: mengonsumsi `EngineOutput` -> menggambar ke layar.
//!
//! Catatan arsitektur: trait `Renderer` memakai `Buffer`/`Rect` ratatui sebagai
//! target gambar kanonik untuk renderer berbasis-sel. Mengganti ke renderer sel
//! lain (mis. ANSI stdout langsung) = 1 impl baru; engine tak tersentuh. Backend
//! non-sel (SDL) berada di luar cakupan (lihat NON-GOALS PRD bagian 12).

pub mod ratatui_renderer;
pub mod style_cache;

use crate::engine::EngineOutput;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub use ratatui_renderer::RatatuiRenderer;

pub trait Renderer {
    /// Gambar `output` ke region `area` pada buffer sel `buf`.
    fn draw(&mut self, output: &EngineOutput, area: Rect, buf: &mut Buffer);
}

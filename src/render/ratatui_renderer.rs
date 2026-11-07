//! Implementasi `Renderer` untuk ratatui: tulis tiap `Run` langsung ke sel
//! buffer (tanpa membangun string ANSI). Ratatui melakukan cell-diffing sendiri,
//! jadi hanya sel yang berubah yang benar-benar ditulis ke terminal.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::style_cache::StyleCache;
use super::Renderer;
use crate::engine::EngineOutput;

pub struct RatatuiRenderer {
    styles: StyleCache,
}

impl RatatuiRenderer {
    pub fn new(quant_bits: u8) -> Self {
        Self {
            styles: StyleCache::new(quant_bits),
        }
    }

    pub fn set_quant_bits(&mut self, quant_bits: u8) {
        self.styles.set_quant_bits(quant_bits);
    }
}

impl Renderer for RatatuiRenderer {
    fn draw(&mut self, output: &EngineOutput, area: Rect, buf: &mut Buffer) {
        let max_rows = output.height.min(area.height as usize);
        let x_end = area.x.saturating_add(area.width);

        for y in 0..max_rows {
            let cy = area.y + y as u16;
            let mut cx = area.x;
            for run in &output.rows[y] {
                if cx >= x_end {
                    break;
                }
                let style = self.styles.style(run.color_key);
                for ch in run.text.chars() {
                    if cx >= x_end {
                        break;
                    }
                    if let Some(cell) = buf.cell_mut((cx, cy)) {
                        cell.set_char(ch);
                        cell.set_style(style);
                    }
                    cx += 1;
                }
            }
        }
    }
}

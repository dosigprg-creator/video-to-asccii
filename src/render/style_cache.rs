//! Cache `color_key -> Style` agar objek warna tidak dibuat ulang tiap sel.

use ahash::AHashMap;
use ratatui::style::{Color, Style};

use crate::engine::{dequantize, NO_COLOR};

pub struct StyleCache {
    map: AHashMap<u32, Style>,
    quant_bits: u8,
}

impl StyleCache {
    pub fn new(quant_bits: u8) -> Self {
        Self {
            map: AHashMap::new(),
            quant_bits,
        }
    }

    /// Ganti quant_bits (mis. dari menu) — invalidasi cache.
    pub fn set_quant_bits(&mut self, quant_bits: u8) {
        if quant_bits != self.quant_bits {
            self.quant_bits = quant_bits;
            self.map.clear();
        }
    }

    /// Ambil (atau bangun lalu cache) `Style` untuk sebuah color key.
    pub fn style(&mut self, key: u32) -> Style {
        if key == NO_COLOR {
            return Style::default();
        }
        let quant_bits = self.quant_bits;
        *self.map.entry(key).or_insert_with(|| {
            let (r, g, b) = dequantize(key, quant_bits);
            Style::default().fg(Color::Rgb(r, g, b))
        })
    }
}

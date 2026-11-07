//! Run-length encoding per baris + hash baris.
//!
//! Fungsi ini dipanggil dari dalam `rayon::par_iter` (satu baris = satu task),
//! sehingga harus bebas state bersama yang mutable.

use std::hash::Hasher;

use super::quantize::quantize;
use super::{Run, NO_COLOR};

/// Bangun daftar `Run` untuk baris `y` sekaligus hash baris tersebut.
///
/// Pixel berturut-turut dengan `color_key` sama digabung jadi satu run. Saat
/// `colored=false`, seluruh baris memakai `NO_COLOR` sehingga jadi satu run.
pub fn build_row(
    y: usize,
    out_w: usize,
    resized: &[u8],
    lut: &[u8; 256],
    charset: &[char],
    colored: bool,
    quant_bits: u8,
) -> (Vec<Run>, u64) {
    let base = y * out_w * 3;
    let mut runs: Vec<Run> = Vec::with_capacity(8);
    let mut cur_key: u32 = 0;
    let mut cur_text = String::new();

    for x in 0..out_w {
        let i = base + x * 3;
        let r = resized[i];
        let g = resized[i + 1];
        let b = resized[i + 2];

        let gray = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
        let ch = charset[lut[gray as usize & 0xff] as usize];
        let key = if colored {
            quantize(r, g, b, quant_bits)
        } else {
            NO_COLOR
        };

        if x == 0 {
            cur_key = key;
            cur_text.push(ch);
        } else if key == cur_key {
            cur_text.push(ch);
        } else {
            runs.push(Run {
                text: std::mem::take(&mut cur_text),
                color_key: cur_key,
            });
            cur_key = key;
            cur_text.push(ch);
        }
    }
    if !cur_text.is_empty() {
        runs.push(Run {
            text: cur_text,
            color_key: cur_key,
        });
    }

    let hash = hash_runs(&runs);
    (runs, hash)
}

/// Bangun `Run` untuk satu baris dari hasil GPU: tiap sel dikemas sebagai
/// `(char_index << 24) | (color_key & 0x00ff_ffff)`. Run-length + hash identik
/// dengan jalur CPU sehingga renderer/EngineOutput tidak berubah.
pub fn build_row_from_packed(
    y: usize,
    out_w: usize,
    packed: &[u32],
    charset: &[char],
    colored: bool,
) -> (Vec<Run>, u64) {
    let base = y * out_w;
    let n = charset.len().max(1);
    let mut runs: Vec<Run> = Vec::with_capacity(8);
    let mut cur_key: u32 = 0;
    let mut cur_text = String::new();

    for x in 0..out_w {
        let v = packed[base + x];
        let idx = ((v >> 24) & 0xff) as usize;
        let ch = charset[idx.min(n - 1)];
        let key = if colored { v & 0x00ff_ffff } else { NO_COLOR };

        if x == 0 {
            cur_key = key;
            cur_text.push(ch);
        } else if key == cur_key {
            cur_text.push(ch);
        } else {
            runs.push(Run {
                text: std::mem::take(&mut cur_text),
                color_key: cur_key,
            });
            cur_key = key;
            cur_text.push(ch);
        }
    }
    if !cur_text.is_empty() {
        runs.push(Run {
            text: cur_text,
            color_key: cur_key,
        });
    }

    let hash = hash_runs(&runs);
    (runs, hash)
}

/// Hash isi baris (warna + teks) untuk row cache renderer.
fn hash_runs(runs: &[Run]) -> u64 {
    let mut hasher = ahash::AHasher::default();
    for run in runs {
        hasher.write_u32(run.color_key);
        hasher.write(run.text.as_bytes());
        hasher.write_u8(0xff); // pemisah antar-run
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn charset() -> Vec<char> {
        " .:-=+&#%@".chars().collect()
    }

    #[test]
    fn baris_warna_seragam_jadi_satu_run() {
        // 4 pixel merah identik -> 1 run.
        let resized = vec![200, 0, 0, 200, 0, 0, 200, 0, 0, 200, 0, 0];
        let lut = crate::engine::charmap::build_lut(10);
        let cs = charset();
        let (runs, _) = build_row(0, 4, &resized, &lut, &cs, true, 2);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text.chars().count(), 4);
    }

    #[test]
    fn perubahan_warna_memecah_run() {
        // merah, merah, biru, biru -> 2 run.
        let resized = vec![200, 0, 0, 200, 0, 0, 0, 0, 200, 0, 0, 200];
        let lut = crate::engine::charmap::build_lut(10);
        let cs = charset();
        let (runs, _) = build_row(0, 4, &resized, &lut, &cs, true, 2);
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn grayscale_seluruh_baris_satu_run() {
        // colored=false -> 1 run apa pun isinya.
        let resized = vec![10, 10, 10, 250, 250, 250, 128, 128, 128];
        let lut = crate::engine::charmap::build_lut(10);
        let cs = charset();
        let (runs, _) = build_row(0, 3, &resized, &lut, &cs, false, 2);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].color_key, NO_COLOR);
        assert_eq!(runs[0].text.chars().count(), 3);
    }
}

//! LUT (look-up table) intensitas -> indeks karakter.
//!
//! Dihitung sekali saat engine dibuat, lalu dipakai per-pixel tanpa cabang.

/// Bangun tabel 256-entri: intensitas 0..=255 dipetakan ke indeks 0..len-1.
///
/// Pixel gelap (intensitas kecil) -> karakter awal charset (mis. ' '),
/// pixel terang -> karakter akhir (mis. '@'). Ini mengasumsikan charset diurut
/// dari "ringan" ke "padat".
pub fn build_lut(charset_len: usize) -> [u8; 256] {
    let mut lut = [0u8; 256];
    let n = charset_len.max(1);
    for (i, slot) in lut.iter_mut().enumerate() {
        let idx = (i * n) / 256; // menghasilkan 0..=n-1
        *slot = idx.min(n - 1) as u8;
    }
    lut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lut_batas_benar() {
        let lut = build_lut(10);
        assert_eq!(lut[0], 0, "pixel paling gelap -> indeks 0");
        assert_eq!(lut[255], 9, "pixel paling terang -> indeks terakhir");
    }

    #[test]
    fn lut_monoton_naik() {
        let lut = build_lut(10);
        for w in lut.windows(2) {
            assert!(w[1] >= w[0], "LUT harus monoton naik");
        }
    }
}

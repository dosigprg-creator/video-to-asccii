//! Kuantisasi warna RGB -> color key terkuantisasi, dan kebalikannya.
//!
//! Membuang `quant_bits` bit terendah tiap channel menaikkan rasio kompресi
//! run-length dan cache hit (pixel yang "mirip" jadi identik).

/// Kuantisasi (r,g,b) -> key. Default `quant_bits = 2` -> 6 bit/channel (64 level).
///
/// Layout key: `[rq | gq | bq]`, masing-masing selebar `8 - quant_bits` bit.
#[inline]
pub fn quantize(r: u8, g: u8, b: u8, quant_bits: u8) -> u32 {
    let q = quant_bits as u32;
    let bits = 8 - q;
    let rq = (r as u32) >> q;
    let gq = (g as u32) >> q;
    let bq = (b as u32) >> q;
    (rq << (2 * bits)) | (gq << bits) | bq
}

/// Kembalikan key terkuantisasi -> (r,g,b) 8-bit (perkiraan).
///
/// Ekspansi memakai skala `v * 255 / max` agar rentang penuh 0..=255 tercapai
/// tanpa kasus tepi pergeseran bit.
#[inline]
pub fn dequantize(key: u32, quant_bits: u8) -> (u8, u8, u8) {
    let q = quant_bits as u32;
    let bits = 8 - q;
    let mask = (1u32 << bits) - 1;
    let rq = (key >> (2 * bits)) & mask;
    let gq = (key >> bits) & mask;
    let bq = key & mask;
    (expand(rq, mask), expand(gq, mask), expand(bq, mask))
}

#[inline]
fn expand(v: u32, mask: u32) -> u8 {
    // mask==0 (quant_bits==8) -> None -> 0; selain itu pembulatan terdekat.
    (v * 255 + mask / 2).checked_div(mask).unwrap_or(0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_mendekati_asli() {
        // Dengan quant_bits=2, error maksimum per channel harus kecil.
        for &(r, g, b) in &[(0, 0, 0), (255, 255, 255), (200, 100, 50), (17, 33, 240)] {
            let key = quantize(r, g, b, 2);
            let (dr, dg, db) = dequantize(key, 2);
            assert!((dr as i32 - r as i32).abs() <= 6, "r: {r} -> {dr}");
            assert!((dg as i32 - g as i32).abs() <= 6, "g: {g} -> {dg}");
            assert!((db as i32 - b as i32).abs() <= 6, "b: {b} -> {db}");
        }
    }

    #[test]
    fn quant_bits_nol_adalah_identitas() {
        let key = quantize(123, 45, 200, 0);
        assert_eq!(dequantize(key, 0), (123, 45, 200));
    }
}

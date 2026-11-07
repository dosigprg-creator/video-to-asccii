//! Konfigurasi runtime yang dipakai UI (startup & menu): pilihan resolusi dan
//! preset karakter set.

/// Pilihan resolusi kolom. `None` = Auto (ikuti lebar terminal).
/// 400-800 hanya efektif bila terminal benar-benar selebar itu (font kecil/zoom-out).
pub const RESOLUTIONS: &[Option<usize>] = &[
    None,
    Some(40),
    Some(80),
    Some(120),
    Some(160),
    Some(200),
    Some(400),
    Some(600),
    Some(800),
];

pub fn resolution_label(r: Option<usize>) -> String {
    match r {
        None => "Auto".to_string(),
        Some(c) => format!("{c} kolom"),
    }
}

/// Preset karakter (diurut dari "ringan" ke "padat").
pub struct CharsetPreset {
    pub name: &'static str,
    pub chars: &'static str,
}

pub const CHARSETS: &[CharsetPreset] = &[
    CharsetPreset {
        name: "Standar",
        chars: " .:-=+&#%@",
    },
    CharsetPreset {
        name: "Blok",
        chars: " .:-=+*oO8@#",
    },
    CharsetPreset {
        name: "Detil",
        chars: " .,:;irsXA253hMHGS#9B&@",
    },
    CharsetPreset {
        // Ramp klasik 70 level (Paul Bourke), diurut terang -> padat.
        name: "Ultra",
        chars: r#" .'`^",:;Il!i><~+_-?][}{1)(|\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$"#,
    },
];

pub fn charset_chars(idx: usize) -> Vec<char> {
    CHARSETS[idx.min(CHARSETS.len() - 1)].chars.chars().collect()
}

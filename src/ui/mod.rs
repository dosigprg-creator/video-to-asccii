//! Lapisan UI: layout + widget. Clean mode di M3; startup & menu di M6.

pub mod clean;
pub mod menu;
pub mod startup;
pub mod stats_bar;

use ratatui::layout::Rect;

/// Pusatkan kotak `w x h` di dalam `area` (untuk padding ASCII saat rasio
/// tidak mengisi penuh area).
pub fn center_rect(w: u16, h: u16, area: Rect) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

//! Clean mode: baris atas = stats bar, sisanya = ASCII fullscreen (dipusatkan).

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};

use super::center_rect;
use super::stats_bar::{render_stats_bar, StatsSnapshot};
use crate::engine::EngineOutput;
use crate::render::Renderer;

/// Area yang tersisa untuk ASCII setelah stats bar (1 baris) diambil.
/// Dipakai app untuk menghitung dimensi output engine sebelum render.
pub fn ascii_area(full: Rect) -> Rect {
    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(full);
    chunks[1]
}

/// Susun clean mode ke dalam buffer.
pub fn draw_clean(
    output: &EngineOutput,
    stats: &StatsSnapshot,
    full: Rect,
    renderer: &mut dyn Renderer,
    buf: &mut Buffer,
) {
    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(full);
    render_stats_bar(stats, chunks[0], buf);
    let area = center_rect(output.width as u16, output.height as u16, chunks[1]);
    renderer.draw(output, area, buf);
}

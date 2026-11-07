//! Mode menu: sidebar dari kiri. Video pause saat menu terbuka.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, StatefulWidget, Widget};

/// Lebar sidebar menu.
pub const MENU_WIDTH: u16 = 22;

pub const MENU_ITEMS: &[&str] = &["Resume", "Buka Video", "Resolusi", "Karakter Set", "Keluar"];

pub enum MenuAction {
    None,
    Resume,
    OpenVideo,
    CycleResolution,
    CycleCharset,
    Quit,
}

pub struct MenuState {
    pub selected: usize,
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuState {
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    fn move_sel(&mut self, delta: i32) {
        let n = MENU_ITEMS.len() as i32;
        self.selected = ((self.selected as i32 + delta).rem_euclid(n)) as usize;
    }

    /// Proses tombol. `q`/Esc menutup menu (Resume). Enter memilih item.
    pub fn on_key(&mut self, key: KeyEvent) -> MenuAction {
        match key.code {
            KeyCode::Char('q') => MenuAction::Resume,
            KeyCode::Esc => MenuAction::Quit,
            KeyCode::Up => {
                self.move_sel(-1);
                MenuAction::None
            }
            KeyCode::Down => {
                self.move_sel(1);
                MenuAction::None
            }
            KeyCode::Enter => self.activate(),
            _ => MenuAction::None,
        }
    }

    fn activate(&self) -> MenuAction {
        match self.selected {
            0 => MenuAction::Resume,
            1 => MenuAction::OpenVideo,
            2 => MenuAction::CycleResolution,
            3 => MenuAction::CycleCharset,
            4 => MenuAction::Quit,
            _ => MenuAction::None,
        }
    }
}

/// Gambar sidebar menu di `area` (biasanya kolom kiri, tinggi penuh).
pub fn draw_menu_sidebar(state: &MenuState, area: Rect, buf: &mut Buffer) {
    // Bersihkan area sidebar agar tidak tembus ke ASCII di bawahnya.
    Clear.render(area, buf);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" MENU ")
        .style(Style::default().bg(Color::Black).fg(Color::White));

    let items: Vec<ListItem> = MENU_ITEMS
        .iter()
        .map(|s| ListItem::new(Line::from(Span::raw(*s))))
        .collect();

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let mut ls = ListState::default();
    ls.select(Some(state.selected));
    StatefulWidget::render(list, area, buf, &mut ls);
}

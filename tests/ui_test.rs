//! Verifikasi headless state machine UI (startup & menu) + fungsi draw.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use video_to_ascii::config::RESOLUTIONS;
use video_to_ascii::hardware::HardwareInfo;
use video_to_ascii::ui::menu::{MenuAction, MenuState};
use video_to_ascii::ui::startup::{
    draw_startup, StartupAction, StartupField, StartupState,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn buffer_text(width: u16, height: u16, draw: impl FnOnce(&mut ratatui::Frame)) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(draw).unwrap();
    let buf = terminal.backend().buffer();
    let mut s = String::new();
    for y in 0..height {
        for x in 0..width {
            s.push_str(buf.cell((x, y)).unwrap().symbol());
        }
        s.push('\n');
    }
    s
}

#[test]
fn startup_menampilkan_judul_dan_tombol() {
    let state = StartupState::new("video.mp4".to_string());
    let hw = HardwareInfo::detect();
    let text = buffer_text(70, 20, |f| draw_startup(&state, &hw, f.area(), f.buffer_mut()));
    assert!(text.contains("VIDEO TO ASCII"), "judul tidak tampil:\n{text}");
    assert!(text.contains("MASUK"), "tombol MASUK tidak tampil:\n{text}");
    assert!(text.contains("video.mp4"), "path tidak tampil:\n{text}");
}

#[test]
fn startup_ketik_path_lalu_submit() {
    let mut state = StartupState::new(String::new());
    // fokus default = Path
    for c in "a.mp4".chars() {
        assert!(matches!(state.on_key(key(KeyCode::Char(c))), StartupAction::None));
    }
    assert_eq!(state.path, "a.mp4");
    assert!(matches!(state.on_key(key(KeyCode::Enter)), StartupAction::Submit));
}

#[test]
fn startup_backspace_menghapus_path() {
    let mut state = StartupState::new("abc".to_string());
    state.on_key(key(KeyCode::Backspace));
    assert_eq!(state.path, "ab");
}

#[test]
fn startup_navigasi_field_dan_cycle_resolusi() {
    let mut state = StartupState::new("x".to_string());
    assert_eq!(state.field, StartupField::Path);
    state.on_key(key(KeyCode::Tab)); // -> Resolution
    assert_eq!(state.field, StartupField::Resolution);
    assert_eq!(state.res_idx, 0);
    state.on_key(key(KeyCode::Right)); // cycle
    assert_eq!(state.res_idx, 1);
    state.on_key(key(KeyCode::Left));
    assert_eq!(state.res_idx, 0);
    // wrap ke belakang
    state.on_key(key(KeyCode::Left));
    assert_eq!(state.res_idx, RESOLUTIONS.len() - 1);
}

#[test]
fn startup_esc_keluar() {
    let mut state = StartupState::new("x".to_string());
    assert!(matches!(state.on_key(key(KeyCode::Esc)), StartupAction::Quit));
}

#[test]
fn menu_navigasi_dan_aktivasi() {
    let mut menu = MenuState::new();
    assert_eq!(menu.selected, 0);
    // Enter di item 0 = Resume
    assert!(matches!(menu.on_key(key(KeyCode::Enter)), MenuAction::Resume));
    // Down ke Keluar (index 4) lalu Enter
    for _ in 0..4 {
        menu.on_key(key(KeyCode::Down));
    }
    assert_eq!(menu.selected, 4);
    assert!(matches!(menu.on_key(key(KeyCode::Enter)), MenuAction::Quit));
    // 'q' menutup menu (Resume)
    assert!(matches!(menu.on_key(key(KeyCode::Char('q'))), MenuAction::Resume));
}

//! Layar startup: input path, pilih resolusi, toggle warna, info hardware.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use super::center_rect;
use crate::config::{resolution_label, CHARSETS, RESOLUTIONS};
use crate::hardware::HardwareInfo;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StartupField {
    Path,
    Resolution,
    Charset,
    Colored,
    Enter,
}

const FIELD_ORDER: [StartupField; 5] = [
    StartupField::Path,
    StartupField::Resolution,
    StartupField::Charset,
    StartupField::Colored,
    StartupField::Enter,
];

pub enum StartupAction {
    None,
    Submit,
    Quit,
}

pub struct StartupState {
    pub path: String,
    pub res_idx: usize,
    pub charset_idx: usize,
    pub colored: bool,
    pub field: StartupField,
    pub message: Option<String>,
}

impl StartupState {
    pub fn new(default_path: String) -> Self {
        Self {
            path: default_path,
            res_idx: 0,
            charset_idx: 0,
            colored: true,
            field: StartupField::Path,
            message: None,
        }
    }

    fn field_index(&self) -> usize {
        FIELD_ORDER.iter().position(|f| *f == self.field).unwrap_or(0)
    }

    fn focus(&mut self, delta: i32) {
        let n = FIELD_ORDER.len() as i32;
        let idx = (self.field_index() as i32 + delta).rem_euclid(n);
        self.field = FIELD_ORDER[idx as usize];
    }

    fn cycle_res(&mut self, delta: i32) {
        let n = RESOLUTIONS.len() as i32;
        self.res_idx = ((self.res_idx as i32 + delta).rem_euclid(n)) as usize;
    }

    fn cycle_charset(&mut self, delta: i32) {
        let n = CHARSETS.len() as i32;
        self.charset_idx = ((self.charset_idx as i32 + delta).rem_euclid(n)) as usize;
    }

    pub fn on_key(&mut self, key: KeyEvent) -> StartupAction {
        match key.code {
            KeyCode::Esc => return StartupAction::Quit,
            KeyCode::Enter => return StartupAction::Submit,
            KeyCode::Tab | KeyCode::Down => self.focus(1),
            KeyCode::BackTab | KeyCode::Up => self.focus(-1),
            KeyCode::Left => match self.field {
                StartupField::Resolution => self.cycle_res(-1),
                StartupField::Charset => self.cycle_charset(-1),
                StartupField::Colored => self.colored = !self.colored,
                _ => {}
            },
            KeyCode::Right => match self.field {
                StartupField::Resolution => self.cycle_res(1),
                StartupField::Charset => self.cycle_charset(1),
                StartupField::Colored => self.colored = !self.colored,
                _ => {}
            },
            KeyCode::Char(' ') if self.field == StartupField::Colored => {
                self.colored = !self.colored;
            }
            KeyCode::Char(c) if self.field == StartupField::Path => self.path.push(c),
            KeyCode::Backspace if self.field == StartupField::Path => {
                self.path.pop();
            }
            _ => {}
        }
        StartupAction::None
    }
}

fn field_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    }
}

pub fn draw_startup(state: &StartupState, hw: &HardwareInfo, area: Rect, buf: &mut Buffer) {
    let box_area = center_rect(62, 17, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" VIDEO TO ASCII ")
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black));

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!("GPU: {}   |   CPU: {} core", hw.gpu_summary(), hw.cpu_cores),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let path_disp = if state.path.is_empty() {
        "(kosong)".to_string()
    } else {
        state.path.clone()
    };
    lines.push(Line::from(vec![
        Span::raw("Sumber     : "),
        Span::styled(path_disp, field_style(state.field == StartupField::Path)),
    ]));
    lines.push(Line::from(Span::styled(
        "             (path file, atau \"webcam\" untuk kamera)",
        Style::default().fg(Color::DarkGray),
    )));

    lines.push(Line::from(vec![
        Span::raw("Resolusi   : "),
        Span::styled(
            format!("< {} >", resolution_label(RESOLUTIONS[state.res_idx])),
            field_style(state.field == StartupField::Resolution),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::raw("Karakter   : "),
        Span::styled(
            format!("< {} >", CHARSETS[state.charset_idx].name),
            field_style(state.field == StartupField::Charset),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::raw("Warna ASCII: "),
        Span::styled(
            format!("[{}]", if state.colored { "x" } else { " " }),
            field_style(state.field == StartupField::Colored),
        ),
    ]));

    lines.push(Line::from(""));
    if let Some(msg) = &state.message {
        lines.push(Line::from(Span::styled(
            format!("! {msg}"),
            Style::default().fg(Color::Yellow),
        )));
    } else {
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        "        [ MASUK ]        ",
        field_style(state.field == StartupField::Enter),
    )));
    lines.push(Line::from(Span::styled(
        "Tab: pindah  Enter: mulai  Esc: keluar",
        Style::default().fg(Color::DarkGray),
    )));

    Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left)
        .render(box_area, buf);
}

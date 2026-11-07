//! Terminal video-to-ASCII player.
//!
//! Arsitektur tiga lapis yang terpisah total (lihat PRD bagian 3):
//! `source` (frame mentah) -> `engine` (frame -> ASCII) -> `render` (ASCII -> layar).
//! `ui` menyusun layout, `app` mengorkestrasi event loop.

pub mod app;
pub mod config;
pub mod engine;
pub mod hardware;
pub mod pipeline;
pub mod render;
pub mod source;
pub mod ui;

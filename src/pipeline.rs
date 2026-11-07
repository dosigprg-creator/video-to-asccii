//! Pipeline decode di thread terpisah dengan bounded channel + drop-oldest.
//!
//! Decode berjalan di thread sendiri agar tidak memblokir render. Channel
//! dibatasi kecil; saat penuh, frame TERLAMA dibuang supaya render selalu
//! memproses frame terbaru (buang yang basi).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, TrySendError};

use crate::source::{Frame, FrameSource};

pub struct DecodePipeline {
    rx: Receiver<Frame>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl DecodePipeline {
    /// Spawn thread decode. `capacity` = kedalaman buffer frame (2-4 disarankan).
    pub fn spawn<S: FrameSource + 'static>(mut source: S, capacity: usize) -> Self {
        let (tx, rx) = bounded::<Frame>(capacity.max(1));
        // Handle receiver kedua khusus untuk membuang frame terlama saat penuh.
        let rx_drain = rx.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let paused_thread = paused.clone();

        let handle = thread::spawn(move || {
            while !stop_thread.load(Ordering::Relaxed) {
                // Saat pause: berhenti membaca sumber. Pipe ffmpeg akan penuh dan
                // ffmpeg ikut ter-backpressure (pause sejati, tanpa boros CPU).
                if paused_thread.load(Ordering::Relaxed) {
                    thread::sleep(std::time::Duration::from_millis(20));
                    continue;
                }
                let Some(mut frame) = source.next_frame() else {
                    break; // sumber habis (tidak terjadi bila looping)
                };
                loop {
                    match tx.try_send(frame) {
                        Ok(()) => break,
                        Err(TrySendError::Full(returned)) => {
                            // Buang frame terlama, lalu coba kirim yang baru lagi.
                            let _ = rx_drain.try_recv();
                            frame = returned;
                            if stop_thread.load(Ordering::Relaxed) {
                                return;
                            }
                        }
                        Err(TrySendError::Disconnected(_)) => return,
                    }
                }
            }
        });

        Self {
            rx,
            stop,
            paused,
            handle: Some(handle),
        }
    } 

    /// Pause/resume decode. Saat paused, decode berhenti membaca sumber.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    /// Ambil frame TERBARU yang tersedia (drain frame basi). `None` = timeout
    /// atau sumber selesai.
    pub fn latest(&self, timeout: Duration) -> Option<Frame> {
        let mut frame = self.rx.recv_timeout(timeout).ok()?;
        while let Ok(newer) = self.rx.try_recv() {
            frame = newer;
        }
        Some(frame)
    }
}

impl Drop for DecodePipeline {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Kosongkan channel agar decode thread tidak terblokir menunggu slot.
        while self.rx.try_recv().is_ok() {}
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

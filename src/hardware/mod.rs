//! Deteksi hardware + sampling stats real-time.
//!
//! - Nama GPU via wgpu (lintas-vendor), VRAM total via DXGI (Windows) — lihat ADR-003.
//! - CPU% via sysinfo, disampling di thread terpisah agar tidak memblokir render.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Info hardware statis (dideteksi sekali saat startup).
#[derive(Clone, Debug)]
pub struct HardwareInfo {
    pub cpu_cores: usize,
    pub gpu_name: Option<String>,
    pub gpu_kind: Option<String>,
    pub vram_total_mb: Option<u64>,
}

impl HardwareInfo {
    pub fn detect() -> Self {
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let (gpu_name, gpu_kind) = detect_gpu();
        let vram_total_mb = detect_vram_mb();
        Self {
            cpu_cores,
            gpu_name,
            gpu_kind,
            vram_total_mb,
        }
    }

    /// Ringkasan GPU untuk ditampilkan (fallback bila belum terdeteksi).
    pub fn gpu_summary(&self) -> String {
        match (&self.gpu_name, self.vram_total_mb) {
            (Some(name), Some(vram)) => format!("{name} ({vram} MB)"),
            (Some(name), None) => name.clone(),
            _ => "GPU tidak terdeteksi".to_string(),
        }
    }

    /// VRAM total dalam MB sebagai f32 (untuk stats bar).
    pub fn vram_total_mb_f32(&self) -> Option<f32> {
        self.vram_total_mb.map(|v| v as f32)
    }
}

/// Pilih adapter GPU terbaik via wgpu dan kembalikan (nama, jenis).
fn detect_gpu() -> (Option<String>, Option<String>) {
    use wgpu::{Backends, DeviceType, Instance};

    let instance = Instance::default();
    let adapters = pollster::block_on(instance.enumerate_adapters(Backends::all()));

    let rank = |t: DeviceType| match t {
        DeviceType::DiscreteGpu => 3,
        DeviceType::IntegratedGpu => 2,
        DeviceType::VirtualGpu | DeviceType::Other => 1,
        DeviceType::Cpu => 0,
    };

    let mut best: Option<wgpu::AdapterInfo> = None;
    for adapter in &adapters {
        let info = adapter.get_info();
        let better = best
            .as_ref()
            .map(|b| rank(info.device_type) > rank(b.device_type))
            .unwrap_or(true);
        if better {
            best = Some(info);
        }
    }

    match best {
        Some(info) => {
            let kind = match info.device_type {
                DeviceType::DiscreteGpu => "Discrete",
                DeviceType::IntegratedGpu => "Integrated",
                DeviceType::VirtualGpu => "Virtual",
                DeviceType::Cpu => "CPU",
                DeviceType::Other => "Other",
            };
            (Some(info.name), Some(kind.to_string()))
        }
        None => (None, None),
    }
}

/// VRAM total (MB) via DXGI di Windows; None di platform lain.
#[cfg(windows)]
fn detect_vram_mb() -> Option<u64> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };

    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut best: usize = 0;
        let mut i = 0u32;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            i += 1;
            if let Ok(desc) = adapter.GetDesc1() {
                let is_software = (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0;
                if !is_software && desc.DedicatedVideoMemory > best {
                    best = desc.DedicatedVideoMemory;
                }
            }
        }
        if best > 0 {
            Some((best / (1024 * 1024)) as u64)
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
fn detect_vram_mb() -> Option<u64> {
    None
}

/// Sampler CPU% real-time di thread terpisah (sysinfo). Nilai disimpan sebagai
/// persen*100 dalam atomic agar render loop membacanya tanpa lock.
pub struct StatsProvider {
    cpu_centi: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl StatsProvider {
    pub fn spawn() -> Self {
        let cpu_centi = Arc::new(AtomicU32::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let cpu_w = cpu_centi.clone();
        let stop_w = stop.clone();

        let handle = thread::spawn(move || {
            let mut sys = sysinfo::System::new();
            // Sampling pertama menetapkan baseline (nilai awal tak berarti).
            sys.refresh_cpu_usage();
            while !stop_w.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(500));
                sys.refresh_cpu_usage();
                let pct = sys.global_cpu_usage(); // 0..100
                cpu_w.store((pct * 100.0) as u32, Ordering::Relaxed);
            }
        });

        Self {
            cpu_centi,
            stop,
            handle: Some(handle),
        }
    }

    /// CPU% terakhir yang tersampling.
    pub fn cpu_pct(&self) -> f32 {
        self.cpu_centi.load(Ordering::Relaxed) as f32 / 100.0
    }
}

impl Drop for StatsProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

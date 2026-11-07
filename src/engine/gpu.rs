//! Engine varian GPU (wgpu compute shader). Menghasilkan `EngineOutput` yang
//! IDENTIK dengan jalur CPU — renderer & app tak berubah.
//!
//! Pembagian kerja: resize (box filter) + grayscale + char-index + kuantisasi
//! warna dikerjakan di GPU per-sel; hasil (char_index+color_key terkemas u32)
//! di-readback lalu run-length + hash tetap di CPU (serial per baris, murah).
//!
//! Catatan: ini opsi eksperimen (flag `--gpu`). Untuk output kecil, ongkos
//! upload frame + readback + sync biasanya menutup keuntungan compute — lihat
//! `--bench-gpu` untuk angka head-to-head. CPU tetap default (ADR-004).

use anyhow::{anyhow, Result};
use rayon::prelude::*;
use wgpu::util::DeviceExt;

use super::{charmap, runlength, EngineConfig, EngineOutput, Run};
use crate::source::Frame;

const WGSL: &str = r#"
struct Params {
    src_w: u32, src_h: u32, out_w: u32, out_h: u32,
    quant_bits: u32, colored: u32, charset_len: u32, _pad: u32,
};
@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read> lut: array<u32>;      // 256 entri
@group(0) @binding(3) var<storage, read_write> outbuf: array<u32>;

fn get_byte(i: u32) -> u32 {
    let w = src[i / 4u];
    let sh = (i % 4u) * 8u;
    return (w >> sh) & 0xffu;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let cx = gid.x;
    let cy = gid.y;
    if (cx >= p.out_w || cy >= p.out_h) { return; }

    // Wilayah sumber untuk sel ini (box filter, ~INTER_AREA).
    let sx0 = (cx * p.src_w) / p.out_w;
    let sx1 = max(sx0 + 1u, ((cx + 1u) * p.src_w) / p.out_w);
    let sy0 = (cy * p.src_h) / p.out_h;
    let sy1 = max(sy0 + 1u, ((cy + 1u) * p.src_h) / p.out_h);

    var rs: u32 = 0u;
    var gs: u32 = 0u;
    var bs: u32 = 0u;
    var cnt: u32 = 0u;
    for (var sy = sy0; sy < sy1; sy = sy + 1u) {
        let row = sy * p.src_w;
        for (var sx = sx0; sx < sx1; sx = sx + 1u) {
            let base = (row + sx) * 3u;
            rs = rs + get_byte(base);
            gs = gs + get_byte(base + 1u);
            bs = bs + get_byte(base + 2u);
            cnt = cnt + 1u;
        }
    }
    if (cnt == 0u) { cnt = 1u; }
    let r = rs / cnt;
    let g = gs / cnt;
    let b = bs / cnt;

    let gray = (r * 299u + g * 587u + b * 114u) / 1000u;
    let idx = lut[min(gray, 255u)];

    var key: u32 = 0u;
    if (p.colored != 0u) {
        let q = p.quant_bits;
        let bits = 8u - q;
        key = ((r >> q) << (2u * bits)) | ((g >> q) << bits) | (b >> q);
    }

    outbuf[cy * p.out_w + cx] = (idx << 24u) | (key & 0x00ffffffu);
}
"#;

pub struct GpuEngine {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,

    params_buf: wgpu::Buffer,
    lut_buf: wgpu::Buffer,
    src_buf: Option<wgpu::Buffer>,
    out_buf: Option<wgpu::Buffer>,
    staging_buf: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,

    src_dims: (usize, usize),
    out_dims: (usize, usize),

    cfg: EngineConfig,
    upload: Vec<u8>,     // salinan RGB frame, di-pad ke kelipatan 4
    readback: Vec<u32>,  // hasil per-sel (char_idx+key)
    adapter_name: String,
}

impl GpuEngine {
    pub fn new(cfg: EngineConfig) -> Result<Self> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| anyhow!("tidak ada adapter GPU: {e}"))?;
        let adapter_name = adapter.get_info().name;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ascii-gpu"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| anyhow!("request_device gagal: {e}"))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ascii-compute"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ascii-bgl"),
            entries: &[
                storage_entry(0, true),
                uniform_entry(1),
                storage_entry(2, true),
                storage_entry(3, false),
            ],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ascii-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ascii-pipeline"),
            layout: Some(&pl),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: 32, // 8 x u32
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let lut = charmap::build_lut(cfg.charset.len());
        let lut_u32: Vec<u32> = lut.iter().map(|&x| x as u32).collect();
        let lut_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lut"),
            contents: bytes_of_u32(&lut_u32),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            bgl,
            params_buf,
            lut_buf,
            src_buf: None,
            out_buf: None,
            staging_buf: None,
            bind_group: None,
            src_dims: (0, 0),
            out_dims: (0, 0),
            cfg,
            upload: Vec::new(),
            readback: Vec::new(),
            adapter_name,
        })
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    pub fn set_config(&mut self, cfg: EngineConfig) {
        let lut = charmap::build_lut(cfg.charset.len());
        let lut_u32: Vec<u32> = lut.iter().map(|&x| x as u32).collect();
        self.queue
            .write_buffer(&self.lut_buf, 0, bytes_of_u32(&lut_u32));
        self.cfg = cfg;
    }

    fn ensure_buffers(&mut self, src_w: usize, src_h: usize, out_w: usize, out_h: usize) {
        let mut recreated = false;

        if self.src_buf.is_none() || self.src_dims != (src_w, src_h) {
            let bytes = src_w * src_h * 3;
            let padded = bytes.div_ceil(4) * 4;
            self.src_buf = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("src"),
                size: padded as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.upload = vec![0u8; padded];
            self.src_dims = (src_w, src_h);
            recreated = true;
        }

        if self.out_buf.is_none() || self.out_dims != (out_w, out_h) {
            let cells = out_w * out_h;
            let size = (cells * 4) as u64;
            self.out_buf = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out"),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }));
            self.staging_buf = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("staging"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.readback = vec![0u32; cells];
            self.out_dims = (out_w, out_h);
            recreated = true;
        }

        if recreated || self.bind_group.is_none() {
            self.bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ascii-bg"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.src_buf.as_ref().unwrap().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.params_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.lut_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.out_buf.as_ref().unwrap().as_entire_binding(),
                    },
                ],
            }));
        }
    }

    pub fn process(&mut self, frame: &Frame, out_w: usize, out_h: usize) -> EngineOutput {
        self.ensure_buffers(frame.width, frame.height, out_w, out_h);

        // Upload frame (di-pad ke kelipatan 4).
        let n = frame.data.len().min(self.upload.len());
        self.upload[..n].copy_from_slice(&frame.data[..n]);
        self.queue
            .write_buffer(self.src_buf.as_ref().unwrap(), 0, &self.upload);

        // Params.
        let params: [u32; 8] = [
            frame.width as u32,
            frame.height as u32,
            out_w as u32,
            out_h as u32,
            self.cfg.quant_bits as u32,
            self.cfg.colored as u32,
            self.cfg.charset.len() as u32,
            0,
        ];
        self.queue
            .write_buffer(&self.params_buf, 0, bytes_of_u32(&params));

        // Dispatch.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ascii-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, self.bind_group.as_ref().unwrap(), &[]);
            let gx = (out_w as u32).div_ceil(8);
            let gy = (out_h as u32).div_ceil(8);
            pass.dispatch_workgroups(gx, gy, 1);
        }
        let size = (out_w * out_h * 4) as u64;
        encoder.copy_buffer_to_buffer(
            self.out_buf.as_ref().unwrap(),
            0,
            self.staging_buf.as_ref().unwrap(),
            0,
            size,
        );
        self.queue.submit(Some(encoder.finish()));

        // Readback (blocking).
        let staging = self.staging_buf.as_ref().unwrap();
        {
            let slice = staging.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            let _ = rx.recv();
            let data = slice.get_mapped_range().expect("map readback gagal");
            for (i, chunk) in data.chunks_exact(4).enumerate() {
                self.readback[i] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            }
        }
        staging.unmap();

        // Run-length + hash di CPU (paralel per baris).
        let readback = &self.readback;
        let charset = &self.cfg.charset;
        let colored = self.cfg.colored;
        let built: Vec<(Vec<Run>, u64)> = (0..out_h)
            .into_par_iter()
            .map(|y| runlength::build_row_from_packed(y, out_w, readback, charset, colored))
            .collect();

        let mut rows = Vec::with_capacity(out_h);
        let mut row_hashes = Vec::with_capacity(out_h);
        for (runs, h) in built {
            rows.push(runs);
            row_hashes.push(h);
        }

        EngineOutput {
            width: out_w,
            height: out_h,
            rows,
            row_hashes,
        }
    }
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Reinterpret slice u32 sebagai byte little-endian untuk write_buffer.
fn bytes_of_u32(v: &[u32]) -> &[u8] {
    // SAFETY: u32 slice selalu valid dibaca sebagai byte; panjang = 4x.
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

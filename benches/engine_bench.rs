//! Benchmark engine: bandingkan paralel (rayon) vs single-thread, dan ukur
//! fase resize secara terpisah. Target akseptansi: `process()` < 8 ms untuk
//! output 200x55 dari sumber 1080p.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use video_to_ascii::engine::{AsciiEngine, EngineConfig};
use video_to_ascii::source::Frame;

/// Frame sintetis 1080p dengan gradien 3-arah (konservatif: gradien menghasilkan
/// banyak run pendek sehingga run-length bekerja keras).
fn make_frame(w: usize, h: usize) -> Frame {
    let mut data = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 3;
            data[i] = (x * 255 / w.max(1)) as u8;
            data[i + 1] = (y * 255 / h.max(1)) as u8;
            data[i + 2] = ((x + y) * 255 / (w + h).max(1)) as u8;
        }
    }
    Frame {
        data,
        width: w,
        height: h,
    }
}

fn bench_engine(c: &mut Criterion) {
    let frame = make_frame(1920, 1080);

    for &(w, h) in &[(200usize, 55usize), (384, 108), (600, 169), (800, 225)] {
        let mut group = c.benchmark_group(format!("engine_{w}x{h}"));

        group.bench_with_input(BenchmarkId::new("process_parallel", ""), &(w, h), |b, &(w, h)| {
            let mut e = AsciiEngine::new(EngineConfig::default());
            b.iter(|| {
                let out = e.process(black_box(&frame), w, h);
                black_box(out.rows.len())
            });
        });

        group.bench_with_input(BenchmarkId::new("process_sequential", ""), &(w, h), |b, &(w, h)| {
            let mut e = AsciiEngine::new(EngineConfig::default());
            b.iter(|| {
                let out = e.process_sequential(black_box(&frame), w, h);
                black_box(out.rows.len())
            });
        });

        group.bench_with_input(BenchmarkId::new("resize_only", ""), &(w, h), |b, &(w, h)| {
            let mut e = AsciiEngine::new(EngineConfig::default());
            b.iter(|| {
                let s = e.resize_frame(black_box(&frame), w, h);
                black_box(s.len())
            });
        });

        group.finish();
    }
}

criterion_group!(benches, bench_engine);
criterion_main!(benches);

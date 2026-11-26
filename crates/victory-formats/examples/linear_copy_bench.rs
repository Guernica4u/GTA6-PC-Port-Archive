//! Synthetic CPU benchmark; no GPU or external assets required.
//! Run: cargo run --release -p victory-formats --example linear_copy_bench
use std::{hint::black_box, time::Instant};
use victory_formats::texture::{Desc, Dimension, Layout, Swizzle};

fn main() {
    for (name, width, height, depth, mips, padding) in [
        ("dense 256x256", 256, 256, 1, 1, 0),
        ("dense 1920x1080", 1920, 1080, 1, 1, 0),
        ("dense volume", 256, 64, 16, 1, 0),
        ("mip-chain volume", 256, 64, 16, 4, 0),
        ("padded rows", 256, 256, 1, 1, 13),
        ("unaligned width", 255, 256, 1, 1, 0),
    ] {
        let layout = Layout::new(&Desc {
            dimension: if depth > 1 {
                Dimension::Tex3d
            } else {
                Dimension::Tex2d
            },
            width,
            height,
            depth,
            mips,
            element_bytes: 4,
            block: 1,
            samples: 1,
            swizzle: Swizzle::LINEAR,
        })
        .unwrap();
        let row = width as usize * 4 + padding;
        let plane = row * height as usize;
        let bytes = width as usize * 4 * height as usize * depth as usize;
        let mut tiled = vec![0x5a; layout.size as usize];
        let mut linear = vec![0; plane * depth as usize];
        let iterations = (128usize << 20).div_ceil(bytes);
        for detile in [true, false] {
            let mut samples = Vec::new();
            for _ in 0..7 {
                let start = Instant::now();
                for _ in 0..iterations {
                    if detile {
                        layout
                            .detile(black_box(&tiled), 0, 0, black_box(&mut linear), row, plane)
                            .unwrap();
                    } else {
                        layout
                            .tile(black_box(&mut tiled), 0, 0, black_box(&linear), row, plane)
                            .unwrap();
                    }
                }
                samples.push(start.elapsed().as_secs_f64() * 1e9 / iterations as f64);
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "{name:20} {:6} {:10.1} ns/copy {:10.1} MB/s",
                if detile { "detile" } else { "tile" },
                samples[3],
                bytes as f64 * 1e3 / samples[3]
            );
        }
    }
}

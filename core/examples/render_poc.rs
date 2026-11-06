//! M4 rendering PoC driver.
//!
//! Renders pages from a PDF through PDFium and independently verifies each
//! written PNG by reopening it with the `image` crate.
//!
//! Usage:
//!   cargo run -p confi-core --features render --example render_poc -- \
//!       <pdfium_library_dir> <pdf> <outdir> [dpi] [--all]
//!
//!   <pdfium_library_dir>  directory holding pdfium.dll / libpdfium.so
//!   [dpi]                 render resolution, default 150
//!   --all                 render every page instead of first/middle/last

use std::path::{Path, PathBuf};
use std::time::Instant;

use confi_core::pdf_engine::render;

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 3 {
        eprintln!("usage: render_poc <pdfium_library_dir> <pdf> <outdir> [dpi] [--all]");
        std::process::exit(2);
    }
    let lib_dir = PathBuf::from(args.remove(0));
    let pdf = PathBuf::from(args.remove(0));
    let outdir = PathBuf::from(args.remove(0));

    let mut dpi = 150u32;
    let mut all_pages = false;
    for arg in args {
        if arg == "--all" {
            all_pages = true;
        } else if let Ok(parsed) = arg.parse::<u32>() {
            dpi = parsed;
        } else {
            eprintln!("unknown argument: {arg}");
            std::process::exit(2);
        }
    }

    if let Err(e) = std::fs::create_dir_all(&outdir) {
        eprintln!("cannot create output directory: {e}");
        std::process::exit(2);
    }

    // ── Bind ────────────────────────────────────────────────────────────────
    let started = Instant::now();
    let pdfium = match render::bind_pdfium(&lib_dir) {
        Ok(pdfium) => pdfium,
        Err(e) => {
            println!(
                "BIND_FAILED code={} message={}",
                e.code, e.message
            );
            if let Some(detail) = &e.detail {
                println!("BIND_FAILED detail={detail}");
            }
            std::process::exit(3);
        }
    };
    let bind_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!("BIND ok bind_ms={bind_ms:.2} library_dir={}", lib_dir.display());

    // ── Open ────────────────────────────────────────────────────────────────
    let started = Instant::now();
    let page_count = match render::page_count(&pdfium, &pdf) {
        Ok(count) => count,
        Err(e) => {
            println!("OPEN_FAILED code={} message={}", e.code, e.message);
            if let Some(detail) = &e.detail {
                println!("OPEN_FAILED detail={detail}");
            }
            std::process::exit(4);
        }
    };
    let open_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "DOC ok pages={page_count} open_ms={open_ms:.2} dpi={dpi} file={}",
        pdf.display()
    );

    if page_count == 0 {
        println!("DOC_FAILED reason=no_pages");
        std::process::exit(5);
    }

    // ── Choose pages ────────────────────────────────────────────────────────
    let targets: Vec<u32> = if all_pages {
        (1..=page_count).collect()
    } else {
        let mut set = vec![1u32];
        set.push(page_count / 2 + 1); // middle
        set.push(page_count); // last
        set.sort_unstable();
        set.dedup();
        set
    };

    let stem = pdf
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document")
        .to_string();

    let mut failures = 0u32;
    let mut total_render_ms = 0.0f64;

    for page in &targets {
        let output = outdir.join(format!("{stem}-page-{page}.png"));
        let started = Instant::now();
        let result = match render::render_page_to_png(&pdfium, &pdf, *page, dpi, &output) {
            Ok(result) => result,
            Err(e) => {
                println!("PAGE {page} FAILED code={} message={}", e.code, e.message);
                failures += 1;
                continue;
            }
        };
        let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
        total_render_ms += wall_ms;

        // Independent verification: reopen the written file as an image.
        match verify_png(&output) {
            Ok(stats) => println!(
                "PAGE {page} ok {}x{} bytes={} render_ms={:.2} wall_ms={:.2} reopened={}x{} png={} non_white={:.4} distinct_colors={}",
                result.pixel_width,
                result.pixel_height,
                result.size_bytes,
                result.render_time.as_secs_f64() * 1000.0,
                wall_ms,
                stats.width,
                stats.height,
                stats.format_ok,
                stats.non_white,
                stats.distinct_colors
            ),
            Err(reason) => {
                println!("PAGE {page} VERIFY_FAILED reason={reason}");
                failures += 1;
            }
        }
    }

    println!(
        "SUMMARY pages_rendered={} failures={} total_ms={:.2} mean_ms={:.2} peak_working_set_mb={:.1} outdir={}",
        targets.len(),
        failures,
        total_render_ms,
        total_render_ms / targets.len().max(1) as f64,
        peak_working_set_mb(),
        outdir.display()
    );

    if failures > 0 {
        std::process::exit(6);
    }
}

/// Peak resident memory of this process, in MiB.
///
/// Read straight from the OS rather than sampled externally: the measurement
/// belongs with the run that produced it.
#[cfg(windows)]
fn peak_working_set_mb() -> f64 {
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut ProcessMemoryCounters, size: u32) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };

    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    if ok == 0 {
        0.0
    } else {
        counters.peak_working_set_size as f64 / (1024.0 * 1024.0)
    }
}

#[cfg(not(windows))]
fn peak_working_set_mb() -> f64 {
    0.0
}

struct PngStats {
    width: u32,
    height: u32,
    format_ok: bool,
    non_white: f64,
    distinct_colors: usize,
}

fn verify_png(path: &Path) -> Result<PngStats, String> {
    let decoded = image::open(path).map_err(|e| format!("image::open failed: {e}"))?;
    let rgb = decoded.to_rgb8();
    let (width, height) = rgb.dimensions();

    let pixels = (width as usize * height as usize).max(1);
    let mut non_white = 0usize;
    let mut colors = std::collections::BTreeSet::new();

    for pixel in rgb.pixels() {
        if pixel[0] < 250 || pixel[1] < 250 || pixel[2] < 250 {
            non_white += 1;
        }
        colors.insert((pixel[0], pixel[1], pixel[2]));
    }

    let bytes = std::fs::read(path).map_err(|e| format!("read failed: {e}"))?;
    let format_ok = bytes.starts_with(&[0x89, b'P', b'N', b'G']);

    Ok(PngStats {
        width,
        height,
        format_ok,
        non_white: non_white as f64 / pixels as f64,
        distinct_colors: colors.len(),
    })
}

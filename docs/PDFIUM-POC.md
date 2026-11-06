# CONFI — M4 PDF Rendering PoC (PDFium)

**Status:** PoC complete for Windows x64. Android: **NOT TESTED** (build blocked, see §7).
**Date:** 2026-11-06
**Scope:** Proves PDFium can rasterise real PDFs inside the CONFI core. Does **not**
implement the full M4 PDF engine (organize, thumbnails, PDF↔image workflows).

---

## 1. Verdict

| Question | Result |
|---|---|
| Can PDFium render a real PDF to PNG from Rust? | **Yes** — 232 pages rendered across 8 documents, 0 failures |
| Correct pixel dimensions? | **Yes** — matches `MediaBox × dpi / 72` on every page |
| Are `/Rotate` pages honoured? | **Yes** — page 2/3 of `rotated.pdf` render 1754×1240 (landscape) |
| Rotated, mixed sizes, embedded fonts, scanned, large docs? | **All pass** |
| Are outputs independently verifiable images? | **Yes** — every PNG reopened with the `image` crate, dimensions and pixel stats checked |
| Acceptable performance? | **Yes** — 4-11 ms/page for text pages, 110-450 ms for full-page images |
| Acceptable footprint? | **7.46 MiB** DLL + ~1.8 MiB Rust-side delta |
| Android arm64 runtime? | **NOT TESTED** — no NDK/SDK/JDK on this machine |

---

## 2. Exact versions and provenance

| Item | Value |
|---|---|
| PDFium release | `chromium/8138` = **PDFium 159.0.8138.0**, published 2026-11-04 |
| Source | `github.com/bblanchon/pdfium-binaries` (build of PDFium branch `chromium/8138`) |
| Windows asset | `pdfium-win-x64.tgz`, 4,031,929 bytes |
| Windows SHA-256 | `8d48973e3eb472dda0f5f2435e595ac043f9f2219dd71883392f549445afa8a5` ✔ verified |
| Android asset | `pdfium-android-arm64.tgz`, 3,531,684 bytes |
| Android SHA-256 | `fc73c8020d7204e52f7a9d8153489b3cac2e9c5dc5d70d3a37a976a7398c1a86` ✔ verified |
| PDFium build config | `is_debug=false`, `pdf_enable_v8=false`, `pdf_enable_xfa=false`, `pdf_is_standalone=true`, `pdf_use_partition_alloc=false` |
| Rust binding | `pdfium-render` **0.9.5** (MIT), dynamic binding via `libloading` 0.9.0 |
| Rust toolchain | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1`, target `x86_64-pc-windows-msvc` |
| Host | Windows, release profile (`cargo build --release --features render`) |

Reproduce the download: `pwsh -File tools/fetch-pdfium.ps1` (pins tag and SHA-256).

**Licensing:** PDFium is BSD-3-Clause; `pdfium-render` is MIT. Full licence texts and the
build config are committed under `LICENSES/pdfium/` (21 files). Nothing from GPL/AGPL.

---

## 3. Architecture: rendering is isolated from structural operations

```
pdf_engine/
  merge.rs   split.rs   rotate.rs     ← lopdf, pure Rust, no native library
  render.rs  (feature = "render")     ← PDFium, the only place it is used
```

- **`render` is an optional Cargo feature.** Without it, `confi-core` builds and passes
  every test with no native library present. Merge/split/rotate never call PDFium.
- PDFium is bound **dynamically**: nothing links at build time; the caller passes the
  directory holding `pdfium.dll` / `libpdfium.so`.
- **Bind once per process.** `bind_pdfium()` is called once and the `Pdfium` handle is
  passed to each render call; every call re-uses it (measured bind cost in §5).
- **PDFium is not thread-safe.** The core never spawns threads (AGENTS.md §5); the host
  application owns the instance and serialises access.
- Output follows the same rule as every other engine operation: write to a temp file,
  validate, then rename into place (`common::fs`).

### Interface exposed to the desktop (Tauri) and Android (UniFFI) layers

```rust
// Bind once, at application start-up.
pub fn bind_pdfium(library_dir: &Path) -> ConfiResult<Pdfium>;

// Metadata (independent of lopdf, useful for cross-checking)
pub fn page_count(pdfium: &Pdfium, pdf: &Path) -> ConfiResult<u32>;
pub fn page_size(pdfium: &Pdfium, pdf: &Path, page: u32) -> ConfiResult<(f32, f32)>; // points, /Rotate applied

// Rasterise
pub fn render_page_to_png(
    pdfium: &Pdfium,
    pdf: &Path,
    page: u32,        // 1-indexed
    dpi: u32,         // 72..=600
    output: &Path,
) -> ConfiResult<RenderResult>;     // dims, size, per-page render time

pub fn render_page_rgba(
    pdfium: &Pdfium,
    pdf: &Path,
    page: u32,
    dpi: u32,
) -> ConfiResult<RgbaBitmap>;       // for thumbnails that never touch disk
```

Deferred to M4-full (not built here): `render_page_thumbnail`, `pdf_to_images` with
progress/cancellation, and any caching layer.

---

## 4. Test inputs

Fixtures are **generated, not committed** — `core/examples/make_fixtures.rs` writes them
into a temp directory, which keeps binary PDFs and a font program out of the repo.

```
cargo run -p confi-core --example make_fixtures -- <outdir> C:\Windows\Fonts\LiberationSans-Regular.ttf
```

| Fixture | Pages | Purpose |
|---|---|---|
| `multipage-5.pdf` | 5 | First / middle / last page selection |
| `rotated.pdf` | 3 | `/Rotate` 0, 90, 270 |
| `mixed-sizes.pdf` | 5 | A4, US Letter, A3, 300×400, A4 landscape |
| `embedded-font.pdf` | 1 | TrueType `FontFile2` (LiberationSans, SIL OFL) |
| `scanned.pdf` | 2 | Full-page `DCTDecode` JPEG image XObjects |
| `large-200.pdf` | 200 | Volume / stability |

Two real documents from the user's own Documents folder were also rendered
(read-only, outputs kept in temp; no copies made):

| Document | Size | Pages | Page size |
|---|---|---|---|
| `01 -Foundations of IoT.pdf` | 1.68 MB | 42 | 1531×862 @150dpi |
| `terminator salvation.pdf` | 18.86 MB | 17 | 3000×1687 @150dpi |

---

## 5. Windows x64 results

Command per fixture (release build):

```
target\release\examples\render_poc.exe <pdfium_dir> <pdf> <outdir> [dpi] [--all]
```

### Synthetic fixtures

| Fixture | Rendered | Pixel size @150dpi | Render ms (min/mean/max) | Peak working set |
|---|---|---|---|---|
| `multipage-5.pdf` | 3 (1, 3, 5) | 1240×1754 | 4.82 / 5.90 / 7.00 | 27.3 MiB |
| `rotated.pdf` | 3 (1, 2, 3) | 1240×1754, **1754×1240**, **1754×1240** | 4.35 / 5.30 / 6.65 | 27.3 MiB |
| `mixed-sizes.pdf` | 5 (all) | 1240×1754, 1275×1650, 1754×2481, 625×833, 1754×1240 | 2.90 / 5.33 / 7.67 | 43.8 MiB |
| `embedded-font.pdf` | 1 | 1240×1754 | 4.93 | 27.1 MiB |
| `scanned.pdf` | 2 | 1240×1754 | 10.70 / 10.95 / 11.19 | 30.0 MiB |
| `large-200.pdf` @96dpi | **200 (all)** | 793×1123 each | 2.63 / 9.09 / 7.38* | 18.9 MiB |

\* `large-200` total 1,817.81 ms for 200 pages → **9.09 ms/page mean**; the max column is
per-page wall time including PNG encode.

Initialisation and document-open cost:

| Measurement | Value |
|---|---|
| `bind_pdfium` (library load) | **6.2 – 9.4 ms** per process |
| Open + page count, small PDF | 0.31 – 0.49 ms |
| Open + page count, 1.68 MB / 42 pp | 27.53 ms |
| Open + page count, 18.86 MB / 17 pp | 24.00 ms |

### Real documents

| Document | Rendered | Pixel size | Render ms | Peak working set |
|---|---|---|---|---|
| `01 -Foundations of IoT.pdf` | 1, 22, 42 | 1531×862 | 450.10 / 11.27 / 13.33 | 42.2 MiB |
| `terminator salvation.pdf` | 1, 9, 17 | 3000×1687 | 110.60 / 184.24 / 108.09 | 77.7 MiB |

The 450 ms outlier is page 1 of a slide deck — a full-page photographic background
(a 353 KB PNG, 8,937 distinct colours). Text pages stay at ~11 ms. A 3000×1687 page at
150 dpi peaks at 77.7 MiB, which is the realistic upper bound for a single page here.

### Footprint

| Artifact | Size |
|---|---|
| `pdfium.dll` | 7,824,384 bytes (**7.46 MiB**) |
| `render_poc.exe` (with `render`) | 2,244,096 bytes (2.14 MiB) |
| `make_fixtures.exe` (no PDFium) | 356,864 bytes (0.34 MiB) |
| **Rust-side delta from the feature** | **≈ 1.80 MiB** |
| **Total added to a Windows install** | **≈ 9.6 MiB** |

---

## 6. Independent verification

Every render is verified by re-reading the written file with the **`image` crate** — a
different code path from PDFium's rasteriser:

- file opens as a valid image, PNG signature `89 50 4E 47` present;
- decoded dimensions equal the dimensions PDFium reported;
- `non_white` pixel ratio and distinct-colour count prove the page is not blank;
- visual inspection of two outputs confirmed content:
  - `rotated-page-2.png` — landscape, text upright in reading orientation (PDFium
    applied `/Rotate 90`);
  - `embedded-font-page-1.png` — glyphs rasterised from the embedded `FontFile2`
    program, not from a system font.

Pixel statistics per family:

| Fixture | `non_white` | Distinct colours | Interpretation |
|---|---|---|---|
| `multipage-5` pages | 0.086 | 262 | header band + border + text |
| `mixed-sizes` page 4 (300×400) | 0.195 | 263 | small page, same drawing scaled |
| `embedded-font` | 0.103 | 262 | glyph rasterisation present |
| `scanned` pages | 1.000 | 1343 | full-page image, every pixel non-white |
| real deck page 1 | 0.9996 | 8937 | photographic content |

---

## 7. Android ARM64 — **NOT TESTED**

### What was actually attempted

```
rustup target add aarch64-linux-android        → installed ✔
cargo build -p confi-core --features render --target aarch64-linux-android
                                               → FAILED ✘
```

```
error occurred in cc-rs: failed to find tool "clang.exe": program not found
  [libdeflate-sys 1.26.1]
```

79 crates compiled for `aarch64-linux-android` before the failure (including
`libloading`, `lopdf`, `zune-jpeg`, `png`, `rayon`). The failure is **not** PDFium and
**not** `pdfium-render`: it is `libdeflate-sys`, a C dependency pulled in by
`oxipng` → `libdeflater`, which needs a C toolchain for the target.

### Why it could not proceed

| Prerequisite | State on this machine |
|---|---|
| `aarch64-linux-android` Rust target | ✔ installed |
| Android NDK (provides `aarch64-linux-android26-clang`) | ✘ **not installed** |
| `ANDROID_NDK_HOME` / `ANDROID_HOME` | ✘ empty |
| Android SDK / `adb` | ✘ not installed |
| JDK (`java` on PATH) | ✘ not found |
| Android device or emulator | ✘ none |

Therefore: **no `.so` was produced, no APK was built, and nothing was run on a device.**
No Android runtime claim is made. Dependency compilation is not evidence of compatibility.

### What M4-full must do (documented, not yet executed)

1. **Acquire** the library with the same pinned tag — `pdfium-android-arm64.tgz`,
   SHA-256 `fc73c802…c1a86`, verified the same way as Windows.
2. **Package** `libpdfium.so` (6,763,376 bytes, 6.45 MiB) at
   `app/src/main/jniLibs/arm64-v8a/libpdfium.so`. ABI configuration:
   `ndk { abiFilters += "arm64-v8a" }` in `build.gradle.kts`. The same release also
   publishes `arm`, `x86` and `x64` variants if 32-bit or emulator ABIs are needed.
3. **Load** it before binding: `System.loadLibrary("pdfium")` from the UniFFI
   initialisation path, then hand the extracted-directory equivalent to
   `bind_pdfium()`. On Android the .so ships inside the APK, so the host resolves the
   path from `ApplicationInfo.nativeLibraryDir`.
4. **APK size impact:** +6.45 MiB uncompressed per ABI. With the default
   `extractNativeLibs`/compressed packaging expect roughly 3–4 MiB added to the
   download; with `useLegacyPackaging=false` (uncompressed, mmap-friendly) the full
   6.45 MiB is added. Ship `arm64-v8a` only for v1.0 to keep this to one ABI.
5. **Unblock the build:** install the NDK and point the C toolchain at it
   (`CC_aarch64_linux_android`, `CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER`,
   `cargo-ndk` is the low-friction option). Alternative: drop `oxipng` on Android
   (PNG optimisation) so the pure-Rust stack cross-compiles without a C compiler.
6. **Runtime test** on an arm64 device/emulator, then record render timings and memory
   the same way §5 does for Windows.

---

## 8. Error handling

| Case | Command fragment | Result | Exit |
|---|---|---|---|
| Missing PDFium library | library dir with no `pdfium.dll` | `BIND_FAILED code=INVALID_INPUT` — "The PDFium library was not found at … Rendering is unavailable; folder and image tools are unaffected." | 3 |
| Invalid PDF (text bytes) | `invalid.pdf` | `OPEN_FAILED code=CORRUPTED_FILE` (detail `FormatError`) | 4 |
| Nonexistent file | `does-not-exist.pdf` | `OPEN_FAILED code=INVALID_INPUT` — "File not found: …" | 4 |
| Page out of range | page 2 of a 1-page PDF (test) | `INVALID_INPUT` | — |
| DPI outside 72..=600 | dpi 0 / 71 / 601 (test) | `INVALID_INPUT` | — |
| Output file exists | re-run into same dir | `OUTPUT_ERROR`, original untouched | — |

**NOT TESTED:** encrypted PDFs through the render path. The code maps
`PdfiumInternalError::PasswordError` to `PASSWORD_REQUIRED`, and the file-validation
module separately detects `/Encrypt`, but no encrypted fixture was available to exercise
the render path end to end.

---

## 9. Automated tests added

`core/tests/render_tests.rs` — 6 tests, run only when `CONFI_PDFIUM_DIR` is set; each
prints `SKIP` and returns otherwise so the suite stays green without the native library:

```
$env:CONFI_PDFIUM_DIR = (Resolve-Path vendor\pdfium\win-x64\bin).Path
cargo test -p confi-core --features render --test render_tests
→ 6 passed; 0 failed
```

Covers: exact pixel dimensions at 144 dpi (1190×1684) plus dark-pixel check, out-of-range
page, three invalid DPI values, missing library directory, corrupt PDF, missing input file.

---

## 10. Limitations and open questions for M4-full

1. **PNG encoding dominates wall time.** PDFium rasterises in 2–11 ms; the remaining
   wall time is `image`'s PNG encoder. JPEG (or a faster PNG setting) will matter for
   bulk export and thumbnails.
2. **Full-page images are 40× slower** than text pages (450 ms vs 11 ms). Consider
   progressive rendering or a lower thumbnail DPI for previews.
3. **Memory scales with page pixels**, not page count: 77.7 MiB peak for a 3000×1687
   page at 150 dpi. Cap DPI for previews; a 600 dpi cap already exists in the API.
4. **The embedded-font fixture uses uniform widths**, so its glyph spacing is not
   typographically correct by design. It proves the embedded program is used; it does
   not validate metrics.
5. **PDFium version pinning** must be enforced in CI: the binding is validated against
   `chromium/8138` only. `tools/fetch-pdfium.ps1` pins tag + SHA-256.
6. **ToS/update path:** decide whether the app ships PDFium as a Tauri resource or a
   sidecar binary, and how it is updated independently of the app.

---

## 11. Reproduction (full evidence trail)

```powershell
# 1. Native library (pinned, checksum-verified)
pwsh -File tools/fetch-pdfium.ps1 -Android

# 2. Fixtures (generated, not committed)
cargo run -p confi-core --example make_fixtures -- "$env:TEMP\confi-m4-poc\fixtures" `
    'C:\Windows\Fonts\LiberationSans-Regular.ttf'

# 3. Render matrix
cargo build --release -p confi-core --features render --examples
$exe = 'target\release\examples\render_poc.exe'
$lib = (Resolve-Path 'vendor\pdfium\win-x64\bin').Path
& $exe $lib "$env:TEMP\confi-m4-poc\fixtures\rotated.pdf" "$env:TEMP\confi-m4-poc\renders" 150

# 4. Automated checks
$env:CONFI_PDFIUM_DIR = $lib
cargo test -p confi-core --features render --test render_tests
```

Raw run logs from this session are kept outside the repository in
`%TEMP%\confi-m4-poc\poc-*.log` (they contain absolute paths and, for the real documents,
are intentionally not committed).

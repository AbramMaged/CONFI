# CONFI — M4 PDF Rendering PoC (PDFium)

**Status:** PoC **accepted as a technical demonstration**. The **M4 milestone is NOT
complete** — page organise, thumbnails, and the PDF↔image workflows remain.
**Windows x64:** validated. **Android arm64:** cross-compiles with the NDK; **runtime
NOT TESTED** (no device, emulator, SDK or APK).
**PoC implementation commit:** `c68fff7`
**This review pass:** the commit that introduced this revision — `git log -1 --format=%H -- docs/PDFIUM-POC.md`
**Date:** 2026-11-06

---

## 1. Verdict

| Question | Windows x64 | Android arm64 |
|---|---|---|
| PDFium renders PDFs from Rust | **Yes** — 220 pages across 8 documents, 0 failures (recorded pass) | **NOT TESTED** |
| Correct pixel dimensions | **Yes** — matches `MediaBox × dpi / 72` | — |
| `/Rotate` honoured | **Yes** — landscape output for 90°/270° | — |
| Rotated, mixed sizes, embedded fonts, scanned, large | **Yes** | — |
| Outputs independently verifiable | **Yes** — reopened and pixel-checked | — |
| Compiles for the target | Yes | **Yes** (with NDK r27c) |
| Native library loads at runtime | **Yes** | **NOT TESTED** |

Compiling is not compatibility. Android makes no runtime claim beyond "the crate builds".

---

## 2. Versions and provenance

| Item | Value |
|---|---|
| PDFium | **159.0.8138.0** (`chromium/8138`), published 2026-11-04, built by `github.com/bblanchon/pdfium-binaries` |
| PDFium build config | `is_debug=false`, `pdf_enable_v8=false`, `pdf_enable_xfa=false`, `pdf_is_standalone=true`, `pdf_use_partition_alloc=false` |
| Windows assets | `pdfium-win-x64.tgz` 4,031,929 B — SHA-256 `8d48973e3eb472dda0f5f2435e595ac043f9f2219dd71883392f549445afa8a5` ✔ |
| Android assets | `pdfium-android-arm64.tgz` 3,531,684 B — SHA-256 `fc73c8020d7204e52f7a9d8153489b3cac2e9c5dc5d70d3a37a976a7398c1a86` ✔ |
| Android PDFium config | `default_min_sdk_version = 23`, `target_cpu = "arm64"`, no V8/XFA |
| Rust binding | `pdfium-render` **0.9.5** (MIT OR Apache-2.0; CONFI uses MIT) |
| Rust toolchain | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1`, rustfmt `1.10.0-stable` |
| Host | Windows, release profile `--features render` |

Reproduce: `pwsh -File tools/fetch-pdfium.ps1 -Android` (pins tag + SHA-256).

**Licensing:** PDFium is BSD-3-Clause. Licence texts, the exact build config and
version are committed under `LICENSES/pdfium/` (21 files). `lopdf` (MIT) and
`pdfium-render` licence texts are in `LICENSES/`. No GPL/AGPL anywhere.

---

## 3. Architecture — rendering isolated from structural operations

```
pdf_engine/
  merge.rs   split.rs   rotate.rs    ← lopdf (pure Rust); never touches PDFium
  render.rs  (feature = "render")     ← PDFium; the only file that binds it
```

- **`render` is an optional Cargo feature.** Without it `confi-core` builds and all
  its tests pass with no native library on disk.
- PDFium is **dynamically bound** (`libloading`): nothing links at build time; the
  caller passes the directory holding `pdfium.dll` / `libpdfium.so`.
- Output follows the engine-wide rule: temp file → validate → rename (`common::fs`).

### 3.1 Thread safety — corrected

The earlier revision of this document said the host application must serialise access.
**That was wrong**, and the review caught it. `pdfium-render` enables its `thread_safe`
feature **by default**, which wraps every `FPDF_*` call in a mutex marshaller
(`ThreadSafePdfiumBindings`) and declares `unsafe impl Send/Sync for Pdfium`.

Consequences, now documented honestly:

- Host-side locking is **unnecessary**; `Pdfium` may be shared across threads.
- The trade-off is that **concurrent renders queue on one global lock** — no parallel
  speed-up from sharing a handle. Nothing in CONFI currently renders concurrently.
- Disabling `thread_safe` to get per-thread parallelism would require one library
  instance per thread and is unsafe; not done.

### 3.2 Library lifecycle — verified, not assumed

`pdfium-render` stores bindings in a process-global `OnceCell`, and `Pdfium::new` calls
`BINDINGS.get_or_init(...)`. Therefore:

- The library is initialised **exactly once per process**.
- Binding is process-wide **by construction**, not by convention: a second
  `bind_pdfium` call does not re-initialise, and its path argument is effectively
  ignored (the six render tests bind repeatedly in one process and rely on this).
- Each page render **does re-open the document** — PDFium re-parses the xref per call.
  This is a real inefficiency, documented in §9, with a document-handle API deferred to
  M4-full.

---

## 4. Implementation review findings

| Area | What was found | Action |
|---|---|---|
| Bind once vs per page | `bind_pdfium` is called once per process; the API forces a `&Pdfium` after that. No per-page rebinding is possible without an explicit second bind. | Verified; documented in §3.2 |
| Document lifecycle | **Every render re-opens and re-parses the document.** | Documented as a limitation (§9); deferred to a document-handle API |
| Thread safety | Crate serialises internally (`thread_safe` default), `Pdfium: Send + Sync`. | Docs corrected (§3.1) |
| DPI → pixels | `scale = dpi / 72`; verified A4@150 → 1240×1754 (expected 1239.6×1754.2), A4@144 → 1190×1684 exactly. | Test `renders_page_at_expected_pixel_size` |
| DPI validation | Was checked only in `render_page_to_png`, so a direct `render_page_rgba` call could request any DPI. | **Fixed** — validated once in `render_page_rgba`, the shared entry point |
| Integer overflow | `bitmap.width()` is `Pixels = i32`, cast with `as u32`; the byte count used an unchecked `w * h * 4`. | **Fixed** — `u32::try_from` + `checked_mul` chain |
| Maximum allocation | No ceiling: A3 at 600 dpi is ~69 Mpixels (~278 MB RGBA); a huge `MediaBox` could far exceed that. | **Fixed** — 100 Mpixel budget checked from page size × DPI *before* PDFium allocates. Test with a 20,000×20,000 pt page (400 Mpixels) → `INVALID_INPUT`, no output |
| Page 0 | `page_size` used `saturating_sub(1)`, silently treating page 0 as page 1. | **Fixed** — explicit `INVALID_INPUT` |
| Output atomicity | Written to `.confi-tmp-*` beside the destination, then `rename` (atomic on one volume). | OK |
| Output validation | **`render_page_to_png` renamed the PNG without re-reading it**, unlike the image engine. | **Fixed** — temp PNG's header is reopened, dimensions compared, mismatch discards the file |
| Temp cleanup | Encode failure, validation failure and rename failure each remove the temp file. | Verified; test `failed_render_leaves_no_output_file` asserts no output and no `confi-tmp` leftovers |
| Error propagation | Library/password/format/I/O failures map to `PASSWORD_REQUIRED`, `CORRUPTED_FILE`, `INVALID_INPUT`; nothing is swallowed. | Verified by tests |

Test suite for the render module: **10 tests**, `core/tests/render_tests.rs`, skipped with
a `SKIP` message unless `CONFI_PDFIUM_DIR` is set.

---

## 5. Performance — corrected

### 5.1 Method and why the previous numbers were wrong

The earlier revision reported large-200 as `min / mean / max = 2.63 / 9.09 / 7.38`,
which is impossible (mean above max). The cause: **9.09 was the mean of `wall_ms` over all
200 pages, while 7.38 was the max of `render_ms` taken from only 5 sampled lines.** Two
different metrics over two different populations were printed in one row.

This revision recomputes every statistic from **all** per-page lines of the raw log, and
keeps the two timings apart:

- **`render_ms`** — measured inside `render_page_rgba`: document re-open + PDFium
  rasterisation + RGBA copy. Includes the per-call re-parse from §3.2.
- **`wall_ms`** — the whole `render_page_to_png` call: `render_ms` + PNG encode + fs write.

Raw logs are preserved verbatim in:
- [`docs/poc-logs/render-timings-raw.log`](poc-logs/render-timings-raw.log) — the pass this section is computed from
- [`docs/poc-logs/render-timings-first-pass.log`](poc-logs/render-timings-first-pass.log) — the original PoC pass

Both contain local absolute paths and the names of two real documents; they are committed
at the user's request and can be redacted if that is preferred.

### 5.2 Individual page timings (small fixtures, every page listed)

| Fixture | Page | Pixels | `render_ms` | `wall_ms` |
|---|---|---|---|---|
| multipage-5 | 1 | 1240×1754 | 6.36 | 10.77 |
| multipage-5 | 3 | 1240×1754 | 4.14 | 9.57 |
| multipage-5 | 5 | 1240×1754 | 4.01 | 8.77 |
| rotated | 1 | 1240×1754 | 7.35 | 12.65 |
| rotated | 2 | **1754×1240** | 4.33 | 9.32 |
| rotated | 3 | **1754×1240** | 4.36 | 10.06 |
| mixed-sizes | 1 (A4) | 1240×1754 | 5.96 | 10.36 |
| mixed-sizes | 2 (Letter) | 1275×1650 | 3.99 | 8.36 |
| mixed-sizes | 3 (A3) | 1754×2481 | 7.66 | 14.71 |
| mixed-sizes | 4 (300×400) | 625×833 | 2.66 | 5.27 |
| mixed-sizes | 5 (A4 landscape) | 1754×1240 | 4.05 | 8.32 |
| embedded-font | 1 | 1240×1754 | 5.25 | 10.10 |
| scanned | 1 | 1240×1754 | 13.01 | 18.14 |
| scanned | 2 | 1240×1754 | 10.73 | 15.62 |
| real `01 -Foundations of IoT.pdf` | 1 | 1531×862 | **429.68** | 437.30 |
| real `01 -Foundations of IoT.pdf` | 22 | 1531×862 | 12.76 | 17.48 |
| real `01 -Foundations of IoT.pdf` | 42 | 1531×862 | 12.38 | 17.76 |
| real `terminator salvation.pdf` | 1 | 3000×1687 | 110.09 | 131.24 |
| real `terminator salvation.pdf` | 9 | 3000×1687 | 174.16 | 194.88 |
| real `terminator salvation.pdf` | 17 | 3000×1687 | 96.36 | 116.78 |

### 5.3 Aggregate statistics (computed over every rendered page)

`render_ms` — document re-open + rasterise + RGBA copy:

| Suite | Pages rendered | min | median | mean | max | Peak working set |
|---|---|---|---|---|---|---|
| multipage-5 @150 | 3 of 5 | 4.01 | 4.14 | 4.84 | 6.36 | 27.3 MB |
| rotated @150 | 3 of 3 | 4.33 | 4.36 | 5.35 | 7.35 | 27.3 MB |
| mixed-sizes @150 | 5 of 5 | 2.66 | 4.05 | 4.86 | 7.66 | 43.9 MB |
| embedded-font @150 | 1 of 1 | 5.25 | 5.25 | 5.25 | 5.25 | 27.1 MB |
| scanned @150 | 2 of 2 | 10.73 | 11.87 | 11.87 | 13.01 | 30.1 MB |
| **large-200 @96** | **200 of 200** | **2.51** | **4.04** | **4.61** | **10.24** | 18.6 MB |
| real 42-page doc @150 | 3 of 42 | 12.38 | 12.76 | 151.61 | 429.68 | 42.2 MB |
| real 17-page doc @150 | 3 of 17 | 96.36 | 110.09 | 126.87 | 174.16 | 77.7 MB |

`wall_ms` — the same calls including PNG encode and file write:

| Suite | min | median | mean | max |
|---|---|---|---|---|
| multipage-5 | 8.77 | 9.57 | 9.70 | 10.77 |
| rotated | 9.32 | 10.06 | 10.68 | 12.65 |
| mixed-sizes | 5.27 | 8.36 | 9.40 | 14.71 |
| embedded-font | 10.10 | 10.10 | 10.10 | 10.10 |
| scanned | 15.62 | 16.88 | 16.88 | 18.14 |
| large-200 | 5.28 | 8.13 | 9.16 | 19.23 |
| real 42-page doc | 17.48 | 17.76 | 157.51 | 437.30 |
| real 17-page doc | 116.78 | 131.24 | 147.63 | 194.88 |

Reading of these numbers:

- **Text pages are 2.5–7 ms of rasterisation**; PNG encoding adds 2–5 ms.
- **The 42-page real document has one 430 ms outlier** (page 1, a photographic slide).
  That single page lifts the mean to 151.61 ms while the median stays at 12.76 ms — the
  clearest reason to report median alongside mean.
- **A 3000×1687 slide page costs 96–174 ms** and peaks at 77.7 MB RSS; large pages are
  bounded by pixels, not by page count.
- **large-200 totals 1,817 ms for 200 pages** (mean 9.16 ms `wall_ms`/page, median 8.13).

### 5.4 One-time costs

| Measurement | Range across 8 process runs |
|---|---|
| `bind_pdfium` (library load + init) | 5.47 – 10.57 ms |
| Document open + page count, small fixtures | 0.34 – 0.47 ms |
| Document open + page count, 42-page 1.7 MB | 4.76 ms |
| Document open + page count, 17-page 18.9 MB | 0.46 ms |
| First process in a cold session | `bind` 7.00 ms, `open` **12.26 ms** (cold file cache) |

### 5.5 Footprint

| Artifact | Size |
|---|---|
| `pdfium.dll` (x64) | 7,824,384 B (**7.46 MiB**) |
| `libpdfium.so` (arm64) | 6,763,376 B (**6.45 MiB**) |
| `render_poc.exe` (with `render`) | 2,244,096 B (2.14 MiB) |
| `make_fixtures.exe` (no PDFium) | 356,864 B (0.34 MiB) |
| Rust-side delta from the feature | **≈ 1.80 MiB** |
| Total added to a Windows install | **≈ 9.6 MiB** |

---

## 6. Independent verification

Every rendered file is re-read with the **`image` crate**, a different code path from
PDFium's rasteriser:

- valid image, PNG signature `89 50 4E 47`;
- decoded dimensions equal the dimensions PDFium reported;
- `non_white` ratio and distinct-colour count prove the page is not blank;
- **the library itself now performs the same reopen check** before renaming the temp file
  into place (§4).

| Fixture | `non_white` | Distinct colours | Interpretation |
|---|---|---|---|
| multipage-5 | 0.086 | 262 | header band + border + text |
| mixed-sizes page 4 | 0.195 | 263 | small page, same drawing |
| embedded-font | 0.103 | 262 | glyph rasterisation present |
| scanned | 1.000 | 1343 | full-page image |
| real deck page 1 | 0.9996 | 8937 | photographic content |

Visual inspection of two outputs confirmed content: the rotated page renders landscape with
upright reading orientation, and the embedded-font page draws glyphs from the embedded
`FontFile2` program.

---

## 7. Error handling (executed)

| Case | Result | Exit |
|---|---|---|
| Missing PDFium library | `BIND_FAILED code=INVALID_INPUT` — "The PDFium library was not found at … Rendering is unavailable; folder and image tools are unaffected." | 3 |
| Invalid PDF (text bytes) | `OPEN_FAILED code=CORRUPTED_FILE` (detail `FormatError`) | 4 |
| Nonexistent file | `OPEN_FAILED code=INVALID_INPUT` — "File not found: …" | 4 |
| Page out of range | `INVALID_INPUT` (test) | — |
| Page 0 | `INVALID_INPUT` (test) | — |
| DPI outside 72..=600 (both entry points) | `INVALID_INPUT` (test) | — |
| Allocation budget exceeded | `INVALID_INPUT`, cites the pixel budget, no output written (test) | — |
| Output file exists | `OUTPUT_ERROR`, original untouched | — |

**NOT TESTED:** encrypted PDFs through the render path. The code maps
`PdfiumInternalError::PasswordError` → `PASSWORD_REQUIRED` and the file engine separately
detects `/Encrypt`, but no encrypted fixture was available.

---

## 8. Android arm64 — builds; runtime NOT TESTED

### 8.1 Required NDK version

- **NDK r27c = 27.2.12479018** (downloaded `android-ndk-r27c-windows.zip`, 745.3 MB,
  SHA-256 `27e49f11e0cee5800983d8af8f4acd5bf09987aa6f790d4439dda9f3643d2494`).
- Minimum requirement is any NDK providing `aarch64-linux-android*-clang` (r23+).
  **PDFium's own binaries require `minSdkVersion >= 23`** (`default_min_sdk_version = 23`
  in its build config), so the API-23 clang wrapper is used.

### 8.2 Build attempt — first attempt failed, retry succeeded

First attempt (no NDK):

```
cargo build -p confi-core --features render --target aarch64-linux-android
→ error in cc-rs: failed to find tool "clang.exe"   [libdeflate-sys 1.26.1]
```

Retry with the NDK configured:

```powershell
$tc = 'C:\work\android-ndk-r27c\toolchains\llvm\prebuilt\windows-x86_64\bin'
$env:CC_aarch64_linux_android  = "$tc\aarch64-linux-android23-clang.cmd"
$env:AR_aarch64_linux_android  = "$tc\llvm-ar.exe"
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "$tc\aarch64-linux-android23-clang.cmd"
$env:CFLAGS_aarch64_linux_android = '--target=aarch64-linux-android23'
cargo build -p confi-core --features render --target aarch64-linux-android
```

**Result: `EXIT=0`, finished in 29.84 s.** `libdeflater v1.26.1`, `oxipng v9.1.5`,
`image v0.25.10`, `pdfium-render v0.9.5` and `confi-core` all compiled for
`aarch64-linux-android`. **`libdeflate-sys` builds successfully** once a C toolchain for
the target is present — the earlier failure was purely the missing NDK, not an
incompatibility.

Artifacts:

| Artifact | Size | Note |
|---|---|---|
| `target/aarch64-linux-android/debug/libconfi_core.rlib` | 16,557,056 B | the library itself |
| `…/deps/libpdfium_render-*.so` | 42,125,608 B | an artifact of `pdfium-render`'s own declared crate-types; **not shipped** — CONFI links it as an rlib |
| PDFium `.so` linked into our output | **none** | dynamic binding: PDFium is loaded at runtime, never linked |

`core/Cargo.toml` declares `crate-type = ["lib"]`, so no `.so` is produced for CONFI yet.
A `cdylib` target is required for the UniFFI bridge — that belongs to M8.

### 8.3 Runtime — NOT TESTED

| Prerequisite | State |
|---|---|
| `aarch64-linux-android` target | ✔ installed |
| NDK r27c | ✔ installed |
| Android SDK / `adb` | ✘ not installed |
| JDK | ✘ not found |
| Emulator or device | ✘ none |
| APK containing `libpdfium.so` | ✘ not built |

**No `.so` was loaded and no PDF was rendered on Android.** No Android support is claimed.

### 8.4 What M8 must do (documented, not executed)

1. **Acquire**: `tools/fetch-pdfium.ps1 -Android` (same pinned tag, SHA-256 verified).
2. **Package**: `app/src/main/jniLibs/arm64-v8a/libpdfium.so`; `abiFilters += "arm64-v8a"`.
3. **Load**: `System.loadLibrary("pdfium")` before binding; resolve the directory from
   `ApplicationInfo.nativeLibraryDir` and pass it to `bind_pdfium`.
4. **APK size**: +6.45 MiB uncompressed per ABI; ≈ +3–4 MiB compressed in the APK, or the
   full 6.45 MiB if `useLegacyPackaging=false`. Ship only `arm64-v8a` for v1.0.
5. **UniFFI**: add a `cdylib` crate type for the bridge.
6. **Runtime test** on a device/emulator, recording timings and RSS the way §5 does here.

### 8.5 Platform-specific behaviour — not introduced

No platform-specific code path, no feature removal, and **`oxipng` was not disabled**.
The only platform input is the NDK toolchain in the build environment. If a future decision
drops `oxipng` on Android, the trade-offs to document are: loss of lossless PNG
optimisation (larger PNG outputs) in exchange for a pure-Rust, no-C-toolchain build.

---

## 9. Structural PDF operations — unchanged and unevaluated by this PoC

- `merge`, `split`, `rotate` remain on **lopdf**, in separate modules, with no reference to
  PDFium. Nothing in this PoC changes or validates them.
- **This PoC is not evidence that lopdf structural operations are reliable** on real-world
  PDFs. Their current tests use synthesised fixtures; no corpus has been run.
- **qpdf remains an open architectural decision** for structural optimisation
  (`docs/ARCHITECTURE.md` §6, `LICENSES/INVENTORY.md`). PDFium supplies rasterisation only
  and does not answer the qpdf question.

---

## 10. Remaining limitations

1. **Document re-opened per page** — PDFium re-parses the xref on every call. A
   document-handle API is the M4-full fix; until then bulk export pays a fixed per-page
   parse cost (visible as the low end of `render_ms` on the 18.9 MB document).
2. **PNG encode dominates wall time** for light pages (2–5 ms of ~9 ms). JPEG output or a
   faster PNG setting matters for bulk export.
3. **Full-page images are ~10–40× slower** than text pages (430 ms worst case).
4. **Memory scales with page pixels**: 77.7 MB peak for a 3000×1687 page at 150 dpi. The
   new 100 Mpixel guard caps the worst case but does not make it cheap.
5. **Concurrent renders serialise** on the `thread_safe` mutex (§3.1).
6. **The embedded-font fixture uses uniform widths** — it proves the embedded glyph
   program is used, not that metrics are correct.
7. **Encrypted PDFs are untested** on the render path (§7).
8. **`pdfium-render` must be bumped together with the PDFium version**; CI should pin both.
   `tools/fetch-pdfium.ps1` pins the tag and SHA-256.
9. **Real-document timings come from two files on one machine** — not a benchmark corpus.

---

## 11. Reproduction

```powershell
# 1. Native library (pinned, checksum-verified)
pwsh -File tools/fetch-pdfium.ps1 -Android

# 2. Fixtures (generated, never committed)
cargo run -p confi-core --example make_fixtures -- "$env:TEMP\confi-m4-poc\fixtures" `
    'C:\Windows\Fonts\LiberationSans-Regular.ttf'

# 3. Render
cargo build --release -p confi-core --features render --examples
$exe = 'target\release\examples\render_poc.exe'
$lib = (Resolve-Path 'vendor\pdfium\win-x64\bin').Path
& $exe $lib "$env:TEMP\confi-m4-poc\fixtures\rotated.pdf" "$env:TEMP\confi-m4-poc\renders" 150
& $exe $lib "$env:TEMP\confi-m4-poc\fixtures\large-200.pdf" "$env:TEMP\confi-m4-poc\renders" 96 --all

# 4. Verification suite
$env:CONFI_PDFIUM_DIR = $lib
cargo test -p confi-core --features render --test render_tests

# 5. Gate checks
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
```

**Gate results at this revision** (executed, not assumed):

| Command | Result |
|---|---|
| `cargo fmt --all --check` | **pass** (exit 0) |
| `cargo check --workspace` | **pass** (exit 0, no warnings) |
| `cargo check --workspace --features confi-core/render` | **pass** (exit 0, no warnings) |
| `cargo test --workspace` | **111 passed, 0 failed** |
| `cargo test -p confi-core --features render` (with `CONFI_PDFIUM_DIR`) | **108 passed, 0 failed** (includes the 10 render tests) |
| `cargo build --release -p confi-core --features render --examples` | **pass** |
| `cargo build -p confi-core --features render --target aarch64-linux-android` (NDK r27c) | **pass** (29.84 s) |

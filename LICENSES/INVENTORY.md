# CONFI — Dependency and License Inventory

**Document Version:** 1.0  
**Date:** 2026-10-31  
**Status:** Preliminary — candidates listed; decisions pending PoC validation

---

## Evaluation Status Key

| Status | Meaning |
|---|---|
| ✅ Approved | Confirmed compatible; safe to use |
| 🔍 Evaluate | Needs PoC validation before committing |
| ⚠️ Review | License or distribution concern identified |
| ❌ Rejected | Must not use for identified reason |

---

## 1. Rust Core Dependencies

### File Detection

| Library | Version | License | Windows | Android | Status | Notes |
|---|---|---|---|---|---|---|
| `infer` | latest | MIT | ✓ | ✓ | ✅ Approved | Pure Rust magic-byte detection |

### PDF Processing

| Library | Version | License | Windows | Android | Status | Notes |
|---|---|---|---|---|---|---|
| `lopdf` | 0.34 | MIT | ✓ | ✓ | ✅ Approved | Pure Rust PDF read/write; used by `pdf_engine` for merge, split and rotate (M3). No rendering. Android cross-compilation to be confirmed in M8. |
| `pdf` | latest | MIT/Apache | ✓ | ✓ | 🔍 Evaluate | Alternative pure-Rust reader |
| `pdfium-render` | latest | MIT (bindings) | ✓ | ✓ | 🔍 Evaluate | Rust bindings to PDFium; PDFium is BSD-3. Requires bundling prebuilt PDFium binary. Android cross-compilation needs verification. |
| `qpdf` (sys/FFI) | — | Apache 2.0 | ✓ | 🔍 | 🔍 Evaluate | qpdf itself is Apache 2.0. Android cross-compilation requires toolchain work. |
| PDFBox | — | Apache 2.0 | ✓ | 🔍 | ⚠️ Review | JVM dependency complicates Android integration from Rust. Likely unsuitable for shared core. |

> **Decision (resolved at M3):** `lopdf` (MIT, pure Rust) is committed for PDF
> manipulation — merge, split and rotate are implemented and tested on it.
> `pdfium-render` remains 🔍 Evaluate and is only needed if M4 rendering and
> thumbnails require a rasterizer; `qpdf` is not needed for v1.0.

### Image Processing

| Library | Version | License | Windows | Android | Status | Notes |
|---|---|---|---|---|---|---|
| `image` | latest | MIT/Apache-2.0 | ✓ | ✓ | ✅ Approved | Pure Rust; excellent format coverage; JPG/PNG/WEBP/BMP all supported |
| `oxipng` | latest | MIT | ✓ | ✓ | ✅ Approved | PNG lossless optimization; pure Rust |
| `mozjpeg-sys` | latest | MPL 2.0 + IJG | ✓ | 🔍 | ⚠️ Review | Higher-quality JPEG compression; MPL 2.0 requires source availability for modifications; IJG license is permissive. Evaluate whether `image` crate JPEG quality is sufficient before adding this dependency. |
| `webp` | latest | Apache-2.0 | ✓ | ✓ | 🔍 Evaluate | WebP encode/decode; wraps libwebp. Android cross-compilation needed. |
| `libvips` (via FFI) | — | LGPL 2.1+ | ✓ | ✗ | ⚠️ Review | LGPL 2.1+ requires dynamic linking or source distribution in some configurations. Android support is limited. Likely **not used** in favor of pure-Rust alternatives. |

### Utilities

| Library | Version | License | Windows | Android | Status | Notes |
|---|---|---|---|---|---|---|
| `uuid` | latest | MIT/Apache-2.0 | ✓ | ✓ | ✅ Approved | Job ID generation |
| `chrono` | latest | MIT/Apache-2.0 | ✓ | ✓ | ✅ Approved | Timestamps |
| `serde` + `serde_json` | latest | MIT/Apache-2.0 | ✓ | ✓ | ✅ Approved | Serialization |
| `thiserror` | latest | MIT/Apache-2.0 | ✓ | ✓ | ✅ Approved | Error types |
| `tokio` | latest | MIT | ✓ | ✓ | ✅ Approved | Async runtime (for Tauri integration) |

---

## 2. Windows Application Dependencies

| Library | Version | License | Status | Notes |
|---|---|---|---|---|
| Tauri | 2.x | MIT/Apache-2.0 | ✅ Approved | Desktop framework |
| React | 18.x | MIT | ✅ Approved | UI framework |
| TypeScript | 5.x | Apache-2.0 | ✅ Approved | Type safety |
| Vite | 5.x | MIT | ✅ Approved | Build tool |
| Zustand | latest | MIT | ✅ Approved | Lightweight state management |

---

## 3. Android Application Dependencies

| Library | Version | License | Status | Notes |
|---|---|---|---|---|
| Kotlin | 1.9+ | Apache-2.0 | ✅ Approved | Language |
| Jetpack Compose | latest stable | Apache-2.0 | ✅ Approved | UI framework |
| UniFFI | latest | Mozilla Public License 2.0 | ✅ Approved | Rust-to-Kotlin FFI generator; MPL 2.0 but tool-generated code is not MPL |
| WorkManager | latest | Apache-2.0 | ✅ Approved | Background processing |
| Android Storage Access Framework | built-in | Apache-2.0 | ✅ Approved | Content URI file access |

---

## 4. PDFium Distribution Notes

PDFium (the rendering engine used by Chrome) is available under a BSD-3-Clause license.

Key distribution considerations:
- **Prebuilt binaries:** Available from third-party distributors (e.g., pdfium-binaries project on GitHub, MIT license for the distribution scripts). The PDFium binaries themselves remain BSD-3.
- **Binary size:** PDFium adds ~10–30 MB to the binary depending on the platform.
- **Android:** arm64-v8a and x86_64 prebuilt binaries are available but must be verified.
- **Obligation:** Include `LICENSE-PDFium` in the `LICENSES/` directory.

**Status:** 🔍 Evaluate during Milestone 1 PoC.

---

## 5. License Compatibility Summary

| License | Commercial Use | Distribution | Modification | Notes |
|---|---|---|---|---|
| MIT | ✅ Yes | ✅ Yes | ✅ Yes | Must include copyright notice |
| Apache-2.0 | ✅ Yes | ✅ Yes | ✅ Yes | Must include NOTICE file if applicable |
| BSD-3-Clause | ✅ Yes | ✅ Yes | ✅ Yes | Must include copyright notice; no endorsement |
| MPL 2.0 | ✅ Yes | ✅ Yes | ✅ (modifications must be open source) | File-level copyleft; not project-level |
| LGPL 2.1+ | ✅ Yes | ✅ Yes | ⚠️ | Dynamic linking required; source of LGPL component available |
| GPL 2/3 | ⚠️ | ⚠️ | ⚠️ | Would require CONFI source to be GPL. **DO NOT USE.** |
| AGPL | ❌ | ❌ | ❌ | Network use triggers copyleft. **DO NOT USE.** |
| Commercial | ❌ | — | — | Requires separate evaluation. **DO NOT USE without explicit sign-off.** |

---

## 6. Required License Files in LICENSES/

When a dependency is approved, add its license text to the `LICENSES/` directory:

- `LICENSE-infer` — MIT
- `LICENSE-lopdf` — MIT (pending approval)
- `LICENSE-pdfium` — BSD-3-Clause (pending approval)
- `LICENSE-image` — MIT/Apache-2.0
- `LICENSE-oxipng` — MIT
- `LICENSE-webp` — Apache-2.0 (pending approval)
- `LICENSE-tauri` — MIT/Apache-2.0
- `LICENSE-uniffi` — MPL-2.0

---

## 7. Inventory Update Process

1. Before adding a dependency: add it to this inventory with status 🔍 Evaluate.
2. After successful PoC validation: update to ✅ Approved or document the rejection reason.
3. Add the license text to `LICENSES/`.
4. PR description must reference the inventory entry.
5. Do not merge a PR that adds a dependency without updating this file.

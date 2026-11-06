# CONFI

**Every file. One place.**

CONFI is an offline-first, privacy-respecting file utility for Windows and Android. It combines a comprehensive PDF toolkit with universal image conversion, compression, and resizing — without requiring an internet connection, user account, or any cloud dependency.

---

## What CONFI Does

### PDF Tools
- Merge multiple PDFs into one document
- Split PDFs by page range or into individual pages
- Rotate pages (90°, 180°, 270°)
- Reorder, delete, and duplicate pages visually
- Compress PDFs with preset quality levels
- Export PDF pages as images (JPG)
- Create PDFs from images

### Image Tools
- Convert between JPG, PNG, WEBP, and BMP
- Compress images with quality control
- Resize by dimensions or percentage
- Batch process multiple files

---

## Platforms

| Platform | Stack | Status |
|---|---|---|
| Windows 10/11 | Tauri + React + TypeScript + Rust | In development |
| Android 10+ | Kotlin + Jetpack Compose + Rust (UniFFI) | In development |

---

## Architecture

CONFI uses a shared Rust processing engine accessed by both platform applications. All file processing happens locally — no files are transmitted to any server.

```
Windows App (Tauri + React)       Android App (Kotlin + Compose)
         │                                    │
   Tauri Commands                       UniFFI FFI
         │                                    │
         └─────────── Shared Rust Core ───────┘
                     file_engine
                     pdf_engine
                     image_engine
                     job_manager
```

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full architecture document.

---

## Documentation

| Document | Description |
|---|---|
| [`docs/SRS.md`](docs/SRS.md) | Software Requirements Specification |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Technical architecture |
| [`docs/API.md`](docs/API.md) | Core processing API reference |
| [`docs/PDFIUM-POC.md`](docs/PDFIUM-POC.md) | M4 PDFium rendering PoC: measurements, evidence, Android status |
| [`docs/SUPPORTED_FORMATS.md`](docs/SUPPORTED_FORMATS.md) | Supported file formats |
| [`docs/TESTING.md`](docs/TESTING.md) | Testing strategy and acceptance criteria |
| [`LICENSES/INVENTORY.md`](LICENSES/INVENTORY.md) | Dependency and license inventory |
| [`AGENTS.md`](AGENTS.md) | Development agent instructions |

---

## Development Status

**Current milestone:** M4 rendering PoC accepted (Windows validated, Android
cross-compiles but runtime untested); **M6 Windows app started** — a first slice runs
and performs detection, image convert/compress/resize, and PDF merge/split/rotate.

| Area | State |
|---|---|
| `file_engine` — detection, validation | ✅ done, tested |
| `image_engine` — convert, compress, resize | ✅ done, tested |
| `pdf_engine` — merge, split, rotate | ✅ done, tested |
| `pdf_engine` — render (PDFium) | 🟡 PoC: Windows x64 validated · Android arm64 builds, runtime NOT TESTED ([report](docs/PDFIUM-POC.md)) |
| `pdf_engine` — organize, compress, PDF↔image | ⬜ M4–M5 |
| `job_manager` — queue, progress, cancellation | ⬜ not started |
| Windows app (Tauri + React) | 🟡 first slice runs: file detection + 6 operations ([app](apps/desktop/README.md)) |
| Explorer context menu | ⬜ prototype only, not registered |
| Android app (Kotlin + Compose) | ⬜ not started |

See [`docs/SRS.md`](docs/SRS.md) for the full development milestone plan.

---

## Non-Goals

CONFI Core will **not** include:
- AI features of any kind
- Cloud storage or sync
- User accounts or subscriptions
- PDF-to-Office document reconstruction
- OCR, signatures, forms, or watermarks (future releases)
- Video or audio conversion (future releases)

---

## License

CONFI itself is proprietary. Third-party dependency licenses are documented in [`LICENSES/INVENTORY.md`](LICENSES/INVENTORY.md).

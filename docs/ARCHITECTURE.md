# CONFI — Architecture Document

**Document Version:** 1.0  
**Status:** Proposed  
**Date:** 2026-10-31

---

## 1. Architectural Overview

CONFI uses a **shared Rust processing engine** accessed by two separate platform applications.

```
                    PRODUCT INTERFACES
                           |
            +--------------+--------------+
            |                             |
         WINDOWS                       ANDROID
    Tauri + React + TS             Kotlin + Compose
            |                             |
     Tauri Commands (IPC)         UniFFI FFI Bridge
            |                             |
            +--------------+--------------+
                           |
                   SHARED RUST CORE
                           |
         +-----------------+------------------+
         |                 |                  |
     file_engine       pdf_engine        image_engine
         |                 |                  |
   Detection          Merge/Split        Conversion
   Validation         Rotate/Organize    Compression
   Metadata           Compress           Resizing
                      Render/Export
         |                 |                  |
         +-----------------+------------------+
                           |
                     job_manager
                     queue / progress / cancellation
```

### Design Rationale

- **Shared core** eliminates duplicated processing logic between platforms.
- **Platform-native UI** ensures correct interaction conventions per OS.
- **Strict boundary** between UI and processing: the React frontend never touches file bytes directly.
- **UniFFI** generates Kotlin bindings from Rust interface definitions, reducing hand-written FFI boilerplate.

### Unified Request Format (Core Design Principle)

All entry points — the application UI, the Windows shell verb, and the Windows 11 COM extension — produce the same internal request format:

```json
{
  "operation": "merge_pdf",
  "files": [
    "C:\\Documents\\report.pdf",
    "C:\\Documents\\appendix.pdf"
  ],
  "options": {}
}
```

This `ConfiRequest` JSON is the single contract between any external trigger and the processing engine. The engine has no knowledge of whether the request came from the UI or from Explorer. The shell helper (`confi-shell.exe`) and the COM extension are both just **delivery mechanisms** — they translate Windows-specific invocation (registry verbs, `IShellItemArray`) into a `ConfiRequest` and forward it via named pipe or deep-link URI.

```
Entry points                     Unified request              Engine
────────────────                 ───────────────              ──────
App UI (drag & drop)  ──────┐
Shell verb (Win 10)   ──────┤──→  ConfiRequest JSON  ──────→  Rust core
IExplorerCommand      ──────┘     { operation, files }         (one code path)
(Win 11 COM)
```

This design means:
- Adding a new entry point (e.g., a CLI tool, a keyboard shortcut app) requires only building a new delivery mechanism — no changes to the processing engine.
- Testing the engine does not require a UI or a shell extension.
- The named pipe protocol is testable with a simple JSON sender.

---

## 2. Repository Structure

```
confi/
├── apps/
│   ├── desktop/                  # Windows application
│   │   ├── src/                  # React + TypeScript UI
│   │   │   ├── components/       # Shared UI components
│   │   │   ├── pages/            # Screen implementations
│   │   │   ├── hooks/            # React hooks
│   │   │   ├── stores/           # State management (Zustand or similar)
│   │   │   └── styles/           # CSS variables, design tokens
│   │   └── src-tauri/            # Tauri host process
│   │       ├── src/
│   │       │   ├── commands/     # Tauri command handlers
│   │       │   └── main.rs
│   │       └── tauri.conf.json
│   │
│   └── android/
│       └── app/
│           ├── src/main/
│           │   ├── kotlin/       # Kotlin source
│           │   └── res/          # Android resources
│           └── build.gradle.kts
│
├── core/                         # Shared Rust crate (library)
│   ├── src/
│   │   ├── lib.rs
│   │   ├── common/
│   │   │   ├── errors.rs         # Error types and codes
│   │   │   ├── models.rs         # FileInfo, Job, JobStatus, etc.
│   │   │   └── configuration.rs  # Shared configuration types
│   │   ├── file_engine/
│   │   │   ├── detection.rs      # Magic-byte file detection
│   │   │   ├── validation.rs     # Input validation
│   │   │   └── metadata.rs       # File metadata extraction
│   │   ├── pdf_engine/
│   │   │   ├── merge.rs
│   │   │   ├── split.rs
│   │   │   ├── rotate.rs
│   │   │   ├── organize.rs
│   │   │   ├── compress.rs
│   │   │   ├── render.rs         # Page thumbnail/image rendering (PDFium)
│   │   │   └── image_conversion.rs  # PDF<->Image conversion
│   │   ├── image_engine/
│   │   │   ├── conversion.rs
│   │   │   ├── compression.rs
│   │   │   └── resize.rs
│   │   ├── job_manager/
│   │   │   ├── queue.rs
│   │   │   ├── progress.rs
│   │   │   └── cancellation.rs
│   │   └── platform/
│   │       ├── windows.rs        # Windows-specific file I/O abstractions
│   │       └── android.rs        # Android content URI abstractions
│   └── tests/
│       ├── unit/
│       └── integration/
│
├── bindings/
│   └── uniffi/
│       └── confi.udl             # UniFFI interface definition
│
├── integrations/
│   └── windows/
│       ├── context_menu/         # Shell extension / context menu helper
│       └── installer/            # NSIS or WiX installer configuration
│
├── tests/
│   ├── fixtures/                 # Test PDFs and images
│   ├── unit/                     # Cross-platform unit tests
│   └── integration/              # End-to-end processing tests
│
├── docs/
│   ├── SRS.md
│   ├── ARCHITECTURE.md           # This file
│   ├── API.md
│   ├── SUPPORTED_FORMATS.md
│   └── TESTING.md
│
├── LICENSES/
│   └── INVENTORY.md              # Dependency and license inventory
│
├── AGENTS.md
└── README.md
```

---

## 3. Windows Application

### Technology Stack

| Component | Technology | Responsibility |
|---|---|---|
| UI Framework | React 18 + TypeScript | All user interface |
| Desktop Shell | Tauri 2 | Window management, native APIs, IPC |
| Processing | Rust (shared core) | All file operations |
| Styling | CSS custom properties | Design system tokens |
| Build | Vite | Frontend bundling |

### Communication Boundary

The React frontend communicates with Rust exclusively through **Tauri commands** (`invoke`). No file processing code belongs in TypeScript.

```
React Component
    │
    │ invoke("convert_image", { ... })
    ▼
Tauri Command Handler  (src-tauri/src/commands/)
    │
    │ Calls Rust core functions
    ▼
Shared Core (core/src/)
    │
    │ Returns OperationResult
    ▼
Tauri serializes response to JSON → React updates UI
```

### Tauri Command Categories

| Category | Commands |
|---|---|
| File Management | detect_file, open_file_picker, select_output_dir |
| PDF Operations | merge_pdfs, split_pdf, rotate_pdf, organize_pdf, compress_pdf, pdf_to_images, images_to_pdf |
| Image Operations | convert_image, resize_image |
| Job Management | get_job, cancel_job, get_job_progress, list_jobs |

### Windows Explorer Context-Menu Integration (Core Requirement)

Context-menu integration is a **core Windows release requirement**. It is implemented across two components: the `confi-shell.exe` CLI helper and the main application's IPC listener.

#### Architecture

```
Windows Explorer (right-click)
        │
        │  Shell verb registered in registry
        ▼
  confi-shell.exe  (lightweight CLI helper, <1 MB)
        │
        │  Check: is CONFI main process running?
        ├── Yes: send request via named pipe
        │           └── CONFI receives → opens correct operation with files
        └── No: launch CONFI with deep-link URI argument
                    └── CONFI reads URI → opens correct operation with files
```

#### Shell Verb Registration

Shell verbs are registered **per file type** at the user level (no admin required):

```
HKCU\Software\Classes\.pdf\shell\confi.compress\
    command\  →  "confi-shell.exe" --action compress --files "%1"

HKCU\Software\Classes\.pdf\shell\confi.split\
    command\  →  "confi-shell.exe" --action split --files "%1"
```

For multiple-file selection (Windows passes `%1` only for single selection), the shell verb uses `AppliesTo` with the `MultiSelectModel` set to `Player` or uses a `DelegateExecute` approach. This detail must be validated during M7.

#### Windows 10 vs Windows 11

| Platform | Entry location | Approach |
|---|---|---|
| Windows 10 | Classic context menu (top level) | Registry shell verbs via `HKCU\Software\Classes` |
| Windows 11 | "Show more options" by default | Same shell verbs; appear in the cascaded classic menu |
| Windows 11 (packaged) | Modern context menu (top level) | Requires MSIX or SparsePackage + `PackageExtension` manifest entry |

**Decision for M7:** Evaluate the MSIX/SparsePackage path for Windows 11 top-level placement. If the packaging overhead is acceptable (installer size, signing requirements), implement it. If not, document the "Show more options" limitation clearly and prioritize stability over placement. Do not use unsupported COM workarounds.

#### IPC: Named Pipe

- Pipe name: `\\.\pipe\confi-ipc-{username}`
- Protocol: newline-delimited JSON
- Message format: `{ "action": "compress", "files": ["C:\\path\\to\\file.pdf"] }`
- Timeout: 2 seconds. If CONFI does not respond, fall back to launching a new instance.

#### Deep-Link URI (launch case)

When CONFI is not running, `confi-shell.exe` launches the main app with a URI argument:

```
confi.exe --deep-link "confi://action/compress?files=C%3A%5Cpath%5Cto%5Cfile.pdf"
```

The Tauri app registers a URI handler and parses this on startup.

#### File Path Safety

- All paths are passed as `%1` through the registry verb (shell-quoted by Windows).
- `confi-shell.exe` validates each path exists before sending the request.
- Unicode filenames are handled as UTF-16 by the Windows shell and re-encoded to UTF-8 before transmission.
- Paths exceeding MAX_PATH must be tested; Tauri and Rust handle long paths if the manifest opts in.

#### Settings Toggle

A toggle in CONFI Settings calls a Tauri command that adds or removes the registry entries. No restart required. No admin elevation.

---


## 4. Android Application

### Technology Stack

| Component | Technology | Responsibility |
|---|---|---|
| UI Framework | Jetpack Compose | All user interface |
| Language | Kotlin | Android lifecycle, file access |
| Shared Core | Rust via UniFFI | All file operations |
| Background Work | WorkManager | Long-running processing jobs |
| File Access | Storage Access Framework | Content URIs |

### Android-Specific Rules

- **Never assume a content URI has an accessible filesystem path.** Always copy content to a temporary file before passing to the Rust core.
- Use `WorkManager` for processing that may survive app lifecycle changes.
- Request and respect `persistableUriPermission` grants.
- Handle `EXTRA_STREAM` and `EXTRA_STREAM` arrays in share intents.
- Output files written through a FileProvider; never write directly to arbitrary paths.

### UniFFI Binding

The `confi.udl` interface definition describes all core functions exposed to Kotlin. UniFFI generates the Kotlin glue code. Manual FFI is not used.

```
core/src/ (Rust)
    │
    │  compiled with uniffi_build
    ▼
bindings/uniffi/confi.udl
    │
    │  uniffi-bindgen
    ▼
android/app/src/main/kotlin/confi/uniffi/*.kt  (generated)
```

---

## 5. Shared Rust Core

### Design Principles

1. The core has **no UI dependencies**. It is a pure library crate.
2. All I/O goes through an **input/output abstraction** supporting both filesystem paths and byte streams.
3. **Progress reporting** uses callbacks/channels that the platform layer wires to the UI.
4. Cancellation uses a `CancellationToken` passed into long-running operations.
5. The core **never spawns threads on its own**. Thread management belongs to the host application.

### Module Responsibilities

| Module | Responsibilities |
|---|---|
| `common::errors` | All error types with codes and messages |
| `common::models` | FileFormat, ImageOutputFormat, ResizeDimensions. Job, JobStatus, Progress and OperationResult are defined in `job_manager` alongside their first consumer |
| `common::configuration` | Compression presets, quality defaults, format constraints |
| `file_engine::detection` | Magic-byte analysis, MIME type resolution |
| `file_engine::validation` | Input sanity checks before operations begin |
| `file_engine::metadata` | File size, format, page count (PDFs), dimensions (images) |
| `pdf_engine::merge` | PDF concatenation, page preservation |
| `pdf_engine::split` | Page-range splitting |
| `pdf_engine::rotate` | Per-page rotation |
| `pdf_engine::organize` | Page reordering, deletion, duplication |
| `pdf_engine::compress` | Structural + image optimization of PDFs |
| `pdf_engine::render` | Page thumbnail rasterization (PDFium) |
| `pdf_engine::image_conversion` | PDF-to-image and image-to-PDF workflows |
| `image_engine::conversion` | Format conversion |
| `image_engine::compression` | Lossy/lossless compression |
| `image_engine::resize` | Dimension-based and percentage-based resizing |
| `job_manager::queue` | Job lifecycle management |
| `job_manager::progress` | Progress event emission |
| `job_manager::cancellation` | Cancellation token creation and propagation |

### Input/Output Abstraction

All engine functions accept an `InputSource` enum:

```rust
pub enum InputSource {
    FilePath(PathBuf),
    ByteStream(Vec<u8>),   // For Android content URIs copied to memory
}

pub enum OutputTarget {
    FilePath(PathBuf),
    TempFile { prefix: String },
}
```

---

## 6. Processing Library Strategy

### Evaluation Criteria (applied to every library)

1. License compatibility (MIT/Apache preferred; LGPL needs evaluation; GPL/AGPL requires explicit sign-off)
2. Commercial distribution restrictions
3. Windows x64 support
4. Android (arm64-v8a, x86_64) support
5. Rust integration method (native crate, C FFI wrapper, bindgen)
6. Maintenance activity (last commit date, open issues)
7. Binary size contribution
8. Known security history

### Candidate Libraries

| Library | Purpose | License | Status |
|---|---|---|---|
| `lopdf` | PDF creation and manipulation (pure Rust) | MIT | Evaluate for merge/split/rotate |
| `pdf` crate | PDF reading | MIT/Apache | Evaluate |
| `pdfium-render` | PDF rendering via PDFium | MIT (bindings) + BSD (PDFium) | Evaluate for thumbnails |
| `qpdf` (via FFI) | PDF structural optimization | Apache 2.0 | Evaluate for compression |
| `image` (Rust) | Image encoding/decoding | MIT/Apache | Approve — pure Rust |
| `oxipng` | PNG optimization | MIT | Approve |
| `mozjpeg-sys` | JPEG compression | MPL 2.0 + IJG | Evaluate carefully |
| `libvips` (via FFI) | High-performance image ops | LGPL 2.1+ | Evaluate — LGPL implications |
| `webp` crate | WEBP encode/decode | Apache 2.0 | Evaluate |
| `infer` | Magic-byte file detection | MIT | Approve |

**Decision:** A technical proof of concept (Milestone 1) will validate the PDF library choice before committing to `lopdf` vs. PDFium-based solutions. The full license inventory will be written to `LICENSES/INVENTORY.md` as each dependency is confirmed.

---

## 7. Security Boundaries

| Boundary | Rule |
|---|---|
| Frontend → Core | Only through Tauri invoke commands with explicit allowlist |
| File paths | Sanitized; no path traversal; within allowed directories only |
| Temp files | Written to OS temp directory with unique prefix `confi-tmp-` |
| Input files | Validated before any processing begins |
| Output files | Written to temp, validated, then finalized |
| Android content URIs | Permissions checked; not assumed permanent |

---

## 8. Key Architecture Decisions and Rationale

| Decision | Choice | Rationale |
|---|---|---|
| Desktop framework | Tauri 2 | Smaller binary vs. Electron; native OS APIs; Rust integration |
| Mobile framework | Kotlin + Compose | Native Android; best SAF integration; Compose is current standard |
| Shared language | Rust | Memory safety; no GC pauses; cross-compile to Android targets |
| Android bridge | UniFFI | Reduces hand-written FFI; generates type-safe Kotlin bindings from Rust |
| State management | Zustand (tentative) | Lightweight; no boilerplate; works naturally with Tauri events |
| PDF library | TBD (lopdf vs PDFium) | Requires PoC to evaluate rendering quality and Android cross-compilation |
| Image library | `image` crate | Pure Rust; MIT/Apache; good format coverage; no native library burden |

---

## 9. Future Architecture Considerations

- **Plugin system** for additional format modules (document, audio, video) — design for it but do not implement.
- **Windows ARM64** — Rust cross-compilation supported; requires library evaluation.
- **macOS** — Tauri supports macOS; core is cross-platform; UI would need platform-specific adjustments.
- **Windows IExplorerCommand** — More native context-menu integration for a future release.

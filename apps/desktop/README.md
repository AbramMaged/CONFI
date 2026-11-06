# CONFI — desktop app (Windows)

Tauri 2 host + React/TypeScript frontend. The frontend never touches file bytes:
every operation goes through a Tauri command backed by the shared Rust core.

## Prerequisites

- Rust (MSVC toolchain) — verified with `rustc 1.99.1`
- Node.js 20+ (verified with Node 24) and npm
- WebView2 runtime (ships with Windows 10/11)

## Run (development)

Two processes: the Vite dev server and the Tauri host. `npm run tauri dev` starts
both; if you start them by hand, the dev server must be up first because the debug
build loads `http://localhost:1420` (a release build embeds `dist/` instead).

```powershell
cd apps/desktop
npm install
npm run tauri dev
```

Manual equivalent:

```powershell
npm run dev                                   # terminal 1: Vite on :1420
cd src-tauri; cargo run                       # terminal 2: host (debug, needs Vite)
```

## Build (release, self-contained)

```powershell
cd apps/desktop
npm run build                                 # frontend → dist/
cd src-tauri
cargo build --release                         # embeds dist/, no dev server needed
```

The binary is `src-tauri/target/release/confi-desktop.exe`.

## What this first slice does

- Add PDFs and images; each file is detected by magic bytes and structurally validated.
- Operations, all handled by `confi-core`:
  - Merge PDFs (2+ files)
  - Split a PDF into one file per page
  - Rotate every page of a PDF (90/180/270)
  - Convert an image (png/jpeg/webp/bmp, quality for lossy)
  - Compress an image in place-format
  - Resize an image by percentage or explicit pixels

## Not wired up yet

- PDF rendering/thumbnails (the `render` feature exists in `confi-core`; the app
  does not enable it yet — that needs PDFium next to the executable)
- Drag and drop, batch queue, progress reporting, job cancellation
- Explorer context-menu deep links (M7)

## Tests

```powershell
cd src-tauri
cargo test --lib        # adapter smoke tests: detection rows, error shape
```

Engine behaviour is covered by `core/tests/`.

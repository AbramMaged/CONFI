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

## Build a distributable

Always build through the Tauri CLI. A plain `cargo build --release` **does not embed
the frontend** — `generate_context!` then falls back to `devUrl` and the window shows
`ERR_CONNECTION_REFUSED` for `localhost:1420`. The CLI sets the environment that makes
the build embed `dist/`.

```powershell
cd apps/desktop
npm run tauri build
```

Artifacts (`src-tauri/target/release/`):

| Artifact | Size (v0.1.0) | Use |
|---|---|---|
| `confi-desktop.exe` | 5.87 MB | portable, runs anywhere with WebView2 |
| `bundle/msi/CONFI_0.1.0_x64_en-US.msi` | 2.92 MB | Windows installer |
| `bundle/nsis/CONFI_0.1.0_x64-setup.exe` | 1.97 MB | installer (smaller, recommended) |

The first bundle build downloads WiX and NSIS from GitHub; subsequent builds are offline.

Requirement on the target machine: **WebView2 runtime** (already present on Windows 10/11
with Edge). No .NET, no Rust, no Node needed by the end user.

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

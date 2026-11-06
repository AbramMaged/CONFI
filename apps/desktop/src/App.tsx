import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./App.css";

// ── Types ───────────────────────────────────────────────────────────────────

type ErrorDto = { code: string; message: string };

type FileRow = {
  path: string;
  name: string;
  format: string;
  size_bytes: number;
  is_valid: boolean;
  detail: string | null;
  error: ErrorDto | null;
};

type Operation = "convert_document" | "merge" | "split" | "rotate" | "convert" | "compress" | "resize";
type Screen = "files" | "operations" | "jobs" | "settings";
type ThemeChoice = "light" | "dark" | "system";
type JobStatus = "completed" | "failed" | "running";

type JobRecord = {
  id: number;
  operation: string;
  files: string[];
  output: string;
  status: JobStatus;
  startedAt: number;
  finishedAt?: number;
  message?: string;
  code?: string;
};

const OFFICE_FORMATS = ["word", "powerpoint", "excel"];

const OPERATIONS: { id: Operation; label: string; hint: string; needs: string }[] = [
  { id: "convert_document", label: "Convert document to PDF", hint: "Word, PowerPoint or Excel → PDF", needs: "1 document" },
  { id: "merge", label: "Merge PDFs", hint: "Combine selected PDFs in order", needs: "2 or more PDFs" },
  { id: "split", label: "Split PDF into pages", hint: "One file per page", needs: "1 PDF" },
  { id: "rotate", label: "Rotate PDF pages", hint: "Applies to every page", needs: "1 PDF" },
  { id: "convert", label: "Convert image", hint: "Change format", needs: "1 image" },
  { id: "compress", label: "Compress image", hint: "Keep the format, shrink the file", needs: "1 image" },
  { id: "resize", label: "Resize image", hint: "By percentage or pixels", needs: "1 image" },
];

const IMAGE_FORMATS = ["png", "jpeg", "webp", "bmp"];

// ── Helpers ─────────────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function errorOf(error: unknown): ErrorDto {
  if (error && typeof error === "object" && "message" in error) {
    const e = error as Partial<ErrorDto>;
    return { code: e.code ?? "PROCESSING_ERROR", message: e.message ?? String(error) };
  }
  return { code: "PROCESSING_ERROR", message: String(error) };
}

function formatOf(file: FileRow): string {
  return file.format === "jpeg" ? "JPG" : file.format.toUpperCase();
}

/** One or two letters for the type badge. */
function badgeLetter(file: FileRow): string {
  if (file.format === "pdf") return "PDF";
  if (file.format === "word") return "DOC";
  if (file.format === "powerpoint") return "PPT";
  if (file.format === "excel") return "XLS";
  return "IMG";
}

function applyTheme(choice: ThemeChoice) {
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const resolved = choice === "system" ? (systemDark ? "dark" : "light") : choice;
  document.documentElement.dataset.theme = resolved;
}

// ── App ─────────────────────────────────────────────────────────────────────

export default function App() {
  const [screen, setScreen] = useState<Screen>("files");
  const [files, setFiles] = useState<FileRow[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [dragging, setDragging] = useState(false);

  const [operation, setOperation] = useState<Operation>("convert_document");
  const [output, setOutput] = useState("");
  const [quality, setQuality] = useState(80);
  const [format, setFormat] = useState("png");
  const [degrees, setDegrees] = useState(90);
  const [resizeMode, setResizeMode] = useState("percentage");
  const [width, setWidth] = useState(800);
  const [height, setHeight] = useState(600);
  const [percentage, setPercentage] = useState(50);

  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<{ kind: "ok" | "error"; text: string; code?: string } | null>(null);
  const [jobs, setJobs] = useState<JobRecord[]>([]);

  const [theme, setTheme] = useState<ThemeChoice>(
    () => (localStorage.getItem("confi.theme") as ThemeChoice | null) ?? "system",
  );
  const [defaultOutputDir, setDefaultOutputDir] = useState<string>(
    () => localStorage.getItem("confi.defaultOutputDir") ?? "",
  );

  // Theme: apply + persist + follow the system when "system" is chosen.
  useEffect(() => {
    localStorage.setItem("confi.theme", theme);
    applyTheme(theme);
    if (theme !== "system") return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const listener = () => applyTheme("system");
    media.addEventListener("change", listener);
    return () => media.removeEventListener("change", listener);
  }, [theme]);

  const validFiles = files.filter((f) => f.is_valid);
  const pdfs = validFiles.filter((f) => f.format === "pdf");
  const documents = validFiles.filter((f) => OFFICE_FORMATS.includes(f.format));
  const images = validFiles.filter((f) => f.format !== "pdf" && !OFFICE_FORMATS.includes(f.format));
  const selectedFiles = files.filter((f) => selected.has(f.path));

  const addPaths = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    const rows = await invoke<FileRow[]>("detect_files", { paths });
    setFiles((previous) => {
      const merged = [...previous];
      for (const row of rows) {
        if (!merged.some((existing) => existing.path === row.path)) merged.push(row);
      }
      return merged;
    });
    setSelected((previous) => {
      const next = new Set(previous);
      for (const row of rows) if (row.is_valid) next.add(row.path);
      return next;
    });
  }, []);

  const addFiles = useCallback(async () => {
    const picked = await open({
      multiple: true,
      title: "Select files",
      filters: [
        {
          name: "PDF, images and documents",
          extensions: ["pdf", "jpg", "jpeg", "png", "webp", "bmp", "doc", "docx", "rtf", "odt", "txt", "ppt", "pptx", "odp", "xls", "xlsx", "ods", "csv"],
        },
        { name: "Documents", extensions: ["doc", "docx", "rtf", "odt", "txt", "ppt", "pptx", "odp", "xls", "xlsx", "ods", "csv"] },
        { name: "PDF", extensions: ["pdf"] },
        { name: "Images", extensions: ["jpg", "jpeg", "png", "webp", "bmp"] },
      ],
    });
    if (!picked) return;
    await addPaths(Array.isArray(picked) ? picked : [picked]);
  }, [addPaths]);

  // Native file drop from Explorer.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") setDragging(true);
        else if (event.payload.type === "leave") setDragging(false);
        else if (event.payload.type === "drop") {
          setDragging(false);
          void addPaths(event.payload.paths);
        }
      })
      .then((stop) => {
        unlisten = stop;
      });
    return () => unlisten?.();
  }, [addPaths]);

  function toggle(path: string) {
    setSelected((previous) => {
      const next = new Set(previous);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  function removeFile(path: string) {
    setFiles((previous) => previous.filter((f) => f.path !== path));
    setSelected((previous) => {
      const next = new Set(previous);
      next.delete(path);
      return next;
    });
  }

  function selectAll() {
    setSelected(new Set(files.filter((f) => f.is_valid).map((f) => f.path)));
  }

  function clearSelection() {
    setSelected(new Set());
  }

  function clearAll() {
    setFiles([]);
    setSelected(new Set());
    setOutput("");
    setStatus(null);
  }

  const current = OPERATIONS.find((o) => o.id === operation)!;

  function suggestedName(): string {
    const first = validFiles[0];
    const stem = first ? first.name.replace(/\.[^.]+$/, "") : "output";
    switch (operation) {
      case "convert_document":
        return `${stem}.pdf`;
      case "merge":
        return "merged.pdf";
      case "rotate":
        return `${stem}-rotated.pdf`;
      case "convert":
        return `${stem}.${format === "jpeg" ? "jpg" : format}`;
      case "compress":
        return `${stem}-compressed`;
      case "resize":
        return `${stem}-resized.png`;
      default:
        return stem;
    }
  }

  async function chooseOutput() {
    if (operation === "split") {
      const dir = await open({ directory: true, title: "Choose output folder" });
      if (dir) setOutput(String(dir));
      return;
    }
    const base = defaultOutputDir ? `${defaultOutputDir}\\${suggestedName()}` : suggestedName();
    const path = await save({ title: "Save output as", defaultPath: base });
    if (path) setOutput(path);
  }

  async function chooseDefaultDir() {
    const dir = await open({ directory: true, title: "Choose default output folder" });
    if (!dir) return;
    setDefaultOutputDir(String(dir));
    localStorage.setItem("confi.defaultOutputDir", String(dir));
  }

  const canRun =
    !busy &&
    output !== "" &&
    ((operation === "convert_document" && documents.length === 1) ||
      (operation === "merge" && pdfs.length >= 2) ||
      (operation === "split" && pdfs.length === 1) ||
      (operation === "rotate" && pdfs.length === 1) ||
      (["convert", "compress", "resize"].includes(operation) && images.length === 1));

  async function run() {
    setBusy(true);
    setStatus(null);
    const jobId = Date.now();
    const record: JobRecord = {
      id: jobId,
      operation: current.label,
      files: validFiles.map((f) => f.name),
      output,
      status: "running",
      startedAt: Date.now(),
    };
    setJobs((previous) => [record, ...previous]);

    const finish = (patch: Partial<JobRecord>) =>
      setJobs((previous) => previous.map((job) => (job.id === jobId ? { ...job, ...patch } : job)));

    try {
      let summary: string;
      switch (operation) {
        case "convert_document":
          summary = await invoke("convert_document", { input: documents[0].path, output });
          break;
        case "merge":
          summary = await invoke("merge_pdfs", { inputs: pdfs.map((f) => f.path), output });
          break;
        case "split":
          summary = await invoke("split_pdf", { input: pdfs[0].path, dir: output });
          break;
        case "rotate":
          summary = await invoke("rotate_pdf", { input: pdfs[0].path, degrees, output });
          break;
        case "convert":
          summary = await invoke("convert_image", {
            input: images[0].path,
            format,
            quality: format === "png" || format === "bmp" ? null : quality,
            output,
          });
          break;
        case "compress":
          summary = await invoke("compress_image", { input: images[0].path, quality, output });
          break;
        case "resize":
          summary = await invoke("resize_image", {
            input: images[0].path,
            mode: resizeMode,
            width: resizeMode === "pixels" ? width : null,
            height: resizeMode === "pixels" ? height : null,
            percentage: resizeMode === "percentage" ? percentage : null,
            output,
          });
          break;
      }
      setStatus({ kind: "ok", text: summary });
      finish({ status: "completed", finishedAt: Date.now(), message: summary });
    } catch (error) {
      const e = errorOf(error);
      setStatus({ kind: "error", text: e.message, code: e.code });
      finish({ status: "failed", finishedAt: Date.now(), message: e.message, code: e.code });
    } finally {
      setBusy(false);
    }
  }

  const nav: { id: Screen; label: string; icon: string }[] = useMemo(
    () => [
      { id: "files", label: "Files", icon: "▤" },
      { id: "operations", label: "Operations", icon: "◈" },
      { id: "jobs", label: "Job Manager", icon: "◷" },
      { id: "settings", label: "Settings", icon: "⚙" },
    ],
    [],
  );

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">◆</span>
          <span className="brand-name">CONFI</span>
        </div>

        <nav className="nav">
          {nav.map((item) => (
            <button
              key={item.id}
              className={screen === item.id ? "nav-item nav-item--active" : "nav-item"}
              onClick={() => setScreen(item.id)}
            >
              <span className="nav-icon" aria-hidden="true">{item.icon}</span>
              {item.label}
              {item.id === "files" && files.length > 0 && <span className="nav-count">{files.length}</span>}
            </button>
          ))}
        </nav>

        <div className="sidebar-footer">
          <span>v0.1.0</span>
          <span className="dot" />
          <span>Offline</span>
        </div>
      </aside>

      <main className="main">
        {screen === "files" && (
          <>
            <header className="page-head">
              <div>
                <h1>Files</h1>
                <p className="subtitle">
                  {files.length === 0
                    ? "Nothing loaded yet"
                    : `${files.length} loaded · ${selectedFiles.length} selected`}
                </p>
              </div>
              <div className="head-actions">
                <button className="btn" onClick={addFiles}>Add files</button>
                <button className="btn btn--quiet" onClick={clearSelection} disabled={selected.size === 0}>
                  Clear selection
                </button>
                <button className="btn btn--quiet" onClick={clearAll} disabled={files.length === 0 || busy}>
                  Remove all
                </button>
              </div>
            </header>

            {files.length === 0 ? (
              <div className={dragging ? "empty empty--dragging" : "empty"}>
                <div className="empty-icon" aria-hidden="true">⇪</div>
                <h2>Drop files here</h2>
                <p>PDFs, images, Word, PowerPoint and Excel documents. Everything stays on this machine.</p>
                <button className="btn btn--primary" onClick={addFiles}>Add files</button>
              </div>
            ) : (
              <>
                <div className="list-toolbar">
                  <label className="checkline">
                    <input
                      type="checkbox"
                      checked={selected.size > 0 && selected.size === validFiles.length}
                      onChange={(e) => (e.target.checked ? selectAll() : clearSelection())}
                      aria-label="Select all files"
                    />
                    Select all
                  </label>
                  <span className="toolbar-note">{validFiles.length} usable</span>
                </div>

                <div className={dragging ? "file-list file-list--dragging" : "file-list"}>
                  {files.map((file) => (
                    <div className={selected.has(file.path) ? "file-row file-row--selected" : "file-row"} key={file.path}>
                      <input
                        type="checkbox"
                        checked={selected.has(file.path)}
                        onChange={() => toggle(file.path)}
                        aria-label={`Select ${file.name}`}
                      />
                      <span className={`badge badge--${file.format}`}>{badgeLetter(file)}</span>
                      <div className="file-main">
                        <span className="file-name" title={file.path}>{file.name}</span>
                        <span className="file-meta">
                          {formatOf(file)}
                          {" · "}
                          {formatBytes(file.size_bytes)}
                          {file.detail ? ` · ${file.detail}` : ""}
                          {!file.is_valid && file.error ? ` · ${file.error.code}` : ""}
                        </span>
                      </div>
                      <span className={file.is_valid ? "state state--ok" : "state state--bad"}>
                        {file.is_valid ? "ready" : "unusable"}
                      </span>
                      <button className="icon-btn" onClick={() => removeFile(file.path)} title="Remove" aria-label={`Remove ${file.name}`}>
                        ✕
                      </button>
                    </div>
                  ))}
                </div>
              </>
            )}
          </>
        )}

        {screen === "operations" && (
          <>
            <header className="page-head">
              <div>
                <h1>Operations</h1>
                <p className="subtitle">
                  {selectedFiles.length > 0
                    ? `${selectedFiles.length} selected file${selectedFiles.length === 1 ? "" : "s"}`
                    : "Select files on the Files screen first"}
                </p>
              </div>
            </header>

            <section className="card">
              <h2 className="card-title">Operation</h2>
              <div className="op-grid">
                {OPERATIONS.map((op) => (
                  <button
                    key={op.id}
                    className={operation === op.id ? "op-tile op-tile--active" : "op-tile"}
                    onClick={() => {
                      setOperation(op.id);
                      setOutput("");
                      setStatus(null);
                    }}
                    disabled={busy}
                  >
                    <span className="op-label">{op.label}</span>
                    <span className="op-hint">{op.hint}</span>
                  </button>
                ))}
              </div>
              <p className="field-hint">Selected operation needs {current.needs}.</p>
            </section>

            <section className="card">
              <h2 className="card-title">Settings for {current.label}</h2>

              {operation === "convert" && (
                <div className="field">
                  <label htmlFor="format">Output format</label>
                  <select id="format" value={format} onChange={(e) => setFormat(e.target.value)} disabled={busy}>
                    {IMAGE_FORMATS.map((f) => <option key={f} value={f}>{f}</option>)}
                  </select>
                </div>
              )}

              {(operation === "convert" && (format === "jpeg" || format === "webp")) && (
                <div className="field">
                  <label htmlFor="q-convert">Quality</label>
                  <input id="q-convert" type="range" min={1} max={100} value={quality}
                    onChange={(e) => setQuality(Number(e.target.value))} disabled={busy} />
                  <span className="value">{quality}</span>
                </div>
              )}

              {operation === "compress" && (
                <div className="field">
                  <label htmlFor="q-compress">Quality</label>
                  <input id="q-compress" type="range" min={1} max={100} value={quality}
                    onChange={(e) => setQuality(Number(e.target.value))} disabled={busy} />
                  <span className="value">{quality}</span>
                </div>
              )}

              {operation === "rotate" && (
                <div className="field">
                  <label htmlFor="degrees">Rotation</label>
                  <select id="degrees" value={degrees} onChange={(e) => setDegrees(Number(e.target.value))} disabled={busy}>
                    <option value={90}>90° clockwise</option>
                    <option value={180}>180°</option>
                    <option value={270}>270° clockwise</option>
                  </select>
                </div>
              )}

              {operation === "resize" && (
                <>
                  <div className="field">
                    <label htmlFor="resize-mode">Resize by</label>
                    <select id="resize-mode" value={resizeMode} onChange={(e) => setResizeMode(e.target.value)} disabled={busy}>
                      <option value="percentage">Percentage</option>
                      <option value="pixels">Pixels</option>
                    </select>
                  </div>
                  {resizeMode === "percentage" ? (
                    <div className="field">
                      <label htmlFor="pct">Percentage</label>
                      <input id="pct" type="number" min={1} max={1000} value={percentage}
                        onChange={(e) => setPercentage(Number(e.target.value))} disabled={busy} />
                      <span className="field-hint">1–1000%</span>
                    </div>
                  ) : (
                    <div className="field">
                      <label>Size</label>
                      <input type="number" min={1} value={width} onChange={(e) => setWidth(Number(e.target.value))}
                        disabled={busy} aria-label="Width in pixels" />
                      <span className="field-hint">×</span>
                      <input type="number" min={1} value={height} onChange={(e) => setHeight(Number(e.target.value))}
                        disabled={busy} aria-label="Height in pixels" />
                      <span className="field-hint">px</span>
                    </div>
                  )}
                </>
              )}

              {operation === "merge" && (
                <p className="field-hint">
                  Pages are merged in list order. Reorder by removing and re-adding files.
                </p>
              )}

              <div className="field field--output">
                <label>{operation === "split" ? "Output folder" : "Save as"}</label>
                <input className="path" value={output} readOnly
                  placeholder={operation === "split" ? "Choose a folder" : "Choose a destination"} />
                <button className="btn btn--quiet" onClick={chooseOutput} disabled={busy}>Choose</button>
              </div>

              <div className="run-row">
                <button className="btn btn--primary" onClick={run} disabled={!canRun}>
                  {busy ? "Working…" : "Run"}
                </button>
                {status && (
                  <p className={status.kind === "ok" ? "status status--ok" : "status status--error"}>
                    {status.code && <span className="status-code">{status.code}</span>}
                    {status.text}
                  </p>
                )}
              </div>
            </section>
          </>
        )}

        {screen === "jobs" && (
          <>
            <header className="page-head">
              <div>
                <h1>Job Manager</h1>
                <p className="subtitle">Operations run one at a time, in this session.</p>
              </div>
              <div className="head-actions">
                <button className="btn btn--quiet" onClick={() => setJobs([])} disabled={jobs.length === 0}>
                  Clear history
                </button>
              </div>
            </header>

            {jobs.length === 0 ? (
              <div className="empty empty--small">
                <div className="empty-icon" aria-hidden="true">◷</div>
                <h2>No jobs yet</h2>
                <p>Run an operation and it will appear here with its result.</p>
              </div>
            ) : (
              <div className="file-list">
                {jobs.map((job) => (
                  <div className="job-row" key={job.id}>
                    <span className={`state ${job.status === "completed" ? "state--ok" : job.status === "failed" ? "state--bad" : "state--busy"}`}>
                      {job.status}
                    </span>
                    <div className="file-main">
                      <span className="file-name">{job.operation}</span>
                      <span className="file-meta">
                        {job.files.join(", ")} → {job.output}
                      </span>
                      {job.message && <span className="file-meta">{job.message}</span>}
                    </div>
                    <span className="file-meta">
                      {new Date(job.startedAt).toLocaleTimeString()}
                      {job.finishedAt ? ` · ${((job.finishedAt - job.startedAt) / 1000).toFixed(1)}s` : ""}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </>
        )}

        {screen === "settings" && (
          <>
            <header className="page-head">
              <div>
                <h1>Settings</h1>
                <p className="subtitle">Preferences are stored on this machine only.</p>
              </div>
            </header>

            <section className="card">
              <h2 className="card-title">Appearance</h2>
              <div className="field">
                <label>Theme</label>
                <div className="segmented">
                  {(["light", "dark", "system"] as ThemeChoice[]).map((option) => (
                    <button
                      key={option}
                      className={theme === option ? "segment segment--active" : "segment"}
                      onClick={() => setTheme(option)}
                    >
                      {option === "system" ? "System default" : option === "dark" ? "Dark" : "Light"}
                    </button>
                  ))}
                </div>
              </div>
            </section>

            <section className="card">
              <h2 className="card-title">General</h2>
              <div className="field">
                <label>Default output folder</label>
                <input className="path" value={defaultOutputDir} readOnly placeholder="Same folder as the input" />
                <button className="btn btn--quiet" onClick={chooseDefaultDir}>Choose</button>
              </div>
              <p className="field-hint">Used as the starting location when saving results.</p>
            </section>

            <section className="card">
              <h2 className="card-title">About</h2>
              <div className="about">
                <p><strong>CONFI</strong> v0.1.0 — Every file. One place.</p>
                <p className="field-hint">
                  Offline-first: no network calls, no accounts, files never leave this machine.
                </p>
                <p className="field-hint">
                  Document conversion uses the Microsoft Office installed on this PC.
                </p>
              </div>
            </section>
          </>
        )}
      </main>
    </div>
  );
}

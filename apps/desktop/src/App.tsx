import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./App.css";

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

const OFFICE_FORMATS = ["word", "powerpoint", "excel"];

const OPERATIONS: { id: Operation; label: string; needs: string }[] = [
  { id: "convert_document", label: "Convert document to PDF", needs: "1 Word, PowerPoint or Excel file" },
  { id: "merge", label: "Merge PDFs", needs: "2 or more PDFs" },
  { id: "split", label: "Split PDF into pages", needs: "1 PDF" },
  { id: "rotate", label: "Rotate PDF pages", needs: "1 PDF" },
  { id: "convert", label: "Convert image", needs: "1 image" },
  { id: "compress", label: "Compress image", needs: "1 image" },
  { id: "resize", label: "Resize image", needs: "1 image" },
];

const IMAGE_FORMATS = ["png", "jpeg", "webp", "bmp"];

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

export default function App() {
  const [files, setFiles] = useState<FileRow[]>([]);
  const [operation, setOperation] = useState<Operation>("merge");
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

  const pdfs = files.filter((f) => f.format === "pdf" && f.is_valid);
  const images = files.filter((f) => f.format !== "pdf" && !OFFICE_FORMATS.includes(f.format) && f.is_valid);
  const documents = files.filter((f) => OFFICE_FORMATS.includes(f.format) && f.is_valid);

  async function addFiles() {
    const selected = await open({
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
    if (!selected) return;

    const paths = Array.isArray(selected) ? selected : [selected];
    const rows = await invoke<FileRow[]>("detect_files", { paths });

    setFiles((previous) => {
      const merged = [...previous];
      for (const row of rows) {
        if (!merged.some((existing) => existing.path === row.path)) merged.push(row);
      }
      return merged;
    });
  }

  function clearFiles() {
    setFiles([]);
    setOutput("");
    setStatus(null);
  }

  function suggestedName(): string {
    const first = files[0];
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
    const path = await save({ title: "Save output as", defaultPath: suggestedName() });
    if (path) setOutput(path);
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
    } catch (error) {
      const e = errorOf(error);
      setStatus({ kind: "error", text: e.message, code: e.code });
    } finally {
      setBusy(false);
    }
  }

  const current = OPERATIONS.find((o) => o.id === operation)!;

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">CONFI</div>
        <nav>
          <div className="nav-item nav-item--active">Files</div>
        </nav>
        <div className="sidebar-foot">v0.1.0</div>
      </aside>

      <main className="main">
        <header className="page-head">
          <h1>Files</h1>
          <span className="meta">
            {files.length} file{files.length === 1 ? "" : "s"} selected
          </span>
        </header>

        <section className="toolbar">
          <button className="btn" onClick={addFiles}>
            Add files
          </button>
          <button className="btn btn--quiet" onClick={clearFiles} disabled={files.length === 0 || busy}>
            Clear
          </button>
        </section>

        <section className="file-list">
          {files.length === 0 ? (
            <p className="empty">No files yet. Add PDFs or images to begin.</p>
          ) : (
            files.map((file) => (
              <div className="file-row" key={file.path}>
                <span className="file-name" title={file.path}>
                  {file.name}
                </span>
                <span className="file-format">{file.format}</span>
                <span className="file-detail">
                  {file.is_valid ? file.detail ?? "—" : file.error?.code ?? "invalid"}
                </span>
                <span className="file-size">{formatBytes(file.size_bytes)}</span>
                <span className={file.is_valid ? "file-state" : "file-state file-state--bad"}>
                  {file.is_valid ? "ready" : "unusable"}
                </span>
              </div>
            ))
          )}
        </section>

        <section className="panel">
          <div className="panel-row">
            <label htmlFor="operation">Operation</label>
            <div className="panel-control">
              <select
                id="operation"
                value={operation}
                onChange={(e) => {
                  setOperation(e.target.value as Operation);
                  setOutput("");
                  setStatus(null);
                }}
                disabled={busy}
              >
                {OPERATIONS.map((op) => (
                  <option key={op.id} value={op.id}>
                    {op.label}
                  </option>
                ))}
              </select>
              <span className="hint">needs {current.needs}</span>
            </div>
          </div>

          {operation === "convert" && (
            <>
              <div className="panel-row">
                <label htmlFor="format">Output format</label>
                <div className="panel-control">
                  <select id="format" value={format} onChange={(e) => setFormat(e.target.value)} disabled={busy}>
                    {IMAGE_FORMATS.map((f) => (
                      <option key={f} value={f}>
                        {f}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
              {(format === "jpeg" || format === "webp") && (
                <div className="panel-row">
                  <label htmlFor="quality-convert">Quality</label>
                  <div className="panel-control">
                    <input
                      id="quality-convert"
                      type="range"
                      min={1}
                      max={100}
                      value={quality}
                      onChange={(e) => setQuality(Number(e.target.value))}
                      disabled={busy}
                    />
                    <span className="value">{quality}</span>
                  </div>
                </div>
              )}
            </>
          )}

          {operation === "compress" && (
            <div className="panel-row">
              <label htmlFor="quality-compress">Quality</label>
              <div className="panel-control">
                <input
                  id="quality-compress"
                  type="range"
                  min={1}
                  max={100}
                  value={quality}
                  onChange={(e) => setQuality(Number(e.target.value))}
                  disabled={busy}
                />
                <span className="value">{quality}</span>
              </div>
            </div>
          )}

          {operation === "rotate" && (
            <div className="panel-row">
              <label htmlFor="degrees">Rotation</label>
              <div className="panel-control">
                <select id="degrees" value={degrees} onChange={(e) => setDegrees(Number(e.target.value))} disabled={busy}>
                  <option value={90}>90° clockwise</option>
                  <option value={180}>180°</option>
                  <option value={270}>270° clockwise</option>
                </select>
                <span className="hint">applies to every page</span>
              </div>
            </div>
          )}

          {operation === "resize" && (
            <>
              <div className="panel-row">
                <label htmlFor="resize-mode">Resize by</label>
                <div className="panel-control">
                  <select
                    id="resize-mode"
                    value={resizeMode}
                    onChange={(e) => setResizeMode(e.target.value)}
                    disabled={busy}
                  >
                    <option value="percentage">Percentage</option>
                    <option value="pixels">Pixels</option>
                  </select>
                </div>
              </div>
              {resizeMode === "percentage" ? (
                <div className="panel-row">
                  <label htmlFor="percentage">Percentage</label>
                  <div className="panel-control">
                    <input
                      id="percentage"
                      type="number"
                      min={1}
                      max={1000}
                      value={percentage}
                      onChange={(e) => setPercentage(Number(e.target.value))}
                      disabled={busy}
                    />
                  </div>
                </div>
              ) : (
                <div className="panel-row">
                  <label>Size</label>
                  <div className="panel-control">
                    <input
                      type="number"
                      min={1}
                      value={width}
                      onChange={(e) => setWidth(Number(e.target.value))}
                      disabled={busy}
                      aria-label="Width in pixels"
                    />
                    <span className="hint">×</span>
                    <input
                      type="number"
                      min={1}
                      value={height}
                      onChange={(e) => setHeight(Number(e.target.value))}
                      disabled={busy}
                      aria-label="Height in pixels"
                    />
                    <span className="hint">px</span>
                  </div>
                </div>
              )}
            </>
          )}

          <div className="panel-row">
            <label>{operation === "split" ? "Output folder" : "Save as"}</label>
            <div className="panel-control">
              <input
                className="path"
                value={output}
                readOnly
                placeholder={operation === "split" ? "Choose a folder" : "Choose a destination"}
              />
              <button className="btn btn--quiet" onClick={chooseOutput} disabled={busy}>
                Choose
              </button>
            </div>
          </div>

          <div className="panel-row panel-row--actions">
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
      </main>
    </div>
  );
}

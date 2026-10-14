import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  storageApi,
  type Drive,
  type DuplicateGroup,
  type LargeFile,
  type Level,
  type StorageProgress,
} from "../api";
import { formatBytes, formatCount, formatDate, plural } from "../format";
import { squarify } from "../treemap";

type Tab = "map" | "large" | "duplicates";

const TILE_COLORS = ["#2563eb", "#7c3aed", "#0891b2", "#059669", "#d97706", "#db2777", "#4f46e5", "#65a30d"];

export default function Storage() {
  const [tab, setTab] = useState<Tab>("map");
  // Whether a folder analysis exists, so the large-files tab knows to load.
  const [analyzedAt, setAnalyzedAt] = useState(0);

  return (
    <div className="page">
      <header className="page-header">
        <h1>Storage</h1>
        <div className="tabs">
          <button className={tab === "map" ? "active" : ""} onClick={() => setTab("map")}>
            Space map
          </button>
          <button className={tab === "large" ? "active" : ""} onClick={() => setTab("large")}>
            Large files
          </button>
          <button className={tab === "duplicates" ? "active" : ""} onClick={() => setTab("duplicates")}>
            Duplicates
          </button>
        </div>
      </header>
      {/* Kept mounted so switching tabs doesn't lose results. */}
      <div hidden={tab !== "map"} className="tab-body">
        <SpaceMap onAnalyzed={() => setAnalyzedAt(Date.now())} />
      </div>
      <div hidden={tab !== "large"} className="tab-body">
        <LargeFiles analyzedAt={analyzedAt} goToMap={() => setTab("map")} />
      </div>
      <div hidden={tab !== "duplicates"} className="tab-body">
        <Duplicates />
      </div>
    </div>
  );
}

function useStorageProgress(active: boolean) {
  const [progress, setProgress] = useState<StorageProgress>({ files: 0, bytes: 0 });
  useEffect(() => {
    if (!active) return;
    setProgress({ files: 0, bytes: 0 });
    const unlisten = storageApi.onProgress(setProgress);
    return () => {
      unlisten.then((f) => f());
    };
  }, [active]);
  return progress;
}

function Working({ label, active }: { label: string; active: boolean }) {
  const progress = useStorageProgress(active);
  return (
    <div className="empty">
      <p>{label}</p>
      <p className="muted">
        {formatCount(progress.files)} files · {formatBytes(progress.bytes)}
      </p>
      <progress />
      <button className="secondary" onClick={() => storageApi.cancel()}>
        Cancel
      </button>
    </div>
  );
}

async function pickFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}

function SpaceMap({ onAnalyzed }: { onAnalyzed: () => void }) {
  const [drives, setDrives] = useState<Drive[]>([]);
  const [busy, setBusy] = useState(false);
  const [level, setLevel] = useState<Level | null>(null);
  const [segments, setSegments] = useState<string[]>([]);
  const [root, setRoot] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    storageApi
      .listDrives()
      .then(setDrives)
      .catch((e) => setError(String(e)));
  }, []);

  async function analyze(path: string) {
    setError(null);
    setBusy(true);
    try {
      const result = await storageApi.analyze(path);
      setRoot(path);
      setSegments([]);
      setLevel(result);
      onAnalyzed();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function goTo(next: string[]) {
    try {
      setLevel(await storageApi.level(next));
      setSegments(next);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <>
      <div className="drives">
        {drives.map((d) => {
          const used = d.totalBytes - d.freeBytes;
          return (
            <button
              key={d.mountPoint}
              className="drive"
              disabled={busy}
              onClick={() => analyze(d.mountPoint)}
            >
              <strong>{d.mountPoint}</strong> <span className="muted">{d.name}</span>
              <div className="meter">
                <div style={{ width: `${(used / d.totalBytes) * 100}%` }} />
              </div>
              <span className="muted">
                {formatBytes(d.freeBytes)} free of {formatBytes(d.totalBytes)}
              </span>
            </button>
          );
        })}
        <button
          className="drive choose"
          disabled={busy}
          onClick={async () => {
            const folder = await pickFolder();
            if (folder) analyze(folder);
          }}
        >
          Choose a folder...
        </button>
      </div>
      {error && <div className="banner error">{error}</div>}

      <section className="results">
        {busy && <Working label="Measuring folders..." active={busy} />}
        {!busy && !level && (
          <div className="empty">
            <p>Pick a drive or folder to see what takes up space.</p>
            <p className="muted">Analyzing a whole drive can take a few minutes.</p>
          </div>
        )}
        {!busy && level && (
          <>
            <div className="crumbs">
              <button className="link" onClick={() => goTo([])}>
                {root}
              </button>
              {segments.map((s, i) => (
                <span key={i}>
                  {" "}
                  ›{" "}
                  <button className="link" onClick={() => goTo(segments.slice(0, i + 1))}>
                    {s}
                  </button>
                </span>
              ))}
              <span className="muted">
                {" "}
                · {formatBytes(level.size)} in {plural(level.fileCount, "file", "files")}
              </span>
              <button className="link right" onClick={() => storageApi.reveal(level.path)}>
                Show in Explorer
              </button>
            </div>
            <Treemap level={level} onOpen={(name) => goTo([...segments, name])} />
          </>
        )}
      </section>
    </>
  );
}

function Treemap({ level, onOpen }: { level: Level; onOpen: (name: string) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const rects = useMemo(
    () =>
      squarify(
        level.entries.map((e) => e.size),
        { x: 0, y: 0, w: size.w, h: size.h },
      ),
    [level, size],
  );

  return (
    <div className="map-layout">
      <div className="treemap" ref={ref}>
        {level.entries.map((e, i) => {
          const r = rects[i];
          if (!r) return null;
          const clickable = e.isDir && e.hasChildren;
          return (
            <div
              key={e.name}
              className={`tile ${clickable ? "clickable" : ""} ${e.isDir ? "" : "loose"}`}
              style={{
                left: r.x,
                top: r.y,
                width: r.w,
                height: r.h,
                background: e.isDir ? TILE_COLORS[i % TILE_COLORS.length] : undefined,
              }}
              title={`${e.name}: ${formatBytes(e.size)}`}
              onClick={() => clickable && onOpen(e.name)}
            >
              {r.w > 70 && r.h > 34 && (
                <>
                  <span className="tile-name">{e.name}</span>
                  <span className="tile-size">{formatBytes(e.size)}</span>
                </>
              )}
            </div>
          );
        })}
      </div>
      <ul className="level-list">
        {level.entries.map((e, i) => (
          <li
            key={e.name}
            className={e.isDir && e.hasChildren ? "clickable" : ""}
            onClick={() => e.isDir && e.hasChildren && onOpen(e.name)}
          >
            <span
              className="swatch"
              style={{ background: e.isDir ? TILE_COLORS[i % TILE_COLORS.length] : "var(--border)" }}
            />
            <span className="name">
              {e.name}
              {e.incomplete && <span className="tag">Partly unreadable</span>}
            </span>
            <span className="num">{formatBytes(e.size)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function ConfirmRecycle({
  count,
  bytes,
  onCancel,
  onConfirm,
}: {
  count: number;
  bytes: number;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="overlay">
      <div className="dialog">
        <h2>Move to the Recycle Bin?</h2>
        <p>
          {plural(count, "file", "files")} ({formatBytes(bytes)}) will be moved to the Recycle Bin. You can
          restore them from there until you empty it.
        </p>
        <div className="actions">
          <button className="secondary" onClick={onCancel}>
            Cancel
          </button>
          <button className="primary" onClick={onConfirm}>
            Move to Recycle Bin
          </button>
        </div>
      </div>
    </div>
  );
}

function LargeFiles({ analyzedAt, goToMap }: { analyzedAt: number; goToMap: () => void }) {
  const [files, setFiles] = useState<LargeFile[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (analyzedAt) storageApi.largestFiles().then(setFiles);
    setSelected(new Set());
  }, [analyzedAt]);

  const selectedBytes = files.filter((f) => selected.has(f.path)).reduce((n, f) => n + f.size, 0);

  async function recycle() {
    setConfirming(false);
    try {
      const report = await storageApi.recycleLargeFiles([...selected]);
      setMessage(
        `Moved ${plural(report.moved.length, "file", "files")} to the Recycle Bin.` +
          (report.failures.length
            ? ` ${plural(report.failures.length, "file", "files")} could not be moved.`
            : ""),
      );
      setFiles(await storageApi.largestFiles());
      setSelected(new Set());
    } catch (e) {
      setMessage(String(e));
    }
  }

  if (!analyzedAt) {
    return (
      <section className="results">
        <div className="empty">
          <p>Analyze a drive or folder in the Space map first.</p>
          <button className="secondary" onClick={goToMap}>
            Go to Space map
          </button>
        </div>
      </section>
    );
  }

  return (
    <>
      <div className="toolbar">
        <span className="muted">The {files.length} largest files from the last analysis.</span>
        <button className="primary" disabled={selected.size === 0} onClick={() => setConfirming(true)}>
          Move {selected.size || ""} to Recycle Bin
        </button>
      </div>
      {message && <div className="banner">{message}</div>}
      <section className="results">
        <table>
          <thead>
            <tr>
              <th />
              <th>File</th>
              <th className="num">Modified</th>
              <th className="num">Size</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {files.map((f) => (
              <tr key={f.path}>
                <td>
                  <input
                    type="checkbox"
                    checked={selected.has(f.path)}
                    onChange={() => {
                      const next = new Set(selected);
                      if (next.has(f.path)) next.delete(f.path);
                      else next.add(f.path);
                      setSelected(next);
                    }}
                  />
                </td>
                <td className="path" title={f.path}>
                  {f.path}
                </td>
                <td className="num muted">{formatDate(f.modified)}</td>
                <td className="num">{formatBytes(f.size)}</td>
                <td className="num">
                  <button className="link" onClick={() => storageApi.reveal(f.path)}>
                    Show
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
      {confirming && (
        <ConfirmRecycle
          count={selected.size}
          bytes={selectedBytes}
          onCancel={() => setConfirming(false)}
          onConfirm={recycle}
        />
      )}
    </>
  );
}

const MIN_SIZES = [
  { label: "1 MB", bytes: 1024 * 1024 },
  { label: "10 MB", bytes: 10 * 1024 * 1024 },
  { label: "100 MB", bytes: 100 * 1024 * 1024 },
  { label: "Any size", bytes: 1 },
];

/** Shows at most this many groups, so huge result sets stay responsive. */
const MAX_GROUPS = 300;

function Duplicates() {
  const [roots, setRoots] = useState<string[]>([]);
  const [minSize, setMinSize] = useState(MIN_SIZES[0].bytes);
  const [busy, setBusy] = useState(false);
  const [groups, setGroups] = useState<DuplicateGroup[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [clouds, setClouds] = useState<{ name: string; path: string }[]>([]);

  useEffect(() => {
    storageApi.homeFolder().then((home) => home && setRoots([home]));
    storageApi
      .cloudFolders()
      .then(setClouds)
      .catch(() => setClouds([]));
  }, []);

  async function search() {
    setMessage(null);
    setBusy(true);
    setSelected(new Set());
    try {
      setGroups(await storageApi.findDuplicates(roots, minSize));
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(false);
    }
  }

  function selectAllButOldest() {
    const next = new Set<string>();
    for (const g of groups ?? []) for (const f of g.files.slice(1)) next.add(f.path);
    setSelected(next);
  }

  function toggle(group: DuplicateGroup, path: string) {
    const next = new Set(selected);
    if (next.has(path)) next.delete(path);
    else if (group.files.filter((f) => !next.has(f.path)).length > 1) next.add(path);
    setSelected(next);
  }

  async function recycle() {
    setConfirming(false);
    try {
      const report = await storageApi.recycleDuplicates([...selected]);
      const moved = new Set(report.moved);
      setGroups((gs) =>
        (gs ?? [])
          .map((g) => ({ ...g, files: g.files.filter((f) => !moved.has(f.path)) }))
          .filter((g) => g.files.length > 1),
      );
      setSelected(new Set());
      setMessage(
        `Moved ${plural(report.moved.length, "file", "files")} to the Recycle Bin.` +
          (report.failures.length
            ? ` ${plural(report.failures.length, "file", "files")} could not be moved.`
            : ""),
      );
    } catch (e) {
      setMessage(String(e));
    }
  }

  const sizeOf = new Map((groups ?? []).flatMap((g) => g.files.map((f) => [f.path, g.size] as const)));
  const selectedBytes = [...selected].reduce((n, p) => n + (sizeOf.get(p) ?? 0), 0);
  const wasted = (groups ?? []).reduce((n, g) => n + g.size * (g.files.length - 1), 0);

  return (
    <>
      <div className="toolbar">
        <div className="roots">
          {roots.map((r) => (
            <span key={r} className="chip">
              {r}
              <button className="link" disabled={busy} onClick={() => setRoots(roots.filter((x) => x !== r))}>
                ×
              </button>
            </span>
          ))}
          <button
            className="link"
            disabled={busy}
            onClick={async () => {
              const folder = await pickFolder();
              if (folder && !roots.includes(folder)) setRoots([...roots, folder]);
            }}
          >
            + Add folder
          </button>
          {clouds
            .filter((c) => !roots.includes(c.path))
            .map((c) => (
              <button
                key={c.path}
                className="link"
                disabled={busy}
                title={`${c.path} (files that are only in the cloud are skipped, so nothing is downloaded)`}
                onClick={() => setRoots([...roots, c.path])}
              >
                + {c.name}
              </button>
            ))}
        </div>
        <label className="muted">
          Larger than{" "}
          <select value={minSize} disabled={busy} onChange={(e) => setMinSize(Number(e.target.value))}>
            {MIN_SIZES.map((m) => (
              <option key={m.bytes} value={m.bytes}>
                {m.label}
              </option>
            ))}
          </select>
        </label>
        <button className="secondary" disabled={busy || roots.length === 0} onClick={search}>
          Find duplicates
        </button>
      </div>
      {message && <div className="banner">{message}</div>}

      <section className="results">
        {busy && <Working label="Comparing files..." active={busy} />}
        {!busy && !groups && (
          <div className="empty">
            <p>Pick folders to search, then press Find duplicates.</p>
            <p className="muted">Files are compared by their contents, not by name.</p>
          </div>
        )}
        {!busy && groups && groups.length === 0 && (
          <div className="empty">
            <p>No duplicate files found.</p>
          </div>
        )}
        {!busy && groups && groups.length > 0 && (
          <>
            <div className="toolbar">
              <span>
                {plural(groups.length, "set", "sets")} of duplicates, <strong>{formatBytes(wasted)}</strong>{" "}
                could be freed.
              </span>
              <button className="link" onClick={selectAllButOldest}>
                Select all but the oldest copy
              </button>
              <button className="primary" disabled={selected.size === 0} onClick={() => setConfirming(true)}>
                Move {selected.size || ""} to Recycle Bin
              </button>
            </div>
            {groups.slice(0, MAX_GROUPS).map((g) => (
              <div key={g.files[0].path} className="dup-group">
                <div className="muted">
                  {g.files.length} copies · {formatBytes(g.size)} each
                </div>
                {g.files.map((f) => (
                  <label key={f.path} className="dup-file">
                    <input
                      type="checkbox"
                      checked={selected.has(f.path)}
                      onChange={() => toggle(g, f.path)}
                    />
                    <span className="path" title={f.path}>
                      {f.path}
                    </span>
                    <span className="muted">{formatDate(f.modified)}</span>
                    <button className="link" onClick={(e) => (e.preventDefault(), storageApi.reveal(f.path))}>
                      Show
                    </button>
                  </label>
                ))}
              </div>
            ))}
            {groups.length > MAX_GROUPS && (
              <p className="muted">Showing the {MAX_GROUPS} sets that waste the most space.</p>
            )}
          </>
        )}
      </section>
      {confirming && (
        <ConfirmRecycle
          count={selected.size}
          bytes={selectedBytes}
          onCancel={() => setConfirming(false)}
          onConfirm={recycle}
        />
      )}
    </>
  );
}

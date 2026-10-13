import { useEffect, useMemo, useState } from "react";
import { api, type Category, type CategoryClean, type CategorySummary } from "../api";
import { formatBytes, formatCount, plural } from "../format";

const SELECTION_KEY = "clearwin.cleaner.selection";

type Phase =
  | { kind: "idle" }
  | { kind: "scanning"; current: string; done: number; total: number }
  | { kind: "scanned"; results: CategorySummary[] }
  | { kind: "confirming"; results: CategorySummary[] }
  | { kind: "cleaning"; results: CategorySummary[] }
  | { kind: "cleaned"; report: CategoryClean[] };

function loadSelection(categories: Category[]): Set<string> {
  try {
    const saved = localStorage.getItem(SELECTION_KEY);
    if (saved) {
      const ids = new Set<string>(JSON.parse(saved));
      return new Set(categories.filter((c) => ids.has(c.id)).map((c) => c.id));
    }
  } catch {
    // Fall back to the defaults below.
  }
  return new Set(categories.filter((c) => c.defaultSelected).map((c) => c.id));
}

function saveSelection(selection: Set<string>) {
  try {
    localStorage.setItem(SELECTION_KEY, JSON.stringify([...selection]));
  } catch {
    // Remembering the selection is a convenience only.
  }
}

/** Results that can actually be cleaned now: something found and nothing blocking. */
function cleanable(results: CategorySummary[]) {
  return results.filter((r) => r.itemCount > 0 && !r.blockedReason);
}

export default function Cleaner() {
  const [categories, setCategories] = useState<Category[]>([]);
  const [selection, setSelection] = useState<Set<string>>(new Set());
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [isAdmin, setIsAdmin] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .listCategories()
      .then((cats) => {
        setCategories(cats);
        setSelection(loadSelection(cats));
      })
      .catch((e) => setError(String(e)));
    api.isAdmin().then(setIsAdmin).catch(() => setIsAdmin(true));
  }, []);

  const byId = useMemo(() => new Map(categories.map((c) => [c.id, c])), [categories]);
  const groups = useMemo(() => {
    const map = new Map<string, Category[]>();
    for (const c of categories) map.set(c.group, [...(map.get(c.group) ?? []), c]);
    return [...map.entries()];
  }, [categories]);

  const busy = phase.kind === "scanning" || phase.kind === "cleaning";

  function updateSelection(next: Set<string>) {
    setSelection(next);
    saveSelection(next);
    // A changed selection makes the old preview stale, so it must be analyzed again.
    if (phase.kind !== "idle") setPhase({ kind: "idle" });
  }

  function toggle(id: string) {
    const next = new Set(selection);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    updateSelection(next);
  }

  function toggleGroup(items: Category[], on: boolean) {
    const next = new Set(selection);
    for (const c of items) {
      if (on) next.add(c.id);
      else next.delete(c.id);
    }
    updateSelection(next);
  }

  async function analyze() {
    setError(null);
    const ids = categories.filter((c) => selection.has(c.id)).map((c) => c.id);
    setPhase({ kind: "scanning", current: "", done: 0, total: ids.length });
    const unlisten = await api.onScanProgress((p) =>
      setPhase({ kind: "scanning", current: p.id, done: p.done, total: p.total }),
    );
    try {
      setPhase({ kind: "scanned", results: await api.scan(ids) });
    } catch (e) {
      setError(String(e));
      setPhase({ kind: "idle" });
    } finally {
      unlisten();
    }
  }

  async function clean(results: CategorySummary[]) {
    setError(null);
    setPhase({ kind: "cleaning", results });
    try {
      const report = await api.clean(cleanable(results).map((r) => r.id));
      setPhase({ kind: "cleaned", report });
    } catch (e) {
      setError(String(e));
      setPhase({ kind: "scanned", results });
    }
  }

  async function restartAsAdmin() {
    try {
      await api.restartAsAdmin();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="page">
      <header className="page-header">
        <h1>Cleaner</h1>
        <div className="actions">
          <button className="secondary" disabled={busy || selection.size === 0} onClick={analyze}>
            Analyze
          </button>
          <button
            className="primary"
            disabled={phase.kind !== "scanned" || cleanable(phase.results).length === 0}
            onClick={() => phase.kind === "scanned" && setPhase({ kind: "confirming", results: phase.results })}
          >
            Clean
          </button>
        </div>
      </header>

      {!isAdmin && (
        <div className="banner">
          <span>
            ClearWin is running without administrator rights, so some Windows folders will be
            skipped.
          </span>
          <button className="link" onClick={restartAsAdmin}>
            Restart as administrator
          </button>
        </div>
      )}
      {error && <div className="banner error">{error}</div>}

      <div className="cleaner">
        <aside className="checklist">
          {groups.map(([group, items]) => {
            const allOn = items.every((c) => selection.has(c.id));
            return (
              <section key={group}>
                <label className="group-title">
                  <input
                    type="checkbox"
                    checked={allOn}
                    disabled={busy}
                    onChange={() => toggleGroup(items, !allOn)}
                  />
                  {group}
                </label>
                {items.map((c) => (
                  <label key={c.id} className="check" title={c.description}>
                    <input
                      type="checkbox"
                      checked={selection.has(c.id)}
                      disabled={busy}
                      onChange={() => toggle(c.id)}
                    />
                    <span>
                      {c.name}
                      {c.needsAdmin && !isAdmin && <span className="tag">Admin</span>}
                    </span>
                  </label>
                ))}
              </section>
            );
          })}
        </aside>

        <section className="results">
          {phase.kind === "idle" && (
            <div className="empty">
              <p>Pick what to clean on the left, then press Analyze.</p>
              <p className="muted">Nothing is deleted until you review the list and press Clean.</p>
            </div>
          )}

          {phase.kind === "scanning" && (
            <div className="empty">
              <p>Analyzing {byId.get(phase.current)?.name ?? ""}...</p>
              <progress max={phase.total} value={phase.done} />
            </div>
          )}

          {(phase.kind === "scanned" || phase.kind === "confirming" || phase.kind === "cleaning") && (
            <ScanResults results={phase.results} byId={byId} />
          )}

          {phase.kind === "cleaned" && <CleanReport report={phase.report} byId={byId} />}
        </section>
      </div>

      {phase.kind === "confirming" && (
        <ConfirmDialog
          results={cleanable(phase.results)}
          onCancel={() => setPhase({ kind: "scanned", results: phase.results })}
          onConfirm={() => clean(phase.results)}
        />
      )}
      {phase.kind === "cleaning" && (
        <div className="overlay">
          <div className="dialog">
            <p>Cleaning...</p>
            <progress />
          </div>
        </div>
      )}
    </div>
  );
}

function totals(results: CategorySummary[]) {
  return results.reduce(
    (acc, r) => ({ bytes: acc.bytes + r.totalBytes, items: acc.items + r.itemCount }),
    { bytes: 0, items: 0 },
  );
}

function ScanResults({
  results,
  byId,
}: {
  results: CategorySummary[];
  byId: Map<string, Category>;
}) {
  const [open, setOpen] = useState<string | null>(null);
  const total = totals(cleanable(results));

  return (
    <>
      <div className="summary">
        <strong>{formatBytes(total.bytes)}</strong> can be freed from{" "}
        {plural(total.items, "item", "items")}. Click a row to see the largest files.
      </div>
      <table>
        <thead>
          <tr>
            <th>What</th>
            <th className="num">Items</th>
            <th className="num">Size</th>
          </tr>
        </thead>
        <tbody>
          {results.map((r) => {
            const c = byId.get(r.id);
            const expanded = open === r.id;
            return [
              <tr
                key={r.id}
                className={`row ${r.blockedReason ? "blocked" : ""} ${r.largest.length ? "clickable" : ""}`}
                onClick={() => r.largest.length && setOpen(expanded ? null : r.id)}
              >
                <td>
                  <span className="muted">{c?.group} · </span>
                  {c?.name}
                  {r.blockedReason && <div className="warn">{r.blockedReason}</div>}
                </td>
                <td className="num">{formatCount(r.itemCount)}</td>
                <td className="num">{r.totalBytes ? formatBytes(r.totalBytes) : "–"}</td>
              </tr>,
              expanded && (
                <tr key={`${r.id}-files`} className="files">
                  <td colSpan={3}>
                    <ul>
                      {r.largest.map((f) => (
                        <li key={f.path}>
                          <span className="path">{f.path}</span>
                          <span className="num">{formatBytes(f.size)}</span>
                        </li>
                      ))}
                    </ul>
                    {r.itemCount > r.largest.length && (
                      <p className="muted">
                        Showing the {r.largest.length} largest of {formatCount(r.itemCount)} files.
                      </p>
                    )}
                  </td>
                </tr>
              ),
            ];
          })}
        </tbody>
      </table>
    </>
  );
}

function ConfirmDialog({
  results,
  onCancel,
  onConfirm,
}: {
  results: CategorySummary[];
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const total = totals(results);
  return (
    <div className="overlay">
      <div className="dialog">
        <h2>Delete these items?</h2>
        <p>
          {plural(total.items, "item", "items")} ({formatBytes(total.bytes)}) will be permanently
          deleted. This can't be undone.
        </p>
        <div className="actions">
          <button className="secondary" onClick={onCancel}>
            Cancel
          </button>
          <button className="primary danger" onClick={onConfirm}>
            Delete
          </button>
        </div>
      </div>
    </div>
  );
}

function CleanReport({ report, byId }: { report: CategoryClean[]; byId: Map<string, Category> }) {
  const freed = report.reduce((n, r) => n + r.freedBytes, 0);
  const removed = report.reduce((n, r) => n + r.removedItems, 0);
  const failed = report.reduce((n, r) => n + r.failedItems, 0);

  return (
    <>
      <div className="summary success">
        Freed <strong>{formatBytes(freed)}</strong> by removing {plural(removed, "item", "items")}.
        {failed > 0 && <> {plural(failed, "item was", "items were")} in use or protected and left alone.</>}
      </div>
      <table>
        <thead>
          <tr>
            <th>What</th>
            <th className="num">Removed</th>
            <th className="num">Freed</th>
          </tr>
        </thead>
        <tbody>
          {report.map((r) => {
            const c = byId.get(r.id);
            return (
              <tr key={r.id}>
                <td>
                  <span className="muted">{c?.group} · </span>
                  {c?.name}
                  {r.skippedReason && <div className="warn">{r.skippedReason}</div>}
                  {r.failedItems > 0 && (
                    <details>
                      <summary className="warn">{plural(r.failedItems, "item", "items")} left alone</summary>
                      <ul className="failures">
                        {r.failures.map((f, i) => (
                          <li key={i}>{f}</li>
                        ))}
                      </ul>
                    </details>
                  )}
                </td>
                <td className="num">{formatCount(r.removedItems)}</td>
                <td className="num">{r.freedBytes ? formatBytes(r.freedBytes) : "–"}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </>
  );
}

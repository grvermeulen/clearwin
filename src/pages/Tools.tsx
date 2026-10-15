import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  toolsApi,
  type AppDetails,
  type BackupInfo,
  type InstalledApp,
  type RegistryIssue,
  type StartupItem,
} from "../api";
import { formatBytes, formatDate, plural } from "../format";

type Tab = "startup" | "uninstall" | "registry";

export default function Tools() {
  const [tab, setTab] = useState<Tab>("startup");
  return (
    <div className="page">
      <header className="page-header">
        <h1>Tools</h1>
        <div className="tabs">
          <button className={tab === "startup" ? "active" : ""} onClick={() => setTab("startup")}>
            Startup programs
          </button>
          <button className={tab === "uninstall" ? "active" : ""} onClick={() => setTab("uninstall")}>
            Uninstall
          </button>
          <button className={tab === "registry" ? "active" : ""} onClick={() => setTab("registry")}>
            Registry
          </button>
        </div>
      </header>
      <div hidden={tab !== "startup"} className="tab-body">
        <Startup />
      </div>
      <div hidden={tab !== "uninstall"} className="tab-body">
        <Uninstall />
      </div>
      <div hidden={tab !== "registry"} className="tab-body">
        <Registry />
      </div>
    </div>
  );
}

function Confirm({
  title,
  children,
  action,
  danger,
  onCancel,
  onConfirm,
}: {
  title: string;
  children: ReactNode;
  action: string;
  danger?: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="overlay">
      <div className="dialog">
        <h2>{title}</h2>
        {children}
        <div className="actions">
          <button className="secondary" onClick={onCancel}>
            Cancel
          </button>
          <button className={`primary ${danger ? "danger" : ""}`} onClick={onConfirm}>
            {action}
          </button>
        </div>
      </div>
    </div>
  );
}

function Startup() {
  const [items, setItems] = useState<StartupItem[] | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const load = () =>
    toolsApi
      .listStartup()
      .then(setItems)
      .catch((e) => setMessage(String(e)));
  useEffect(() => {
    load();
  }, []);

  async function toggle(item: StartupItem) {
    setMessage(null);
    try {
      await toolsApi.setStartupEnabled(item.id, !item.enabled);
      await load();
    } catch (e) {
      setMessage(String(e));
    }
  }

  return (
    <>
      <div className="toolbar">
        <span className="muted">
          Turning a program off stops it starting with Windows. It stays installed and you can turn it back on
          any time.
        </span>
        <button className="secondary" onClick={load}>
          Refresh
        </button>
      </div>
      {message && <div className="banner error">{message}</div>}
      <section className="results">
        {items === null && <div className="empty">Loading...</div>}
        {items?.length === 0 && <div className="empty">No startup programs found.</div>}
        {items && items.length > 0 && (
          <table>
            <thead>
              <tr>
                <th>On</th>
                <th>Program</th>
                <th>Where</th>
              </tr>
            </thead>
            <tbody>
              {items.map((item) => (
                <tr key={item.id} className={item.enabled ? "" : "blocked"}>
                  <td>
                    <label className="switch">
                      <input type="checkbox" checked={item.enabled} onChange={() => toggle(item)} />
                      <span />
                    </label>
                  </td>
                  <td className="path-cell">
                    {item.name}
                    {item.targetMissing && <span className="tag warn-tag">Program missing</span>}
                    <div className="muted small path" title={item.command}>
                      {item.command}
                    </div>
                  </td>
                  <td className="muted nowrap">
                    {item.location}
                    {item.needsAdmin && <span className="tag">Admin</span>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </>
  );
}

type SortKey = "name" | "size" | "installed" | "lastUsed";

/** Shown for a column that sorts; clicking again flips the direction. */
function SortHeader({
  label,
  column,
  sort,
  onSort,
  className,
}: {
  label: string;
  column: SortKey;
  sort: { key: SortKey; desc: boolean };
  onSort: (key: SortKey) => void;
  className?: string;
}) {
  const active = sort.key === column;
  return (
    <th className={`sortable ${className ?? ""}`} onClick={() => onSort(column)}>
      {label}
      <span className="sort-mark">{active ? (sort.desc ? " ▼" : " ▲") : ""}</span>
    </th>
  );
}

function Uninstall() {
  const [apps, setApps] = useState<InstalledApp[] | null>(null);
  const [details, setDetails] = useState<Map<string, AppDetails> | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "name", desc: false });
  const [confirming, setConfirming] = useState<InstalledApp | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const load = () => {
    setDetails(null);
    toolsApi
      .listApps()
      .then(setApps)
      .catch((e) => setMessage(String(e)));
    // Folder sizes and last use take longer, so they fill in afterwards.
    toolsApi
      .appDetails()
      .then((list) => setDetails(new Map(list.map((d) => [d.id, d]))))
      .catch(() => setDetails(new Map()));
  };
  useEffect(() => {
    load();
  }, []);

  const sizeOf = (a: InstalledApp) => a.sizeBytes ?? details?.get(a.id)?.measuredBytes ?? null;
  const lastUsedOf = (a: InstalledApp) => details?.get(a.id)?.lastUsed ?? null;

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = (apps ?? []).filter(
      (a) => !q || a.name.toLowerCase().includes(q) || a.publisher?.toLowerCase().includes(q),
    );
    const value = (a: InstalledApp): string | number | null => {
      switch (sort.key) {
        case "name":
          return a.name.toLowerCase();
        case "size":
          return sizeOf(a);
        case "installed":
          return a.installDate;
        case "lastUsed":
          return lastUsedOf(a);
      }
    };
    return [...list].sort((x, y) => {
      const a = value(x);
      const b = value(y);
      // Unknown values always go last, whichever way the column is sorted.
      if (a === null || b === null) return a === b ? 0 : a === null ? 1 : -1;
      const order = a < b ? -1 : a > b ? 1 : 0;
      return sort.desc ? -order : order;
    });
  }, [apps, details, query, sort]);

  function onSort(key: SortKey) {
    // Size and dates are most useful biggest / newest first.
    setSort((s) => (s.key === key ? { key, desc: !s.desc } : { key, desc: key !== "name" }));
  }

  async function uninstall(app: InstalledApp) {
    setConfirming(null);
    try {
      await toolsApi.uninstallApp(app.id);
      setMessage(`The uninstaller for ${app.name} has started. Press Refresh when it has finished.`);
    } catch (e) {
      setMessage(String(e));
    }
  }

  return (
    <>
      <div className="toolbar">
        <input
          className="search"
          placeholder="Search programs"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <span className="muted">{apps && plural(apps.length, "program", "programs")}</span>
        {apps && !details && <span className="muted small">Measuring folders...</span>}
        <button className="secondary" onClick={load}>
          Refresh
        </button>
      </div>
      {message && <div className="banner">{message}</div>}
      <section className="results">
        {apps === null && <div className="empty">Loading...</div>}
        {apps && (
          <table>
            <thead>
              <tr>
                <SortHeader label="Program" column="name" sort={sort} onSort={onSort} />
                <th>Publisher</th>
                <SortHeader
                  label="Installed"
                  column="installed"
                  sort={sort}
                  onSort={onSort}
                  className="num"
                />
                <SortHeader label="Last used" column="lastUsed" sort={sort} onSort={onSort} className="num" />
                <SortHeader label="Size" column="size" sort={sort} onSort={onSort} className="num" />
                <th />
              </tr>
            </thead>
            <tbody>
              {shown.map((a) => {
                const size = sizeOf(a);
                const lastUsed = lastUsedOf(a);
                return (
                  <tr key={a.id}>
                    <td>
                      {a.name}
                      {a.version && <div className="muted small">{a.version}</div>}
                    </td>
                    <td className="muted">{a.publisher}</td>
                    <td className="num muted nowrap">{a.installDate}</td>
                    <td className="num muted nowrap">
                      {lastUsed ? formatDate(lastUsed) : details ? "" : "..."}
                    </td>
                    <td className="num nowrap">{size !== null ? formatBytes(size) : details ? "" : "..."}</td>
                    <td className="num">
                      <button className="link" onClick={() => setConfirming(a)}>
                        Uninstall
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
        {apps && details && (
          <p className="muted small">
            Last used comes from what Windows records when a program starts. Programs started by another
            launcher, like Steam, may show no date. Restart ClearWin as administrator to read more of these
            records.
          </p>
        )}
      </section>
      {confirming && (
        <Confirm
          title={`Uninstall ${confirming.name}?`}
          action="Uninstall"
          danger
          onCancel={() => setConfirming(null)}
          onConfirm={() => uninstall(confirming)}
        >
          <p>The program's own uninstaller will open. Windows may ask for permission first.</p>
        </Confirm>
      )}
    </>
  );
}

function Registry() {
  const [issues, setIssues] = useState<RegistryIssue[] | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [restoring, setRestoring] = useState<BackupInfo | null>(null);
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [message, setMessage] = useState<string | null>(null);

  const loadBackups = () =>
    toolsApi
      .listBackups()
      .then(setBackups)
      .catch(() => setBackups([]));
  useEffect(() => {
    loadBackups();
  }, []);

  async function scan() {
    setMessage(null);
    setBusy(true);
    try {
      const found = await toolsApi.scanRegistry();
      setIssues(found);
      setSelected(new Set(found.map((i) => i.id)));
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function fix() {
    setConfirming(false);
    setBusy(true);
    try {
      const report = await toolsApi.fixRegistry([...selected]);
      setMessage(
        `Fixed ${plural(report.fixed, "issue", "issues")}. A backup was saved first, so you can undo this below.` +
          (report.failures.length
            ? ` ${plural(report.failures.length, "issue", "issues")} could not be fixed: ${report.failures.join("; ")}`
            : ""),
      );
      setIssues((all) => (all ?? []).filter((i) => !selected.has(i.id)));
      setSelected(new Set());
      loadBackups();
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function restore(backup: BackupInfo) {
    setRestoring(null);
    try {
      const files = await toolsApi.restoreBackup(backup.path);
      setMessage(`Restored ${plural(files, "backup file", "backup files")}.`);
    } catch (e) {
      setMessage(String(e));
    }
  }

  const groups = useMemo(() => {
    const map = new Map<string, RegistryIssue[]>();
    for (const i of issues ?? []) map.set(i.kind, [...(map.get(i.kind) ?? []), i]);
    return [...map.entries()];
  }, [issues]);

  return (
    <>
      <div className="toolbar">
        <span className="muted">
          Finds registry entries that point at programs or files that no longer exist. A backup is saved
          before anything changes.
        </span>
        <button className="secondary" disabled={busy} onClick={scan}>
          Scan for issues
        </button>
        <button
          className="primary"
          disabled={busy || selected.size === 0}
          onClick={() => setConfirming(true)}
        >
          Fix {selected.size || ""} selected
        </button>
      </div>
      {message && <div className="banner">{message}</div>}
      <section className="results">
        {busy && (
          <div className="empty">
            <p>Working...</p>
            <progress />
          </div>
        )}
        {!busy && issues === null && (
          <div className="empty">
            <p>Press Scan for issues to check the registry.</p>
            <p className="muted">ClearWin only looks for entries it can be sure are broken.</p>
          </div>
        )}
        {!busy && issues?.length === 0 && <div className="empty">No registry issues found.</div>}
        {!busy &&
          groups.map(([kind, items]) => (
            <div key={kind} className="dup-group">
              <label className="group-title">
                <input
                  type="checkbox"
                  checked={items.every((i) => selected.has(i.id))}
                  onChange={(e) => {
                    const next = new Set(selected);
                    for (const i of items) {
                      if (e.target.checked) next.add(i.id);
                      else next.delete(i.id);
                    }
                    setSelected(next);
                  }}
                />
                {kind} ({items.length})
              </label>
              {items.map((i) => (
                <label key={i.id} className="dup-file">
                  <input
                    type="checkbox"
                    checked={selected.has(i.id)}
                    onChange={() => {
                      const next = new Set(selected);
                      if (next.has(i.id)) next.delete(i.id);
                      else next.add(i.id);
                      setSelected(next);
                    }}
                  />
                  <span className="path" title={`${i.key}${i.value ? ` → ${i.value}` : ""}`}>
                    {i.missingPath}
                  </span>
                  {i.needsAdmin && <span className="tag">Admin</span>}
                </label>
              ))}
            </div>
          ))}

        {backups.length > 0 && (
          <>
            <h2 className="section-title">Backups</h2>
            <table>
              <tbody>
                {backups.map((b) => (
                  <tr key={b.path}>
                    <td>{new Date(b.created * 1000).toLocaleString()}</td>
                    <td className="muted">{plural(b.issues.length, "entry", "entries")}</td>
                    <td className="num">
                      <button className="link" onClick={() => setRestoring(b)}>
                        Restore
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        )}
      </section>
      {confirming && (
        <Confirm
          title="Fix registry issues?"
          action="Fix"
          onCancel={() => setConfirming(false)}
          onConfirm={fix}
        >
          <p>
            {plural(selected.size, "entry", "entries")} will be removed. ClearWin saves a backup first, which
            you can restore from this page.
          </p>
        </Confirm>
      )}
      {restoring && (
        <Confirm
          title="Restore this backup?"
          action="Restore"
          onCancel={() => setRestoring(null)}
          onConfirm={() => restore(restoring)}
        >
          <p>
            The registry entries saved on {formatDate(restoring.created)} will be put back as they were before
            fixing.
          </p>
        </Confirm>
      )}
    </>
  );
}

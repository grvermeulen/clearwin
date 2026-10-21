import { useEffect, useRef, useState } from "react";
import {
  api,
  updatesApi,
  type DriverScan,
  type SoftwareUpdate,
  type UpdateFailure,
  type UpdateFix,
} from "../api";
import { formatBytes, formatDate, plural } from "../format";

type Tab = "software" | "drivers";

export default function Updates() {
  const [tab, setTab] = useState<Tab>("software");
  return (
    <div className="page">
      <header className="page-header">
        <h1>Updates</h1>
        <div className="tabs">
          <button className={tab === "software" ? "active" : ""} onClick={() => setTab("software")}>
            Programs
          </button>
          <button className={tab === "drivers" ? "active" : ""} onClick={() => setTab("drivers")}>
            Drivers
          </button>
        </div>
      </header>
      <div hidden={tab !== "software"} className="tab-body">
        <Software />
      </div>
      <div hidden={tab !== "drivers"} className="tab-body">
        <Drivers />
      </div>
    </div>
  );
}

type Outcome =
  | { state: "waiting" | "updating" | "done" | "ignored" }
  | { state: "failed"; error: string; fix: UpdateFix | null };

/** Commands send back either a plain message or a failure with a suggested fix. */
function asFailure(e: unknown): UpdateFailure {
  if (e && typeof e === "object" && "reason" in e) return e as UpdateFailure;
  return { reason: String(e), fix: null };
}

const FIXES: Record<UpdateFix, { label: string; hint: string; confirm?: string }> = {
  runAsAdmin: {
    label: "Update as administrator",
    hint: "Windows asks for permission first.",
  },
  closeAndRetry: {
    label: "Close it and update",
    hint: "Closes the program and stops its background services, updates, then starts the services again. Unsaved work in it is lost.",
    confirm: "Close and update",
  },
  reinstall: {
    label: "Reinstall new version",
    hint: "Removes the installed version, then installs the new one. Settings usually stay, but check afterwards.",
    confirm: "Remove and reinstall",
  },
};

function Software() {
  const [updates, setUpdates] = useState<SoftwareUpdate[] | null>(null);
  const [checking, setChecking] = useState(false);
  /** What happened to each package in this session, by id. */
  const [outcomes, setOutcomes] = useState<Record<string, Outcome>>({});
  /** Progress of an "Update all" run. */
  const [run, setRun] = useState<{ total: number; finished: number; current: string } | null>(null);
  const stopRequested = useRef(false);
  const [stopping, setStopping] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  async function check() {
    setChecking(true);
    setMessage(null);
    setOutcomes({});
    try {
      setUpdates(await updatesApi.listSoftware());
    } catch (e) {
      setMessage(String(e));
    } finally {
      setChecking(false);
    }
  }
  useEffect(() => {
    check();
  }, []);

  const setOutcome = (id: string, outcome: Outcome) => setOutcomes((o) => ({ ...o, [id]: outcome }));

  /** Updates one program and records how it went; never throws. */
  async function updateOne(u: SoftwareUpdate): Promise<boolean> {
    setOutcome(u.id, { state: "updating" });
    try {
      await updatesApi.updateSoftware(u.id);
      setOutcome(u.id, { state: "done" });
      return true;
    } catch (e) {
      const { reason, fix } = asFailure(e);
      setOutcome(u.id, { state: "failed", error: reason, fix });
      return false;
    }
  }

  /** The program whose fix waits for a second click to confirm, by id. */
  const [confirming, setConfirming] = useState<string | null>(null);

  async function applyFix(u: SoftwareUpdate, fix: UpdateFix) {
    setConfirming(null);
    setOutcome(u.id, { state: "updating" });
    try {
      await updatesApi.fixSoftwareUpdate(fix, u);
      setOutcome(u.id, { state: "done" });
    } catch (e) {
      const { reason, fix: next } = asFailure(e);
      setOutcome(u.id, { state: "failed", error: reason, fix: next });
    }
  }

  async function ignore(u: SoftwareUpdate) {
    const before = outcomes[u.id];
    setOutcome(u.id, { state: "updating" });
    try {
      await updatesApi.ignoreSoftwareUpdate(u.id);
      setOutcome(u.id, { state: "ignored" });
    } catch (e) {
      setOutcome(u.id, before);
      setMessage(String(e));
    }
  }

  async function updateAll() {
    const todo = (updates ?? []).filter((u) => !["done", "ignored", "updating"].includes(outcomes[u.id]?.state ?? ""));
    stopRequested.current = false;
    setStopping(false);
    setMessage(null);
    setOutcomes((o) => ({
      ...o,
      ...Object.fromEntries(todo.map((u) => [u.id, { state: "waiting" } as Outcome])),
    }));
    let ok = 0;
    let failed = 0;
    for (const [i, u] of todo.entries()) {
      if (stopRequested.current) break;
      setRun({ total: todo.length, finished: i, current: u.name });
      // A failure is recorded on its row and the next program goes ahead.
      if (await updateOne(u)) ok++;
      else failed++;
    }
    setRun(null);
    // Anything not reached after Stop goes back to plain "Update".
    setOutcomes((o) => Object.fromEntries(Object.entries(o).filter(([, v]) => v.state !== "waiting")));
    setMessage(
      `Updated ${plural(ok, "program", "programs")}.` +
        (failed
          ? ` ${plural(failed, "update", "updates")} failed; the reason and, where ClearWin can, a fix are shown on each one.`
          : "") +
        (stopRequested.current ? " Stopped before the rest." : ""),
    );
  }

  const running = run !== null;
  // A single-row fix counts too, so two winget runs never touch the same program.
  const busy = running || Object.values(outcomes).some((o) => o.state === "updating");
  const failedCount = Object.values(outcomes).filter((o) => o.state === "failed").length;
  const untouched = (updates ?? []).filter((u) => !outcomes[u.id]).length;
  return (
    <>
      <div className="toolbar">
        <span className="muted">
          Updates come from winget, the app installer built into Windows. Old versions are a common way in for
          viruses.
        </span>
        <button className="secondary" disabled={checking || busy} onClick={check}>
          Check again
        </button>
        {running ? (
          <button
            className="secondary"
            disabled={stopping}
            onClick={() => {
              stopRequested.current = true;
              setStopping(true);
            }}
          >
            {stopping ? "Stopping..." : "Stop after this one"}
          </button>
        ) : (
          <button className="primary" disabled={checking || busy || !(failedCount + untouched)} onClick={updateAll}>
            {!failedCount ? "Update all" : untouched ? "Retry failed and update the rest" : "Retry failed"}
          </button>
        )}
      </div>
      {run && (
        <div className="update-progress">
          <div>
            Updating <strong>{run.current}</strong> ({run.finished + 1} of {run.total})
          </div>
          <div className="meter">
            <div style={{ width: `${(run.finished / run.total) * 100}%` }} />
          </div>
        </div>
      )}
      {message && <div className="banner">{message}</div>}
      <section className="results">
        {checking && (
          <div className="empty">
            <p>Checking for newer versions...</p>
            <progress />
          </div>
        )}
        {!checking && updates?.length === 0 && (
          <div className="empty">
            <p className="good">All your programs are up to date.</p>
          </div>
        )}
        {!checking && updates && updates.length > 0 && (
          <table>
            <thead>
              <tr>
                <th>Program</th>
                <th>Installed</th>
                <th>New version</th>
                <th className="num">Status</th>
              </tr>
            </thead>
            <tbody>
              {updates.map((u) => {
                const o = outcomes[u.id];
                return (
                  <tr key={u.id} className={o?.state === "updating" ? "row-active" : ""}>
                    <td>
                      {u.name}
                      <div className="muted small">{u.id}</div>
                      {o?.state === "failed" && <div className="warn">{o.error}</div>}
                      {o?.state === "failed" && o.fix && (
                        <div className="small">
                          {confirming === u.id ? (
                            <>
                              <span className="muted">{FIXES[o.fix].hint} </span>
                              <button className="link" disabled={busy} onClick={() => applyFix(u, o.fix!)}>
                                {FIXES[o.fix].confirm}
                              </button>{" "}
                              <button className="link" onClick={() => setConfirming(null)}>
                                Cancel
                              </button>
                            </>
                          ) : (
                            <>
                              <button
                                className="link"
                                disabled={busy}
                                title={FIXES[o.fix].hint}
                                onClick={() =>
                                  FIXES[o.fix!].confirm ? setConfirming(u.id) : applyFix(u, o.fix!)
                                }
                              >
                                {FIXES[o.fix].label}
                              </button>
                              {o.fix === "reinstall" && (
                                <>
                                  {" or "}
                                  <button
                                    className="link"
                                    disabled={busy}
                                    title="Stops winget from offering updates for this program."
                                    onClick={() => ignore(u)}
                                  >
                                    stop offering this update
                                  </button>
                                </>
                              )}
                            </>
                          )}
                        </div>
                      )}
                    </td>
                    <td className="muted">{u.version}</td>
                    <td>{u.available}</td>
                    <td className="num nowrap">
                      {o?.state === "updating" && <span>Updating...</span>}
                      {o?.state === "waiting" && <span className="muted">Waiting</span>}
                      {o?.state === "done" && <span className="good">Updated</span>}
                      {o?.state === "ignored" && <span className="muted">Ignored</span>}
                      {(!o || o.state === "failed") && (
                        <button className="link" disabled={busy} onClick={() => updateOne(u)}>
                          {o?.state === "failed" ? "Retry" : "Update"}
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>
    </>
  );
}

function Drivers() {
  const [scan, setScan] = useState<DriverScan | null>(null);
  const [busy, setBusy] = useState<"scan" | "install" | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [admin, setAdmin] = useState(true);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    api
      .isAdmin()
      .then(setAdmin)
      .catch(() => setAdmin(false));
  }, []);

  async function check() {
    setBusy("scan");
    setMessage(null);
    try {
      const found = await updatesApi.scanDrivers();
      setScan(found);
      setSelected(new Set(found.updates.map((u) => u.id)));
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function install() {
    setBusy("install");
    setMessage(null);
    try {
      const result = await updatesApi.installDrivers([...selected]);
      setMessage(
        `Installed ${plural(result.installed, "driver", "drivers")}.` +
          (result.failed ? ` ${plural(result.failed, "driver", "drivers")} failed.` : "") +
          (result.rebootRequired ? " Restart your PC to finish." : ""),
      );
      await check();
    } catch (e) {
      setMessage(String(e));
      setBusy(null);
    }
  }

  return (
    <>
      <div className="toolbar">
        <span className="muted">
          Drivers come only from Windows Update, so they are tested and signed for your PC.
        </span>
        <button className="secondary" disabled={busy !== null} onClick={check}>
          {scan ? "Check again" : "Check for driver updates"}
        </button>
        {admin ? (
          <button className="primary" disabled={busy !== null || selected.size === 0} onClick={install}>
            Install {selected.size || ""} selected
          </button>
        ) : (
          <button
            className="primary"
            onClick={() => api.restartAsAdmin().catch((e) => setMessage(String(e)))}
          >
            Restart as administrator to install
          </button>
        )}
      </div>
      {message && <div className="banner">{message}</div>}
      <section className="results">
        {busy && (
          <div className="empty">
            <p>
              {busy === "scan"
                ? "Asking Windows Update. This can take a minute..."
                : "Downloading and installing..."}
            </p>
            <progress />
          </div>
        )}
        {!busy && !scan && (
          <div className="empty">
            <p>Press Check for driver updates to look for newer drivers.</p>
          </div>
        )}
        {!busy && scan && (
          <>
            {scan.updates.length === 0 && (
              <div className="empty">
                <p className="good">Windows Update has no newer drivers for this PC.</p>
              </div>
            )}
            {scan.updates.length > 0 && (
              <table>
                <thead>
                  <tr>
                    <th />
                    <th>Driver</th>
                    <th>Type</th>
                    <th className="num">Date</th>
                    <th className="num">Size</th>
                  </tr>
                </thead>
                <tbody>
                  {scan.updates.map((u) => (
                    <tr key={u.id}>
                      <td>
                        <input
                          type="checkbox"
                          checked={selected.has(u.id)}
                          onChange={() => {
                            const next = new Set(selected);
                            if (next.has(u.id)) next.delete(u.id);
                            else next.add(u.id);
                            setSelected(next);
                          }}
                        />
                      </td>
                      <td>
                        {u.title}
                        <div className="muted small">{u.manufacturer}</div>
                      </td>
                      <td className="muted">{u.driverClass}</td>
                      <td className="num muted">{u.date ? formatDate(u.date) : ""}</td>
                      <td className="num">{u.sizeBytes ? formatBytes(u.sizeBytes) : ""}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            {scan.problemDevices.length > 0 && (
              <>
                <h2 className="section-title">Devices with a problem</h2>
                <table>
                  <tbody>
                    {scan.problemDevices.map((d, i) => (
                      <tr key={i}>
                        <td>
                          {d.name || "Unknown device"}
                          <div className="muted small">{d.class}</div>
                        </td>
                        <td className="warn">{d.description}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <p className="muted small">
                  If Windows Update has no driver for these, the manufacturer's website is the next place to
                  look.
                </p>
              </>
            )}
          </>
        )}
      </section>
    </>
  );
}

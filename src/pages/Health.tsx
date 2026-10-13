import { useEffect, useState } from "react";
import { api, securityApi, storageApi, toolsApi, updatesApi } from "../api";
import { formatBytes, plural } from "../format";
import { problems } from "./Security";

export type Destination = "cleaner" | "storage" | "tools" | "updates" | "security";

type State = "checking" | "ok" | "warn" | "unknown";

interface Check {
  state: State;
  title: string;
  detail: string;
}

type CheckId = "security" | "junk" | "software" | "disk" | "startup" | "drivers";

/** How many points each check is worth out of 100. */
const WEIGHTS: Record<CheckId, number> = {
  security: 30,
  junk: 20,
  software: 20,
  disk: 15,
  startup: 10,
  drivers: 5,
};

const ORDER: { id: CheckId; label: string; goTo: Destination; goLabel: string }[] = [
  { id: "security", label: "Virus protection", goTo: "security", goLabel: "Open Security" },
  { id: "junk", label: "Junk files", goTo: "cleaner", goLabel: "Open Cleaner" },
  { id: "software", label: "Program updates", goTo: "updates", goLabel: "Open Updates" },
  { id: "disk", label: "Free space", goTo: "storage", goLabel: "Open Storage" },
  { id: "startup", label: "Startup programs", goTo: "tools", goLabel: "Open Startup programs" },
  { id: "drivers", label: "Drivers", goTo: "updates", goLabel: "Open Updates" },
];

const JUNK_LIMIT = 500 * 1024 * 1024;
const STARTUP_LIMIT = 8;

const checking = (title: string): Check => ({ state: "checking", title, detail: "" });

export default function Health({ onNavigate }: { onNavigate: (to: Destination) => void }) {
  const [checks, setChecks] = useState<Record<CheckId, Check>>(initial);
  const [fixing, setFixing] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const set = (id: CheckId, check: Check) => setChecks((c) => ({ ...c, [id]: check }));
  const fail = (id: CheckId) => (e: unknown) =>
    set(id, { state: "unknown", title: "Could not check", detail: String(e) });

  function runChecks() {
    setChecks(initial());
    setMessage(null);

    securityApi
      .status()
      .then((s) => {
        const issues = s.runningMode.toLowerCase().includes("passive") ? [] : problems(s);
        set(
          "security",
          issues.length
            ? { state: "warn", title: "Your PC is not fully protected", detail: issues.join(" ") }
            : { state: "ok", title: "Protected", detail: "Real-time protection is on and up to date." },
        );
      })
      .catch(fail("security"));

    api
      .measureJunk()
      .then((bytes) =>
        set(
          "junk",
          bytes > JUNK_LIMIT
            ? {
                state: "warn",
                title: `${formatBytes(bytes)} can be cleaned`,
                detail: "Temporary files, caches and the Recycle Bin.",
              }
            : {
                state: "ok",
                title: bytes ? `Only ${formatBytes(bytes)} of junk` : "No junk found",
                detail: "",
              },
        ),
      )
      .catch(fail("junk"));

    updatesApi
      .listSoftware()
      .then((u) =>
        set(
          "software",
          u.length
            ? {
                state: "warn",
                title: `${plural(u.length, "program has", "programs have")} an update`,
                detail:
                  u
                    .slice(0, 4)
                    .map((x) => x.name)
                    .join(", ") + (u.length > 4 ? ", ..." : ""),
              }
            : { state: "ok", title: "All programs are up to date", detail: "" },
        ),
      )
      .catch(fail("software"));

    storageApi
      .listDrives()
      .then((drives) => {
        const system = drives.find((d) => d.mountPoint.toUpperCase().startsWith("C:")) ?? drives[0];
        if (!system) return set("disk", { state: "unknown", title: "No drives found", detail: "" });
        const share = system.freeBytes / system.totalBytes;
        set("disk", {
          state: share < 0.1 ? "warn" : "ok",
          title: `${formatBytes(system.freeBytes)} free on ${system.mountPoint}`,
          detail: share < 0.1 ? "Windows slows down when the system drive is almost full." : "",
        });
      })
      .catch(fail("disk"));

    toolsApi
      .listStartup()
      .then((items) => {
        const on = items.filter((i) => i.enabled).length;
        set("startup", {
          state: on > STARTUP_LIMIT ? "warn" : "ok",
          title: `${plural(on, "program starts", "programs start")} with Windows`,
          detail: on > STARTUP_LIMIT ? "Turning off ones you don't need makes Windows start faster." : "",
        });
      })
      .catch(fail("startup"));

    updatesApi
      .scanDrivers()
      .then((scan) => {
        const n = scan.updates.length;
        const broken = scan.problemDevices.length;
        set("drivers", {
          state: n || broken ? "warn" : "ok",
          title: n
            ? `${plural(n, "driver update", "driver updates")} from Windows Update`
            : "Drivers are up to date",
          detail: broken ? `${plural(broken, "device has", "devices have")} a problem.` : "",
        });
      })
      .catch(fail("drivers"));
  }

  useEffect(runChecks, []);

  const done = ORDER.filter((o) => checks[o.id].state !== "checking");
  const score = Math.round(
    ORDER.reduce((n, o) => n + (checks[o.id].state === "warn" ? 0 : WEIGHTS[o.id]), 0),
  );
  const allDone = done.length === ORDER.length;
  const canFix = checks.junk.state === "warn" || checks.software.state === "warn";

  async function fixAll() {
    setConfirming(false);
    const parts: string[] = [];
    try {
      if (checks.junk.state === "warn") {
        setFixing("Cleaning junk files...");
        parts.push(`${formatBytes(await api.cleanJunkNow())} cleaned`);
      }
      if (checks.software.state === "warn") {
        setFixing("Updating programs...");
        await updatesApi.updateSoftware(null);
        parts.push("programs updated");
      }
      setMessage(`Done: ${parts.join(", ")}.`);
    } catch (e) {
      setMessage(String(e));
    } finally {
      setFixing(null);
      runChecks();
    }
  }

  return (
    <div className="page">
      <header className="page-header">
        <h1>Health check</h1>
        <div className="actions">
          <button className="secondary" disabled={!allDone || fixing !== null} onClick={runChecks}>
            Check again
          </button>
          <button
            className="primary"
            disabled={!allDone || !canFix || fixing !== null}
            onClick={() => setConfirming(true)}
          >
            {fixing ?? "Fix all"}
          </button>
        </div>
      </header>
      {message && <div className="banner">{message}</div>}

      <section className={`hero health ${!allDone ? "" : score >= 80 ? "ok" : "bad"}`}>
        <div className="score">{allDone ? score : "..."}</div>
        <div>
          <div className="hero-title">
            {!allDone
              ? `Checking your PC (${done.length} of ${ORDER.length})...`
              : score >= 80
                ? "Your PC is in good shape"
                : "Your PC needs attention"}
          </div>
          <p className="muted">
            Out of 100. Checks protection, junk, updates, free space, startup programs and drivers.
          </p>
        </div>
      </section>

      <section className="results">
        <ul className="health-list">
          {ORDER.map((o) => {
            const c = checks[o.id];
            return (
              <li key={o.id} className={`state-${c.state}`}>
                <span className="dot" />
                <div className="health-text">
                  <span className="muted small">{o.label}</span>
                  <strong>{c.state === "checking" ? "Checking..." : c.title}</strong>
                  {c.detail && <span className="muted small">{c.detail}</span>}
                </div>
                {c.state !== "checking" && (
                  <button className="link" onClick={() => onNavigate(o.goTo)}>
                    {o.goLabel}
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      </section>

      {confirming && (
        <div className="overlay">
          <div className="dialog">
            <h2>Fix all?</h2>
            <ul>
              {checks.junk.state === "warn" && (
                <li>Clean {checks.junk.title.replace(" can be cleaned", "")} of junk files</li>
              )}
              {checks.software.state === "warn" && <li>Update every program that has an update</li>}
            </ul>
            <p className="muted">
              Protection, startup programs and drivers need your choice, so open those pages to fix them.
            </p>
            <div className="actions">
              <button className="secondary" onClick={() => setConfirming(false)}>
                Cancel
              </button>
              <button className="primary" onClick={fixAll}>
                Fix
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function initial(): Record<CheckId, Check> {
  return {
    security: checking("Virus protection"),
    junk: checking("Junk files"),
    software: checking("Program updates"),
    disk: checking("Free space"),
    startup: checking("Startup programs"),
    drivers: checking("Drivers"),
  };
}

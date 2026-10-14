import { useEffect, useState } from "react";
import { api, updatesApi, type DriverScan, type SoftwareUpdate } from "../api";
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

function Software() {
  const [updates, setUpdates] = useState<SoftwareUpdate[] | null>(null);
  const [checking, setChecking] = useState(false);
  /** Package ids being updated right now; "*" means all. */
  const [working, setWorking] = useState<Set<string>>(new Set());
  const [message, setMessage] = useState<string | null>(null);

  async function check() {
    setChecking(true);
    setMessage(null);
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

  async function update(id: string | null) {
    const key = id ?? "*";
    setWorking((w) => new Set(w).add(key));
    setMessage(null);
    try {
      await updatesApi.updateSoftware(id);
      if (id) setUpdates((u) => (u ?? []).filter((x) => x.id !== id));
      else await check();
      setMessage(id ? "Updated." : "All programs that could be updated have been updated.");
    } catch (e) {
      setMessage(String(e));
    } finally {
      setWorking((w) => {
        const next = new Set(w);
        next.delete(key);
        return next;
      });
    }
  }

  const all = working.has("*");
  return (
    <>
      <div className="toolbar">
        <span className="muted">
          Updates come from winget, the app installer built into Windows. Old versions are a common way in for
          viruses.
        </span>
        <button className="secondary" disabled={checking || all} onClick={check}>
          Check again
        </button>
        <button
          className="primary"
          disabled={checking || all || !updates?.length}
          onClick={() => update(null)}
        >
          {all ? "Updating..." : "Update all"}
        </button>
      </div>
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
                <th />
              </tr>
            </thead>
            <tbody>
              {updates.map((u) => (
                <tr key={u.id}>
                  <td>
                    {u.name}
                    <div className="muted small">{u.id}</div>
                  </td>
                  <td className="muted">{u.version}</td>
                  <td>{u.available}</td>
                  <td className="num">
                    <button className="link" disabled={all || working.has(u.id)} onClick={() => update(u.id)}>
                      {working.has(u.id) ? "Updating..." : "Update"}
                    </button>
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

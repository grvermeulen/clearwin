import { useEffect, useState, type ReactNode } from "react";
import { performanceApi, type HeavyApp, type PowerPlan, type SystemInfo } from "../api";
import { formatBytes, formatDate } from "../format";

type Tab = "speed" | "system";

export default function Performance() {
  const [tab, setTab] = useState<Tab>("speed");
  return (
    <div className="page">
      <header className="page-header">
        <h1>Performance</h1>
        <div className="tabs">
          <button className={tab === "speed" ? "active" : ""} onClick={() => setTab("speed")}>
            Speed up
          </button>
          <button className={tab === "system" ? "active" : ""} onClick={() => setTab("system")}>
            System info
          </button>
        </div>
      </header>
      <div hidden={tab !== "speed"} className="tab-body">
        <SpeedUp />
      </div>
      <div hidden={tab !== "system"} className="tab-body">
        {tab === "system" && <System />}
      </div>
    </div>
  );
}

function SpeedUp() {
  const [plans, setPlans] = useState<PowerPlan[] | null>(null);
  const [apps, setApps] = useState<HeavyApp[] | null>(null);
  const [ending, setEnding] = useState<HeavyApp | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const loadPlans = () =>
    performanceApi
      .powerPlans()
      .then(setPlans)
      .catch((e) => setMessage(String(e)));
  const loadApps = () => {
    setApps(null);
    performanceApi
      .heavyApps()
      .then(setApps)
      .catch((e) => setMessage(String(e)));
  };
  useEffect(() => {
    loadPlans();
    loadApps();
  }, []);

  async function choosePlan(plan: PowerPlan) {
    setMessage(null);
    try {
      await performanceApi.setPowerPlan(plan.guid);
      await loadPlans();
    } catch (e) {
      setMessage(String(e));
    }
  }

  async function end(app: HeavyApp) {
    setEnding(null);
    try {
      await performanceApi.endApp(app.name);
      setMessage(`${app.name} was closed.`);
      loadApps();
    } catch (e) {
      setMessage(String(e));
    }
  }

  return (
    <>
      {message && <div className="banner">{message}</div>}
      <section className="results">
        <section className="card">
          <h2>Power plan</h2>
          <p className="muted">
            "High performance" keeps the processor at full speed. "Balanced" saves energy and is best for
            laptops on battery.
          </p>
          {plans === null && <p className="muted">Loading...</p>}
          <div className="plans">
            {plans?.map((p) => (
              <label key={p.guid} className={`plan ${p.active ? "active" : ""}`}>
                <input type="radio" name="plan" checked={p.active} onChange={() => choosePlan(p)} />
                {p.name}
              </label>
            ))}
          </div>
        </section>

        <div className="toolbar">
          <h2 className="section-title">Programs using the most memory</h2>
          <button className="secondary" onClick={loadApps}>
            Refresh
          </button>
        </div>
        <p className="muted">
          Closing programs you are not using frees memory and processor time. Windows itself is not listed.
        </p>
        {apps === null && (
          <div className="empty">
            <p>Measuring...</p>
            <progress />
          </div>
        )}
        {apps && (
          <table>
            <thead>
              <tr>
                <th>Program</th>
                <th className="num">Memory</th>
                <th className="num">Processor</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {apps.map((a) => (
                <tr key={a.name}>
                  <td>
                    {a.name}
                    {a.processes > 1 && <span className="muted small"> ({a.processes} processes)</span>}
                  </td>
                  <td className="num">{formatBytes(a.memoryBytes)}</td>
                  <td className="num">{a.cpuPercent.toFixed(0)}%</td>
                  <td className="num">
                    <button className="link" onClick={() => setEnding(a)}>
                      Close
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
      {ending && (
        <div className="overlay">
          <div className="dialog">
            <h2>Close {ending.name}?</h2>
            <p>Like "End task" in Task Manager. Anything not saved in this program is lost.</p>
            <div className="actions">
              <button className="secondary" onClick={() => setEnding(null)}>
                Cancel
              </button>
              <button className="primary danger" onClick={() => end(ending)}>
                Close program
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <tr>
      <td className="muted nowrap">{label}</td>
      <td>{children}</td>
    </tr>
  );
}

function System() {
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    performanceApi
      .systemInfo()
      .then(setInfo)
      .catch((e) => setError(String(e)));
  }, []);

  if (error) return <div className="banner error">{error}</div>;
  if (!info)
    return (
      <div className="empty">
        <p>Reading system information...</p>
        <progress />
      </div>
    );

  return (
    <section className="results">
      <div className="facts">
        <div className="fact">
          <span className="muted small">Processor</span>
          <strong>{info.cpu}</strong>
          <span className="muted small">
            {info.cores} cores, {info.threads} threads
            {info.cpuMhz ? `, ${(info.cpuMhz / 1000).toFixed(1)} GHz` : ""}
          </span>
        </div>
        <div className="fact">
          <span className="muted small">Memory</span>
          <strong>{formatBytes(info.memoryBytes)}</strong>
          <span className="muted small">
            {info.memoryModules.length} module{info.memoryModules.length === 1 ? "" : "s"}
            {info.memoryModules[0]?.speedMhz ? `, ${info.memoryModules[0].speedMhz} MHz` : ""}
          </span>
        </div>
        <div className="fact">
          <span className="muted small">Windows</span>
          <strong>{info.os.replace("Microsoft ", "")}</strong>
          <span className="muted small">Build {info.osBuild}</span>
        </div>
        {info.batteryPercent !== null && (
          <div className="fact">
            <span className="muted small">Battery</span>
            <strong>{info.batteryPercent}%</strong>
          </div>
        )}
      </div>

      <h2 className="section-title">Disks</h2>
      <table>
        <tbody>
          {info.disks.map((d, i) => (
            <tr key={i}>
              <td>
                {d.name}
                <div className="muted small">
                  {[d.media, d.bus].filter((x) => x && x !== "Unspecified").join(", ")}
                </div>
              </td>
              <td className="num">{formatBytes(d.sizeBytes)}</td>
              <td className={d.health === "Healthy" ? "good" : "warn"}>{d.health || "Unknown"}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2 className="section-title">Details</h2>
      <table>
        <tbody>
          <Row label="Computer">
            {info.computer}
            {info.manufacturer && ` (${info.manufacturer} ${info.model})`}
          </Row>
          <Row label="Windows version">{info.osVersion}</Row>
          {info.installed && <Row label="Windows installed">{formatDate(info.installed)}</Row>}
          {info.lastBoot && <Row label="Last restart">{formatDate(info.lastBoot)}</Row>}
          {info.gpus.map((g, i) => (
            <Row key={i} label="Graphics">
              {g.name}
              <div className="muted small">
                Driver {g.driverVersion}
                {g.resolution && `, ${g.resolution}`}
              </div>
            </Row>
          ))}
          {info.memoryModules.map((m, i) => (
            <Row key={i} label={`Memory ${m.slot}`}>
              {formatBytes(m.capacityBytes)}
              {m.speedMhz ? ` at ${m.speedMhz} MHz` : ""}
              {m.manufacturer && <span className="muted"> ({m.manufacturer})</span>}
            </Row>
          ))}
          <Row label="Motherboard">{info.motherboard}</Row>
          <Row label="BIOS">{info.bios}</Row>
        </tbody>
      </table>
    </section>
  );
}

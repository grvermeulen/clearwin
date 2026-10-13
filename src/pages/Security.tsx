import { useEffect, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { securityApi, type DefenderStatus, type ScanKind, type Threat } from "../api";
import { formatDate, plural } from "../format";

const SCAN_LABELS: Record<ScanKind, string> = {
  quick: "Quick scan",
  full: "Full scan",
  custom: "Folder scan",
};

function when(unixSeconds: number | null): string {
  return unixSeconds ? new Date(unixSeconds * 1000).toLocaleString() : "Never";
}

/** Problems worth telling the user about, in plain words. */
function problems(s: DefenderStatus): string[] {
  const out: string[] = [];
  if (!s.antivirusEnabled) out.push("Microsoft Defender antivirus is turned off.");
  else if (!s.realTimeProtection) out.push("Real-time protection is off, so new files are not checked as they arrive.");
  if (s.signatureAgeDays > 3) out.push(`Virus definitions are ${s.signatureAgeDays} days old.`);
  return out;
}

export default function Security() {
  const [status, setStatus] = useState<DefenderStatus | null>(null);
  const [threats, setThreats] = useState<Threat[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [scanning, setScanning] = useState<ScanKind | null>(null);
  const [updating, setUpdating] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  async function load() {
    try {
      setStatus(await securityApi.status());
      setThreats(await securityApi.threats());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    load();
  }, []);

  async function scan(kind: ScanKind) {
    let path: string | undefined;
    if (kind === "custom") {
      const picked = await open({ directory: true, multiple: false });
      if (typeof picked !== "string") return;
      path = picked;
    }
    setMessage(null);
    setScanning(kind);
    const before = threats.length;
    try {
      await securityApi.scan(kind, path);
      await load();
      const found = (await securityApi.threats()).length - before;
      setMessage(
        found > 0
          ? `${SCAN_LABELS[kind]} finished and found ${plural(found, "threat", "threats")}. See the list below.`
          : `${SCAN_LABELS[kind]} finished. No threats found.`,
      );
    } catch (e) {
      setMessage(String(e));
    } finally {
      setScanning(null);
    }
  }

  async function update() {
    setUpdating(true);
    setMessage(null);
    try {
      await securityApi.update();
      await load();
      setMessage("Virus definitions are up to date.");
    } catch (e) {
      setMessage(String(e));
    } finally {
      setUpdating(false);
    }
  }

  const passive = status && status.runningMode && status.runningMode !== "Normal";
  const others = status?.products.filter((p) => !/defender/i.test(p)) ?? [];
  const issues = status && !passive ? problems(status) : [];

  return (
    <div className="page">
      <header className="page-header">
        <h1>Security</h1>
        <div className="actions">
          <button className="secondary" onClick={() => securityApi.openWindowsSecurity()}>
            Open Windows Security
          </button>
        </div>
      </header>
      {error && (
        <div className="banner error">
          Could not reach Microsoft Defender: {error}
        </div>
      )}
      {message && <div className="banner">{message}</div>}

      {status && (
        <>
          <section className={`hero ${passive || issues.length === 0 ? "ok" : "bad"}`}>
            <div className="hero-title">
              {passive
                ? `Protected by ${others[0] ?? "another antivirus"}`
                : issues.length === 0
                  ? "Your PC is protected"
                  : "Your PC needs attention"}
            </div>
            {passive ? (
              <p>
                Microsoft Defender is in {status.runningMode.toLowerCase()} because another antivirus is active. You
                can still run Defender scans below.
              </p>
            ) : issues.length === 0 ? (
              <p>Microsoft Defender is scanning files continuously and its virus definitions are current.</p>
            ) : (
              <ul>
                {issues.map((p) => (
                  <li key={p}>{p}</li>
                ))}
              </ul>
            )}
            {issues.length > 0 && (
              <button className="secondary" onClick={() => securityApi.openWindowsSecurity()}>
                Fix in Windows Security
              </button>
            )}
          </section>

          <div className="facts">
            <Fact label="Real-time protection" value={status.realTimeProtection ? "On" : "Off"} good={status.realTimeProtection} />
            <Fact label="Tamper protection" value={status.tamperProtection ? "On" : "Off"} good={status.tamperProtection} />
            <Fact
              label="Virus definitions"
              value={`Updated ${formatDate(status.signatureUpdated)}`}
              detail={status.signatureVersion}
              good={status.signatureAgeDays <= 3}
              action={
                <button className="link" disabled={updating} onClick={update}>
                  {updating ? "Updating..." : "Update now"}
                </button>
              }
            />
            <Fact label="Last quick scan" value={when(status.quickScanEnded)} />
            <Fact label="Last full scan" value={when(status.fullScanEnded)} />
          </div>

          <section className="results">
            <div className="toolbar">
              <span>
                {scanning
                  ? `${SCAN_LABELS[scanning]} running. A full scan can take an hour; you can keep using ClearWin.`
                  : "Run a scan any time. Defender also checks new files on its own."}
              </span>
              {(["quick", "full", "custom"] as ScanKind[]).map((kind) => (
                <button key={kind} className={kind === "quick" ? "primary" : "secondary"} disabled={!!scanning} onClick={() => scan(kind)}>
                  {kind === "custom" ? "Scan a folder..." : SCAN_LABELS[kind]}
                </button>
              ))}
            </div>
            {scanning && <progress className="wide" />}

            <h2 className="section-title">Threat history</h2>
            {threats.length === 0 ? (
              <p className="muted">No threats have been found on this PC.</p>
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>Threat</th>
                    <th>Severity</th>
                    <th>Status</th>
                    <th className="num">Found</th>
                  </tr>
                </thead>
                <tbody>
                  {threats.map((t, i) => (
                    <tr key={i}>
                      <td className="path-cell">
                        {t.name}
                        {t.resources.map((r) => (
                          <div key={r} className="muted small path" title={r}>
                            {r}
                          </div>
                        ))}
                      </td>
                      <td>{t.severity}</td>
                      <td className={t.handled ? "good" : "warn"}>{t.status}</td>
                      <td className="num muted">{formatDate(t.detected)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </section>
        </>
      )}
      {!status && !error && <div className="empty">Checking Microsoft Defender...</div>}
    </div>
  );
}

function Fact({
  label,
  value,
  detail,
  good,
  action,
}: {
  label: string;
  value: string;
  detail?: string;
  good?: boolean;
  action?: ReactNode;
}) {
  return (
    <div className="fact">
      <span className="muted small">{label}</span>
      <strong className={good === undefined ? "" : good ? "good" : "warn"}>{value}</strong>
      {detail && <span className="muted small">{detail}</span>}
      {action}
    </div>
  );
}

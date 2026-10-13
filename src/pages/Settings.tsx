import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import type { Update } from "@tauri-apps/plugin-updater";
import { settingsApi, type HistoryEntry, type Schedule, type Settings as SettingsData } from "../api";
import { formatBytes, plural } from "../format";
import { findUpdate, installUpdate } from "../updates";

export default function Settings() {
  const [settings, setSettings] = useState<SettingsData | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    settingsApi
      .get()
      .then(setSettings)
      .catch((e) => setMessage(String(e)));
    settingsApi
      .history()
      .then(setHistory)
      .catch(() => setHistory([]));
  }, []);

  async function save(action: Promise<SettingsData>, done?: string) {
    try {
      setSettings(await action);
      setMessage(done ?? null);
    } catch (e) {
      setMessage(String(e));
    }
  }

  if (!settings)
    return <div className="page">{message && <div className="banner error">{message}</div>}</div>;

  return (
    <div className="page">
      <header className="page-header">
        <h1>Settings</h1>
      </header>
      {message && <div className="banner">{message}</div>}

      <ScheduleCard
        schedule={settings.schedule}
        onSave={(s) =>
          save(settingsApi.setSchedule(s), s ? "Automatic cleaning is on." : "Automatic cleaning is off.")
        }
      />

      <section className="card">
        <h2>Clean browsers when they close</h2>
        <p className="muted">
          ClearWin runs quietly in the background and cleans a browser the moment you close it, using the
          browser items ticked in the Cleaner. It starts with Windows while this is on.
        </p>
        <label className="check-row">
          <input
            type="checkbox"
            checked={settings.cleanOnBrowserClose}
            onChange={(e) =>
              save(
                settingsApi.setBrowserCloseCleaning(e.target.checked),
                e.target.checked
                  ? "Browsers are now cleaned when you close them."
                  : "Browsers are no longer cleaned when you close them.",
              )
            }
          />
          Clean automatically when I close a browser
        </label>
      </section>

      <section className="card">
        <h2>Never clean</h2>
        <p className="muted">
          The Cleaner skips these folders and files, including during automatic cleaning.
        </p>
        {settings.exclusions.length === 0 && <p className="muted">Nothing excluded.</p>}
        <ul className="plain-list">
          {settings.exclusions.map((p) => (
            <li key={p}>
              <span className="path">{p}</span>
              <button
                className="link"
                onClick={() => save(settingsApi.setExclusions(settings.exclusions.filter((x) => x !== p)))}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
        <div className="actions left">
          <button
            className="secondary"
            onClick={async () => {
              const picked = await open({ directory: true, multiple: false });
              if (typeof picked === "string" && !settings.exclusions.includes(picked)) {
                save(settingsApi.setExclusions([...settings.exclusions, picked]));
              }
            }}
          >
            Add folder...
          </button>
          <button
            className="secondary"
            onClick={async () => {
              const picked = await open({ directory: false, multiple: false });
              if (typeof picked === "string" && !settings.exclusions.includes(picked)) {
                save(settingsApi.setExclusions([...settings.exclusions, picked]));
              }
            }}
          >
            Add file...
          </button>
        </div>
      </section>

      <UpdatesCard />

      <section className="card">
        <h2>History</h2>
        {history.length === 0 ? (
          <p className="muted">Nothing cleaned yet.</p>
        ) : (
          <table>
            <tbody>
              {history.slice(0, 100).map((h, i) => (
                <tr key={i}>
                  <td className="nowrap muted">{new Date(h.time * 1000).toLocaleString()}</td>
                  <td>
                    {h.action}
                    {h.details && <div className="muted small">{h.details}</div>}
                  </td>
                  <td className="num">{plural(h.items, "item", "items")}</td>
                  <td className="num">{h.bytes ? formatBytes(h.bytes) : ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}

function ScheduleCard({
  schedule,
  onSave,
}: {
  schedule: Schedule | null;
  onSave: (s: Schedule | null) => void;
}) {
  const [frequency, setFrequency] = useState<Schedule["frequency"]>(schedule?.frequency ?? "weekly");
  const [time, setTime] = useState(schedule?.time ?? "10:00");

  return (
    <section className="card">
      <h2>Automatic cleaning</h2>
      <p className="muted">
        Runs the Cleaner in the background with the items you ticked on the Cleaner page. Open browsers are
        skipped.
      </p>
      <div className="actions left">
        <select value={frequency} onChange={(e) => setFrequency(e.target.value as Schedule["frequency"])}>
          <option value="daily">Every day</option>
          <option value="weekly">Every Sunday</option>
        </select>
        <span className="muted">at</span>
        <input type="time" value={time} onChange={(e) => setTime(e.target.value)} />
        <button className="primary" onClick={() => onSave({ frequency, time })}>
          {schedule ? "Update schedule" : "Turn on"}
        </button>
        {schedule && (
          <button className="secondary" onClick={() => onSave(null)}>
            Turn off
          </button>
        )}
      </div>
      {schedule && (
        <p className="good small">
          On: {schedule.frequency === "daily" ? "every day" : "every Sunday"} at {schedule.time}.
        </p>
      )}
    </section>
  );
}

function UpdatesCard() {
  const [version, setVersion] = useState("");
  const [state, setState] = useState<"idle" | "checking" | "current" | "installing">("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setVersion);
  }, []);

  async function checkNow() {
    setError(null);
    setState("checking");
    try {
      const found = await findUpdate();
      setUpdate(found);
      setState(found ? "idle" : "current");
    } catch (e) {
      setError(String(e));
      setState("idle");
    }
  }

  async function install() {
    if (!update) return;
    setState("installing");
    try {
      await installUpdate(update);
    } catch (e) {
      setError(String(e));
      setState("idle");
    }
  }

  return (
    <section className="card">
      <h2>Updates</h2>
      <p className="muted">
        You're on ClearWin {version}. ClearWin checks for new versions when it starts, and installs them
        without the Windows warning.
      </p>
      <div className="actions left">
        {update ? (
          <button className="primary" disabled={state === "installing"} onClick={install}>
            {state === "installing" ? "Installing..." : `Install version ${update.version} and restart`}
          </button>
        ) : (
          <button className="secondary" disabled={state === "checking"} onClick={checkNow}>
            {state === "checking" ? "Checking..." : "Check for updates"}
          </button>
        )}
        {state === "current" && <span className="good">You have the latest version.</span>}
        {error && <span className="warn">{error}</span>}
      </div>
    </section>
  );
}

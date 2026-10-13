import { useEffect, useState, type ReactElement } from "react";
import type { Update } from "@tauri-apps/plugin-updater";
import Cleaner from "./pages/Cleaner";
import Storage from "./pages/Storage";
import Security from "./pages/Security";
import Settings from "./pages/Settings";
import Tools from "./pages/Tools";
import { findUpdate, installUpdate } from "./updates";

type Page = "cleaner" | "storage" | "tools" | "security" | "settings";

const NAV: { id: Page; label: string }[] = [
  { id: "cleaner", label: "Cleaner" },
  { id: "storage", label: "Storage" },
  { id: "tools", label: "Tools" },
  { id: "security", label: "Security" },
  { id: "settings", label: "Settings" },
];

const PAGES: Record<Page, ReactElement> = {
  cleaner: <Cleaner />,
  storage: <Storage />,
  tools: <Tools />,
  security: <Security />,
  settings: <Settings />,
};

export default function App() {
  const [page, setPage] = useState<Page>("cleaner");
  // Pages stay mounted once opened, so switching away doesn't lose results.
  const [visited, setVisited] = useState<Set<Page>>(new Set(["cleaner"]));
  const [update, setUpdate] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    // Offline or no release yet: just try again next start.
    findUpdate()
      .then(setUpdate)
      .catch(() => setUpdate(null));
  }, []);

  return (
    <div className="app">
      <nav className="sidebar">
        <div className="brand">
          <img src="/icon.png" alt="" />
          ClearWin
        </div>
        {NAV.map((item) => (
          <button
            key={item.id}
            className={`nav-item ${item.id === page ? "active" : ""}`}
            onClick={() => {
              setPage(item.id);
              setVisited((v) => new Set(v).add(item.id));
            }}
          >
            {item.label}
          </button>
        ))}
      </nav>
      <main className="content">
        {update && (
          <div className="update-bar">
            ClearWin {update.version} is available.
            <button
              className="link"
              disabled={installing}
              onClick={() => {
                setInstalling(true);
                installUpdate(update).catch(() => setInstalling(false));
              }}
            >
              {installing ? "Installing..." : "Install and restart"}
            </button>
          </div>
        )}
        {NAV.filter((n) => visited.has(n.id)).map((n) => (
          <div key={n.id} hidden={n.id !== page} className="page-host">
            {PAGES[n.id]}
          </div>
        ))}
      </main>
    </div>
  );
}

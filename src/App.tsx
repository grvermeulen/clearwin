import { useEffect, useState, type ReactElement } from "react";
import type { Update } from "@tauri-apps/plugin-updater";
import Cleaner from "./pages/Cleaner";
import Health from "./pages/Health";
import Performance from "./pages/Performance";
import Storage from "./pages/Storage";
import Security from "./pages/Security";
import Settings from "./pages/Settings";
import Tools from "./pages/Tools";
import Updates from "./pages/Updates";
import { findUpdate, installUpdate } from "./updates";

type Page = "health" | "cleaner" | "storage" | "tools" | "updates" | "performance" | "security" | "settings";

const NAV: { id: Page; label: string }[] = [
  { id: "health", label: "Health check" },
  { id: "cleaner", label: "Cleaner" },
  { id: "storage", label: "Storage" },
  { id: "tools", label: "Tools" },
  { id: "updates", label: "Updates" },
  { id: "performance", label: "Performance" },
  { id: "security", label: "Security" },
  { id: "settings", label: "Settings" },
];

export default function App() {
  const [page, setPage] = useState<Page>("health");
  // Pages stay mounted once opened, so switching away doesn't lose results.
  const [visited, setVisited] = useState<Set<Page>>(new Set(["health"]));

  function go(to: Page) {
    setPage(to);
    setVisited((v) => new Set(v).add(to));
  }

  const pages: Record<Page, ReactElement> = {
    health: <Health onNavigate={go} />,
    cleaner: <Cleaner />,
    storage: <Storage />,
    tools: <Tools />,
    updates: <Updates />,
    performance: <Performance />,
    security: <Security />,
    settings: <Settings />,
  };
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
            onClick={() => go(item.id)}
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
            {pages[n.id]}
          </div>
        ))}
      </main>
    </div>
  );
}

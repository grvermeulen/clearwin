import { useState } from "react";
import Cleaner from "./pages/Cleaner";
import ComingSoon from "./pages/ComingSoon";
import Storage from "./pages/Storage";
import Security from "./pages/Security";
import Tools from "./pages/Tools";

type Page = "cleaner" | "storage" | "tools" | "security";

const NAV: { id: Page; label: string; soon?: string }[] = [
  { id: "cleaner", label: "Cleaner" },
  { id: "storage", label: "Storage" },
  { id: "tools", label: "Tools" },
  { id: "security", label: "Security" },
];

export default function App() {
  const [page, setPage] = useState<Page>("cleaner");
  const current = NAV.find((n) => n.id === page)!;

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
            onClick={() => setPage(item.id)}
          >
            {item.label}
            {item.soon && <span className="badge">Soon</span>}
          </button>
        ))}
      </nav>
      <main className="content">
        {current.soon ? (
          <ComingSoon title={current.label} text={current.soon} />
        ) : page === "storage" ? (
          <Storage />
        ) : page === "tools" ? (
          <Tools />
        ) : page === "security" ? (
          <Security />
        ) : (
          <Cleaner />
        )}
      </main>
    </div>
  );
}

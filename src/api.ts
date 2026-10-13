import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Mirrors the Rust types in crates/clearwin-core.

export interface Category {
  id: string;
  group: string;
  name: string;
  description: string;
  defaultSelected: boolean;
  needsAdmin: boolean;
  closeApp: string | null;
}

export interface FileEntry {
  path: string;
  size: number;
}

export interface CategorySummary {
  id: string;
  totalBytes: number;
  itemCount: number;
  blockedReason: string | null;
  largest: FileEntry[];
}

export interface CategoryClean {
  id: string;
  removedItems: number;
  freedBytes: number;
  failedItems: number;
  failures: string[];
  skippedReason: string | null;
}

export interface ScanProgress {
  id: string;
  done: number;
  total: number;
}

export const api = {
  listCategories: () => invoke<Category[]>("list_categories"),
  scan: (ids: string[]) => invoke<CategorySummary[]>("scan", { ids }),
  clean: (ids: string[]) => invoke<CategoryClean[]>("clean", { ids }),
  isAdmin: () => invoke<boolean>("is_admin"),
  restartAsAdmin: () => invoke<void>("restart_as_admin"),
  onScanProgress: (handler: (p: ScanProgress) => void): Promise<UnlistenFn> =>
    listen<ScanProgress>("scan-progress", (e) => handler(e.payload)),
};

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

export interface Drive {
  mountPoint: string;
  name: string;
  totalBytes: number;
  freeBytes: number;
}

export interface LevelEntry {
  name: string;
  size: number;
  fileCount: number;
  isDir: boolean;
  hasChildren: boolean;
  incomplete: boolean;
}

export interface Level {
  path: string;
  size: number;
  fileCount: number;
  entries: LevelEntry[];
}

export interface LargeFile {
  size: number;
  path: string;
  modified: number | null;
}

export interface DuplicateGroup {
  size: number;
  files: { path: string; modified: number | null }[];
}

export interface RecycleReport {
  moved: string[];
  failures: string[];
}

export interface StorageProgress {
  files: number;
  bytes: number;
}

export const storageApi = {
  listDrives: () => invoke<Drive[]>("list_drives"),
  homeFolder: () => invoke<string | null>("home_folder"),
  cancel: () => invoke<void>("cancel_storage"),
  analyze: (path: string) => invoke<Level>("analyze_storage", { path }),
  level: (segments: string[]) => invoke<Level>("storage_level", { segments }),
  largestFiles: () => invoke<LargeFile[]>("largest_files"),
  findDuplicates: (roots: string[], minSize: number) =>
    invoke<DuplicateGroup[]>("find_duplicates", { roots, minSize }),
  recycleDuplicates: (paths: string[]) => invoke<RecycleReport>("recycle_duplicates", { paths }),
  recycleLargeFiles: (paths: string[]) => invoke<RecycleReport>("recycle_large_files", { paths }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  onProgress: (handler: (p: StorageProgress) => void): Promise<UnlistenFn> =>
    listen<StorageProgress>("storage-progress", (e) => handler(e.payload)),
};

export interface StartupItem {
  id: string;
  name: string;
  command: string;
  location: string;
  enabled: boolean;
  targetMissing: boolean;
  needsAdmin: boolean;
}

export interface InstalledApp {
  id: string;
  name: string;
  publisher: string | null;
  version: string | null;
  installDate: string | null;
  sizeBytes: number | null;
}

export interface RegistryIssue {
  id: number;
  kind: string;
  key: string;
  value: string | null;
  missingPath: string;
  needsAdmin: boolean;
}

export interface FixReport {
  fixed: number;
  failures: string[];
  backup: string | null;
}

export interface BackupInfo {
  path: string;
  created: number;
  issues: RegistryIssue[];
}

export const toolsApi = {
  listStartup: () => invoke<StartupItem[]>("list_startup"),
  setStartupEnabled: (id: string, enabled: boolean) => invoke<void>("set_startup_enabled", { id, enabled }),
  listApps: () => invoke<InstalledApp[]>("list_apps"),
  uninstallApp: (id: string) => invoke<void>("uninstall_app", { id }),
  scanRegistry: () => invoke<RegistryIssue[]>("scan_registry"),
  fixRegistry: (ids: number[]) => invoke<FixReport>("fix_registry", { ids }),
  listBackups: () => invoke<BackupInfo[]>("list_registry_backups"),
  restoreBackup: (path: string) => invoke<number>("restore_registry_backup", { path }),
};

export interface DefenderStatus {
  serviceEnabled: boolean;
  antivirusEnabled: boolean;
  realTimeProtection: boolean;
  tamperProtection: boolean;
  runningMode: string;
  signatureVersion: string;
  signatureUpdated: number | null;
  signatureAgeDays: number;
  quickScanEnded: number | null;
  fullScanEnded: number | null;
  products: string[];
}

export interface Threat {
  name: string;
  severity: string;
  status: string;
  detected: number | null;
  resources: string[];
  handled: boolean;
}

export type ScanKind = "quick" | "full" | "custom";

export const securityApi = {
  status: () => invoke<DefenderStatus>("defender_status"),
  threats: () => invoke<Threat[]>("defender_threats"),
  scan: (kind: ScanKind, path?: string) => invoke<void>("defender_scan", { kind, path: path ?? null }),
  update: () => invoke<void>("defender_update"),
  openWindowsSecurity: () => invoke<void>("open_windows_security"),
};

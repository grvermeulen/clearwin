import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/** The newer version published on GitHub, or null when ClearWin is up to date. */
export async function findUpdate(): Promise<Update | null> {
  return await check();
}

/** Downloads and installs the update, then restarts ClearWin on the new version. */
export async function installUpdate(update: Update): Promise<void> {
  await update.downloadAndInstall();
  await relaunch();
}

# ClearWin

A personal Windows cleaner in the spirit of CCleaner, built with [Tauri](https://tauri.app)
(a Rust core with a web-based interface).

## Install

Download the latest installer from the
[Latest build](https://github.com/grvermeulen/clearwin/releases/tag/latest) release and run it.
The installer is not code-signed, so Windows SmartScreen warns you the first time: choose
**More info → Run anyway**. After that ClearWin updates itself without the warning.

## What it does

| Area         | Features                                                                                                                                                         |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Health check | One score out of 100 for protection, junk, program updates, free space, startup programs and drivers, with Fix all                                               |
| Cleaner      | Temp files, Recycle Bin, thumbnail and shader caches, crash dumps, Windows Update downloads, browser cache / history / cookies (Chrome, Edge, Brave, Firefox)    |
| Storage      | Space map of any drive or folder, the 500 largest files, duplicate finder (compares contents, with one-click OneDrive / Google Drive / Dropbox / iCloud folders) |
| Tools        | Startup programs on/off, uninstaller, registry cleanup with automatic backups and restore                                                                        |
| Updates      | Program updates through winget, driver updates from Windows Update, devices with a problem                                                                       |
| Performance  | Power plan switcher, programs using the most memory with Close, system info (CPU, memory, graphics, disk health)                                                 |
| Security     | Microsoft Defender status, quick / full / folder scans, definition updates, threat history                                                                       |
| Settings     | Scheduled automatic cleaning, clean a browser as soon as it closes, folders and files to never clean, activity history, auto-update                              |

### Safety rules

- **Preview first.** Analyze lists exactly what would be removed; Clean deletes only files
  from that list, and re-checks each one is still a plain file inside the expected folder.
- **Never follows links.** Symlinks and junctions are skipped, so cleaning can't escape a folder.
- **Recent temp files are kept.** Temp files changed in the last 24 hours are left alone.
- **Open browsers are skipped.** A browser's data is only cleaned while that browser is closed.
- **Bookmarks are kept.** Firefox history is removed row by row from its database, so bookmarks stay.
- **Storage deletions are recoverable.** Large files and duplicates go to the Recycle Bin, and the last
  copy of a duplicate can't be removed.
- **Registry changes are backed up.** Every affected key is exported to a `.reg` file first, and
  backups can be restored from the Registry tab. Only entries pointing at files that are gone are flagged.
- **Cloud files stay in the cloud.** The duplicate finder skips files that are only stored online, so
  nothing gets downloaded just to compare it.
- **Drivers only from Windows Update.** No third-party driver sites.
- **Startup changes are reversible.** Programs are switched off the same way Task Manager does it.

## Updates

Installed copies check the `latest` release for a newer version when they start and update
themselves, so the SmartScreen warning only appears on the very first install. Update packages are
signed with a separate updater key: add the private key as the repository secret
`TAURI_SIGNING_PRIVATE_KEY` (Settings → Secrets and variables → Actions). Without it the installer
still builds, but no update is published.

## Development

Requirements: Rust (stable), Node.js 22, and on Windows the WebView2 runtime (preinstalled on
Windows 10/11).

```sh
npm install
npm run tauri dev     # run the app with live reload
cargo test --workspace
npx tauri build       # build the installer into target/release/bundle/nsis
```

Layout:

- `crates/clearwin-core`: scanning and cleaning engine, no UI; unit-tested on any OS.
- `src-tauri`: the desktop app; exposes the engine to the interface as commands.
- `src`: the interface (React + TypeScript).

Every push and pull request is built on a Windows runner by GitHub Actions
(`.github/workflows/build.yml`), and pushes to `main` refresh the Latest build release.

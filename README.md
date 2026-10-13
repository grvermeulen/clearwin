# ClearWin

A personal Windows cleaner in the spirit of CCleaner, built with [Tauri](https://tauri.app)
(a Rust core with a web-based interface).

## Install

Download the latest installer from the
[Latest build](https://github.com/grvermeulen/clearwin/releases/tag/latest) release and run it.
The installer is not code-signed, so Windows SmartScreen may warn you: choose
**More info → Run anyway**.

## What it does

| Area     | Features                                                                 | Status  |
| -------- | ------------------------------------------------------------------------ | ------- |
| Cleaner  | Temp files, Recycle Bin, thumbnail and shader caches, crash dumps, Windows Update downloads, browser cache / history / cookies (Chrome, Edge, Brave, Firefox) | Done |
| Storage  | Big-folder map, large-file finder, duplicate finder                      | Planned |
| Tools    | Startup programs, uninstaller, registry cleanup with backups             | Planned |
| Security | Microsoft Defender status and scans                                      | Planned |

### Safety rules

- **Preview first.** Analyze lists exactly what would be removed; Clean deletes only files
  from that list, and re-checks each one is still a plain file inside the expected folder.
- **Never follows links.** Symlinks and junctions are skipped, so cleaning can't escape a folder.
- **Recent temp files are kept.** Temp files changed in the last 24 hours are left alone.
- **Open browsers are skipped.** A browser's data is only cleaned while that browser is closed.
- **Bookmarks are kept.** Firefox history is removed row by row from its database, so bookmarks stay.

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

# Easy Uninstaller

A lightweight, simple app to uninstall apps and free up space on Windows.
Built for everyone: plain language, big clear choices, and nothing is ever deleted
without a way back.

![Home](docs/screenshots/home.png)

> **Status: early skeleton.** The interface is in place, but apart from the disk
> usage and the file scan, every screen still shows sample data. See the roadmap below.

## Principles

- **Simple.** A guided home screen with four choices: Apps, Files, Leftovers, Space map.
- **Light.** Built with [Tauri 2](https://tauri.app): it uses the WebView2 engine already in
  Windows, so the installer stays at a few MB instead of 100+ MB.
- **Safe.** Everything removed goes to a quarantine for 7 days and can be restored with one
  click. A restore point is created before uninstalling programs.
- **Clear about risk.** Every file group is labelled *Safe*, *Check* or *Personal*.
- **English and Italian.** Picked from the system language, changeable in Settings.

## Screens

| Home | Apps | Files | Space map |
|---|---|---|---|
| ![](docs/screenshots/home.png) | ![](docs/screenshots/apps.png) | ![](docs/screenshots/files.png) | ![](docs/screenshots/map.png) |

The original design mockups are in [`docs/design`](docs/design).

## Roadmap

1. ✅ App skeleton: sidebar, screens, sample data, Windows CI build
2. ⬜ Installed apps: read Win32/MSI programs from the registry and Store apps (read only)
3. ✅ File scan: temp files, caches, used installers, old downloads, duplicates, large files (read only)
4. ⬜ Quarantine and restore
5. ⬜ Uninstall + leftovers cleanup (after 2 and 4)
6. ⬜ Space map with real data

## Development

Requirements: [Node.js 22](https://nodejs.org), [Rust](https://rustup.rs) and, on Windows,
the [Tauri prerequisites](https://tauri.app/start/prerequisites/) (WebView2 and the MSVC build tools).

```sh
npm install
npm run tauri dev     # run the desktop app with hot reload
npm run dev           # UI only, in the browser, with sample data
npm run check         # type-check the UI
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build   # build the Windows installer (src-tauri/target/release/bundle/nsis)
```

Every push and pull request is built and tested on Windows by GitHub Actions; the installer
is attached to the run as an artifact.

## Project layout

```
src/                    UI (Svelte 5 + TypeScript)
  lib/api.ts            the only place the UI talks to the system
  lib/types.ts          data shapes shared with Rust
  lib/mock.ts           sample data, removed piece by piece as scanners land
  lib/i18n/             translations: en.json (reference), it.json
  lib/screens/          one component per screen
  styles/theme.css      design tokens (colors, radii)
src-tauri/              backend (Rust)
  src/lib.rs            registers the commands
  src/disk.rs           system drive usage
  src/files.rs          file scan: temp, installers, old downloads, duplicates, large files
docs/                   screenshots and design mockups
```

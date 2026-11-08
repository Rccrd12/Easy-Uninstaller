# Easy Uninstaller

**Uninstall apps and free up space on Windows, simply and safely.**

Easy Uninstaller is a small, free app for everyone, not just for tech people. It speaks
plain language, offers a few big clear choices, and never deletes anything without a way
back: everything it removes waits in a quarantine for 7 days and comes back with one click.

![Easy Uninstaller home screen](docs/screenshots/home.png)

> [!NOTE]
> **Early preview.** Installed apps, the file scan, the quarantine and restore points are
> real. Uninstalling apps, the *Leftovers* screen and the *Space map* are still in progress
> and show sample data for now. See the [roadmap](#roadmap).

## Why another uninstaller?

- **Simple.** The home screen offers four choices: *Apps*, *Files*, *Leftovers* and
  *Space map*. No jargon, no settings you have to understand first.
- **Safe.** Nothing is deleted right away. See [How your files are protected](#how-your-files-are-protected).
- **Clear about risk.** Every group of files is labelled *Safe*, *Check* or *Personal*,
  so you know what you are removing before you remove it.
- **Light.** Built with [Tauri 2](https://tauri.app): it uses the WebView2 engine that is
  already part of Windows, so the installer is a few MB instead of 100+ MB.
- **Private.** It works entirely on your PC. No account, no ads, no tracking.
- **English and Italian.** Picked from your system language, changeable in Settings.

## Screenshots

| Apps | Files |
|---|---|
| ![Apps screen: installed programs and Store apps, sorted by size](docs/screenshots/apps.png) | ![Files screen: groups of files labelled Safe or Check](docs/screenshots/files.png) |
| **Quarantine** | **Space map** |
| ![Quarantine screen: removed items with Restore buttons](docs/screenshots/quarantine.png) | ![Space map: what takes up the most space](docs/screenshots/map.png) |

<sub>Screenshots use sample data.</sub>

## Download and install

1. Open the [latest release](../../releases/latest) and download
   `EasyUninstaller-<version>-setup.exe`.
2. Run it. It installs for your user only, so it does not ask for administrator rights.
3. Requirements: Windows 10 or 11 (64-bit). If the WebView2 runtime is missing (rare on
   Windows 10, never on Windows 11), the installer downloads it for you.

> [!IMPORTANT]
> The installer is **not code-signed yet**, so Windows SmartScreen may say *"Windows
> protected your PC"*. Click **More info**, then **Run anyway**. Signing is planned before
> the first stable release. Each release lists the SHA-256 checksum of the installer, so you
> can check that the file you downloaded is the one built here.

To uninstall Easy Uninstaller itself, use *Settings > Apps > Installed apps* in Windows,
like any other program.

## How your files are protected

Safety is the first goal of this project, ahead of features and speed.

- **Scans only read.** Looking for apps and files never changes anything on your PC.
- **Moved, never deleted.** When you remove files, they are moved into a quarantine
  folder (`%LOCALAPPDATA%\com.rccrd12.easyuninstaller\Quarantine`) together with a list
  of where each one came from.
- **7 days to change your mind.** Everything in quarantine can be restored to its
  original place with one click, or deleted earlier if you want the space back now.
  After 7 days it is deleted for good.
- **Undo right away.** After moving files, an *Undo* button puts them back immediately.
- **No surprises.** A file that changed after the scan is left where it is. If a move is
  interrupted (for example, a power cut), the files already moved can still be restored.
- **Restore points for programs.** Before uninstalling programs, the app can create a
  Windows restore point (it asks for permission, since Windows requires administrator
  rights for this). You can turn it off in Settings.
- **Personal files are never pre-selected.** Groups labelled *Check* or *Personal* are
  only removed if you tick them yourself.

Found a problem that could put someone's files at risk? Please open an issue, or contact the
maintainer privately for anything sensitive.

## Roadmap

1. ✅ App skeleton: sidebar, screens, sample data, Windows CI build
2. ✅ Installed apps: Win32/MSI programs from the registry and Microsoft Store apps (read only)
3. ✅ File scan: temp files, caches, used installers, old downloads, duplicates, large files (read only)
4. ✅ Quarantine and restore: files are moved, kept 7 days, restorable with one click; Windows restore points
5. ⬜ Uninstall apps, with their leftovers moved to quarantine
6. ⬜ Leftovers screen with real data
7. ⬜ Space map with real data
8. ⬜ Signed installer and first stable release

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
is attached to the run as an artifact. Contributors (and coding agents) should read
[`CLAUDE.md`](CLAUDE.md) for the project rules and the checks to run before pushing.

### Making a release

1. Set the same version in `package.json`, `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json`, and merge that to `main`.
2. Push a tag for it: `git tag v0.2.0 && git push origin v0.2.0`.
3. The [Release workflow](.github/workflows/release.yml) builds the installer on Windows and
   creates a **draft** release with the installer and its checksum. Review it on GitHub and
   press *Publish*. Tags with a suffix (such as `v0.2.0-beta.1`) become pre-releases.

Code signing options are described in [`docs/code-signing.md`](docs/code-signing.md).

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
  src/apps.rs           installed apps: registry (Win32/MSI) and Microsoft Store, with icons
  src/files.rs          file scan: temp, installers, old downloads, duplicates, large files
  src/quarantine.rs     moves files to quarantine, restores and expires them
  src/restore_point.rs  Windows restore points
docs/                   screenshots, design mockups, code signing notes
```

## License

[MIT](LICENSE). You can use, change and share it freely; it comes with no warranty.

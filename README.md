# Easy Uninstaller

**Uninstall apps and free up space on Windows, simply and safely.**

Easy Uninstaller is a small, free app for everyone, not just for tech people. It speaks
plain language, offers a few big clear choices, and never deletes anything without a way
back: everything it removes waits in a quarantine (7 days by default) and comes back with
one click. The only exceptions are temporary files, caches, thumbnails, crash reports and
old Recycle Bin items, which nobody needs back: those are deleted right away.

![Easy Uninstaller home screen](docs/screenshots/home.png)

> [!NOTE]
> **Early preview.** Every screen works with real data: one-click uninstall with leftover
> cleanup, Quick clean, the file scan with "What is this?" explanations, the space map,
> the quarantine and restore points. It still needs more testing on real PCs before a
> stable release. See the [roadmap](#roadmap).

## Why another uninstaller?

- **Simple.** The home screen offers four choices: *Apps*, *Files*, *Leftovers* and
  *Space map*, plus a one-click *Quick clean* that only removes what is safe. No jargon,
  no settings you have to understand first.
- **Safe.** Nothing is deleted right away. See [How your files are protected](#how-your-files-are-protected).
- **Clear about risk.** Every group of files is labelled *Safe*, *Check* or *Personal*,
  so you know what you are removing before you remove it. Unknown folders come with a
  plain *What is this?* explanation, built in: no AI, nothing sent anywhere.
- **AI only if you want it.** Off by default. When you turn it on, it explains folders the
  built-in list does not know, from their names and sizes only, and its answer is advice in
  its own purple tag: it never ticks or moves anything. See [AI help](#ai-help).
- **Light.** Built with [Tauri 2](https://tauri.app): it uses the WebView2 engine that is
  already part of Windows, so the installer is a few MB instead of 100+ MB.
- **Private.** It works entirely on your PC. No account, no ads, no tracking. The optional
  AI help is the only thing that ever sends anything, and only when you ask it to.
- **English and Italian.** Picked from your system language, changeable in Settings.
- **Light and dark.** Follows the Windows mode by default, changeable in Settings.

## Screenshots

| Apps | Files |
|---|---|
| ![Apps screen: installed programs and Store apps, sorted by size](docs/screenshots/apps.png) | ![Files screen: groups of files labelled Safe or Check](docs/screenshots/files.png) |
| **Quarantine** | **Space map** |
| ![Quarantine screen: removed items with Restore buttons](docs/screenshots/quarantine.png) | ![Space map: what takes up the most space](docs/screenshots/map.png) |
| **Dark mode** | **Space map, dark** |
| ![Home screen in dark mode](docs/screenshots/home-dark.png) | ![Space map in dark mode](docs/screenshots/map-dark.png) |

| **Uninstall: one click** | **Uninstall: done** |
|---|---|
| ![Uninstall confirmation](docs/screenshots/uninstall-confirm.png) | ![Uninstall done, leftovers moved to quarantine](docs/screenshots/uninstall-done.png) |
| **Leftovers** | **What is this?** |
| ![Leftovers screen](docs/screenshots/leftovers.png) | ![Built-in explanation of an unknown folder](docs/screenshots/files-explain.png) |
| **Quick clean** | **Quarantine settings** |
| ![Quick clean: only what is safe, in one click](docs/screenshots/quick-clean.png) | ![Quarantine duration and drive in Settings](docs/screenshots/settings-quarantine.png) |
| **Review files one by one** | **Hidden items** |
| ![Reviewing a group file by file, with previews](docs/screenshots/files-review.png) | ![Hidden files, folders and groups in Settings, with Show again](docs/screenshots/hidden-items.png) |

| **AI help: choose the service** | **See what is sent** |
|---|---|
| ![AI setup: a subscription's program, an API key or a model on this PC](docs/screenshots/ai-setup.png) | ![What is sent to the AI, shown before asking](docs/screenshots/ai-confirm.png) |
| **The AI's answer** | **Every unknown folder** |
| ![The AI's answer, with what the app checked on this PC shown apart](docs/screenshots/ai-answer.png) | ![Asking the AI about every unknown folder at once](docs/screenshots/ai-all-folders.png) |

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
- **Moved, not deleted.** When you remove files, folders, leftover registry keys or
  shortcuts, they are moved into a quarantine folder
  (`%LOCALAPPDATA%\com.rccrd12.easyuninstaller\Quarantine`, or another drive you pick in
  Settings) together with a list of where each one came from. Temporary files, caches,
  thumbnails, crash reports and old Recycle Bin items are the only exception: they are
  deleted right away, because nobody needs them back (Windows and apps recreate the first three).
- **Time to change your mind.** Everything in quarantine can be restored to its original
  place with one click, or deleted earlier if you want the space back now. After 7 days
  (or 3 or 30, as you choose in Settings) it is deleted for good.
- **Undo right away.** After moving files, an *Undo* button puts them back immediately.
- **No surprises.** A file that changed after the scan is left where it is. If a move is
  interrupted (for example, a power cut), the files already moved can still be restored.
- **Restore points for programs.** Before uninstalling programs, the app can create a
  Windows restore point (it asks for permission, since Windows requires administrator
  rights for this). It is off by default; turn it on in Settings.
- **You choose file by file.** Any group can be reviewed one file at a time, with
  previews; every duplicate set always keeps one copy. Files, folders, whole groups or
  leftovers you want to keep can be hidden, and shown again from Settings: hiding never
  moves or deletes anything.
- **Personal files are never pre-selected.** Groups labelled *Check* or *Personal* are
  only removed if you tick them yourself.

## AI help

Off by default; turn it on in Settings or the first time you press *Ask AI* on a folder
the app does not know. You choose where it runs and which model answers:

- **A subscription you already have**, through its own program: Claude Code, Codex or
  Antigravity CLI. The app runs it with no tools, in an empty folder, with a time limit, and
  never reads its login.
- **An API key**: Claude (Anthropic), OpenAI, Google Gemini, OpenRouter or any
  OpenAI-compatible service. The key is kept in Windows Credential Manager.
- **A model on this PC**: Ollama, LM Studio, Unsloth Studio or another local server.

What is sent: the folder's path with your user folder replaced by `%LOCALAPPDATA%` and the
like, its size, file count, last-modified month and the names of some subfolders and files.
Never file contents, your username, your list of apps or anything from your personal
folders. By default the app shows exactly what will be sent before asking. Answers are kept
on your PC, so the same folder is not asked about twice.

The built-in explanations always win, Windows folders and the app's own folders are never
sent, and the AI's verdict never shows as *Safe*: if an installed app matches the folder,
a "can be removed" answer is turned into "keep it".

Found a problem that could put someone's files at risk? Please open an issue, or contact the
maintainer privately for anything sensitive.

## Roadmap

1. ✅ App skeleton: sidebar, screens, sample data, Windows CI build
2. ✅ Installed apps: Win32/MSI programs from the registry and Microsoft Store apps (read only)
3. ✅ File scan: temp files, caches, used installers, old downloads, duplicates, large files (read only)
4. ✅ Quarantine and restore: files are moved, kept 7 days, restorable with one click; Windows restore points
5. ✅ One-click uninstall: quiet when the installer allows it (MSI, Inno Setup, NSIS, Squirrel,
   WiX bundles), else the app's own uninstaller; Store apps via PackageManager. Then its leftover
   folders, registry keys, shortcuts and settings go to quarantine on their own
6. ✅ Leftovers screen with real data, including folders no installed app owns and untouched
   for 6 months (offered unticked)
7. ✅ Space map with real data: apps, profile folders, app data, caches and Recycle Bin (read only)
8. ✅ Dark mode, following Windows or chosen in Settings
9. ✅ "What is this?": built-in explanations (EN/IT) for common folders; new groups for old
   Recycle Bin items, error reports, thumbnail cache and big unknown folders
10. ✅ Quick clean in one click; quarantine duration (3, 7 or 30 days) and drive in Settings
11. ✅ Review files one by one with previews; hide files, folders, groups or leftovers
12. ✅ AI help for unknown folders, off by default: through a subscription's own program (Claude Code,
    Codex, Antigravity CLI), an API key (Claude, OpenAI, Gemini, OpenRouter, any OpenAI-compatible
    service) or a model on this PC (Ollama, LM Studio, Unsloth Studio). Only names and numbers are
    sent; the answer is advice in its own purple tag and never moves or ticks anything
13. ⬜ Signed installer and first stable release

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
  styles/theme.css      design tokens (colors, radii), light and dark
src-tauri/              backend (Rust)
  src/lib.rs            registers the commands
  src/disk.rs           system drive usage
  src/apps.rs           installed apps: registry (Win32/MSI) and Microsoft Store, with icons
  src/files.rs          file scan: temp, installers, old downloads, duplicates, large files
  src/files/extra.rs    Recycle Bin, error reports, thumbnails, big app folders
  src/explain.rs        "What is this?": matches paths against known_folders.json
  src/space.rs          space map: what takes up the system drive
  src/uninstall.rs      runs uninstallers (quietly when possible) and Store app removal
  src/leftovers.rs      finds what an app left behind: folders, registry keys, shortcuts
  src/cleanup.rs        removes scanned files: some groups deleted, the rest to quarantine
  src/quarantine.rs     moves items to quarantine, restores and expires them
  src/elevated.rs       the few steps that need administrator rights
  src/review.rs         reviewing files one by one, and the items the user hid
  src/restore_point.rs  Windows restore points
  src/ai.rs             AI help: services, prompt, answers kept on this PC (ai/ for the parts)
docs/                   screenshots, design mockups, code signing notes
```

## License

[MIT](LICENSE). You can use, change and share it freely; it comes with no warranty.

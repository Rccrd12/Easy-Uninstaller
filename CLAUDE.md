# Easy Uninstaller: notes for contributors and coding agents

Windows app (Tauri 2 + Svelte 5 + TypeScript, Rust backend) to uninstall apps and free disk
space. Goals in order: **safe**, **simple for non-technical users**, **lightweight**.

## Rules

- Code, comments, commits and docs are in English. The UI ships in English and Italian.
- UI strings live in `src/lib/i18n/en.json` (reference) and `it.json`; every key must exist in both.
  Never hard-code user-visible text in components; use `t("key")` from `src/lib/i18n/index.svelte.ts`.
  Data coming from Rust that is shown to the user (e.g. file group titles) should be an i18n key, not English text.
- The UI reaches the system only through `src/lib/api.ts`. Each function there calls a Rust
  command via `invoke` when `isTauri()` and falls back to `src/lib/mock.ts` in the browser.
  When you implement a real scanner, keep the browser fallback.
- Keep `src/lib/types.ts` in sync with the Rust structs (`#[serde(rename_all = "camelCase")]`).
- One Rust module per area (`disk.rs`, `apps.rs`, `files.rs`, `quarantine.rs`, ...), registered in `lib.rs`.
- **Never delete user data directly.** Anything removed goes through the quarantine module.
  Scanners are read only. Destructive features land only after quarantine exists.
  The one exception (the owner's decision): file groups listed in `DELETE_NOW_GROUPS`
  (`cleanup.rs`, mirrored in `api.ts`), i.e. temporary files, caches (including the app caches of `app_cache.rs`), thumbnails, crash reports and old Recycle Bin items,
  are deleted right away by `cleanup.rs`, and only the exact files the scan found, unchanged since.
  Every removal of scanned files goes through `cleanup::remove`.
- Moving files is not removing them: "Tidy up Downloads" (`tidy.rs`) moves loose files into
  folders inside the user's own folders, never replaces a file, and records every move so
  the whole tidy can be undone.
- The AI never acts alone: it explains (`ai.rs`), turns a sentence into a search filter
  (`search.rs`) or proposes folders (`tidy.rs`). Only names and numbers are sent, never
  file contents, and nothing is ticked, moved or removed until the user presses a button.
- The local index (`semantic.rs`) is read only. What it downloads (model, llama.cpp) is pinned
  by URL and SHA-256 in `semantic/models.rs` and never run before the check; everything it
  keeps lives in its own `LocalIndex` folder, which "Delete model and index" removes whole.
- Windows-only code goes behind `#[cfg(windows)]` with a stub for other platforms, so the crate
  still builds and tests on Linux/macOS.
- Lightness matters: think twice before adding a dependency (frontend or Rust); prefer
  `default-features = false`.
- Colors and radii come from CSS variables in `src/styles/theme.css`.

## Checks to run before pushing

```sh
npm run check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
# Windows-specific code: rustup target add x86_64-pc-windows-msvc, then
cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
```

CI (`.github/workflows/ci.yml`) runs all of this on `windows-latest` and builds the NSIS installer.

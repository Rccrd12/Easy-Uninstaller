# Easy Uninstaller: notes for contributors and coding agents

Windows app (Tauri 2 + Svelte 5 + TypeScript, Rust backend) to uninstall apps and free disk
space. Goals in order: **safe**, **simple for non-technical users**, **lightweight**.

## Rules

- Everything in the repo is in English: code, comments, commits, docs, UI strings.
- UI strings live in `src/lib/i18n/en.json`. Never hard-code user-visible text in components; use `t("key")`.
- The UI reaches the system only through `src/lib/api.ts`. Each function there calls a Rust
  command via `invoke` when `isTauri()` and falls back to `src/lib/mock.ts` in the browser.
  When you implement a real scanner, keep the browser fallback.
- Keep `src/lib/types.ts` in sync with the Rust structs (`#[serde(rename_all = "camelCase")]`).
- One Rust module per area (`disk.rs`, `apps.rs`, `files.rs`, `quarantine.rs`, ...), registered in `lib.rs`.
- **Never delete user data directly.** Anything removed goes through the quarantine module.
  Scanners are read only. Destructive features land only after quarantine exists.
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

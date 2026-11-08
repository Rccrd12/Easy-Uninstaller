# In-app updates

Settings has a *Check for updates* button, and by default the app also checks by itself
once a day, a few seconds after it starts. When a newer version exists, the user presses
*Update now*: the app downloads the new installer, checks its signature, runs it quietly
(a small progress window, no questions) and opens again on the new version. Settings,
Quarantine and hidden items are kept, since they live outside the install folder.

## How it works

- `src-tauri/src/updater.rs` uses the official [Tauri updater plugin](https://v2.tauri.app/plugin/updater/).
  The UI reaches it through `checkForUpdate` and `installUpdate` in `src/lib/api.ts`;
  the shared state lives in `src/lib/updates.svelte.ts`.
- The app asks `https://github.com/Rccrd12/Easy-Uninstaller/releases/latest/download/latest.json`
  (set in `tauri.conf.json`, `plugins.updater.endpoints`). GitHub points `latest` at the
  newest **published, non pre-release** release, so drafts and betas are never offered.
  The repository must be public for this address to work.
- `latest.json` and the installer's `.sig` file are made by the
  [Release workflow](../.github/workflows/release.yml) and attached to the release next to
  the installer.
- Only the version number is asked for: nothing about the PC is sent. The daily check can
  be turned off in Settings.

## The update signing key

Every update is signed with a private key; the app only installs an installer whose
signature matches the public key built into it (`plugins.updater.pubkey`). This is separate
from Windows code signing (`docs/code-signing.md`): it protects the update channel even
while the installer itself is not code-signed.

- The private key is stored as the repository secret **`TAURI_SIGNING_PRIVATE_KEY`**
  (Settings > Secrets and variables > Actions). It has no password.
- Keep a backup of it somewhere safe, such as a password manager. **If it is lost, the
  copies already installed can no longer update**: people would have to download the new
  version by hand once, from a build carrying a new public key.
- If it leaks, make a new pair (`npx tauri signer generate --ci -p "" -w updater.key`),
  put the new public key in `tauri.conf.json`, the new private key in the secret, and
  release. Copies that installed the release with the new key are protected again.

## When Windows code signing is added

Code signing changes the bytes of the installer, so the update signature must be made
**after** it: sign the installer, then run
`npx tauri signer sign -k "$TAURI_SIGNING_PRIVATE_KEY" -p "" <installer>.exe` and use the new
`.sig` in `latest.json`. Otherwise the app will refuse the update.

## Trying it out

In the browser (`npm run dev`) the check always finds a sample version 0.3.0; open the page
with `?update=none` to see the "latest version" state. To try a real update, install an
older release, publish a newer one, and press *Check for updates*.

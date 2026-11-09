# In-app updates

Settings has a *Check for updates* button, and by default the app also checks by itself
each time it opens, a few seconds after start. When a newer version exists, the user presses
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
- Only the version number is asked for: nothing about the PC is sent. The check at start can
  be turned off in Settings.

## Beta versions

Settings also has *Beta versions*, off by default. When it is on, the check asks a second
address too, `releases/download/beta/latest.json`: the `beta` pre-release, which holds main
with every open pull request merged in. The newest of the two is offered.

- The [Beta workflow](../.github/workflows/beta.yml) makes it, with `scripts/beta.mjs`. It runs
  each time CI ends (on main or on a pull request), after each Release run, and by hand
  (Actions > Beta > Run workflow). Only pull requests whose CI passed on their latest commit go
  in; one that fails, is still running or does not merge is left out, and the release's notes
  say so. When nothing is left that the last release does not have, the beta is taken down.
- A beta is versioned as the next patch with the run number, `0.3.1-beta.42` (or
  `0.3.0-beta.42` while main's 0.3.0 has no tag yet). A release is always newer than the betas
  before it, so beta users get it as a normal update.
- What the app shows under *What's new* are the titles of the pull requests in the beta.
- Turning *Beta versions* off while on a beta offers the latest release (*Back to stable*), the
  only time an older version is installed.
- The build job runs the pull requests' code, so it gets no write access and no signing key.
  The publish job signs the installer and writes `latest.json` without running any of it.
- The release is replaced each time; its `beta` tag is made once on main and then stays. If
  GitHub refuses to create it from the workflow, create it once by hand:
  `git tag beta origin/main && git push origin beta`.

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

In the browser (`npm run dev`) the check always finds a sample version 0.3.0 (a sample beta
when *Beta versions* is on); open the page with `?update=none` to see the "latest version"
state, or `?update=back` for *Back to stable*. To try a real update, install an
older release, publish a newer one, and press *Check for updates*.

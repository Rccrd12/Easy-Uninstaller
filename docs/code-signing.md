# Code signing the Windows installer

Without a signature, Windows SmartScreen shows *"Windows protected your PC"* when someone
runs the installer, and the publisher appears as *Unknown*. For an app that asks people to
trust it with their files, that warning matters. This page compares the ways to sign it.
Nothing has been bought or set up yet.

Two facts apply to every option:

- **No certificate removes the warning on day one.** Since 2024, even EV certificates no
  longer skip SmartScreen. A signed installer builds *reputation* as people download it,
  and the warning goes away after a while. Signing still helps right away: the publisher
  name is shown, and the reputation carries over from one release to the next.
- **The private key can no longer live in a file.** Certificates are issued on a hardware
  token or in a cloud HSM, so signing in GitHub Actions needs a cloud service.

## Options

| | SignPath Foundation | Certum Open Source | Azure Artifact Signing | Commercial OV/EV certificate |
|---|---|---|---|---|
| Cost | Free | About €49 the first year (plus renewal) | $9.99 a month | About €200 to €600 a year |
| Publisher shown | *SignPath Foundation* | *Open Source Developer, \<your name\>* | Your verified name | Your name or company |
| Works from GitHub Actions | Yes, official action | Hard: the cloud key is unlocked from a phone app | Yes, official action | Depends on the vendor's cloud HSM |
| Available to an individual in Italy | Yes, for open source projects | Yes | No: individuals only in the US and Canada; EU only for registered organizations | Yes |
| Main requirement | Public repo, OSI license, a "Code signing policy" section, every release approved in SignPath | Identity check with an ID document | Registered business in the EU | Identity check, sometimes a business |

Prices and rules change; check the provider's page before deciding.

## Recommendation

1. **SignPath Foundation** once the repository is public. It is free, it is made for
   projects like this one, and it plugs into the release workflow. Its conditions are a
   good fit: MIT license, builds only from GitHub Actions, no telemetry, a clear uninstall.
   What we would need to add: a "Code signing policy" section in the README naming who
   approves releases, and multi-factor authentication on GitHub and SignPath. The first
   release must already be out (unsigned) before applying.
2. **Certum Open Source** as a fallback if SignPath declines. It shows your own name, but
   it is awkward to automate, so signing would be a manual step on your PC.
3. Azure Artifact Signing and commercial certificates only make sense if the project later
   becomes a registered business.

## How it plugs in

The release workflow (`.github/workflows/release.yml`) has a marked place for signing,
right after the build and before the checksum. With SignPath, the job uploads the unsigned
installer as an artifact, calls `signpath/github-action-submit-signing-request`, waits for
the approval, and attaches the signed file to the draft release. The SignPath API token is
stored as a repository secret; nothing secret goes in the repository.

The app itself (`easy-uninstaller.exe` inside the installer) should be signed too. Tauri
can run a custom `signCommand` (in `tauri.conf.json`, `bundle.windows`) for each binary it
builds; the setup depends on the option chosen.

## Sources

- SignPath Foundation conditions: <https://signpath.org/terms>
- Certum Open Source Code Signing: <https://shop.certum.eu/open-source-code-signing-on-simplysign.html>
- Microsoft, code signing options for Windows apps: <https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options>
- Azure Artifact Signing availability and pricing (January 2026): <https://www.devclass.com/security/2026/01/14/code-signing-windows-apps-may-be-easier-and-more-secure-with-new-azure-artifact-service/4079554>

//! In-app updates. Asks GitHub Releases whether a newer version exists (the
//! `latest.json` written by the release workflow), then downloads the new
//! installer, checks its signature against the public key in `tauri.conf.json`
//! and runs it. On Windows the installer closes the app, updates it in place
//! and opens it again; nothing on the PC changes until the user presses Update.
//!
//! Beta versions (Settings, off unless turned on) also ask the `beta` pre-release
//! that `.github/workflows/beta.yml` publishes: main with the open pull requests
//! merged in, versioned `0.3.1-beta.12`. A release is newer than the betas before
//! it (0.3.1 > 0.3.1-beta.12), so it still comes as a plain update. Turning betas
//! off while on one offers the latest release, the one time an older version is
//! installed.

use std::sync::Mutex;

use semver::Version;
use serde::Serialize;
use tauri::{ipc::Channel, AppHandle, State, Url};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Where the beta's `latest.json` is: the assets of the `beta` pre-release.
const BETA_ENDPOINT: &str =
    "https://github.com/Rccrd12/Easy-Uninstaller/releases/download/beta/latest.json";

/// At most this many lines of a beta's notes (one per change) reach the UI.
const MAX_NOTES: usize = 30;

/// The update found by the last check, kept so `install_update` installs exactly that one.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

/// The version to install: newer than the one running, or the release when leaving a beta.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// It is a beta (its version has a pre-release part).
    pub beta: bool,
    /// It is older than the running beta: going back to the release.
    pub back: bool,
    /// What a beta holds, one change per line (the titles of its pull requests).
    pub notes: Vec<String>,
}

/// Download progress, sent while the new installer is downloaded.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded_bytes: u64,
    /// `None` when the server does not say how big the file is.
    pub total_bytes: Option<u64>,
}

fn info(update: &Update) -> UpdateInfo {
    let version = Version::parse(&update.version).ok();
    let current = Version::parse(&update.current_version).ok();
    let beta = version.as_ref().is_some_and(|v| !v.pre.is_empty());
    UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        beta,
        back: matches!((&version, &current), (Some(v), Some(c)) if v < c),
        notes: if beta {
            note_lines(update.body.as_deref().unwrap_or(""))
        } else {
            Vec::new()
        },
    }
}

/// A beta's notes as lines for the UI: no blank ones, no list marks, at most `MAX_NOTES`.
fn note_lines(body: &str) -> Vec<String> {
    body.lines()
        .map(|l| l.trim().trim_start_matches(['-', '*']).trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.chars().take(200).collect())
        .take(MAX_NOTES)
        .collect()
}

/// Whether the release channel's `remote` is offered to `current`: a newer one, or,
/// with betas turned off, the release to go back to from a beta.
fn release_wanted(current: &Version, remote: &Version, betas: bool) -> bool {
    remote > current || (!betas && !current.pre.is_empty() && remote.pre.is_empty())
}

/// Of the release and the beta found, the one to offer: the newest.
fn newest(release: Option<Update>, beta: Option<Update>) -> Option<Update> {
    let key = |u: &Update| Version::parse(&u.version).ok();
    match (release, beta) {
        (Some(r), Some(b)) => Some(if key(&b) > key(&r) { b } else { r }),
        (r, b) => r.or(b),
    }
}

/// Returns the version to install, or `None` when this one is the latest. With
/// `beta`, the beta channel is asked too. Errors are short codes for the UI
/// ("offline" or the error text); a beta that cannot be read is simply not offered.
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
    beta: bool,
) -> Result<Option<UpdateInfo>, String> {
    let release = app
        .updater_builder()
        .version_comparator(move |current, remote| release_wanted(&current, &remote.version, beta))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(error_code)?;
    let beta_found = if beta { check_beta(&app).await } else { None };
    let found = newest(release, beta_found);
    let result = found.as_ref().map(info);
    *pending.0.lock().map_err(|e| e.to_string())? = found;
    Ok(result)
}

/// The beta, when one newer than this version is published.
async fn check_beta(app: &AppHandle) -> Option<Update> {
    let url = Url::parse(BETA_ENDPOINT).ok()?;
    app.updater_builder()
        .endpoints(vec![url])
        .ok()?
        .version_comparator(|current, remote| remote.version > current)
        .build()
        .ok()?
        .check()
        .await
        .ok()
        .flatten()
}

/// Downloads and installs the update found by the last check. On Windows this
/// does not return: the installer takes over and reopens the app when done.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
    on_progress: Channel<UpdateProgress>,
) -> Result<(), String> {
    let update = pending
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or_else(|| "noUpdate".to_string())?;
    let mut downloaded_bytes = 0u64;
    update
        .download_and_install(
            |chunk, total_bytes| {
                downloaded_bytes += chunk as u64;
                let _ = on_progress.send(UpdateProgress {
                    downloaded_bytes,
                    total_bytes,
                });
            },
            || {},
        )
        .await
        .map_err(error_code)?;
    // Reached only where the installer does not restart the app itself.
    app.restart();
}

/// Network problems become "offline" so the UI can say so plainly; the rest keep their text.
fn error_code(e: tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error;
    match e {
        Error::Reqwest(_) | Error::Network(_) => "offline".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_and_info_use_camel_case() {
        let p = serde_json::to_value(UpdateProgress {
            downloaded_bytes: 5,
            total_bytes: Some(10),
        })
        .unwrap();
        assert_eq!(
            p,
            serde_json::json!({ "downloadedBytes": 5, "totalBytes": 10 })
        );
        let i = serde_json::to_value(UpdateInfo {
            version: "0.3.0".into(),
            current_version: "0.2.0".into(),
            beta: false,
            back: false,
            notes: vec![],
        })
        .unwrap();
        assert_eq!(
            i,
            serde_json::json!({ "version": "0.3.0", "currentVersion": "0.2.0", "beta": false, "back": false, "notes": [] })
        );
    }

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn a_release_is_newer_than_its_betas() {
        assert!(v("0.3.1") > v("0.3.1-beta.12"));
        assert!(v("0.3.1-beta.12") > v("0.3.1-beta.9"));
        assert!(v("0.3.1-beta.1") > v("0.3.0"));
    }

    #[test]
    fn the_release_channel_offers_newer_versions_only() {
        assert!(release_wanted(&v("0.2.0"), &v("0.3.0"), false));
        assert!(release_wanted(&v("0.2.0"), &v("0.3.0"), true));
        assert!(!release_wanted(&v("0.3.0"), &v("0.3.0"), false));
        assert!(!release_wanted(&v("0.3.0"), &v("0.2.0"), false));
        // On a beta of the coming release, that release is an update either way.
        assert!(release_wanted(&v("0.3.0-beta.4"), &v("0.3.0"), true));
    }

    #[test]
    fn turning_betas_off_on_a_beta_goes_back_to_the_release() {
        assert!(release_wanted(&v("0.3.1-beta.4"), &v("0.3.0"), false));
        assert!(!release_wanted(&v("0.3.1-beta.4"), &v("0.3.0"), true));
    }

    #[test]
    fn beta_notes_become_short_lines() {
        let notes = note_lines("- Manage startup programs\n\n* Reminders  \n");
        assert_eq!(notes, vec!["Manage startup programs", "Reminders"]);
        assert_eq!(note_lines(&"x\n".repeat(50)).len(), MAX_NOTES);
    }

    #[test]
    fn network_errors_read_as_offline() {
        assert_eq!(
            error_code(tauri_plugin_updater::Error::Network("timed out".into())),
            "offline"
        );
        assert_eq!(
            error_code(tauri_plugin_updater::Error::ReleaseNotFound),
            "Could not fetch a valid release JSON from the remote"
        );
    }
}

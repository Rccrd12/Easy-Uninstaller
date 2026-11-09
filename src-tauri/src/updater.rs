//! In-app updates. Asks GitHub Releases whether a newer version exists (the
//! `latest.json` written by the release workflow), then downloads the new
//! installer, checks its signature against the public key in `tauri.conf.json`
//! and runs it. On Windows the installer closes the app, updates it in place
//! and opens it again; nothing on the PC changes until the user presses Update.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The update found by the last check, kept so `install_update` installs exactly that one.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

/// A newer version than the one running.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
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
    UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
    }
}

/// Returns the newer version, or `None` when this one is the latest.
/// Errors are short codes for the UI ("offline" or the error text).
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let found = updater.check().await.map_err(error_code)?;
    let result = found.as_ref().map(info);
    *pending.0.lock().map_err(|e| e.to_string())? = found;
    Ok(result)
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
        })
        .unwrap();
        assert_eq!(
            i,
            serde_json::json!({ "version": "0.3.0", "currentVersion": "0.2.0" })
        );
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

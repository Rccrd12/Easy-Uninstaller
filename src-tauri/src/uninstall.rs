//! Uninstalling apps.
//!
//! - Classic programs: their own uninstaller is run, exactly as "Apps and features"
//!   would run it (`UninstallString`). Windows Installer (MSI) programs are removed
//!   with `msiexec /x {code} /passive /norestart`: a progress bar, no questions, and
//!   never a restart. Programs installed for all users are started as administrator.
//! - Microsoft Store apps: removed by the Windows PackageManager.
//!
//! Before an uninstaller runs, what is needed to find its leftovers later (name,
//! publisher, folders) is kept, because the uninstaller removes the registry entry.

use crate::apps::{self, registry, registry::RawEntry};
use crate::elevated::{self, LaunchError};
use crate::leftovers::{self, AppHints, LeftoverItem};
use crate::quarantine::now_secs;
use crate::regkey::{self, Hive, RegKeyPath};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// How long to wait for the app list entry to go after the uninstaller closes.
const SETTLE: Duration = Duration::from_secs(5);
/// msiexec: the user cancelled.
const MSI_USER_EXIT: u32 = 1602;
/// The user cancelled a prompt.
const ERROR_CANCELLED: u32 = 1223;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UninstallOutcome {
    /// Gone from the app list.
    Removed,
    /// The uninstaller closed but the app is still listed: it may still be working
    /// (many uninstallers hand over to a copy of themselves), or the user stopped it.
    StillInstalled,
    /// The user cancelled in the uninstaller.
    Cancelled,
    /// The user said no to the Windows permission prompt.
    Declined,
}

/// A program and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub file: String,
    pub params: String,
}

/// Splits a command line from the registry into program and arguments. Handles
/// quoted paths and the common unquoted `C:\Program Files\App\uninstall.exe /S`.
pub fn split_command(cmd: &str, exists: &dyn Fn(&Path) -> bool) -> Option<Command> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return None;
    }
    let make = |file: &str, params: &str| Command {
        file: file.trim().to_string(),
        params: params.trim().to_string(),
    };
    if let Some(rest) = cmd.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(make(&rest[..end], &rest[end + 1..]));
    }
    // Every place where ".exe" ends a word: the first that exists wins, else the first.
    let lower = cmd.to_ascii_lowercase();
    let mut ends = Vec::new();
    let mut from = 0;
    while let Some(i) = lower[from..].find(".exe") {
        let end = from + i + 4;
        if end == cmd.len() || cmd.as_bytes()[end] == b' ' {
            ends.push(end);
        }
        from = end;
    }
    if let Some(&end) = ends
        .iter()
        .find(|&&e| exists(Path::new(&cmd[..e])))
        .or(ends.first())
    {
        return Some(make(&cmd[..end], &cmd[end..]));
    }
    let (file, params) = cmd.split_once(' ').unwrap_or((cmd, ""));
    Some(make(file, params))
}

/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` found in `s`.
fn msi_product_code(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let code = s.get(start..start + 38)?;
    let ok = code.ends_with('}')
        && code[1..37].chars().enumerate().all(|(i, c)| {
            matches!(i, 8 | 13 | 18 | 23) == (c == '-') && (c == '-' || c.is_ascii_hexdigit())
        });
    ok.then(|| code.to_string())
}

/// What to run to uninstall a program, and whether it is an MSI removal.
pub fn uninstall_command(
    raw: &RawEntry,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<(Command, bool)> {
    let s = raw.uninstall_string.as_deref()?.trim();
    if raw.windows_installer == Some(1) || s.to_ascii_lowercase().contains("msiexec") {
        if let Some(code) = msi_product_code(s).or_else(|| msi_product_code(&raw.key_name)) {
            let cmd = Command {
                file: "msiexec.exe".into(),
                params: format!("/x {code} /passive /norestart"),
            };
            return Some((cmd, true));
        }
    }
    split_command(s, exists).map(|c| (c, false))
}

/// The folder of the program in a command line or an icon location
/// (`C:\App\app.exe,0`), unless it is one of Windows' own folders.
pub fn program_dir(s: &str) -> Option<PathBuf> {
    let s = s.trim();
    let s = match s.rsplit_once(',') {
        Some((path, index)) if index.trim().trim_start_matches('-').parse::<i32>().is_ok() => path,
        _ => s,
    };
    let cmd = split_command(s, &|p| p.is_file())?;
    let (dir, _) = cmd.file.trim_matches('"').rsplit_once(['\\', '/'])?;
    let b = dir.as_bytes();
    let absolute = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':';
    let dir = PathBuf::from(dir);
    let windows = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .is_some_and(|w| leftovers::inside(&dir, &w));
    (absolute && !windows).then_some(dir)
}

fn hints_from(raw: &RawEntry) -> AppHints {
    AppHints {
        name: raw
            .display_name
            .clone()
            .unwrap_or_default()
            .trim()
            .to_string(),
        publisher: raw.publisher.clone().unwrap_or_default().trim().to_string(),
        install_dir: raw
            .install_location
            .as_deref()
            .and_then(apps::own_install_dir),
        related_dirs: [raw.display_icon.as_deref(), raw.uninstall_string.as_deref()]
            .into_iter()
            .flatten()
            .filter_map(program_dir)
            .filter(|d| d.to_str().and_then(apps::own_install_dir).is_some())
            .collect(),
        uninstalled_at: 0,
    }
}

/// True when the entry is Easy Uninstaller itself.
fn is_self(hints: &AppHints) -> bool {
    let Some(own) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    else {
        return false;
    };
    hints
        .install_dir
        .iter()
        .chain(&hints.related_dirs)
        .any(|d| leftovers::inside(&own, d) || leftovers::inside(d, &own))
}

enum AppId {
    Registry(RegKeyPath),
    Store(String),
}

/// `reg:<scope>:<key>` or `store:<PackageFullName>`, as made by `apps.rs`.
fn parse_id(id: &str) -> Option<AppId> {
    if let Some(rest) = id.strip_prefix("reg:") {
        let (scope, key) = rest.split_once(':')?;
        return registry::uninstall_key(scope, key).map(AppId::Registry);
    }
    let full = id.strip_prefix("store:")?;
    let ok = !full.is_empty()
        && full
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-~".contains(c));
    ok.then(|| AppId::Store(full.to_string()))
}

fn installed(id: &AppId) -> bool {
    match id {
        AppId::Registry(key) => regkey::exists(key),
        AppId::Store(full) => apps::store_installed(full),
    }
}

/// What was known about apps being uninstalled, by app id.
#[derive(Default)]
pub struct Pending(Mutex<HashMap<String, AppHints>>);

impl Pending {
    fn set(&self, id: &str, hints: AppHints) {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(id.to_string(), hints);
    }

    fn get(&self, id: &str) -> Option<AppHints> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
    }
}

/// Runs the app's uninstaller and waits for it. Error texts the UI translates:
/// `notInstalled`, `noUninstaller`, `itself`.
#[tauri::command]
pub async fn uninstall_app(app: AppHandle, id: String) -> Result<UninstallOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || uninstall(&app, &id))
        .await
        .map_err(|e| e.to_string())?
}

fn uninstall(app: &AppHandle, id: &str) -> Result<UninstallOutcome, String> {
    let parsed = parse_id(id).ok_or("notInstalled")?;
    let key = match &parsed {
        AppId::Store(full) => {
            app.state::<Pending>().set(id, AppHints::default());
            apps::store_remove(full)?;
            return Ok(UninstallOutcome::Removed);
        }
        AppId::Registry(key) => key,
    };
    let raw = registry::read_raw(key).ok_or("notInstalled")?;
    let (cmd, msi) = uninstall_command(&raw, &|p| p.is_file()).ok_or("noUninstaller")?;
    let hints = hints_from(&raw);
    if is_self(&hints) {
        return Err("itself".into());
    }
    app.state::<Pending>().set(id, hints);

    let admin = key.hive == Hive::Hklm;
    let code = match elevated::shell_run(&cmd.file, &cmd.params, admin, elevated::main_window(app))
    {
        Ok(code) => code,
        Err(LaunchError::Declined) => return Ok(UninstallOutcome::Declined),
        Err(LaunchError::Failed(e)) => return Err(e),
    };
    let start = Instant::now();
    while start.elapsed() < SETTLE {
        if !regkey::exists(key) {
            return Ok(UninstallOutcome::Removed);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let cancelled = code == Some(ERROR_CANCELLED) || (msi && code == Some(MSI_USER_EXIT));
    Ok(if cancelled {
        UninstallOutcome::Cancelled
    } else {
        UninstallOutcome::StillInstalled
    })
}

/// Whether the app is still in the app list (the UI asks while an uninstaller runs).
#[tauri::command]
pub async fn is_app_installed(id: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || parse_id(&id).is_some_and(|a| installed(&a)))
        .await
        .unwrap_or(true)
}

/// Looks for what an app just uninstalled left behind, and remembers the app so
/// the Leftovers screen can look again later.
#[tauri::command]
pub async fn find_app_leftovers(app: AppHandle, id: String) -> Result<Vec<LeftoverItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let parsed = parse_id(&id).ok_or("notInstalled")?;
        // Never while the app is still there: its files are not leftovers yet.
        if installed(&parsed) {
            return Err("stillInstalled".to_string());
        }
        let Some(mut hints) = app.state::<Pending>().get(&id) else {
            return Ok(Vec::new());
        };
        if hints.name.is_empty() {
            return Ok(Vec::new()); // Store apps: Windows removes their data itself.
        }
        hints.uninstalled_at = now_secs();
        leftovers::remember(&app, &hints);
        Ok(leftovers::search_app(&app, &hints))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(cmd: &str, existing: &[&str]) -> Option<(String, String)> {
        split_command(cmd, &|p: &Path| existing.iter().any(|e| Path::new(e) == p))
            .map(|c| (c.file, c.params))
    }

    #[test]
    fn splits_uninstall_commands() {
        let pair = |a: &str, b: &str| Some((a.to_string(), b.to_string()));
        assert_eq!(
            split(r#""C:\Program Files\VLC\uninstall.exe" /S"#, &[]),
            pair(r"C:\Program Files\VLC\uninstall.exe", "/S")
        );
        assert_eq!(
            split(
                r"C:\Program Files\Zoom App\unins000.exe /SILENT",
                &[r"C:\Program Files\Zoom App\unins000.exe"]
            ),
            pair(r"C:\Program Files\Zoom App\unins000.exe", "/SILENT")
        );
        // Not there any more: still the whole path, not "C:\Program".
        assert_eq!(
            split(r"C:\Program Files\Gone\uninst.exe", &[]),
            pair(r"C:\Program Files\Gone\uninst.exe", "")
        );
        assert_eq!(
            split(r"C:\a.exe.bak\real.exe --x", &[]),
            pair(r"C:\a.exe.bak\real.exe", "--x")
        );
        assert_eq!(
            split("rundll32 setupapi.dll,InstallHinfSection", &[]),
            pair("rundll32", "setupapi.dll,InstallHinfSection")
        );
        assert_eq!(split("   ", &[]), None);
        assert_eq!(split(r#""unterminated"#, &[]), None);
    }

    #[test]
    fn msi_programs_use_their_product_code() {
        let raw = RawEntry {
            key_name: "{23170F69-40C1-2702-2201-000001000000}".into(),
            uninstall_string: Some("MsiExec.exe /I{23170F69-40C1-2702-2201-000001000000}".into()),
            windows_installer: Some(1),
            ..RawEntry::default()
        };
        let (cmd, msi) = uninstall_command(&raw, &|_| false).unwrap();
        assert!(msi);
        assert_eq!(cmd.file, "msiexec.exe");
        assert_eq!(
            cmd.params,
            "/x {23170F69-40C1-2702-2201-000001000000} /passive /norestart"
        );
        assert_eq!(msi_product_code("{not-a-guid}"), None);
        assert_eq!(
            msi_product_code("x{23170F69-40C1-2702-2201-00000100000}"),
            None
        );

        let exe = RawEntry {
            uninstall_string: Some(r#""C:\App\unins000.exe""#.into()),
            ..RawEntry::default()
        };
        let (cmd, msi) = uninstall_command(&exe, &|_| false).unwrap();
        assert!(!msi);
        assert_eq!(cmd.file, r"C:\App\unins000.exe");
    }

    #[test]
    fn program_folders_from_commands_and_icons() {
        assert_eq!(
            program_dir(r"C:\Program Files\VideoLAN\VLC\vlc.exe,0"),
            Some(PathBuf::from(r"C:\Program Files\VideoLAN\VLC"))
        );
        assert_eq!(
            program_dir(r#""C:\Users\me\AppData\Local\Discord\Update.exe" --uninstall"#),
            Some(PathBuf::from(r"C:\Users\me\AppData\Local\Discord"))
        );
        assert_eq!(program_dir("MsiExec.exe /X{x}"), None);
        assert_eq!(program_dir(""), None);
    }

    #[test]
    fn ids_cannot_reach_other_keys() {
        assert!(matches!(
            parse_id("reg:hklm:VLC media player"),
            Some(AppId::Registry(_))
        ));
        assert!(matches!(
            parse_id("store:Microsoft.ZuneMusic_11.2_x64__8wekyb3d8bbwe"),
            Some(AppId::Store(_))
        ));
        assert!(parse_id(r"reg:hklm:..\..\Run").is_none());
        assert!(parse_id("reg:hkcr:x").is_none());
        assert!(parse_id("store:a b").is_none());
        assert!(parse_id("store:").is_none());
        assert!(parse_id("other").is_none());
    }

    #[test]
    fn outcomes_serialize_for_the_ui() {
        let json = serde_json::to_string(&UninstallOutcome::StillInstalled).unwrap();
        assert_eq!(json, "\"stillInstalled\"");
    }
}

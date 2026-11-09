//! Uninstalling apps, without questions whenever possible.
//!
//! - Classic programs are removed quietly when the way is known: the program's own
//!   `QuietUninstallString`, Windows Installer (MSI) with `msiexec /x {code} /passive
//!   /norestart` (a progress bar, never a restart), and the silent switches of the
//!   common installer makers (Inno Setup, NSIS, Squirrel, WiX bundles). Otherwise, or
//!   when the quiet run fails, the app's own uninstaller opens as in "Apps and
//!   features". Programs installed for all users are started as administrator.
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

/// How to uninstall a program: quietly when possible, else with its own window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Runs without questions; tried first.
    pub quiet: Option<Command>,
    /// The program's own uninstaller, as "Apps and features" runs it.
    pub normal: Command,
    /// Windows Installer: `normal` is already quiet and there is nothing else to try.
    pub msi: bool,
}

/// Installer makers whose uninstallers have known silent switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Maker {
    Nsis,
    Other,
}

/// Whether a command line already has one of `switches` (any case).
fn has_switch(params: &str, switches: &[&str]) -> bool {
    params
        .split_whitespace()
        .any(|w| switches.iter().any(|s| w.eq_ignore_ascii_case(s)))
}

fn file_name(file: &str) -> String {
    file.rsplit(['\\', '/'])
        .next()
        .unwrap_or(file)
        .to_ascii_lowercase()
}

/// The silent version of a known uninstaller, or `None`.
fn quiet_version(raw: &RawEntry, cmd: &Command, maker: &dyn Fn(&Path) -> Maker) -> Option<Command> {
    let name = file_name(&cmd.file);
    let with = |extra: &str| Command {
        file: cmd.file.clone(),
        params: format!("{} {extra}", cmd.params).trim().to_string(),
    };
    // Inno Setup: unins000.exe, registry key "<AppId>_is1".
    let inno = raw.key_name.to_ascii_lowercase().ends_with("_is1")
        || (name.starts_with("unins")
            && name.len() == 12
            && name[5..8].bytes().all(|b| b.is_ascii_digit()));
    if inno {
        if has_switch(&cmd.params, &["/VERYSILENT", "/SILENT"]) {
            return Some(cmd.clone());
        }
        return Some(with("/VERYSILENT /SUPPRESSMSGBOXES /NORESTART"));
    }
    // Squirrel (Discord, Teams classic, ...): Update.exe --uninstall.
    if name == "update.exe" && has_switch(&cmd.params, &["--uninstall"]) {
        if has_switch(&cmd.params, &["-s", "--silent"]) {
            return Some(cmd.clone());
        }
        return Some(with("-s"));
    }
    // WiX bundles: cached setup.exe /uninstall.
    if cmd.file.to_ascii_lowercase().contains("\\package cache\\")
        && has_switch(&cmd.params, &["/uninstall"])
    {
        return Some(with("/quiet /norestart"));
    }
    // NSIS: /S, and _?= so that it runs in place and we can wait for it (otherwise
    // it copies itself to a temporary folder and returns at once).
    if maker(Path::new(&cmd.file)) == Maker::Nsis {
        let dir = cmd.file.rsplit_once(['\\', '/'])?.0;
        if cmd.params.contains("_?=") {
            return None;
        }
        let silent = if has_switch(&cmd.params, &["/S"]) {
            ""
        } else {
            "/S "
        };
        return Some(with(&format!("{silent}_?={dir}")));
    }
    None
}

/// How to uninstall a program. `exists` tells whether a file exists, `maker` which
/// installer maker built a file.
pub fn uninstall_plan(
    raw: &RawEntry,
    exists: &dyn Fn(&Path) -> bool,
    maker: &dyn Fn(&Path) -> Maker,
) -> Option<Plan> {
    let s = raw.uninstall_string.as_deref()?.trim();
    if raw.windows_installer == Some(1) || s.to_ascii_lowercase().contains("msiexec") {
        if let Some(code) = msi_product_code(s).or_else(|| msi_product_code(&raw.key_name)) {
            let normal = Command {
                file: "msiexec.exe".into(),
                params: format!("/x {code} /passive /norestart"),
            };
            return Some(Plan {
                quiet: None,
                normal,
                msi: true,
            });
        }
    }
    let normal = split_command(s, exists)?;
    let quiet = raw
        .quiet_uninstall_string
        .as_deref()
        .and_then(|q| split_command(q, exists))
        .or_else(|| quiet_version(raw, &normal, maker));
    Some(Plan {
        quiet,
        normal,
        msi: false,
    })
}

/// Which installer maker built `file`: NSIS programs carry its name in their manifest.
pub fn maker_of(file: &Path) -> Maker {
    use std::io::Read;
    const HEAD: u64 = 1 << 20;
    let mut head = Vec::new();
    let read = std::fs::File::open(file).and_then(|f| f.take(HEAD).read_to_end(&mut head));
    if read.is_ok() && head.windows(8).any(|w| w == b"Nullsoft") {
        Maker::Nsis
    } else {
        Maker::Other
    }
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

/// Whether a program is still installed: its entry is in the app list and, for
/// launcher games, the launcher still has it.
fn entry_present(key: &RegKeyPath) -> bool {
    regkey::exists(key)
        && !registry::read_raw(key).is_some_and(|r| registry::launcher_gone(&r, &|p| p.exists()))
}

/// Whether the app with this id is still installed (false for an id that is not valid).
pub fn still_installed(id: &str) -> bool {
    parse_id(id).is_some_and(|a| installed(&a))
}

fn installed(id: &AppId) -> bool {
    match id {
        AppId::Registry(key) => entry_present(key),
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
    let plan = uninstall_plan(&raw, &|p| p.is_file(), &maker_of).ok_or("noUninstaller")?;
    let hints = hints_from(&raw);
    if is_self(&hints) {
        return Err("itself".into());
    }
    app.state::<Pending>().set(id, hints);
    crate::reinstall::prepare(app, id, &raw);

    // Launchers (steam://uninstall/...) run as the user and ask for rights themselves.
    let via_launcher = raw
        .uninstall_string
        .as_deref()
        .is_some_and(|s| s.contains("://"));
    let admin = key.hive == Hive::Hklm && !via_launcher;
    let window = elevated::main_window(app);
    if let Some(quiet) = &plan.quiet {
        let code = match elevated::shell_run(&quiet.file, &quiet.params, admin, window) {
            Ok(code) => code,
            Err(LaunchError::Declined) => return Ok(UninstallOutcome::Declined),
            Err(LaunchError::Failed(_)) => None,
        };
        if gone_soon(key) {
            return Ok(UninstallOutcome::Removed);
        }
        if code == Some(0) {
            // Finished well but still listed: it may hand over to another process.
            return Ok(UninstallOutcome::StillInstalled);
        }
        // The quiet run failed: open the app's own uninstaller instead.
    }
    let code = match elevated::shell_run(&plan.normal.file, &plan.normal.params, admin, window) {
        Ok(code) => code,
        Err(LaunchError::Declined) => return Ok(UninstallOutcome::Declined),
        Err(LaunchError::Failed(e)) => return Err(e),
    };
    if gone_soon(key) {
        return Ok(UninstallOutcome::Removed);
    }
    let cancelled = code == Some(ERROR_CANCELLED) || (plan.msi && code == Some(MSI_USER_EXIT));
    Ok(if cancelled {
        UninstallOutcome::Cancelled
    } else {
        UninstallOutcome::StillInstalled
    })
}

/// Waits a little for the app list entry to go.
fn gone_soon(key: &RegKeyPath) -> bool {
    let start = Instant::now();
    while start.elapsed() < SETTLE {
        if !entry_present(key) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

/// Whether the app is still in the app list (the UI asks while an uninstaller runs).
#[tauri::command]
pub async fn is_app_installed(id: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || still_installed(&id))
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
        // A launcher may leave the app's entry behind: it goes with the leftovers.
        let stale = match &parsed {
            AppId::Registry(key) if regkey::exists(key) => Some(key),
            _ => None,
        };
        Ok(leftovers::search_app(&app, &hints, stale))
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
        let plan = uninstall_plan(&raw, &|_| false, &|_| Maker::Other).unwrap();
        assert!(plan.msi);
        assert_eq!(plan.quiet, None);
        assert_eq!(plan.normal.file, "msiexec.exe");
        assert_eq!(
            plan.normal.params,
            "/x {23170F69-40C1-2702-2201-000001000000} /passive /norestart"
        );
        assert_eq!(msi_product_code("{not-a-guid}"), None);
        assert_eq!(
            msi_product_code("x{23170F69-40C1-2702-2201-00000100000}"),
            None
        );
    }

    fn quiet(key: &str, cmd: &str, quiet: Option<&str>, maker: Maker) -> Option<String> {
        let raw = RawEntry {
            key_name: key.into(),
            uninstall_string: Some(cmd.into()),
            quiet_uninstall_string: quiet.map(Into::into),
            ..RawEntry::default()
        };
        let plan = uninstall_plan(&raw, &|_| false, &|_| maker).unwrap();
        assert!(!plan.msi);
        plan.quiet.map(|c| format!("{} | {}", c.file, c.params))
    }

    #[test]
    fn quiet_uninstalls_for_known_installers() {
        // The program's own quiet command wins.
        assert_eq!(
            quiet(
                "App",
                r#""C:\App\u.exe""#,
                Some(r#""C:\App\u.exe" /quiet"#),
                Maker::Other
            ),
            Some(r"C:\App\u.exe | /quiet".into())
        );
        // Inno Setup.
        assert_eq!(
            quiet(
                "{X}_is1",
                r#""C:\Program Files\Zoom\unins000.exe""#,
                None,
                Maker::Other
            ),
            Some(
                r"C:\Program Files\Zoom\unins000.exe | /VERYSILENT /SUPPRESSMSGBOXES /NORESTART"
                    .into()
            )
        );
        assert_eq!(
            quiet(
                "Zoom",
                r#""C:\Zoom\unins001.exe" /SILENT"#,
                None,
                Maker::Other
            ),
            Some(r"C:\Zoom\unins001.exe | /SILENT".into())
        );
        // NSIS: runs in place so that it can be waited for.
        assert_eq!(
            quiet(
                "VLC",
                r#""C:\Program Files\VLC\uninstall.exe""#,
                None,
                Maker::Nsis
            ),
            Some(r"C:\Program Files\VLC\uninstall.exe | /S _?=C:\Program Files\VLC".into())
        );
        assert_eq!(
            quiet("VLC", r#""C:\VLC\uninstall.exe" /S"#, None, Maker::Nsis),
            Some(r"C:\VLC\uninstall.exe | /S _?=C:\VLC".into())
        );
        // Squirrel.
        assert_eq!(
            quiet(
                "Discord",
                r#""C:\Users\me\AppData\Local\Discord\Update.exe" --uninstall"#,
                None,
                Maker::Other
            ),
            Some(r"C:\Users\me\AppData\Local\Discord\Update.exe | --uninstall -s".into())
        );
        // WiX bundle.
        assert_eq!(
            quiet(
                "{B}",
                r#""C:\ProgramData\Package Cache\{B}\setup.exe" /uninstall"#,
                None,
                Maker::Other
            ),
            Some(
                r"C:\ProgramData\Package Cache\{B}\setup.exe | /uninstall /quiet /norestart".into()
            )
        );
        // Unknown (e.g. InstallShield scripts): only its own window.
        assert_eq!(
            quiet(
                "{C}",
                r#""C:\Program Files (x86)\InstallShield Installation Information\{C}\setup.exe" -runfromtemp -removeonly"#,
                None,
                Maker::Other
            ),
            None
        );
    }

    #[test]
    fn finds_nsis_programs() {
        let dir = std::env::temp_dir().join(format!("eu-maker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let nsis = dir.join("uninst.exe");
        std::fs::write(
            &nsis,
            b"MZ....<description>Nullsoft Install System</description>",
        )
        .unwrap();
        let other = dir.join("other.exe");
        std::fs::write(&other, b"MZ....Inno").unwrap();
        assert_eq!(maker_of(&nsis), Maker::Nsis);
        assert_eq!(maker_of(&other), Maker::Other);
        assert_eq!(maker_of(&dir.join("missing.exe")), Maker::Other);
        std::fs::remove_dir_all(&dir).unwrap();
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

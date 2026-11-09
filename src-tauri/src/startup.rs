//! Programs that start by themselves when the user signs in to Windows, and turning
//! them off or on again.
//!
//! Reads the same places as the Startup page of Task Manager:
//! - the `Run` keys of the current user and of the PC (64 and 32-bit),
//! - the Startup folders of the current user and of all users,
//! - the startup tasks of Microsoft Store apps.
//!
//! Turning a program off never removes or edits its entry: like Task Manager, it writes
//! the on/off switch Windows keeps next to it (`Explorer\StartupApproved`, or the
//! `State` of a Store app's startup task). Turning it on again puts it back exactly.
//! Switches that belong to all users need administrator rights: those go through the
//! elevated helper (`elevated.rs`), one permission prompt for all of them.
#![cfg_attr(not(windows), allow(dead_code))]

mod known;

use crate::apps;
use crate::elevated::{self, Job};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// How much a program slows down the start of Windows.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Impact {
    Low,
    Medium,
    High,
}

/// What the app suggests.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Advice {
    /// Better off: the user can open it when needed.
    TurnOff,
    /// Useful at startup (sync, drivers, antivirus): better left on.
    Keep,
    /// Part of Windows: not offered here.
    System,
    /// Nothing known about it: the user decides.
    None,
}

/// Where an entry lives. Its name in an id, e.g. `hkcu-run:Spotify`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// `HKCU\...\Run`.
    UserRun,
    /// `HKLM\...\Run`.
    MachineRun,
    /// `HKLM\SOFTWARE\WOW6432Node\...\Run`: 32-bit programs.
    MachineRun32,
    /// `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`.
    UserFolder,
    /// `%ProgramData%\Microsoft\Windows\Start Menu\Programs\StartUp`.
    CommonFolder,
    /// A Store app's startup task: `<PackageFamilyName>\<TaskId>`.
    Store,
}

const LOCATIONS: [(Location, &str); 6] = [
    (Location::UserRun, "hkcu-run"),
    (Location::MachineRun, "hklm-run"),
    (Location::MachineRun32, "hklm-run32"),
    (Location::UserFolder, "user-folder"),
    (Location::CommonFolder, "common-folder"),
    (Location::Store, "store"),
];

impl Location {
    fn tag(self) -> &'static str {
        LOCATIONS
            .iter()
            .find(|(l, _)| *l == self)
            .map_or("", |(_, t)| t)
    }

    /// True when changing it needs administrator rights.
    pub fn all_users(self) -> bool {
        matches!(
            self,
            Location::MachineRun | Location::MachineRun32 | Location::CommonFolder
        )
    }
}

pub fn make_id(location: Location, name: &str) -> String {
    format!("{}:{name}", location.tag())
}

pub fn parse_id(id: &str) -> Option<(Location, &str)> {
    let (tag, name) = id.split_once(':')?;
    let location = LOCATIONS.iter().find(|(_, t)| *t == tag)?.0;
    (!name.is_empty()).then_some((location, name))
}

/// One entry, as read: enough to check and change it. No icons or names yet.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub location: Location,
    /// Value name (Run keys), file name (Startup folders) or `<family>\<task>` (Store).
    pub name: String,
    /// The command line, the shortcut's target or the Store package family name.
    pub command: String,
    pub enabled: bool,
    /// The Store app's policy decides: it cannot be changed here.
    pub locked: bool,
}

impl Entry {
    pub fn id(&self) -> String {
        make_id(self.location, &self.name)
    }
}

/// Mirrors `StartupApp` in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartupApp {
    pub id: String,
    pub name: String,
    pub publisher: String,
    /// i18n key under `startup.notes`, or `None` to show the program's file name.
    pub note: Option<String>,
    /// The program's file name, e.g. `Spotify.exe`. Empty for Store apps.
    pub file_name: String,
    /// The full command, for whoever wants the technical details.
    pub command: String,
    pub enabled: bool,
    pub impact: Impact,
    pub advice: Advice,
    /// Changing it needs administrator rights.
    pub all_users: bool,
    pub icon_color: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartupChange {
    pub id: String,
    pub enabled: bool,
}

/// Mirrors `StartupReport` in `src/lib/types.ts`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartupReport {
    /// Ids of the entries now switched as asked.
    pub changed: Vec<String>,
    /// Ids that could not be changed.
    pub failed: Vec<String>,
    /// Ids left as they were because the user said no to the permission prompt.
    pub declined: Vec<String>,
}

/// Lists the programs that start with Windows: on first, biggest impact first.
#[tauri::command]
pub async fn list_startup_apps() -> Result<Vec<StartupApp>, String> {
    tauri::async_runtime::spawn_blocking(list)
        .await
        .map_err(|e| e.to_string())
}

/// Turns programs on or off at startup. Only entries that exist right now are touched.
#[tauri::command]
pub async fn set_startup_apps(
    app: AppHandle,
    changes: Vec<StartupChange>,
) -> Result<StartupReport, String> {
    tauri::async_runtime::spawn_blocking(move || set(&app, changes))
        .await
        .map_err(|e| e.to_string())?
}

fn list() -> Vec<StartupApp> {
    let entries = sys::entries();
    let store = if entries.iter().any(|e| e.location == Location::Store) {
        apps::store_list()
    } else {
        Vec::new()
    };
    let mut list: Vec<StartupApp> = entries
        .into_iter()
        .filter_map(|e| describe(e, &store))
        .collect();
    sort(&mut list);
    list
}

/// On first, then by impact, then by name.
fn sort(list: &mut [StartupApp]) {
    list.sort_by(|a, b| {
        b.enabled
            .cmp(&a.enabled)
            .then(b.impact.cmp(&a.impact))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// Adds the name, icon, impact and advice. `None` for Store tasks of apps that are gone.
fn describe(e: Entry, store: &[apps::StoreEntry]) -> Option<StartupApp> {
    let id = e.id();
    let all_users = e.location.all_users();
    if e.location == Location::Store {
        let family = e.command.as_str();
        let app = store
            .iter()
            .find(|s| package_family(&s.id).is_some_and(|f| f.eq_ignore_ascii_case(family)))?;
        let known = known::find(&format!("{family} {}", app.name).to_lowercase());
        return Some(StartupApp {
            id,
            name: app.name.clone(),
            publisher: app.publisher.clone(),
            note: known.map(|k| k.note.to_string()),
            file_name: String::new(),
            command: e.name.clone(),
            enabled: e.enabled,
            impact: known.map_or(Impact::Medium, |k| k.impact),
            advice: if e.locked {
                Advice::System
            } else {
                known.map_or(Advice::None, |k| k.advice)
            },
            all_users,
            icon_color: apps::icon_color(&app.name),
            icon: app.logo.as_deref().and_then(apps::icon::from_png_file),
        });
    }

    let program = program_path(&e.command).map(|p| resolve(PathBuf::from(p)));
    let size = program.as_deref().and_then(|p| std::fs::metadata(p).ok());
    // Only a full path can be said to be gone: a bare name is found by Windows at start.
    let missing = program.as_deref().is_some_and(Path::is_absolute) && size.is_none();
    let info = program
        .as_deref()
        .map(sys::version_info)
        .unwrap_or_default();
    let file_name = program
        .as_deref()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let display = display_name(&e.name);
    let known = known::find(&format!("{display} {}", e.command).to_lowercase());
    let name = known
        .and_then(|k| k.name)
        .map(str::to_string)
        .or(info.description.filter(|d| d.chars().count() <= 60))
        .unwrap_or(display);
    let (impact, advice, note) = match known {
        // A switch for a program that is not on the PC any more: nothing to lose.
        _ if missing => (Impact::Low, Advice::TurnOff, Some("missing")),
        Some(k) => (k.impact, k.advice, Some(k.note)),
        None => (
            size.map_or(Impact::Low, |m| impact_from_size(m.len())),
            Advice::None,
            None,
        ),
    };
    Some(StartupApp {
        id,
        icon_color: apps::icon_color(&name),
        icon: program
            .as_deref()
            .filter(|_| !missing)
            .and_then(|p| apps::icon::from_icon_location(&p.to_string_lossy())),
        name,
        publisher: info.company.unwrap_or_default(),
        note: note.map(str::to_string),
        file_name,
        command: e.command,
        enabled: e.enabled,
        impact,
        advice,
        all_users,
    })
}

/// A bare program name (`ctfmon.exe`) is looked up in `System32`, like Windows does.
fn resolve(program: PathBuf) -> PathBuf {
    if program.is_absolute() {
        return program;
    }
    std::env::var_os("SystemRoot")
        .map(|root| PathBuf::from(root).join("System32").join(&program))
        .filter(|p| p.is_file())
        .unwrap_or(program)
}

/// Without a measure from Windows, the size of the program says roughly how much it
/// loads: big ones are usually whole browsers inside an app (Electron).
pub fn impact_from_size(bytes: u64) -> Impact {
    const MB: u64 = 1024 * 1024;
    match bytes {
        b if b >= 60 * MB => Impact::High,
        b if b >= 8 * MB => Impact::Medium,
        _ => Impact::Low,
    }
}

/// `Foo.lnk` to `Foo`; a Run value name stays as it is.
fn display_name(name: &str) -> String {
    let lower = name.to_lowercase();
    for ext in [".lnk", ".url", ".exe", ".bat", ".cmd"] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name.to_string()
}

/// The program a command line starts: `"C:\a b\x.exe" --flag` or `C:\x\y.exe /min`,
/// with `%VARIABLES%` expanded. `rundll32 x.dll,Entry` names the DLL.
pub fn program_path(command: &str) -> Option<String> {
    let command = command.trim();
    let first = match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?.to_string(),
        None => {
            let lower = command.to_lowercase();
            match [".exe", ".com", ".bat", ".cmd", ".lnk"]
                .iter()
                .filter_map(|ext| lower.find(ext).map(|i| i + ext.len()))
                .min()
            {
                Some(end) => command[..end].to_string(),
                None => command.split(' ').next()?.to_string(),
            }
        }
    };
    let first = apps::icon::expand_env(first.trim());
    let lower = first.to_lowercase();
    if lower.ends_with("rundll32.exe") || lower == "rundll32" {
        let rest = command.get(command.to_lowercase().find("rundll32")? + 8..)?;
        let rest = rest
            .trim_start_matches(".exe")
            .trim_start_matches('"')
            .trim();
        let dll = rest
            .trim_matches('"')
            .split(',')
            .next()?
            .trim()
            .trim_matches('"');
        return (!dll.is_empty()).then(|| apps::icon::expand_env(dll));
    }
    (!first.is_empty()).then_some(first)
}

/// `store:Name_1.0.0.0_x64__publisherid` to `Name_publisherid`.
pub fn package_family(store_id: &str) -> Option<String> {
    let full = store_id.strip_prefix("store:").unwrap_or(store_id);
    let mut parts = full.split('_');
    let name = parts.next()?;
    let publisher = parts.next_back()?;
    (!name.is_empty() && !publisher.is_empty()).then(|| format!("{name}_{publisher}"))
}

/// `Explorer\StartupApproved` values: 12 bytes, the first one even when on, odd when
/// off; the other 8 bytes tell when it was turned off. No value means on.
pub fn approved_enabled(data: Option<&[u8]>) -> bool {
    data.and_then(|d| d.first()).is_none_or(|b| b % 2 == 0)
}

/// The value Task Manager writes: `02 00..` for on, `03 00 00 00` + FILETIME for off.
pub fn approved_value(enabled: bool, unix_secs: u64) -> [u8; 12] {
    let mut v = [0u8; 12];
    if enabled {
        v[0] = 2;
    } else {
        v[0] = 3;
        let filetime = (unix_secs + 11_644_473_600) * 10_000_000;
        v[4..].copy_from_slice(&filetime.to_le_bytes());
    }
    v
}

/// Store startup task states: 0 off, 1 off by the user, 2 on, 3 off by policy, 4 on by policy.
pub fn store_state(state: u32) -> Option<(bool, bool)> {
    match state {
        0 | 1 => Some((false, false)),
        2 => Some((true, false)),
        3 => Some((false, true)),
        4 => Some((true, true)),
        _ => None,
    }
}

fn set(app: &AppHandle, changes: Vec<StartupChange>) -> Result<StartupReport, String> {
    let (machine, user): (Vec<_>, Vec<_>) = changes
        .into_iter()
        .partition(|c| parse_id(&c.id).is_some_and(|(l, _)| l.all_users()));
    let mut report = apply(&user);
    if machine.is_empty() {
        return Ok(report);
    }
    match elevated::run(
        app,
        &Job::Startup {
            changes: machine.clone(),
        },
    ) {
        Ok(Some(result)) => {
            let r = result.startup.unwrap_or_default();
            report.changed.extend(r.changed);
            report.failed.extend(r.failed);
        }
        Ok(None) => report.declined.extend(machine.into_iter().map(|c| c.id)),
        Err(e) => {
            eprintln!("startup: elevated change failed: {e}");
            report.failed.extend(machine.into_iter().map(|c| c.id));
        }
    }
    Ok(report)
}

/// Applies changes to entries that exist now. Also what the elevated helper runs.
pub fn apply(changes: &[StartupChange]) -> StartupReport {
    let mut report = StartupReport::default();
    if changes.is_empty() {
        return report;
    }
    let entries = sys::entries();
    let now = crate::quarantine::now_secs();
    for c in changes {
        let entry = entries.iter().find(|e| e.id() == c.id);
        let ok = match entry {
            None => false,
            Some(e) if e.enabled == c.enabled => true,
            Some(e) if e.locked => false,
            Some(e) => match sys::switch(e, c.enabled, now) {
                Ok(()) => true,
                Err(err) => {
                    eprintln!("startup: {}: {err}", c.id);
                    false
                }
            },
        };
        if ok {
            report.changed.push(c.id.clone());
        } else {
            report.failed.push(c.id.clone());
        }
    }
    report
}

/// Name and publisher stored in a program file.
#[derive(Debug, Clone, Default)]
pub struct VersionInfo {
    pub description: Option<String>,
    pub company: Option<String>,
}

#[cfg(windows)]
mod sys {
    use super::{approved_enabled, approved_value, store_state, Entry, Location, VersionInfo};
    use std::io;
    use std::path::{Path, PathBuf};
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";
    const STORE_TASKS: &str = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\SystemAppData";

    fn hive(location: Location) -> RegKey {
        RegKey::predef(match location {
            Location::UserRun | Location::UserFolder | Location::Store => HKEY_CURRENT_USER,
            _ => HKEY_LOCAL_MACHINE,
        })
    }

    /// The `StartupApproved` key that holds this entry's switch.
    fn approved_key(location: Location) -> String {
        let leaf = match location {
            Location::MachineRun32 => "Run32",
            Location::UserFolder | Location::CommonFolder => "StartupFolder",
            _ => "Run",
        };
        format!(r"{APPROVED}\{leaf}")
    }

    fn approved_data(location: Location, name: &str) -> Option<Vec<u8>> {
        hive(location)
            .open_subkey_with_flags(approved_key(location), KEY_READ | KEY_WOW64_64KEY)
            .ok()?
            .get_raw_value(name)
            .ok()
            .map(|v| v.bytes.into_owned())
    }

    fn folder(location: Location) -> Option<PathBuf> {
        let (var, rest) = match location {
            Location::UserFolder => ("APPDATA", r"Microsoft\Windows\Start Menu\Programs\Startup"),
            _ => (
                "ProgramData",
                r"Microsoft\Windows\Start Menu\Programs\StartUp",
            ),
        };
        Some(PathBuf::from(std::env::var_os(var)?).join(rest))
    }

    pub fn entries() -> Vec<Entry> {
        let mut out = Vec::new();
        for (location, path) in [
            (Location::UserRun, RUN),
            (Location::MachineRun, RUN),
            (Location::MachineRun32, RUN32),
        ] {
            let Ok(key) = hive(location).open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY)
            else {
                continue;
            };
            for (name, value) in key.enum_values().flatten() {
                let command = value.to_string().trim().to_string();
                if name.is_empty() || command.is_empty() {
                    continue;
                }
                out.push(Entry {
                    enabled: approved_enabled(approved_data(location, &name).as_deref()),
                    location,
                    name,
                    command,
                    locked: false,
                });
            }
        }
        for location in [Location::UserFolder, Location::CommonFolder] {
            let Some(dir) = folder(location) else {
                continue;
            };
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for file in read.flatten() {
                let name = file.file_name().to_string_lossy().into_owned();
                let path = file.path();
                if name.eq_ignore_ascii_case("desktop.ini") || !path.is_file() {
                    continue;
                }
                let command = if name.to_lowercase().ends_with(".lnk") {
                    std::fs::read(&path)
                        .ok()
                        .and_then(|b| crate::leftovers::lnk::target(&b))
                        .unwrap_or(path)
                } else {
                    path
                };
                out.push(Entry {
                    enabled: approved_enabled(approved_data(location, &name).as_deref()),
                    location,
                    name,
                    command: command.to_string_lossy().into_owned(),
                    locked: false,
                });
            }
        }
        out.extend(store_entries());
        out
    }

    /// Startup tasks of Store apps the user (or the app) has switched at least once.
    fn store_entries() -> Vec<Entry> {
        let Ok(root) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(STORE_TASKS) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for family in root.enum_keys().flatten() {
            let Ok(package) = root.open_subkey(&family) else {
                continue;
            };
            for task in package.enum_keys().flatten() {
                let Ok(state) = package
                    .open_subkey(&task)
                    .and_then(|k| k.get_value::<u32, _>("State"))
                else {
                    continue;
                };
                let Some((enabled, locked)) = store_state(state) else {
                    continue;
                };
                out.push(Entry {
                    location: Location::Store,
                    name: format!(r"{family}\{task}"),
                    command: family.clone(),
                    enabled,
                    locked,
                });
            }
        }
        out
    }

    pub fn switch(e: &Entry, enabled: bool, now: u64) -> io::Result<()> {
        if e.location == Location::Store {
            let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
                .create_subkey(format!(r"{STORE_TASKS}\{}", e.name))?;
            return key.set_value("State", &(if enabled { 2u32 } else { 1u32 }));
        }
        let (key, _) = hive(e.location).create_subkey_with_flags(
            approved_key(e.location),
            KEY_READ | KEY_WRITE | KEY_WOW64_64KEY,
        )?;
        key.set_raw_value(
            &e.name,
            &RegValue {
                bytes: approved_value(enabled, now).to_vec().into(),
                vtype: REG_BINARY,
            },
        )
    }

    /// `FileDescription` and `CompanyName` from the program's version resource.
    pub fn version_info(path: &Path) -> VersionInfo {
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::Storage::FileSystem::{
            GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
        };
        let file = HSTRING::from(path.as_os_str());
        // SAFETY: valid zero-terminated string; no handle is returned.
        let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(file.as_ptr()), None) };
        if size == 0 {
            return VersionInfo::default();
        }
        let mut data = vec![0u8; size as usize];
        // SAFETY: the buffer is `size` bytes long.
        if unsafe {
            GetFileVersionInfoW(PCWSTR(file.as_ptr()), None, size, data.as_mut_ptr().cast())
        }
        .is_err()
        {
            return VersionInfo::default();
        }
        // Returns the block and its length: bytes for binary values, UTF-16 units for strings.
        let query = |sub: &str| -> Option<(*const u8, usize)> {
            let sub = HSTRING::from(sub);
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            // SAFETY: `data` holds the version resource; the result points into it.
            let ok = unsafe {
                VerQueryValueW(
                    data.as_ptr().cast(),
                    PCWSTR(sub.as_ptr()),
                    &mut ptr,
                    &mut len,
                )
            }
            .as_bool();
            (ok && !ptr.is_null() && len > 0).then_some((ptr as *const u8, len as usize))
        };
        let mut langs: Vec<String> = query(r"\VarFileInfo\Translation")
            .map(|(ptr, len)| {
                // SAFETY: `ptr` and `len` (bytes) describe a block inside `data`.
                let t = unsafe { std::slice::from_raw_parts(ptr, len) };
                t.chunks_exact(4)
                    .map(|c| {
                        let lang = u16::from_le_bytes([c[0], c[1]]);
                        let page = u16::from_le_bytes([c[2], c[3]]);
                        format!("{lang:04x}{page:04x}")
                    })
                    .collect()
            })
            .unwrap_or_default();
        langs.extend(["040904b0".into(), "040904e4".into()]);
        let string = |field: &str| -> Option<String> {
            langs.iter().find_map(|l| {
                let (ptr, len) = query(&format!(r"\StringFileInfo\{l}\{field}"))?;
                let units: Vec<u16> = (0..len)
                    // SAFETY: `len` UTF-16 units inside `data`, maybe unaligned.
                    .map(|i| unsafe { ptr.cast::<u16>().add(i).read_unaligned() })
                    .take_while(|u| *u != 0)
                    .collect();
                let text = String::from_utf16_lossy(&units).trim().to_string();
                (!text.is_empty()).then_some(text)
            })
        };
        VersionInfo {
            description: string("FileDescription"),
            company: string("CompanyName"),
        }
    }
}

#[cfg(not(windows))]
mod sys {
    use super::{Entry, VersionInfo};
    use std::io;
    use std::path::Path;

    pub fn entries() -> Vec<Entry> {
        Vec::new()
    }

    pub fn switch(_e: &Entry, _enabled: bool, _now: u64) -> io::Result<()> {
        Err(io::Error::other("only available on Windows"))
    }

    pub fn version_info(_path: &Path) -> VersionInfo {
        VersionInfo::default()
    }
}

#[cfg(test)]
mod tests;

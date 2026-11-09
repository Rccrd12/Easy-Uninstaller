//! Installed apps: classic programs (Win32 and MSI) from the registry Uninstall keys,
//! and Microsoft Store apps from the Windows PackageManager.
//!
//! Read only: this module never changes or removes anything.

pub(crate) mod icon;
pub mod registry;
mod store;

use serde::Serialize;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppSource {
    Win32,
    Msi,
    Store,
}

/// Mirrors `InstalledApp` in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub publisher: String,
    /// 0 when Windows does not know and the install folder could not be measured.
    pub size_bytes: u64,
    pub source: AppSource,
    /// As the app reports it ("12.3.1"); empty when unknown. Shown with the technical details.
    pub version: String,
    /// The install folder, when known. Shown with the technical details.
    pub location: Option<String>,
    /// ISO date (YYYY-MM-DD).
    pub installed_on: Option<String>,
    /// Windows has no reliable "last used" value: always `None` for now.
    pub last_used: Option<String>,
    pub never_used: bool,
    pub preinstalled: bool,
    pub bloatware: bool,
    pub icon_color: String,
    /// `data:image/png;base64,...`, or `None` to draw the colored letter instead.
    pub icon: Option<String>,
}

/// Lists the apps a user can uninstall, biggest first.
/// Runs on a worker thread: reading sizes and icons takes a moment.
#[tauri::command]
pub async fn list_installed_apps() -> Result<Vec<InstalledApp>, String> {
    tauri::async_runtime::spawn_blocking(scan)
        .await
        .map_err(|e| e.to_string())
}

fn scan() -> Vec<InstalledApp> {
    let (classic, store) = std::thread::scope(|s| {
        let store = s.spawn(|| {
            store::list()
                .into_iter()
                .map(from_store)
                .collect::<Vec<_>>()
        });
        let classic: Vec<_> = registry::list().into_iter().map(from_registry).collect();
        (classic, store.join().unwrap_or_default())
    });
    let mut apps = classic;
    apps.extend(store);
    apps.sort_by(|a, b| {
        b.size_bytes
            .cmp(&a.size_bytes)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    apps
}

/// Microsoft Store apps of the current user: name, publisher and folders.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn store_list() -> Vec<store::Entry> {
    store::list()
}

/// Whether a Store app (by package full name) is still installed for this user.
pub fn store_installed(full_name: &str) -> bool {
    store::is_installed(full_name)
}

/// Removes a Store app for this user, with its data.
pub fn store_remove(full_name: &str) -> Result<(), String> {
    store::remove(full_name)
}

fn from_registry(e: registry::Entry) -> InstalledApp {
    let size_bytes = match e.size_kb {
        Some(kb) => kb * 1024,
        None => e
            .install_location
            .as_deref()
            .and_then(own_install_dir)
            .map(|dir| dir_size(&dir))
            .unwrap_or(0),
    };
    InstalledApp {
        icon_color: icon_color(&e.name),
        icon: e.display_icon.as_deref().and_then(icon::from_icon_location),
        id: e.id,
        name: e.name,
        publisher: e.publisher,
        size_bytes,
        source: e.source,
        version: e.version,
        location: e.install_location,
        installed_on: e.installed_on,
        last_used: None,
        never_used: false,
        preinstalled: false,
        bloatware: false,
    }
}

fn from_store(e: store::Entry) -> InstalledApp {
    InstalledApp {
        icon_color: icon_color(&e.name),
        icon: e.logo.as_deref().and_then(icon::from_png_file),
        size_bytes: e.dirs.iter().map(|d| dir_size(d)).sum(),
        id: e.id,
        name: e.name,
        publisher: e.publisher,
        source: AppSource::Store,
        version: e.version,
        location: e.dirs.first().map(|d| d.to_string_lossy().into_owned()),
        installed_on: e.installed_on,
        last_used: None,
        never_used: false,
        preinstalled: false,
        bloatware: false,
    }
}

/// Background colors for the letter tile shown when an app has no icon.
const TILE_COLORS: [&str; 8] = [
    "#0067c0", "#107c10", "#c239b3", "#ca5010", "#038387", "#8764b8", "#e3008c", "#486860",
];

/// Picks a stable tile color from the app name (FNV-1a hash).
fn icon_color(name: &str) -> String {
    let hash = name.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    TILE_COLORS[hash as usize % TILE_COLORS.len()].to_string()
}

/// Returns the folder only if it looks like the app's own install folder, so a bad
/// registry value like `C:\` or `C:\Program Files` never makes us measure the whole disk.
pub fn own_install_dir(location: &str) -> Option<PathBuf> {
    let path = PathBuf::from(location.trim().trim_matches('"'));
    let depth = path
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    if depth < 2 || shared_dirs().iter().any(|d| same_path(d, &path)) || !path.is_dir() {
        return None;
    }
    Some(path)
}

/// Folders that hold many apps: never measured as if they belonged to one.
fn shared_dirs() -> Vec<PathBuf> {
    let vars = [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "CommonProgramFiles",
        "CommonProgramFiles(x86)",
        "ProgramData",
        "APPDATA",
        "LOCALAPPDATA",
        "USERPROFILE",
        "SystemRoot",
    ];
    let mut dirs: Vec<PathBuf> = vars
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(Path::new(&local).join("Programs"));
    }
    dirs
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    norm(a) == norm(b)
}

/// Total size of the files under `dir`. Does not follow links or junctions,
/// skips what it cannot read, and stops after a very large number of entries.
pub fn dir_size(dir: &Path) -> u64 {
    const MAX_ENTRIES: usize = 500_000;
    let mut total = 0;
    let mut seen = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return total;
            }
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

/// Turns a Windows FILETIME (100 ns ticks since 1601-01-01 UTC) into `YYYY-MM-DD`.
#[cfg_attr(not(windows), allow(dead_code))]
fn filetime_to_date(ticks: i64) -> Option<String> {
    const TICKS_PER_DAY: i64 = 864_000_000_000;
    const DAYS_1601_TO_1970: i64 = 134_774;
    if ticks <= 0 {
        return None;
    }
    let (y, m, d) = civil_from_days(ticks / TICKS_PER_DAY - DAYS_1601_TO_1970);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
#[cfg_attr(not(windows), allow(dead_code))]
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetime_converts_to_iso_date() {
        assert_eq!(
            filetime_to_date(116_444_736_000_000_000).as_deref(),
            Some("1970-01-01")
        );
        // 2024-06-02 12:00 UTC
        assert_eq!(
            filetime_to_date(133_618_032_000_000_000).as_deref(),
            Some("2024-06-02")
        );
        assert_eq!(filetime_to_date(0), None);
    }

    #[test]
    fn civil_dates_cross_leap_years() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(19_783), (2024, 3, 1));
    }

    #[test]
    fn icon_color_is_stable_and_from_palette() {
        let a = icon_color("Spotify");
        assert_eq!(a, icon_color("Spotify"));
        assert!(TILE_COLORS.contains(&a.as_str()));
    }

    #[test]
    fn install_dir_must_be_an_app_folder() {
        let tmp = std::env::temp_dir().join("eu-apps-test-install-dir");
        let app = tmp.join("Vendor").join("App");
        std::fs::create_dir_all(&app).unwrap();
        assert_eq!(own_install_dir(app.to_str().unwrap()), Some(app.clone()));
        assert_eq!(
            own_install_dir(&format!("\"{}\"", app.display())),
            Some(app.clone())
        );
        assert_eq!(own_install_dir("/"), None);
        assert_eq!(own_install_dir("/tmp"), None);
        assert_eq!(own_install_dir("C:\\"), None);
        assert_eq!(own_install_dir(""), None);
        assert_eq!(own_install_dir(tmp.join("missing").to_str().unwrap()), None);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn dir_size_sums_nested_files() {
        let tmp = std::env::temp_dir().join("eu-apps-test-dir-size");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("a/b")).unwrap();
        std::fs::write(tmp.join("one.bin"), [0u8; 100]).unwrap();
        std::fs::write(tmp.join("a/b/two.bin"), [0u8; 23]).unwrap();
        assert_eq!(dir_size(&tmp), 123);
        assert_eq!(dir_size(&tmp.join("missing")), 0);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn serializes_like_the_typescript_type() {
        let app = InstalledApp {
            id: "x".into(),
            name: "X".into(),
            publisher: "P".into(),
            size_bytes: 1,
            source: AppSource::Msi,
            version: "1.0".into(),
            location: None,
            installed_on: None,
            last_used: None,
            never_used: false,
            preinstalled: false,
            bloatware: false,
            icon_color: "#000".into(),
            icon: None,
        };
        let json = serde_json::to_string(&app).unwrap();
        assert!(json.contains("\"sizeBytes\":1"));
        assert!(json.contains("\"source\":\"msi\""));
        assert!(json.contains("\"installedOn\":null"));
        assert!(json.contains("\"version\":\"1.0\""));
        assert!(json.contains("\"location\":null"));
        assert!(json.contains("\"neverUsed\":false"));
        assert!(json.contains("\"iconColor\":\"#000\""));
    }
}

//! App caches: emptying the cache of a well-known app (Teams, Spotify, Discord,
//! browsers, ...) without uninstalling it.
//!
//! Only the folders listed in [`CATALOG`] are ever looked at, and only files are
//! removed, never the folders themselves. Measuring is read only and keeps the exact
//! files it found in [`LastAppCaches`]; emptying removes those files through
//! `cleanup::remove` (group `app-cache`, deleted right away like the other caches:
//! the app downloads again what it needs), only while the app is closed, and only
//! files still unchanged since they were measured.

use crate::cleanup;
use crate::files::{self, ScannedFile};
use crate::quarantine::{now_secs, MoveReport, Quarantine};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// The `cleanup::remove` group the cache files go through.
pub const GROUP: &str = "app-cache";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Base {
    /// `%LOCALAPPDATA%`
    Local,
    /// `%APPDATA%`
    Roaming,
}
use Base::{Local, Roaming};

/// A folder below a base; `*` stands for any one subfolder (e.g. browser profiles).
type Place = (Base, &'static str);

/// An app whose cache we know.
struct Known {
    id: &'static str,
    /// Lowercase start of the app's name in the app list.
    names: &'static [&'static str],
    /// Executables (lowercase) that mean the app is open.
    processes: &'static [&'static str],
    /// Folders holding only what the app recreates on its own.
    caches: &'static [Place],
    /// The app's own data folders, cache included: measured for "data and settings".
    data: &'static [Place],
}

/// Apps whose cache we know. The folders are the ones the apps themselves (or their
/// makers' support pages) describe as cache: chats, settings, sign-ins, downloads
/// and offline music are never in them.
const CATALOG: &[Known] = &[
    Known {
        id: "teams",
        names: &["microsoft teams"],
        processes: &["ms-teams.exe", "teams.exe"],
        caches: &[
            // New Teams (Microsoft Store package, Edge WebView2 profiles).
            (
                Local,
                r"Packages\MSTeams_8wekyb3d8bbwe\LocalCache\Microsoft\MSTeams\EBWebView\*\Cache",
            ),
            (
                Local,
                r"Packages\MSTeams_8wekyb3d8bbwe\LocalCache\Microsoft\MSTeams\EBWebView\*\Code Cache",
            ),
            (
                Local,
                r"Packages\MSTeams_8wekyb3d8bbwe\LocalCache\Microsoft\MSTeams\EBWebView\*\GPUCache",
            ),
            (
                Local,
                r"Packages\MSTeams_8wekyb3d8bbwe\LocalCache\Microsoft\MSTeams\EBWebView\*\Service Worker\CacheStorage",
            ),
            // Classic Teams.
            (Roaming, r"Microsoft\Teams\Cache"),
            (Roaming, r"Microsoft\Teams\Code Cache"),
            (Roaming, r"Microsoft\Teams\GPUCache"),
            (Roaming, r"Microsoft\Teams\Service Worker\CacheStorage"),
            (Roaming, r"Microsoft\Teams\tmp"),
        ],
        data: &[
            (Local, r"Packages\MSTeams_8wekyb3d8bbwe"),
            (Roaming, r"Microsoft\Teams"),
        ],
    },
    Known {
        id: "spotify",
        names: &["spotify"],
        processes: &["spotify.exe"],
        caches: &[
            (Local, r"Spotify\Data"),
            (
                Local,
                r"Packages\SpotifyAB.SpotifyMusic_zpdnekdrzrea0\LocalCache\Spotify\Data",
            ),
        ],
        data: &[
            (Local, "Spotify"),
            (Local, r"Packages\SpotifyAB.SpotifyMusic_zpdnekdrzrea0"),
        ],
    },
    Known {
        id: "discord",
        names: &["discord"],
        processes: &["discord.exe", "discordptb.exe", "discordcanary.exe"],
        caches: &[
            (Roaming, r"discord\Cache"),
            (Roaming, r"discord\Code Cache"),
            (Roaming, r"discord\GPUCache"),
            (Roaming, r"discordptb\Cache"),
            (Roaming, r"discordptb\Code Cache"),
            (Roaming, r"discordptb\GPUCache"),
            (Roaming, r"discordcanary\Cache"),
            (Roaming, r"discordcanary\Code Cache"),
            (Roaming, r"discordcanary\GPUCache"),
        ],
        data: &[
            (Roaming, "discord"),
            (Roaming, "discordptb"),
            (Roaming, "discordcanary"),
        ],
    },
    Known {
        id: "slack",
        names: &["slack"],
        processes: &["slack.exe"],
        caches: &[
            (Roaming, r"Slack\Cache"),
            (Roaming, r"Slack\Code Cache"),
            (Roaming, r"Slack\GPUCache"),
            (Roaming, r"Slack\Service Worker\CacheStorage"),
        ],
        data: &[(Roaming, "Slack")],
    },
    Known {
        id: "chrome",
        names: &["google chrome"],
        processes: &["chrome.exe"],
        caches: &[
            (Local, r"Google\Chrome\User Data\*\Cache"),
            (Local, r"Google\Chrome\User Data\*\Code Cache"),
            (Local, r"Google\Chrome\User Data\*\GPUCache"),
            (Local, r"Google\Chrome\User Data\ShaderCache"),
            (Local, r"Google\Chrome\User Data\GrShaderCache"),
        ],
        data: &[(Local, r"Google\Chrome\User Data")],
    },
    Known {
        id: "edge",
        names: &["microsoft edge"],
        processes: &["msedge.exe"],
        caches: &[
            (Local, r"Microsoft\Edge\User Data\*\Cache"),
            (Local, r"Microsoft\Edge\User Data\*\Code Cache"),
            (Local, r"Microsoft\Edge\User Data\*\GPUCache"),
            (Local, r"Microsoft\Edge\User Data\ShaderCache"),
            (Local, r"Microsoft\Edge\User Data\GrShaderCache"),
        ],
        data: &[(Local, r"Microsoft\Edge\User Data")],
    },
    Known {
        id: "brave",
        names: &["brave"],
        processes: &["brave.exe"],
        caches: &[
            (Local, r"BraveSoftware\Brave-Browser\User Data\*\Cache"),
            (Local, r"BraveSoftware\Brave-Browser\User Data\*\Code Cache"),
            (Local, r"BraveSoftware\Brave-Browser\User Data\*\GPUCache"),
            (Local, r"BraveSoftware\Brave-Browser\User Data\ShaderCache"),
            (
                Local,
                r"BraveSoftware\Brave-Browser\User Data\GrShaderCache",
            ),
        ],
        data: &[(Local, r"BraveSoftware\Brave-Browser\User Data")],
    },
    Known {
        id: "opera",
        names: &["opera"],
        processes: &["opera.exe"],
        caches: &[
            (Local, r"Opera Software\*\Cache"),
            (Roaming, r"Opera Software\*\Code Cache"),
            (Roaming, r"Opera Software\*\GPUCache"),
        ],
        data: &[(Local, "Opera Software"), (Roaming, "Opera Software")],
    },
    Known {
        id: "firefox",
        names: &["mozilla firefox", "firefox"],
        processes: &["firefox.exe"],
        caches: &[
            (Local, r"Mozilla\Firefox\Profiles\*\cache2"),
            (Local, r"Mozilla\Firefox\Profiles\*\startupCache"),
        ],
        data: &[(Local, r"Mozilla\Firefox"), (Roaming, r"Mozilla\Firefox")],
    },
    Known {
        id: "vscode",
        names: &["microsoft visual studio code", "visual studio code"],
        processes: &["code.exe"],
        caches: &[
            (Roaming, r"Code\Cache"),
            (Roaming, r"Code\CachedData"),
            (Roaming, r"Code\CachedExtensionVSIXs"),
            (Roaming, r"Code\Code Cache"),
            (Roaming, r"Code\GPUCache"),
        ],
        data: &[(Roaming, "Code")],
    },
    Known {
        id: "steam",
        names: &["steam"],
        processes: &["steam.exe", "steamwebhelper.exe"],
        caches: &[(Local, r"Steam\htmlcache")],
        data: &[(Local, "Steam")],
    },
];

/// Words that mark a helper or add-on, not the app itself (e.g. "Microsoft Edge
/// WebView2 Runtime", "Microsoft Teams Meeting Add-in").
const NOT_THE_APP: [&str; 6] = [
    "webview",
    "update",
    "installer",
    "add-in",
    "meeting",
    "tools",
];

/// The cache this app (by its name in the app list) has, if we know it.
pub fn cache_id_for(app_name: &str) -> Option<&'static str> {
    let name = app_name.trim().to_lowercase();
    if NOT_THE_APP.iter().any(|w| name.contains(w)) {
        return None;
    }
    CATALOG
        .iter()
        .find(|k| {
            k.names.iter().any(|n| {
                // Whole words only: "Steam" is not "Steamworks".
                name.strip_prefix(n)
                    .is_some_and(|rest| rest.is_empty() || !rest.starts_with(char::is_alphanumeric))
            })
        })
        .map(|k| k.id)
}

/// Mirrors `AppCache` in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppCache {
    /// Matches `InstalledApp.cacheId`.
    pub id: String,
    /// What emptying the cache frees.
    pub cache_bytes: u64,
    pub file_count: u64,
    /// The app's own data folders without the cache: chats, settings, profiles.
    pub data_bytes: u64,
    /// The app is open: the cache can be emptied only once it is closed.
    pub running: bool,
}

/// Mirrors `ClearCacheOutcome` in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClearCacheOutcome {
    /// True when nothing was done because the app is open.
    pub running: bool,
    pub report: MoveReport,
    /// The cache measured again afterwards (what is left: files in use, new files).
    pub cache: Option<AppCache>,
}

/// The files of each app cache as last measured, by cache id.
#[derive(Default)]
pub struct LastAppCaches(Mutex<HashMap<String, Vec<ScannedFile>>>);

/// Where `%LOCALAPPDATA%` and `%APPDATA%` are.
#[derive(Debug, Clone, Default)]
struct Bases {
    local: Option<PathBuf>,
    roaming: Option<PathBuf>,
}

impl Bases {
    fn from_env() -> Self {
        let env = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Bases {
            local: env("LOCALAPPDATA"),
            roaming: env("APPDATA"),
        }
    }

    fn get(&self, base: Base) -> Option<&Path> {
        match base {
            Local => self.local.as_deref(),
            Roaming => self.roaming.as_deref(),
        }
    }
}

/// The real folders a place stands for: `*` matches every subfolder (not links).
fn expand(bases: &Bases, (base, rel): Place) -> Vec<PathBuf> {
    let Some(root) = bases.get(base) else {
        return Vec::new();
    };
    let mut found = vec![root.to_path_buf()];
    for part in rel.split('\\') {
        found = if part == "*" {
            found.iter().flat_map(|dir| subdirs(dir)).collect()
        } else {
            found.iter().map(|dir| dir.join(part)).collect()
        };
    }
    found.retain(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()));
    found
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

/// Measures one app: its cache files and the size of its data without them.
fn measure(known: &Known, bases: &Bases) -> (Vec<ScannedFile>, u64) {
    let cache_dirs: Vec<PathBuf> = known
        .caches
        .iter()
        .flat_map(|p| expand(bases, *p))
        .collect();
    let mut files = Vec::new();
    for dir in files::dedup_dirs(&cache_dirs) {
        files::walk(&dir, &|_| false, &mut |path, meta| {
            files.push(ScannedFile {
                path,
                size: meta.len(),
                modified: meta.modified().unwrap_or(std::time::UNIX_EPOCH),
            });
        });
    }
    let cache_bytes: u64 = files.iter().map(|f| f.size).sum();
    let data_dirs: Vec<PathBuf> = known.data.iter().flat_map(|p| expand(bases, *p)).collect();
    let data_total: u64 = files::dedup_dirs(&data_dirs)
        .iter()
        .map(|d| crate::apps::dir_size(d))
        .sum();
    (files, data_total.saturating_sub(cache_bytes))
}

fn summary(
    known: &Known,
    files: &[ScannedFile],
    data_bytes: u64,
    running: &HashSet<String>,
) -> AppCache {
    AppCache {
        id: known.id.to_string(),
        cache_bytes: files.iter().map(|f| f.size).sum(),
        file_count: files.len() as u64,
        data_bytes,
        running: is_running(known, running),
    }
}

fn is_running(known: &Known, running: &HashSet<String>) -> bool {
    known.processes.iter().any(|p| running.contains(*p))
}

/// Measures every known app found on this PC (one thread per app) and keeps the files.
fn scan_all(bases: &Bases, running: &HashSet<String>, last: &LastAppCaches) -> Vec<AppCache> {
    let measured: Vec<(&Known, Vec<ScannedFile>, u64)> = std::thread::scope(|s| {
        let jobs: Vec<_> = CATALOG
            .iter()
            .map(|k| s.spawn(move || (k, measure(k, bases))))
            .collect();
        jobs.into_iter()
            .filter_map(|j| j.join().ok())
            .map(|(k, (files, data))| (k, files, data))
            .collect()
    });
    let mut kept = last.0.lock().unwrap_or_else(|e| e.into_inner());
    kept.clear();
    measured
        .into_iter()
        .filter(|(_, files, data)| !files.is_empty() || *data > 0)
        .map(|(k, files, data)| {
            let out = summary(k, &files, data, running);
            kept.insert(k.id.to_string(), files);
            out
        })
        .collect()
}

/// Empties one cache: nothing while the app is open; otherwise removes the files
/// last measured (unchanged ones only) and measures again.
fn clear(
    id: &str,
    bases: &Bases,
    running: &HashSet<String>,
    last: &LastAppCaches,
    quarantine: &Quarantine,
) -> Result<ClearCacheOutcome, String> {
    let known = CATALOG
        .iter()
        .find(|k| k.id == id)
        .ok_or_else(|| format!("unknown cache: {id}"))?;
    if is_running(known, running) {
        return Ok(ClearCacheOutcome {
            running: true,
            report: MoveReport::default(),
            cache: None,
        });
    }
    let files = last
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(id)
        .cloned()
        .unwrap_or_default();
    let mut report = MoveReport::default();
    cleanup::remove(quarantine, GROUP, &files, now_secs(), &mut report)
        .map_err(|e| e.to_string())?;
    let (left, data) = measure(known, bases);
    let cache = summary(known, &left, data, running);
    last.0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id.to_string(), left);
    Ok(ClearCacheOutcome {
        running: false,
        report,
        cache: Some(cache),
    })
}

/// Measures the caches of the known apps on this PC (read only).
#[tauri::command]
pub async fn list_app_caches(app: AppHandle) -> Result<Vec<AppCache>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        scan_all(
            &Bases::from_env(),
            &platform::running_exes(),
            &app.state::<LastAppCaches>(),
        )
    })
    .await
    .map_err(|e| e.to_string())
}

/// Empties the cache `id` (from `list_app_caches`), only if the app is closed.
#[tauri::command]
pub async fn clear_app_cache(app: AppHandle, id: String) -> Result<ClearCacheOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        clear(
            &id,
            &Bases::from_env(),
            &platform::running_exes(),
            &app.state::<LastAppCaches>(),
            &app.state::<Quarantine>(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(windows)]
mod platform {
    use std::collections::HashSet;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    /// Executable names (lowercase) of the running processes.
    pub fn running_exes() -> HashSet<String> {
        let mut names = HashSet::new();
        // SAFETY: plain Win32 calls on a snapshot handle we own and close.
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return names;
            };
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snap, &mut entry).is_ok();
            while more {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                names.insert(String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase());
                more = Process32NextW(snap, &mut entry).is_ok();
            }
            let _ = CloseHandle(snap);
        }
        names
    }
}

#[cfg(not(windows))]
mod platform {
    use std::collections::HashSet;

    pub fn running_exes() -> HashSet<String> {
        HashSet::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Sandbox(PathBuf);

    impl Sandbox {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("easy-uninstaller-ac-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Sandbox(dir)
        }

        fn bases(&self) -> Bases {
            Bases {
                local: Some(self.0.join("Local")),
                roaming: Some(self.0.join("Roaming")),
            }
        }

        fn file(&self, rel: &str, bytes: usize) -> PathBuf {
            let path = rel.split('/').fold(self.0.clone(), |p, part| p.join(part));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, vec![0u8; bytes]).unwrap();
            path
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn matches_apps_by_name_but_not_their_helpers() {
        assert_eq!(cache_id_for("Spotify"), Some("spotify"));
        assert_eq!(cache_id_for("Spotify Music"), Some("spotify"));
        assert_eq!(cache_id_for("Microsoft Teams"), Some("teams"));
        assert_eq!(cache_id_for("Microsoft Teams classic"), Some("teams"));
        assert_eq!(cache_id_for("Google Chrome"), Some("chrome"));
        assert_eq!(cache_id_for("Mozilla Firefox (x64 it)"), Some("firefox"));
        assert_eq!(
            cache_id_for("Microsoft Visual Studio Code (User)"),
            Some("vscode")
        );
        assert_eq!(cache_id_for("Microsoft Edge"), Some("edge"));
        assert_eq!(cache_id_for("Microsoft Edge WebView2 Runtime"), None);
        assert_eq!(cache_id_for("Microsoft Edge Update"), None);
        assert_eq!(
            cache_id_for("Microsoft Teams Meeting Add-in for Microsoft Office"),
            None
        );
        assert_eq!(cache_id_for("Teams Machine-Wide Installer"), None);
        assert_eq!(cache_id_for("Steamworks Common Redistributables"), None);
        assert_eq!(cache_id_for("VLC media player"), None);
    }

    #[test]
    fn catalog_ids_are_unique_and_every_cache_sits_in_the_app_data() {
        let mut ids = HashSet::new();
        for k in CATALOG {
            assert!(ids.insert(k.id), "{}", k.id);
            for (base, rel) in k.caches {
                assert!(
                    k.data.iter().any(|(b, d)| b == base && rel.starts_with(d)),
                    "{}: {rel} is outside its data folders",
                    k.id
                );
            }
        }
    }

    #[test]
    fn measures_caches_per_profile_and_data_without_them() {
        let s = Sandbox::new("measure");
        s.file(
            "Local/Google/Chrome/User Data/Default/Cache/Cache_Data/f_1",
            1000,
        );
        s.file(
            "Local/Google/Chrome/User Data/Profile 1/Code Cache/js/a",
            200,
        );
        s.file("Local/Google/Chrome/User Data/Default/Bookmarks", 50);
        s.file("Local/Google/Chrome/User Data/Default/History", 70);
        let chrome = CATALOG.iter().find(|k| k.id == "chrome").unwrap();

        let (files, data) = measure(chrome, &s.bases());

        assert_eq!(files.iter().map(|f| f.size).sum::<u64>(), 1200);
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| !f.path.ends_with("Bookmarks")));
        assert_eq!(data, 120, "bookmarks and history, not the cache");
    }

    #[test]
    fn clears_only_the_cache_and_only_when_closed() {
        let s = Sandbox::new("clear");
        let cache = s.file("Roaming/discord/Cache/Cache_Data/data_1", 4000);
        let gpu = s.file("Roaming/discord/GPUCache/data_0", 1000);
        let settings = s.file("Roaming/discord/settings.json", 30);
        let other = s.file("Roaming/Slack/Cache/x", 10);
        let q = Quarantine::new(s.0.join("q"));
        let last = LastAppCaches::default();
        let bases = s.bases();

        let found = scan_all(&bases, &HashSet::new(), &last);
        let discord = found.iter().find(|c| c.id == "discord").unwrap();
        assert_eq!(
            (discord.cache_bytes, discord.file_count, discord.data_bytes),
            (5000, 2, 30)
        );

        // Open: nothing happens.
        let open: HashSet<String> = ["discord.exe".to_string()].into();
        let out = clear("discord", &bases, &open, &last, &q).unwrap();
        assert!(out.running);
        assert!(cache.exists() && gpu.exists());

        // A file changed since it was measured is left alone.
        fs::write(&gpu, b"new").unwrap();
        let out = clear("discord", &bases, &HashSet::new(), &last, &q).unwrap();
        assert!(!out.running);
        assert!(!cache.exists());
        assert!(gpu.exists(), "changed since the measure");
        assert!(settings.exists() && other.exists());
        assert_eq!(
            (out.report.deleted_count, out.report.freed_bytes),
            (1, 4000)
        );
        assert_eq!(out.report.skipped_count, 1);
        assert_eq!(out.cache.unwrap().cache_bytes, 3, "what is left");
        assert!(q.list().is_empty(), "caches never reach the quarantine");

        assert!(clear("nope", &bases, &HashSet::new(), &last, &q).is_err());
    }

    #[test]
    fn serializes_like_the_typescript_type() {
        let c = AppCache {
            id: "spotify".into(),
            cache_bytes: 1,
            file_count: 2,
            data_bytes: 3,
            running: true,
        };
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(
            json,
            r#"{"id":"spotify","cacheBytes":1,"fileCount":2,"dataBytes":3,"running":true}"#
        );
    }
}

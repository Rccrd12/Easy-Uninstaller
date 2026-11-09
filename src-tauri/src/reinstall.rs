//! The apps uninstalled with Easy Uninstaller, kept so they can be reinstalled later:
//! from the Microsoft Store, from Steam, from the maker's website or with a web search.
//!
//! Before an uninstaller runs, `prepare` keeps the way back (the uninstaller removes
//! the registry entry that names the website). Only links made here are ever opened.

use crate::apps::{registry::RawEntry, AppSource};
use crate::quarantine::now_secs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// How many uninstalled apps are kept, most recent first.
const KEEP: usize = 100;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Way {
    Store,
    Steam,
    Website,
    Search,
}

/// Mirrors `UninstalledApp` in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UninstalledApp {
    /// The id the app had when it was installed.
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub source: AppSource,
    pub size_bytes: u64,
    /// Seconds since 1970.
    pub uninstalled_at: u64,
    pub way: Way,
    /// The website's address without `https://`, shown on the button; empty otherwise.
    #[serde(default)]
    pub site: String,
    /// Opened by `reinstall_app`.
    pub link: String,
    pub icon_color: String,
    pub icon: Option<String>,
}

/// What the UI knows about an app it just saw go: the parts of `InstalledApp` kept here.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedApp {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub source: AppSource,
    pub size_bytes: u64,
    pub icon_color: String,
    pub icon: Option<String>,
}

/// The way back for apps being uninstalled, by app id.
#[derive(Default)]
pub struct Prepared(Mutex<HashMap<String, (Way, String)>>);

/// Keeps the way back for a classic program, before its uninstaller runs.
pub fn prepare(app: &AppHandle, id: &str, raw: &RawEntry) {
    let name = raw.display_name.as_deref().unwrap_or("").trim();
    let way = program_way(&raw.key_name, raw.website.as_deref(), name);
    let state = app.state::<Prepared>();
    let mut map = state.0.lock().unwrap_or_else(|e| e.into_inner());
    map.insert(id.to_string(), way);
}

/// Steam games reinstall through Steam; other programs from their website, else a search.
fn program_way(key_name: &str, website: Option<&str>, name: &str) -> (Way, String) {
    if let Some(id) = key_name.strip_prefix("Steam App ") {
        if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
            return (Way::Steam, format!("steam://install/{id}"));
        }
    }
    match website.and_then(web_address) {
        Some(url) => (Way::Website, url),
        None => (Way::Search, search_link(name)),
    }
}

/// A plain `http(s)` address, or `None`. Adds `https://` to `www.example.com`.
fn web_address(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let url = if raw.starts_with("https://") || raw.starts_with("http://") {
        raw.to_string()
    } else if raw.to_ascii_lowercase().starts_with("www.") {
        format!("https://{raw}")
    } else {
        return None;
    };
    let host = site_of(&url);
    let ok_host = host.contains('.')
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    let ok_chars = url.bytes().all(|b| b.is_ascii_graphic());
    (ok_host && ok_chars).then_some(url)
}

/// `https://www.videolan.org/vlc/` to `videolan.org`.
fn site_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#', ':']).next().unwrap_or("");
    let host = host.to_ascii_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_string()
}

/// A web search for the app's download page.
fn search_link(name: &str) -> String {
    let mut q = String::new();
    for b in format!("{name} download").bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' => q.push(b as char),
            b' ' => q.push('+'),
            _ => q.push_str(&format!("%{b:02X}")),
        }
    }
    format!("https://www.bing.com/search?q={q}")
}

/// `ms-windows-store://` link to a Store app's page, from its package full name.
fn store_link(full_name: &str) -> Option<String> {
    let (name, _) = full_name.split_once('_')?;
    let (_, publisher_id) = full_name.rsplit_once('_')?;
    let family = format!("{name}_{publisher_id}");
    let ok = !name.is_empty()
        && !publisher_id.is_empty()
        && family
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    ok.then(|| format!("ms-windows-store://pdp/?PFN={family}"))
}

/// Only links this module makes can be opened.
fn allowed(link: &str) -> bool {
    if let Some(id) = link.strip_prefix("steam://install/") {
        return !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
    }
    if link.starts_with("ms-windows-store://pdp/?PFN=") {
        return link.bytes().all(|b| b.is_ascii_graphic());
    }
    web_address(link).as_deref() == Some(link)
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_local_data_dir().ok()?.join("reinstall.json"))
}

fn read(app: &AppHandle) -> Vec<UninstalledApp> {
    file(app)
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write(app: &AppHandle, list: &[UninstalledApp]) -> Result<(), String> {
    let file = file(app).ok_or("no data folder")?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_vec_pretty(list).map_err(|e| e.to_string())?;
    std::fs::write(file, text).map_err(|e| e.to_string())
}

/// Adds `entry` at the top, replacing an older entry with the same id or name.
fn add(list: &mut Vec<UninstalledApp>, entry: UninstalledApp) {
    list.retain(|e| e.id != entry.id && !e.name.eq_ignore_ascii_case(&entry.name));
    list.insert(0, entry);
    list.truncate(KEEP);
}

/// The record for an app just removed. `prepared` is the way back kept before its
/// uninstaller ran (classic programs only).
fn record(removed: RemovedApp, prepared: Option<(Way, String)>, now: u64) -> UninstalledApp {
    let (way, link) = match removed.id.strip_prefix("store:").and_then(store_link) {
        Some(link) => (Way::Store, link),
        None => prepared.unwrap_or_else(|| (Way::Search, search_link(&removed.name))),
    };
    UninstalledApp {
        site: if way == Way::Website {
            site_of(&link)
        } else {
            String::new()
        },
        id: removed.id,
        name: removed.name,
        publisher: removed.publisher,
        source: removed.source,
        size_bytes: removed.size_bytes,
        uninstalled_at: now,
        way,
        link,
        icon_color: removed.icon_color,
        icon: removed.icon,
    }
}

/// The apps uninstalled here, most recent first.
#[tauri::command]
pub fn list_uninstalled_apps(app: AppHandle) -> Vec<UninstalledApp> {
    read(&app)
}

/// Adds an app to the list once it is really gone.
#[tauri::command]
pub async fn remember_uninstalled(app: AppHandle, removed: RemovedApp) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if crate::uninstall::still_installed(&removed.id) {
            return Err("stillInstalled".to_string());
        }
        let prepared = {
            let state = app.state::<Prepared>();
            let mut map = state.0.lock().unwrap_or_else(|e| e.into_inner());
            map.remove(&removed.id)
        };
        let mut list = read(&app);
        add(&mut list, record(removed, prepared, now_secs()));
        write(&app, &list)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Takes apps off the list (nothing is installed or removed).
#[tauri::command]
pub fn forget_uninstalled(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    let mut list = read(&app);
    list.retain(|e| !ids.contains(&e.id));
    write(&app, &list)
}

/// Opens the way back: the Store page, Steam, the website or a web search.
#[tauri::command]
pub fn reinstall_app(app: AppHandle, id: String) -> Result<(), String> {
    let entry = read(&app)
        .into_iter()
        .find(|e| e.id == id)
        .ok_or("notFound")?;
    if !allowed(&entry.link) {
        return Err("notFound".into());
    }
    crate::ai::open_url(&entry.link);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn removed(id: &str, name: &str) -> RemovedApp {
        RemovedApp {
            id: id.into(),
            name: name.into(),
            publisher: "P".into(),
            source: AppSource::Win32,
            size_bytes: 10,
            icon_color: "#000".into(),
            icon: None,
        }
    }

    #[test]
    fn store_apps_open_their_store_page() {
        let r = record(
            removed(
                "store:SpotifyAB.SpotifyMusic_1.2_x86__zpdnekdrzrea0",
                "Spotify",
            ),
            None,
            5,
        );
        assert_eq!(r.way, Way::Store);
        assert_eq!(
            r.link,
            "ms-windows-store://pdp/?PFN=SpotifyAB.SpotifyMusic_zpdnekdrzrea0"
        );
        assert!(allowed(&r.link));
        assert_eq!(store_link("bad name_x"), None);
    }

    #[test]
    fn programs_open_their_website_or_steam_or_a_search() {
        let (way, link) = program_way("VLC", Some("http://www.videolan.org/"), "VLC");
        assert_eq!(
            (way, link.as_str()),
            (Way::Website, "http://www.videolan.org/")
        );
        assert_eq!(site_of(&link), "videolan.org");

        let (way, link) = program_way("X", Some("www.7-zip.org"), "7-Zip");
        assert_eq!(
            (way, link.as_str()),
            (Way::Website, "https://www.7-zip.org")
        );

        let (way, link) = program_way("Steam App 570", None, "Dota 2");
        assert_eq!((way, link.as_str()), (Way::Steam, "steam://install/570"));

        for bad in [
            "file:///C:/x.exe",
            "mailto:a@b.c",
            "javascript:alert(1)",
            "https://bad host/",
            "C:\\x",
        ] {
            let (way, link) = program_way("X", Some(bad), "Zoom Workplace");
            assert_eq!(way, Way::Search, "{bad}");
            assert_eq!(
                link,
                "https://www.bing.com/search?q=Zoom+Workplace+download"
            );
        }
        assert_eq!(
            search_link("Città & Co"),
            "https://www.bing.com/search?q=Citt%C3%A0+%26+Co+download"
        );
    }

    #[test]
    fn only_links_made_here_open() {
        assert!(allowed("https://www.videolan.org/"));
        assert!(allowed("steam://install/570"));
        assert!(!allowed("steam://uninstall/570"));
        assert!(!allowed("steam://install/570;calc"));
        assert!(!allowed("file:///C:/Windows/System32/calc.exe"));
        assert!(!allowed("calc.exe"));
        assert!(!allowed("ms-windows-store://pdp/?PFN=a b"));
    }

    #[test]
    fn newest_first_without_repeats() {
        let mut list = Vec::new();
        add(&mut list, record(removed("reg:hkcu:a", "Alpha"), None, 1));
        add(&mut list, record(removed("reg:hkcu:b", "Beta"), None, 2));
        add(&mut list, record(removed("reg:hklm:a2", "ALPHA"), None, 3));
        let names: Vec<_> = list.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["ALPHA", "Beta"]);
        for i in 0..KEEP + 5 {
            add(
                &mut list,
                record(removed(&format!("x{i}"), &format!("X{i}")), None, 4),
            );
        }
        assert_eq!(list.len(), KEEP);
    }

    #[test]
    fn serializes_like_the_typescript_type() {
        let r = record(
            removed("reg:hkcu:a", "Alpha"),
            Some((Way::Website, "https://alpha.example/".into())),
            7,
        );
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"way\":\"website\""));
        assert!(json.contains("\"site\":\"alpha.example\""));
        assert!(json.contains("\"uninstalledAt\":7"));
        assert!(json.contains("\"sizeBytes\":10"));
    }
}

//! Leftovers: what programs leave behind after they are uninstalled.
//!
//! Read only. Everything found here is moved through the quarantine, never deleted.
//!
//! What counts as a leftover is deliberately narrow, so nothing that still belongs
//! to something else is ever proposed:
//! - folders in Program Files, ProgramData and AppData, and keys under `Software`,
//!   whose name is exactly the uninstalled app's name (or its install folder's name),
//!   possibly inside a folder named after its publisher;
//! - shortcuts whose target is gone;
//! - entries in the app list whose uninstaller is gone;
//! - "unowned" folders (from uninstalls done elsewhere or by hand): no installed
//!   app, service or working shortcut uses them, their name is nobody's, and nothing
//!   inside changed for six months. These are never ticked by default.
//!
//! A folder or key is never proposed when another installed app has the same name,
//! uses a path inside it, or (for a publisher folder) comes from the same publisher.

mod lnk;

use crate::apps;
use crate::elevated::{self, Job};
use crate::files::Safety;
use crate::quarantine::{self, now_secs, Item, MoveReport, Quarantine, Target, Title};
use crate::regkey::{Hive, RegKeyPath};
use crate::uninstall::split_command;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// How many uninstalled apps are remembered, to look for their leftovers later.
const HISTORY_SIZE: usize = 40;
/// How deep shortcut folders are searched.
const SHORTCUT_DEPTH: usize = 3;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LeftoverKind {
    Folder,
    Registry,
    Shortcut,
}

/// Mirrors `LeftoverItem` in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LeftoverItem {
    pub id: String,
    pub app_name: Option<String>,
    pub kind: LeftoverKind,
    /// A path, or a registry key such as `HKCU\Software\App`.
    pub path: String,
    pub size_bytes: u64,
    /// i18n key under `leftovers.reasons`.
    pub reason: String,
    pub safety: Safety,
    pub needs_admin: bool,
    /// Belongs to no installed app, but no uninstall is known: for the user to check.
    pub unowned: bool,
    /// Last change inside the folder, in seconds since 1970, when known.
    pub modified_at: Option<u64>,
}

/// An app as it was just before it was uninstalled: enough to find what it leaves.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppHints {
    pub name: String,
    pub publisher: String,
    /// Its own install folder (never a shared one like Program Files).
    pub install_dir: Option<PathBuf>,
    /// Other folders it used (its uninstaller's, its icon's): only used to recognise
    /// shortcuts that pointed into them.
    pub related_dirs: Vec<PathBuf>,
    #[serde(default)]
    pub uninstalled_at: u64,
}

/// A place whose subfolders can be an app's leftovers.
#[derive(Debug, Clone)]
pub struct FolderRoot {
    pub path: PathBuf,
    pub reason: &'static str,
    pub safety: Safety,
}

/// Where leftovers are looked for.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    pub folders: Vec<FolderRoot>,
    /// Desktops and Start menu "Programs" folders.
    pub shortcuts: Vec<PathBuf>,
    /// Start menu "Programs" folders, whose app folders can be leftovers too.
    pub start_menus: Vec<PathBuf>,
    /// The user's own folder: anything outside it needs administrator rights.
    pub profile: Option<PathBuf>,
}

impl Roots {
    pub fn from_env() -> Self {
        let env = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let root = |path: PathBuf, reason, safety| FolderRoot {
            path,
            reason,
            safety,
        };
        let mut folders = Vec::new();
        let mut program_files: Vec<PathBuf> = ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
            .iter()
            .filter_map(|v| env(v))
            .collect();
        dedup_paths(&mut program_files);
        for p in program_files {
            folders.push(root(p, "programFolder", Safety::Safe));
        }
        if let Some(local) = env("LOCALAPPDATA") {
            folders.push(root(local.join("Programs"), "programFolder", Safety::Safe));
            folders.push(root(local, "localData", Safety::Safe));
        }
        if let Some(roaming) = env("APPDATA") {
            folders.push(root(roaming, "userSettings", Safety::Check));
        }
        if let Some(data) = env("ProgramData") {
            folders.push(root(data, "sharedData", Safety::Safe));
        }

        let start_menus: Vec<PathBuf> = [env("APPDATA"), env("ProgramData")]
            .into_iter()
            .flatten()
            .map(|p| p.join(r"Microsoft\Windows\Start Menu\Programs"))
            .collect();
        let mut shortcuts: Vec<PathBuf> = [
            env("USERPROFILE").map(|p| p.join("Desktop")),
            env("OneDrive").map(|p| p.join("Desktop")),
            env("PUBLIC").map(|p| p.join("Desktop")),
        ]
        .into_iter()
        .flatten()
        .chain(start_menus.iter().cloned())
        .filter(|p| p.is_dir())
        .collect();
        dedup_paths(&mut shortcuts);
        Roots {
            folders,
            shortcuts,
            start_menus,
            profile: env("USERPROFILE"),
        }
    }

    /// Outside the user's folder (Program Files, ProgramData, HKLM): Windows asks first.
    pub fn needs_admin(&self, target: &Target) -> bool {
        match target {
            Target::Registry(k) => k.needs_admin(),
            Target::File(p) | Target::Folder(p) => {
                self.profile.as_ref().map_or(true, |home| !inside(p, home))
            }
        }
    }

    fn all_root_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.folders
            .iter()
            .map(|r| &r.path)
            .chain(&self.shortcuts)
            .chain(&self.start_menus)
            .chain(&self.profile)
    }
}

/// What installed apps use: never proposed for removal.
#[derive(Debug, Clone, Default)]
pub struct InUse {
    /// Install folders: nothing inside them is a leftover.
    pub install_dirs: Vec<PathBuf>,
    /// Every folder an installed app refers to: a folder containing one is not a leftover.
    pub paths: Vec<PathBuf>,
    /// Normalized names of installed apps (and of their install folders).
    pub names: HashSet<String>,
    pub publishers: HashSet<String>,
}

impl InUse {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn add(
        &mut self,
        name: &str,
        publisher: &str,
        install_dir: Option<&Path>,
        paths: &[PathBuf],
    ) {
        self.names.extend(clean_names(name, publisher));
        self.publishers.extend(publisher_names(publisher));
        if let Some(dir) = install_dir {
            if let Some(n) = dir.file_name().and_then(|n| n.to_str()) {
                self.names.insert(norm(n));
            }
            self.install_dirs.push(dir.to_path_buf());
            self.paths.push(dir.to_path_buf());
        }
        self.paths.extend(paths.iter().cloned());
    }

    fn uses(&self, folder: &Path) -> bool {
        self.install_dirs.iter().any(|d| inside(folder, d))
            || self.paths.iter().any(|p| inside(p, folder))
    }
}

/// Lists the subkeys and values of registry keys (a fake one in tests).
pub trait RegView {
    fn subkeys(&self, key: &RegKeyPath) -> Vec<String>;
    fn value_count(&self, key: &RegKeyPath) -> usize;
}

/// A leftover, with what is needed to move it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub target: Target,
    pub app_name: Option<String>,
    pub reason: &'static str,
    pub safety: Safety,
    pub size: u64,
    /// Found without any uninstalled app to go by: belongs to no installed app.
    pub unowned: bool,
    /// Last change inside a folder, in seconds since 1970 (unowned folders only).
    pub modified: Option<u64>,
}

impl Found {
    pub fn item(&self, roots: &Roots) -> Option<LeftoverItem> {
        Some(LeftoverItem {
            id: leftover_id(&self.target),
            app_name: self.app_name.clone(),
            kind: match &self.target {
                Target::Registry(_) => LeftoverKind::Registry,
                Target::File(_) => LeftoverKind::Shortcut,
                Target::Folder(_) if self.reason == "startMenu" => LeftoverKind::Shortcut,
                Target::Folder(_) => LeftoverKind::Folder,
            },
            path: self.target.display()?,
            size_bytes: self.size,
            reason: self.reason.to_string(),
            safety: self.safety,
            needs_admin: roots.needs_admin(&self.target),
            unowned: self.unowned,
            modified_at: self.modified,
        })
    }
}

/// A stable id for a target, so the UI can refer to it across searches.
fn leftover_id(target: &Target) -> String {
    let text = format!("{:?}", target).to_lowercase();
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

// ---------------------------------------------------------------------------
// Names

/// Folder and key names that are never an app's own, whatever the app is called.
const GENERIC: &[&str] = &[
    "adobe",
    "amd",
    "app",
    "apple",
    "application",
    "applications",
    "apps",
    "assets",
    "bin",
    "cache",
    "caches",
    "common",
    "commonfiles",
    "config",
    "crashdumps",
    "crashreports",
    "d3dscache",
    "data",
    "default",
    "desktop",
    "documents",
    "dotnet",
    "downloads",
    "driver",
    "drivers",
    "extensions",
    "google",
    "history",
    "inetcache",
    "installer",
    "installers",
    "intel",
    "internetexplorer",
    "java",
    "lib",
    "local",
    "locallow",
    "log",
    "logs",
    "microsoft",
    "microsoftnet",
    "modifiablewindowsapps",
    "mozilla",
    "msbuild",
    "music",
    "nvidia",
    "nvidiacorporation",
    "onedrive",
    "oracle",
    "packagecache",
    "packages",
    "pictures",
    "plugins",
    "programdata",
    "programfiles",
    "programs",
    "public",
    "referenceassemblies",
    "resources",
    "roaming",
    "runtime",
    "runtimes",
    "sdk",
    "settings",
    "setup",
    "shared",
    "software",
    "startmenu",
    "system",
    "system32",
    "temp",
    "templates",
    "tmp",
    "tools",
    "uninstall",
    "uninstaller",
    "uninstallinformation",
    "update",
    "updater",
    "updates",
    "users",
    "videos",
    "win32",
    "win64",
    "windows",
    "windowsapps",
    "windowsdefender",
    "windowsmail",
    "windowsmediaplayer",
    "windowsnt",
    "windowsphotoviewer",
    "windowspowershell",
    "windowssidebar",
    "x64",
    "x86",
];

/// Lowercase letters and digits only: `"Mozilla Firefox"` -> `"mozillafirefox"`.
pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn usable(n: &str) -> bool {
    n.chars().count() >= 3 && !GENERIC.contains(&n)
}

/// The names an app's folders could have: its name without version, architecture
/// or anything in brackets, also without the publisher in front.
/// `"Mozilla Firefox (x64 en-US)"` by Mozilla -> `{"mozillafirefox", "firefox"}`.
pub fn clean_names(display: &str, publisher: &str) -> HashSet<String> {
    let mut text = String::new();
    let mut depth = 0;
    for c in display.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => text.push(c),
            _ => {}
        }
    }
    let arch = [
        "x64", "x86", "64-bit", "32-bit", "64bit", "32bit", "amd64", "arm64",
    ];
    let mut words: Vec<&str> = text
        .split_whitespace()
        .filter(|w| *w != "-" && !arch.contains(&w.to_lowercase().as_str()))
        .collect();
    // Trailing versions: "3.0.21", "v2", "2024", "2.1-beta"...
    while words.len() > 1
        && words.last().is_some_and(|w| {
            let w = w.trim_start_matches(['v', 'V']);
            w.starts_with(|c: char| c.is_ascii_digit())
        })
    {
        words.pop();
    }
    let mut out = HashSet::new();
    let full = norm(&words.join(" "));
    if usable(&full) {
        out.insert(full);
    }
    let publisher = norm(publisher);
    if words.len() > 1 && !publisher.is_empty() && publisher.starts_with(&norm(words[0])) {
        let rest = norm(&words[1..].join(" "));
        if usable(&rest) {
            out.insert(rest);
        }
    }
    out
}

/// `"Discord Inc."` -> `{"discord"}`. Empty for generic publishers like Microsoft.
pub fn publisher_names(publisher: &str) -> HashSet<String> {
    const SUFFIXES: &[&str] = &[
        "inc",
        "ltd",
        "llc",
        "gmbh",
        "corp",
        "corporation",
        "co",
        "ab",
        "srl",
        "spa",
        "sa",
        "bv",
        "limited",
        "company",
        "incorporated",
        "ag",
        "oy",
        "as",
        "plc",
        "pty",
        "sas",
    ];
    let mut words: Vec<&str> = publisher
        .split([' ', ',', '.'])
        .filter(|w| !w.is_empty())
        .collect();
    while words
        .last()
        .is_some_and(|w| SUFFIXES.contains(&w.to_lowercase().as_str()))
    {
        words.pop();
    }
    let mut out = HashSet::new();
    for n in [norm(&words.join(" ")), norm(publisher)] {
        if usable(&n) {
            out.insert(n);
        }
    }
    out
}

fn app_names(h: &AppHints) -> HashSet<String> {
    let mut names = clean_names(&h.name, &h.publisher);
    if let Some(n) = h
        .install_dir
        .as_ref()
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
        .map(norm)
        .filter(|n| usable(n))
    {
        names.insert(n);
    }
    names
}

// ---------------------------------------------------------------------------
// Paths

fn path_key(p: &Path) -> String {
    p.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// True when `child` is `parent` or inside it (ignoring case, like Windows).
pub fn inside(child: &Path, parent: &Path) -> bool {
    let (c, p) = (path_key(child), path_key(parent));
    !p.is_empty() && (c == p || c.starts_with(&format!("{p}\\")))
}

fn same(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

fn dedup_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = HashSet::new();
    paths.retain(|p| seen.insert(path_key(p)));
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .and_then(|n| n.to_str())
        .map(norm)
        .unwrap_or_default()
}

fn entries(dir: &Path) -> Vec<fs::DirEntry> {
    fs::read_dir(dir)
        .map(|rd| rd.flatten().collect())
        .unwrap_or_default()
}

fn real_dir(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    entries(dir)
        .into_iter()
        .map(|e| e.path())
        .filter(|p| real_dir(p))
        .collect()
}

/// `C:\` of a path, to tell a missing file from an unplugged drive.
fn drive_root(p: &Path) -> Option<PathBuf> {
    let s = p.to_str()?;
    let b = s.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        .then(|| PathBuf::from(&s[..3]))
}

/// The target is gone, and its drive is there (so it is not just an unplugged USB drive).
fn missing(target: &Path) -> bool {
    let drive_ok = if cfg!(windows) {
        drive_root(target).is_some_and(|d| d.is_dir())
    } else {
        target.is_absolute()
    };
    drive_ok && fs::symlink_metadata(target).is_err()
}

fn shortcut_target(file: &Path) -> Option<PathBuf> {
    let bytes = fs::read(file).ok()?;
    lnk::target(&bytes).filter(|t| !t.to_string_lossy().contains('%'))
}

fn is_lnk(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
}

/// Every `.lnk` under `dir`, a few levels deep.
fn shortcuts_in(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    for e in entries(dir) {
        let p = e.path();
        if real_dir(&p) {
            if depth > 1 {
                shortcuts_in(&p, depth - 1, out);
            }
        } else if is_lnk(&p) {
            out.push(p);
        }
    }
}

// ---------------------------------------------------------------------------
// Rules

/// Whether a target may be moved at all. Checked again right before moving, also
/// by the elevated helper, so a target from anywhere else is always refused.
pub fn allowed(target: &Target, roots: &Roots) -> bool {
    match target {
        Target::Registry(k) => registry_allowed(k),
        Target::File(p) => is_lnk(p) && roots.shortcuts.iter().any(|d| inside(p, d) && !same(p, d)),
        Target::Folder(p) => {
            let under_root = roots
                .folders
                .iter()
                .map(|r| &r.path)
                .chain(&roots.start_menus)
                .any(|r| inside(p, r) && !same(p, r));
            under_root && usable(&name_of(p)) && !roots.all_root_paths().any(|r| inside(r, p))
        }
    }
}

/// Keys below `Software\<name>` with a usable name, or one entry of the app list.
fn registry_allowed(key: &RegKeyPath) -> bool {
    if !key.removable() {
        return false;
    }
    let parts: Vec<String> = key.path.split('\\').map(str::to_lowercase).collect();
    let rest = match parts.get(1).map(String::as_str) {
        Some("wow6432node") => &parts[2..],
        _ => &parts[1..],
    };
    let uninstall = ["microsoft", "windows", "currentversion", "uninstall"];
    if rest.len() == 5 && rest[..4] == uninstall {
        return true;
    }
    // Windows' own keys, never an app's.
    const RESERVED: &[&str] = &["classes", "clients", "policies", "registeredapplications"];
    rest.first()
        .is_some_and(|n| usable(&norm(n)) && !RESERVED.contains(&n.as_str()))
}

// ---------------------------------------------------------------------------
// Search

/// What an uninstalled app left behind.
pub fn find_for_app(h: &AppHints, roots: &Roots, in_use: &InUse, reg: &dyn RegView) -> Vec<Found> {
    let names = app_names(h);
    if names.is_empty() {
        return Vec::new();
    }
    let pubs: HashSet<String> = publisher_names(&h.publisher)
        .into_iter()
        .filter(|p| !in_use.publishers.contains(p))
        .collect();
    let app = Some(h.name.clone());
    let wanted = |n: &str| names.contains(n) && !in_use.names.contains(n);
    let mut out = Vec::new();

    // Folders: named after the app, maybe inside a folder named after the publisher.
    let mut folders: Vec<(PathBuf, &FolderRoot)> = Vec::new();
    for root in &roots.folders {
        for child in subdirs(&root.path) {
            let n = name_of(&child);
            if wanted(&n) {
                folders.push((child, root));
            } else if pubs.contains(&n) {
                let all = entries(&child);
                let mine: Vec<PathBuf> = all
                    .iter()
                    .map(|e| e.path())
                    .filter(|p| real_dir(p) && wanted(&name_of(p)))
                    .collect();
                if !mine.is_empty() && mine.len() == all.len() {
                    folders.push((child, root));
                } else {
                    folders.extend(mine.into_iter().map(|p| (p, root)));
                }
            }
        }
    }
    // The app's own install folder, when its uninstaller left it.
    if let Some(dir) = h.install_dir.as_ref().filter(|d| real_dir(d)) {
        if let Some(root) = roots
            .folders
            .iter()
            .filter(|r| inside(dir, &r.path) && !same(dir, &r.path))
            .max_by_key(|r| path_key(&r.path).len())
        {
            folders.push((dir.clone(), root));
        }
    }
    folders.retain(|(p, _)| {
        allowed(&Target::Folder(p.clone()), roots) && real_dir(p) && !in_use.uses(p)
    });
    // Outermost first, then drop anything inside a folder already taken.
    folders.sort_by_key(|(p, _)| path_key(p).len());
    let mut taken: Vec<PathBuf> = Vec::new();
    for (p, root) in folders {
        if taken.iter().any(|t| inside(&p, t)) {
            continue;
        }
        out.push(Found {
            size: apps::dir_size(&p),
            target: Target::Folder(p.clone()),
            app_name: app.clone(),
            reason: root.reason,
            safety: root.safety,
            unowned: false,
            modified: None,
        });
        taken.push(p);
    }

    // Registry keys, the same way.
    let bases = [
        RegKeyPath::new(Hive::Hkcu, "Software"),
        RegKeyPath::new(Hive::Hklm, "Software"),
        RegKeyPath::new(Hive::Hklm, r"Software\WOW6432Node"),
    ];
    for base in &bases {
        for name in reg.subkeys(base) {
            let key = RegKeyPath::new(base.hive, &format!("{}\\{name}", base.path));
            let n = norm(&name);
            let mut keys = Vec::new();
            if wanted(&n) {
                keys.push(key);
            } else if pubs.contains(&n) {
                let all = reg.subkeys(&key);
                let mine: Vec<String> = all.iter().filter(|s| wanted(&norm(s))).cloned().collect();
                if !mine.is_empty() && mine.len() == all.len() && reg.value_count(&key) == 0 {
                    keys.push(key);
                } else {
                    keys.extend(
                        mine.iter()
                            .map(|s| RegKeyPath::new(key.hive, &format!("{}\\{s}", key.path))),
                    );
                }
            }
            for k in keys.into_iter().filter(registry_allowed) {
                out.push(Found {
                    target: Target::Registry(k),
                    app_name: app.clone(),
                    reason: "registry",
                    safety: Safety::Safe,
                    size: 0,
                    unowned: false,
                    modified: None,
                });
            }
        }
    }

    // Shortcuts that led into the app's folders, or carry its name, and lead nowhere now.
    let app_dirs: Vec<PathBuf> = h
        .install_dir
        .iter()
        .chain(&h.related_dirs)
        .cloned()
        .chain(taken.iter().cloned())
        .collect();
    let points_into_app = |t: &Path| app_dirs.iter().any(|d| inside(t, d));
    let mut shortcut_folders: Vec<PathBuf> = Vec::new();
    for menu in &roots.start_menus {
        for dir in subdirs(menu) {
            if !wanted(&name_of(&dir)) {
                continue;
            }
            let mut links = Vec::new();
            shortcuts_in(&dir, SHORTCUT_DEPTH, &mut links);
            let only_ours = links
                .iter()
                .all(|l| shortcut_target(l).is_some_and(|t| missing(&t) || points_into_app(&t)));
            let only_links = only_shortcut_files(&dir, SHORTCUT_DEPTH);
            if only_ours && only_links && allowed(&Target::Folder(dir.clone()), roots) {
                out.push(Found {
                    target: Target::Folder(dir.clone()),
                    app_name: app.clone(),
                    reason: "startMenu",
                    safety: Safety::Safe,
                    size: apps::dir_size(&dir),
                    unowned: false,
                    modified: None,
                });
                shortcut_folders.push(dir);
            }
        }
    }
    let mut links = Vec::new();
    for dir in &roots.shortcuts {
        shortcuts_in(dir, SHORTCUT_DEPTH, &mut links);
    }
    dedup_paths(&mut links);
    for link in links {
        if shortcut_folders.iter().any(|f| inside(&link, f)) {
            continue;
        }
        let Some(t) = shortcut_target(&link) else {
            continue;
        };
        let stem = link.file_stem().and_then(|s| s.to_str()).map(norm);
        let ours = points_into_app(&t) || stem.is_some_and(|s| names.contains(&s));
        if ours && missing(&t) {
            out.push(shortcut(link, app.clone(), "shortcut"));
        }
    }
    out
}

fn shortcut(link: PathBuf, app_name: Option<String>, reason: &'static str) -> Found {
    Found {
        size: fs::metadata(&link).map_or(0, |m| m.len()),
        target: Target::File(link),
        app_name,
        reason,
        safety: Safety::Safe,
        unowned: false,
        modified: None,
    }
}

/// True when `dir` holds nothing but shortcuts (and `desktop.ini`).
fn only_shortcut_files(dir: &Path, depth: usize) -> bool {
    entries(dir).iter().all(|e| {
        let p = e.path();
        if real_dir(&p) {
            depth > 1 && only_shortcut_files(&p, depth - 1)
        } else {
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_lowercase);
            matches!(ext.as_deref(), Some("lnk" | "url" | "ini"))
        }
    })
}

/// One entry of the app list, as needed to spot a broken one.
#[derive(Debug, Clone)]
pub struct ListEntry {
    pub key: RegKeyPath,
    pub name: String,
    pub uninstall_string: String,
    pub install_location: Option<String>,
    pub msi: bool,
    /// Already removed by its launcher (see `registry::launcher_gone`).
    pub gone: bool,
}

/// Leftovers that need no history: shortcuts that lead nowhere, and entries in
/// the app list whose uninstaller is gone (they can never be uninstalled).
pub fn find_orphans(roots: &Roots, list: &[ListEntry]) -> Vec<Found> {
    let mut out = Vec::new();
    let mut links = Vec::new();
    for dir in &roots.shortcuts {
        shortcuts_in(dir, SHORTCUT_DEPTH, &mut links);
    }
    dedup_paths(&mut links);
    for link in links {
        if shortcut_target(&link).is_some_and(|t| missing(&t)) {
            let name = link.file_stem().map(|s| s.to_string_lossy().into_owned());
            out.push(shortcut(link, name, "brokenShortcut"));
        }
    }
    for e in list {
        if e.gone && registry_allowed(&e.key) {
            out.push(stale_entry(&e.key, &e.name));
            continue;
        }
        if e.msi {
            continue;
        }
        let Some(cmd) = split_command(&e.uninstall_string, &|p| p.is_file()) else {
            continue;
        };
        let file = Path::new(&cmd.file);
        let tool = name_of(file);
        if ["rundll32exe", "msiexecexe", "rundll32", "msiexec"].contains(&tool.as_str()) {
            continue;
        }
        let location_gone = e.install_location.as_deref().map_or(true, |l| {
            l.trim().is_empty() || !Path::new(l.trim().trim_matches('"')).exists()
        });
        if missing(file) && location_gone && registry_allowed(&e.key) {
            out.push(Found {
                target: Target::Registry(e.key.clone()),
                app_name: Some(e.name.clone()),
                reason: "brokenEntry",
                safety: Safety::Safe,
                size: 0,
                unowned: false,
                modified: None,
            });
        }
    }
    out
}

/// An entry left in the app list by an app that is gone.
fn stale_entry(key: &RegKeyPath, name: &str) -> Found {
    Found {
        target: Target::Registry(key.clone()),
        app_name: Some(name.to_string()),
        reason: "brokenEntry",
        safety: Safety::Safe,
        size: 0,
        unowned: false,
        modified: None,
    }
}

/// How long a folder must have been left untouched to be offered as unowned.
pub const UNOWNED_AFTER_DAYS: u64 = 180;
/// Folders with more entries than this are too big to judge, and skipped.
const UNOWNED_MAX_ENTRIES: usize = 200_000;

/// Names (normalized) that Windows, drivers and developer tools use in AppData,
/// ProgramData and Program Files without being in the app list.
const SYSTEM_NAMES: &[&str] = &[
    "assembly",
    "chocolatey",
    "comms",
    "connecteddevicesplatform",
    "docker",
    "elevateddiagnostics",
    "fontcache",
    "gamedvr",
    "git",
    "gobuild",
    "identitynexusintegration",
    "jupyter",
    "lowtemp",
    "node",
    "nodegyp",
    "npm",
    "npmcache",
    "nuget",
    "peernetworking",
    "pip",
    "placeholdertilelogofolder",
    "pnpm",
    "publishers",
    "pypa",
    "python",
    "regid199106commicrosoft",
    "scoop",
    "softwaredistribution",
    "squirreltemp",
    "ssh",
    "usoprivate",
    "usoshared",
    "virtualstore",
    "wsl",
    "yarn",
];
/// Name beginnings of the same kind: Windows itself and hardware drivers.
const SYSTEM_PREFIXES: &[&str] = &[
    "acer",
    "amd",
    "asus",
    "cache",
    "crash",
    "dell",
    "hp",
    "intel",
    "lenovo",
    "log",
    "logitech",
    "microsoft",
    "msedge",
    "nvidia",
    "package",
    "realtek",
    "temp",
    "windows",
];

/// Whether a folder name can be an unowned leftover: not generic, not Windows',
/// and not close to the name or publisher of anything installed.
fn unowned_name(n: &str, in_use: &InUse) -> bool {
    let close = |a: &String| {
        a == n || (a.len() >= 4 && n.contains(a.as_str())) || (n.len() >= 4 && a.contains(n))
    };
    usable(n)
        && !SYSTEM_NAMES.contains(&n)
        && !SYSTEM_PREFIXES.iter().any(|p| n.starts_with(p))
        && !in_use.names.iter().chain(&in_use.publishers).any(close)
}

/// Seconds since 1970 of a file's last change.
fn mtime(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// Size and last change of a folder, or `None` when anything inside changed after
/// `cutoff` (or it is too big to tell).
fn stale_size(dir: &Path, cutoff: u64) -> Option<(u64, u64)> {
    let mut newest = mtime(&fs::symlink_metadata(dir).ok()?);
    let (mut size, mut seen) = (0, 0);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in entries(&current) {
            seen += 1;
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            newest = newest.max(mtime(&meta));
            if newest > cutoff || seen > UNOWNED_MAX_ENTRIES {
                return None;
            }
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                size += meta.len();
            }
        }
    }
    Some((size, newest))
}

/// Folders (and keys with their names) that no installed app owns: leftovers of
/// apps removed by hand or by another tool. `links` are the targets of the
/// shortcuts that still work: a folder they point into is in use.
pub fn find_unowned(
    roots: &Roots,
    in_use: &InUse,
    links: &[PathBuf],
    reg: &dyn RegView,
    now: u64,
) -> Vec<Found> {
    let cutoff = now.saturating_sub(UNOWNED_AFTER_DAYS * 24 * 60 * 60);
    let mut out = Vec::new();
    let mut names: HashMap<String, String> = HashMap::new();
    for root in &roots.folders {
        for dir in subdirs(&root.path) {
            let n = name_of(&dir);
            let candidate = unowned_name(&n, in_use)
                && allowed(&Target::Folder(dir.clone()), roots)
                && !in_use.uses(&dir)
                && !links.iter().any(|l| inside(l, &dir));
            if !candidate {
                continue;
            }
            let Some((size, modified)) = stale_size(&dir, cutoff) else {
                continue;
            };
            let shown = dir
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            names.entry(n).or_insert_with(|| shown.clone());
            out.push(Found {
                target: Target::Folder(dir),
                app_name: Some(shown),
                reason: root.reason,
                safety: Safety::Check,
                size,
                unowned: true,
                modified: Some(modified),
            });
        }
    }
    let bases = [
        RegKeyPath::new(Hive::Hkcu, "Software"),
        RegKeyPath::new(Hive::Hklm, "Software"),
        RegKeyPath::new(Hive::Hklm, r"Software\WOW6432Node"),
    ];
    for base in &bases {
        for name in reg.subkeys(base) {
            let Some(shown) = names.get(&norm(&name)) else {
                continue;
            };
            let key = RegKeyPath::new(base.hive, &format!("{}\\{name}", base.path));
            if registry_allowed(&key) {
                out.push(Found {
                    target: Target::Registry(key),
                    app_name: Some(shown.clone()),
                    reason: "registry",
                    safety: Safety::Check,
                    size: 0,
                    unowned: true,
                    modified: None,
                });
            }
        }
    }
    out
}

/// Where the shortcuts that still work lead.
fn working_link_targets(roots: &Roots) -> Vec<PathBuf> {
    let mut links = Vec::new();
    for dir in &roots.shortcuts {
        shortcuts_in(dir, SHORTCUT_DEPTH, &mut links);
    }
    links
        .iter()
        .filter_map(|l| shortcut_target(l))
        .filter(|t| t.exists())
        .collect()
}

// ---------------------------------------------------------------------------
// State and commands

/// Leftovers found by the last searches, by id: the UI sends ids back, Rust moves
/// exactly what it found.
#[derive(Default)]
pub struct LastLeftovers(Mutex<HashMap<String, Found>>);

impl LastLeftovers {
    fn keep(&self, found: &[Found], roots: &Roots) -> Vec<LeftoverItem> {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut items = Vec::new();
        for f in found {
            if let Some(item) = f.item(roots) {
                map.insert(item.id.clone(), f.clone());
                items.push(item);
            }
        }
        items
    }

    fn take(&self, ids: &[String]) -> Vec<Found> {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        ids.iter().filter_map(|id| map.remove(id)).collect()
    }
}

fn history_file(app: &AppHandle) -> Option<PathBuf> {
    Some(
        app.path()
            .app_local_data_dir()
            .ok()?
            .join("uninstalled.json"),
    )
}

pub fn read_history(app: &AppHandle) -> Vec<AppHints> {
    history_file(app)
        .and_then(|f| fs::read(f).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Remembers an uninstalled app, so its leftovers can be found again later.
pub fn remember(app: &AppHandle, hints: &AppHints) {
    let Some(file) = history_file(app) else {
        return;
    };
    let mut history = read_history(app);
    history.retain(|h| !h.name.eq_ignore_ascii_case(&hints.name));
    history.insert(0, hints.clone());
    history.truncate(HISTORY_SIZE);
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_vec_pretty(&history) {
        let _ = fs::write(file, text);
    }
}

/// Leftovers of one app just uninstalled.
/// `stale`: the app's own entry in the app list, when its launcher left it there.
pub fn search_app(
    app: &AppHandle,
    hints: &AppHints,
    stale: Option<&RegKeyPath>,
) -> Vec<LeftoverItem> {
    let roots = Roots::from_env();
    let mut found = find_for_app(hints, &roots, &platform::in_use(), &platform::Registry);
    if let Some(key) = stale.filter(|k| registry_allowed(k)) {
        found.push(stale_entry(key, &hints.name));
    }
    app.state::<LastLeftovers>().keep(&found, &roots)
}

/// Everything the Leftovers screen shows.
#[tauri::command]
pub async fn list_leftovers(app: AppHandle) -> Result<Vec<LeftoverItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let roots = Roots::from_env();
        let in_use = platform::in_use();
        let mut found = find_orphans(&roots, &platform::app_list());
        for h in read_history(&app) {
            if app_names(&h).iter().any(|n| in_use.names.contains(n)) {
                continue; // Installed again.
            }
            found.extend(find_for_app(&h, &roots, &in_use, &platform::Registry));
        }
        // Last, so that what is known about an app wins over a guess.
        let links = working_link_targets(&roots);
        found.extend(find_unowned(
            &roots,
            &in_use,
            &links,
            &platform::Registry,
            now_secs(),
        ));
        let mut seen = HashSet::new();
        found.retain(|f| seen.insert(leftover_id(&f.target)));
        found.sort_by_key(|f| std::cmp::Reverse(f.size));
        app.state::<LastLeftovers>().keep(&found, &roots)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Moves leftovers into quarantine, one entry per app. Those outside the user's
/// folder go through one elevated helper, so Windows asks only once.
#[tauri::command]
pub async fn quarantine_leftovers(app: AppHandle, ids: Vec<String>) -> Result<MoveReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let roots = Roots::from_env();
        let found = app.state::<LastLeftovers>().take(&ids);
        let mut groups: Vec<(Title, bool, Vec<Item>)> = Vec::new();
        for f in found.into_iter().filter(|f| allowed(&f.target, &roots)) {
            let title = match &f.app_name {
                Some(name) if f.reason != "brokenShortcut" => Title::Text(name.clone()),
                _ => Title::Key("leftovers.title".into()),
            };
            let admin = roots.needs_admin(&f.target);
            let item = Item {
                target: f.target,
                size: f.size,
                modified: None,
            };
            match groups
                .iter_mut()
                .find(|(t, a, _)| *t == title && *a == admin)
            {
                Some((_, _, items)) => items.push(item),
                None => groups.push((title, admin, vec![item])),
            }
        }

        let quarantine = app.state::<Quarantine>();
        let mut report = MoveReport::default();
        let mut elevated_groups = Vec::new();
        for (title, admin, items) in groups {
            if admin {
                elevated_groups.push((title, items));
            } else {
                quarantine
                    .add(title, &items, now_secs(), false, &mut report)
                    .map_err(|e| e.to_string())?;
            }
        }
        if !elevated_groups.is_empty() {
            let count: usize = elevated_groups.iter().map(|(_, i)| i.len()).sum();
            let job = Job::Add {
                root: quarantine::root_of(&app),
                groups: elevated_groups,
            };
            match elevated::run(&app, &job)? {
                Some(result) => report.merge(result.moved.unwrap_or_default()),
                None => report.declined_count += count as u64,
            }
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(windows)]
mod platform {
    use super::{InUse, ListEntry, RegView};
    use crate::apps::registry::{self, platform::open};
    use crate::regkey::RegKeyPath;
    use std::path::PathBuf;

    pub struct Registry;

    impl RegView for Registry {
        fn subkeys(&self, key: &RegKeyPath) -> Vec<String> {
            registry::platform::subkeys(key)
        }

        fn value_count(&self, key: &RegKeyPath) -> usize {
            open(key).map_or(0, |k| k.enum_values().count())
        }
    }

    /// Every installed program (hidden ones too) and Store app.
    pub fn in_use() -> InUse {
        let mut in_use = InUse::default();
        for (_, _, raw) in registry::all_raw() {
            if registry::launcher_gone(&raw, &|p| p.exists()) {
                continue; // Already removed by its launcher.
            }
            let paths: Vec<PathBuf> =
                [raw.display_icon.as_deref(), raw.uninstall_string.as_deref()]
                    .into_iter()
                    .flatten()
                    .filter_map(crate::uninstall::program_dir)
                    .collect();
            let install = raw
                .install_location
                .as_deref()
                .and_then(crate::apps::own_install_dir);
            in_use.add(
                raw.display_name.as_deref().unwrap_or(""),
                raw.publisher.as_deref().unwrap_or(""),
                install.as_deref(),
                &paths,
            );
        }
        for app in crate::apps::store_list() {
            in_use.add(&app.name, &app.publisher, None, &app.dirs);
        }
        // Services and drivers: often not in the app list, but their folders are used.
        let services = RegKeyPath::new(
            crate::regkey::Hive::Hklm,
            r"SYSTEM\CurrentControlSet\Services",
        );
        let dirs: Vec<PathBuf> = registry::platform::subkeys(&services)
            .iter()
            .filter_map(|name| {
                let key = RegKeyPath::new(services.hive, &format!("{}\\{name}", services.path));
                open(&key)?.get_value::<String, _>("ImagePath").ok()
            })
            .filter_map(|image| crate::uninstall::program_dir(&image))
            .collect();
        in_use.add("", "", None, &dirs);
        in_use
    }

    pub fn app_list() -> Vec<ListEntry> {
        registry::all_raw()
            .into_iter()
            .filter_map(|(_, key, raw)| {
                let gone = registry::launcher_gone(&raw, &|p| p.exists());
                Some(ListEntry {
                    key,
                    name: raw.display_name?,
                    uninstall_string: raw.uninstall_string?,
                    install_location: raw.install_location,
                    msi: raw.windows_installer == Some(1),
                    gone,
                })
            })
            .collect()
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{InUse, ListEntry, RegView};
    use crate::regkey::RegKeyPath;

    pub struct Registry;

    impl RegView for Registry {
        fn subkeys(&self, _key: &RegKeyPath) -> Vec<String> {
            Vec::new()
        }

        fn value_count(&self, _key: &RegKeyPath) -> usize {
            0
        }
    }

    pub fn in_use() -> InUse {
        InUse::default()
    }

    pub fn app_list() -> Vec<ListEntry> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;

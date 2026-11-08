//! Classic programs (Win32 and MSI), as listed by "Apps and features" / Control Panel,
//! read from the `Uninstall` registry keys.
#![cfg_attr(not(windows), allow(dead_code))]

use super::AppSource;
use crate::regkey::RegKeyPath;

/// Values read from one `...\CurrentVersion\Uninstall\<key>` subkey.
#[derive(Debug, Default, Clone)]
pub struct RawEntry {
    pub key_name: String,
    pub display_name: Option<String>,
    pub publisher: Option<String>,
    pub display_version: Option<String>,
    /// `EstimatedSize`, in KB.
    pub estimated_size: Option<u32>,
    pub install_date: Option<String>,
    pub install_location: Option<String>,
    pub display_icon: Option<String>,
    pub uninstall_string: Option<String>,
    pub quiet_uninstall_string: Option<String>,
    pub system_component: Option<u32>,
    pub windows_installer: Option<u32>,
    pub parent_key_name: Option<String>,
    pub release_type: Option<String>,
}

/// A program worth showing to the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// `reg:<scope>:<key>`: enough to find the entry again to uninstall it.
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub source: AppSource,
    pub size_kb: Option<u64>,
    pub installed_on: Option<String>,
    pub install_location: Option<String>,
    pub display_icon: Option<String>,
}

/// Keeps only what "Apps and features" would show: entries with a name and an uninstaller,
/// minus system components and updates/patches of other programs.
pub fn classify(scope: &str, raw: RawEntry) -> Option<Entry> {
    let clean = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let name = clean(raw.display_name)?;
    clean(raw.uninstall_string)?;
    if raw.system_component == Some(1) || clean(raw.parent_key_name).is_some() {
        return None;
    }
    let release = clean(raw.release_type).unwrap_or_default().to_lowercase();
    if ["update", "hotfix", "security update", "service pack"].contains(&release.as_str()) {
        return None;
    }
    if is_kb_update(&raw.key_name) {
        return None;
    }
    Some(Entry {
        id: format!("reg:{scope}:{}", raw.key_name),
        name,
        publisher: clean(raw.publisher).unwrap_or_default(),
        version: clean(raw.display_version).unwrap_or_default(),
        source: if raw.windows_installer == Some(1) {
            AppSource::Msi
        } else {
            AppSource::Win32
        },
        size_kb: raw.estimated_size.filter(|&kb| kb > 0).map(u64::from),
        installed_on: raw.install_date.as_deref().and_then(parse_install_date),
        install_location: clean(raw.install_location),
        display_icon: clean(raw.display_icon),
    })
}

/// `KB1234567` style keys are Windows/Office updates.
fn is_kb_update(key: &str) -> bool {
    let digits = key
        .strip_prefix("KB")
        .or_else(|| key.strip_prefix("kb"))
        .unwrap_or("");
    digits.len() >= 6 && digits.bytes().all(|b| b.is_ascii_digit())
}

/// `InstallDate` is usually `YYYYMMDD`; some installers write `YYYY-MM-DD` or `M/D/YYYY`.
pub fn parse_install_date(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let (y, m, d): (u32, u32, u32) = if raw.len() == 8 && raw.bytes().all(|b| b.is_ascii_digit()) {
        (
            raw[0..4].parse().ok()?,
            raw[4..6].parse().ok()?,
            raw[6..8].parse().ok()?,
        )
    } else if let [y, m, d] = raw.split('-').collect::<Vec<_>>()[..] {
        (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)
    } else if let [m, d, y] = raw.split('/').collect::<Vec<_>>()[..] {
        (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)
    } else {
        return None;
    };
    let valid = (1980..=2200).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d);
    valid.then(|| format!("{y:04}-{m:02}-{d:02}"))
}

/// The same program can be registered twice (e.g. per-user and per-machine, or in both
/// registry views). Keeps the first entry for each name + version, filling in a missing size.
pub fn dedup(entries: Vec<Entry>) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::with_capacity(entries.len());
    for e in entries {
        let same = |o: &&mut Entry| {
            o.name.eq_ignore_ascii_case(&e.name) && o.version.eq_ignore_ascii_case(&e.version)
        };
        match out.iter_mut().find(|o| same(o)) {
            Some(existing) => {
                existing.size_kb = existing.size_kb.max(e.size_kb);
                if existing.display_icon.is_none() {
                    existing.display_icon = e.display_icon;
                }
            }
            None => out.push(e),
        }
    }
    out
}

/// The three places Windows keeps the list of programs, as `(scope, key)`.
/// The scope is part of an app id (`reg:<scope>:<key>`).
pub const SCOPES: [(&str, &str); 3] = [
    (
        "hklm",
        r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
    (
        "hklm32",
        r"HKLM\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
    (
        "hkcu",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
];

/// The registry key of one program, from the scope and key name in its id.
pub fn uninstall_key(scope: &str, key_name: &str) -> Option<RegKeyPath> {
    if key_name.is_empty() || key_name.contains('\\') || key_name == ".." {
        return None;
    }
    let (_, root) = SCOPES.iter().find(|(s, _)| *s == scope)?;
    RegKeyPath::parse(&format!("{root}\\{key_name}"))
}

/// All visible programs from the three places Windows keeps them.
pub fn list() -> Vec<Entry> {
    let entries = all_raw()
        .into_iter()
        .filter(|(_, _, raw)| !launcher_gone(raw, &|p| p.exists()))
        .filter_map(|(scope, _, raw)| classify(scope, raw))
        .collect();
    dedup(entries)
}

/// True for a game or app of a launcher (Steam, Ubisoft Connect, Epic, ...) that
/// the launcher already removed while leaving its entry in the app list. Such
/// entries uninstall through a link like `steam://uninstall/359550`.
/// - Steam: its library no longer has `appmanifest_<id>.acf`.
/// - Others: the install folder is gone.
pub fn launcher_gone(raw: &RawEntry, exists: &dyn Fn(&std::path::Path) -> bool) -> bool {
    let launcher = raw
        .uninstall_string
        .as_deref()
        .is_some_and(|s| s.contains("://"));
    let location = raw
        .install_location
        .as_deref()
        .map(|l| l.trim().trim_matches('"').trim_end_matches(['\\', '/']))
        .unwrap_or("");
    if !launcher || location.len() < 4 {
        return false;
    }
    if let Some(id) = raw.key_name.strip_prefix("Steam App ") {
        let lower = location.to_ascii_lowercase();
        if let Some(i) = lower.find("\\steamapps\\common\\") {
            if id.bytes().all(|b| b.is_ascii_digit()) && !id.is_empty() {
                let manifest = format!("{}\\steamapps\\appmanifest_{id}.acf", &location[..i]);
                return !exists(std::path::Path::new(&manifest));
            }
        }
    }
    !exists(std::path::Path::new(location))
}

/// Every program entry, hidden ones included, with its scope and key.
#[cfg(windows)]
pub fn all_raw() -> Vec<(&'static str, RegKeyPath, RawEntry)> {
    let mut out = Vec::new();
    for (scope, root) in SCOPES {
        let Some(root) = RegKeyPath::parse(root) else {
            continue;
        };
        for key_name in platform::subkeys(&root) {
            let key = RegKeyPath::new(root.hive, &format!("{}\\{key_name}", root.path));
            if let Some(raw) = read_raw(&key) {
                out.push((scope, key, raw));
            }
        }
    }
    out
}

#[cfg(not(windows))]
pub fn all_raw() -> Vec<(&'static str, RegKeyPath, RawEntry)> {
    Vec::new()
}

/// Reads one program entry, or `None` when it is gone.
#[cfg(windows)]
pub fn read_raw(key: &RegKeyPath) -> Option<RawEntry> {
    platform::read(key)
}

#[cfg(not(windows))]
pub fn read_raw(_key: &RegKeyPath) -> Option<RawEntry> {
    None
}

#[cfg(windows)]
pub mod platform {
    use super::RawEntry;
    use crate::regkey::{Hive, RegKeyPath};
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
    use winreg::RegKey;

    pub fn open(key: &RegKeyPath) -> Option<RegKey> {
        let hive = match key.hive {
            Hive::Hkcu => HKEY_CURRENT_USER,
            Hive::Hklm => HKEY_LOCAL_MACHINE,
        };
        RegKey::predef(hive)
            .open_subkey_with_flags(&key.path, KEY_READ | KEY_WOW64_64KEY)
            .ok()
    }

    pub fn subkeys(key: &RegKeyPath) -> Vec<String> {
        open(key)
            .map(|k| k.enum_keys().flatten().collect())
            .unwrap_or_default()
    }

    pub fn read(key: &RegKeyPath) -> Option<RawEntry> {
        let k = open(key)?;
        let text = |name: &str| k.get_value::<String, _>(name).ok();
        let number = |name: &str| k.get_value::<u32, _>(name).ok();
        Some(RawEntry {
            display_name: text("DisplayName"),
            publisher: text("Publisher"),
            display_version: text("DisplayVersion"),
            estimated_size: number("EstimatedSize"),
            install_date: text("InstallDate"),
            install_location: text("InstallLocation"),
            display_icon: text("DisplayIcon"),
            uninstall_string: text("UninstallString"),
            quiet_uninstall_string: text("QuietUninstallString"),
            system_component: number("SystemComponent"),
            windows_installer: number("WindowsInstaller"),
            parent_key_name: text("ParentKeyName"),
            release_type: text("ReleaseType"),
            key_name: key.name().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_entries_left_behind() {
        let steam = RawEntry {
            key_name: "Steam App 359550".into(),
            uninstall_string: Some(
                r#""C:\Program Files (x86)\Steam\steam.exe" steam://uninstall/359550"#.into(),
            ),
            install_location: Some(
                r"D:\Games\SteamLibrary\steamapps\common\Rainbow Six Siege".into(),
            ),
            ..RawEntry::default()
        };
        let manifest = r"D:\Games\SteamLibrary\steamapps\appmanifest_359550.acf";
        assert!(!launcher_gone(&steam, &|p| p.to_str() == Some(manifest)));
        // Folder still there with a few files, but Steam dropped the game.
        assert!(launcher_gone(&steam, &|p| p.to_str() != Some(manifest)));

        let uplay = RawEntry {
            key_name: "Uplay Install 635".into(),
            uninstall_string: Some(r#""C:\Ubisoft\upc.exe" uplay://uninstall/635"#.into()),
            install_location: Some(r"C:\Games\R6\".into()),
            ..RawEntry::default()
        };
        assert!(launcher_gone(&uplay, &|_| false));
        assert!(!launcher_gone(&uplay, &|_| true));

        // Normal programs are never judged by their folder.
        let normal = RawEntry {
            uninstall_string: Some(r"C:\App\unins000.exe".into()),
            install_location: Some(r"C:\App".into()),
            ..RawEntry::default()
        };
        assert!(!launcher_gone(&normal, &|_| false));
    }

    fn program(name: &str) -> RawEntry {
        RawEntry {
            key_name: format!("{name}_is1"),
            display_name: Some(name.into()),
            publisher: Some(" VideoLAN ".into()),
            display_version: Some("3.0.21".into()),
            estimated_size: Some(163_840),
            install_date: Some("20231110".into()),
            install_location: Some(r"C:\Program Files\VideoLAN\VLC".into()),
            display_icon: Some(r"C:\Program Files\VideoLAN\VLC\vlc.exe,0".into()),
            uninstall_string: Some(r#""C:\Program Files\VideoLAN\VLC\uninstall.exe""#.into()),
            ..RawEntry::default()
        }
    }

    #[test]
    fn keeps_a_normal_program() {
        let e = classify("hklm", program("VLC media player")).unwrap();
        assert_eq!(e.id, "reg:hklm:VLC media player_is1");
        assert_eq!(e.name, "VLC media player");
        assert_eq!(e.publisher, "VideoLAN");
        assert_eq!(e.source, AppSource::Win32);
        assert_eq!(e.size_kb, Some(163_840));
        assert_eq!(e.installed_on.as_deref(), Some("2023-11-10"));
    }

    #[test]
    fn msi_programs_are_marked() {
        let raw = RawEntry {
            windows_installer: Some(1),
            ..program("7-Zip")
        };
        assert_eq!(classify("hklm", raw).unwrap().source, AppSource::Msi);
    }

    #[test]
    fn skips_entries_without_name_or_uninstaller() {
        let no_name = RawEntry {
            display_name: Some("  ".into()),
            ..program("x")
        };
        let no_uninstaller = RawEntry {
            uninstall_string: None,
            ..program("x")
        };
        assert!(classify("hklm", no_name).is_none());
        assert!(classify("hklm", no_uninstaller).is_none());
    }

    #[test]
    fn skips_system_components_and_updates() {
        let system = RawEntry {
            system_component: Some(1),
            ..program("Microsoft Edge Update")
        };
        let patch = RawEntry {
            parent_key_name: Some("OfficeProfessional".into()),
            ..program("Update for Office")
        };
        let hotfix = RawEntry {
            release_type: Some("Security Update".into()),
            ..program("Security Update for X")
        };
        let kb = RawEntry {
            key_name: "KB2504637".into(),
            ..program("Update for Windows")
        };
        for raw in [system, patch, hotfix, kb] {
            assert!(classify("hklm", raw).is_none());
        }
    }

    #[test]
    fn zero_size_means_unknown() {
        let raw = RawEntry {
            estimated_size: Some(0),
            ..program("x")
        };
        assert_eq!(classify("hkcu", raw).unwrap().size_kb, None);
    }

    #[test]
    fn parses_install_dates() {
        assert_eq!(
            parse_install_date("20240602").as_deref(),
            Some("2024-06-02")
        );
        assert_eq!(
            parse_install_date("2024-6-2").as_deref(),
            Some("2024-06-02")
        );
        assert_eq!(
            parse_install_date("6/2/2024").as_deref(),
            Some("2024-06-02")
        );
        assert_eq!(parse_install_date("20241399"), None);
        assert_eq!(parse_install_date("yesterday"), None);
        assert_eq!(parse_install_date(""), None);
    }

    #[test]
    fn uninstall_keys_from_ids() {
        assert_eq!(
            uninstall_key("hklm32", "VLC media player")
                .unwrap()
                .to_string(),
            r"HKLM\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\VLC media player"
        );
        assert_eq!(
            uninstall_key("hkcu", "{GUID}").unwrap().to_string(),
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{GUID}"
        );
        assert_eq!(uninstall_key("hkcr", "x"), None);
        assert_eq!(uninstall_key("hkcu", r"..\..\Run"), None);
        assert_eq!(uninstall_key("hkcu", ""), None);
    }

    #[test]
    fn kb_keys_are_detected() {
        assert!(is_kb_update("KB2504637"));
        assert!(!is_kb_update("KBMedia"));
        assert!(!is_kb_update("{GUID}"));
    }

    #[test]
    fn dedup_merges_the_same_program_registered_twice() {
        let a = classify(
            "hklm",
            RawEntry {
                estimated_size: None,
                display_icon: None,
                ..program("Zoom")
            },
        )
        .unwrap();
        let b = classify("hkcu", program("zoom")).unwrap();
        let c = classify(
            "hkcu",
            RawEntry {
                display_version: Some("2.0".into()),
                ..program("Zoom")
            },
        )
        .unwrap();
        let out = dedup(vec![a, b, c]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "reg:hklm:Zoom_is1");
        assert_eq!(out[0].size_kb, Some(163_840));
        assert!(out[0].display_icon.is_some());
    }
}

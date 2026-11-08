//! Classic programs (Win32 and MSI), as listed by "Apps and features" / Control Panel,
//! read from the `Uninstall` registry keys.
#![cfg_attr(not(windows), allow(dead_code))]

use super::AppSource;

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

/// All visible programs from the three places Windows keeps them.
#[cfg(windows)]
pub fn list() -> Vec<Entry> {
    use winreg::enums::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
    };
    use winreg::RegKey;

    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    let scopes = [
        ("hklm", HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
        ("hklm32", HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
        ("hkcu", HKEY_CURRENT_USER, 0),
    ];
    let mut entries = Vec::new();
    for (scope, hive, view) in scopes {
        let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(UNINSTALL, KEY_READ | view)
        else {
            continue;
        };
        for key_name in root.enum_keys().flatten() {
            let Ok(key) = root.open_subkey_with_flags(&key_name, KEY_READ | view) else {
                continue;
            };
            let text = |name: &str| key.get_value::<String, _>(name).ok();
            let number = |name: &str| key.get_value::<u32, _>(name).ok();
            let raw = RawEntry {
                display_name: text("DisplayName"),
                publisher: text("Publisher"),
                display_version: text("DisplayVersion"),
                estimated_size: number("EstimatedSize"),
                install_date: text("InstallDate"),
                install_location: text("InstallLocation"),
                display_icon: text("DisplayIcon"),
                uninstall_string: text("UninstallString"),
                system_component: number("SystemComponent"),
                windows_installer: number("WindowsInstaller"),
                parent_key_name: text("ParentKeyName"),
                release_type: text("ReleaseType"),
                key_name,
            };
            entries.extend(classify(scope, raw));
        }
    }
    dedup(entries)
}

#[cfg(not(windows))]
pub fn list() -> Vec<Entry> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

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

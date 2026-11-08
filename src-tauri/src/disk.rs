//! Disk usage of the system drive (the one Windows is installed on), and the
//! other drives the quarantine can be kept on.

use serde::Serialize;
use sysinfo::Disks;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub mount_point: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Another fixed drive, offered in Settings as a place for the quarantine.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    /// `"D:"`.
    pub mount_point: String,
    /// The volume label chosen in Windows (e.g. "Data"); may be empty.
    pub label: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// The drive Windows is on, as `"C:"` (`"/"` elsewhere).
pub fn system_drive() -> String {
    system_mount()
}

/// Fixed drives with a letter, other than the system drive, most free space first.
/// USB sticks and card readers are left out: they come and go.
pub fn other_drives() -> Vec<DriveInfo> {
    let Ok(disks) = Disks::new_with_refreshed_list() else {
        return Vec::new();
    };
    let entries = (&disks).into_iter().filter(|d| !d.is_removable()).map(|d| {
        (
            d.mount_point().to_string_lossy().into_owned(),
            d.name().to_string_lossy().into_owned(),
            d.total_space(),
            d.available_space(),
        )
    });
    drive_choices(entries, &system_mount())
}

/// Keeps drive letters (`"D:\\"`) other than `system`, without duplicates, most free space first.
fn drive_choices(
    disks: impl IntoIterator<Item = (String, String, u64, u64)>,
    system: &str,
) -> Vec<DriveInfo> {
    let mut out: Vec<DriveInfo> = Vec::new();
    for (mount, label, total, free) in disks {
        let letter = mount.trim_end_matches(['\\', '/']);
        let b = letter.as_bytes();
        let is_letter = b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':';
        if !is_letter
            || letter.eq_ignore_ascii_case(system.trim_end_matches(['\\', '/']))
            || out
                .iter()
                .any(|d| d.mount_point.eq_ignore_ascii_case(letter))
        {
            continue;
        }
        out.push(DriveInfo {
            mount_point: letter.to_ascii_uppercase(),
            label,
            total_bytes: total,
            free_bytes: free,
        });
    }
    out.sort_by_key(|d| std::cmp::Reverse(d.free_bytes));
    out
}

/// The mount point to look for: `%SystemDrive%` (usually `C:`) on Windows, `/` elsewhere.
fn system_mount() -> String {
    if cfg!(windows) {
        std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())
    } else {
        "/".into()
    }
}

/// Picks the disk whose mount point matches `wanted` (case-insensitive, ignoring a trailing separator).
fn pick<'a>(
    disks: impl IntoIterator<Item = (&'a str, u64, u64)>,
    wanted: &str,
) -> Option<DiskInfo> {
    let norm = |s: &str| s.trim_end_matches(['\\', '/']).to_ascii_lowercase();
    let wanted = norm(wanted);
    disks
        .into_iter()
        .find(|(mount, _, _)| norm(mount) == wanted)
        .map(|(mount, total, free)| DiskInfo {
            mount_point: mount.trim_end_matches(['\\']).to_string(),
            total_bytes: total,
            free_bytes: free,
        })
}

#[tauri::command]
pub fn get_system_disk() -> Result<DiskInfo, String> {
    let disks = Disks::new_with_refreshed_list().map_err(|e| e.to_string())?;
    let entries: Vec<(String, u64, u64)> = (&disks)
        .into_iter()
        .map(|d| {
            (
                d.mount_point().to_string_lossy().into_owned(),
                d.total_space(),
                d.available_space(),
            )
        })
        .collect();
    let wanted = system_mount();
    pick(
        entries.iter().map(|(m, t, f)| (m.as_str(), *t, *f)),
        &wanted,
    )
    .ok_or_else(|| format!("system drive {wanted} not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_windows_drive_with_trailing_backslash() {
        let disks = [("D:\\", 10, 5), ("C:\\", 100, 40)];
        let got = pick(disks, "C:").unwrap();
        assert_eq!(
            got,
            DiskInfo {
                mount_point: "C:".into(),
                total_bytes: 100,
                free_bytes: 40
            }
        );
    }

    #[test]
    fn picks_root_on_unix() {
        let disks = [("/boot", 1, 1), ("/", 50, 20)];
        assert_eq!(pick(disks, "/").unwrap().total_bytes, 50);
    }

    #[test]
    fn offers_other_drive_letters_only() {
        let d = |m: &str, free| (m.to_string(), "Data".to_string(), 100, free);
        let got = drive_choices(
            [
                d("C:\\", 50),
                d("d:\\", 10),
                d("E:\\", 70),
                d("/boot", 90),
                d("D:\\", 10),
            ],
            "C:",
        );
        let letters: Vec<&str> = got.iter().map(|x| x.mount_point.as_str()).collect();
        assert_eq!(
            letters,
            ["E:", "D:"],
            "most free space first, no C:, no repeats"
        );
        assert!(drive_choices([d("/", 1), d("/home", 1)], "/").is_empty());
    }

    #[test]
    fn missing_drive_is_none() {
        assert!(pick([("D:\\", 1, 1)], "C:").is_none());
    }
}

//! Disk usage of the system drive (the one Windows is installed on).

use serde::Serialize;
use sysinfo::Disks;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub mount_point: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
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
    fn missing_drive_is_none() {
        assert!(pick([("D:\\", 1, 1)], "C:").is_none());
    }
}

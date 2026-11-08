//! The groups added with the "What is this?" explanations: old Recycle Bin items,
//! error reports and crash dumps, the thumbnail cache, and big app folders in
//! AppData and ProgramData. Read only, like the rest of the scan.

use super::{walk, FileItem, ScannedFile, Seen};
use crate::explain::KnownFolders;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Items deleted at least this long ago are offered for removal; newer ones may still be wanted.
pub(super) const RECYCLE_MIN_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// App folders smaller than this are not worth a look.
pub(super) const APP_FOLDER_MIN_BYTES: u64 = 300 * 1024 * 1024;
/// A `$I` file is a few hundred bytes; anything much bigger is not one.
const MAX_INFO_BYTES: u64 = 64 * 1024;

/// One Recycle Bin entry: the deleted file or folder (`$R…`) and its info file (`$I…`).
pub(super) struct BinEntry {
    pub(super) item: FileItem,
    pub(super) files: Vec<ScannedFile>,
}

/// Entries of the given Recycle Bin folders (`X:\$Recycle.Bin\<user SID>`) deleted
/// before `now - RECYCLE_MIN_AGE`.
pub(super) fn recycle_bin(
    bins: &[PathBuf],
    now: SystemTime,
    tick: &mut dyn FnMut(),
) -> Vec<BinEntry> {
    let mut out = Vec::new();
    for bin in bins {
        let Ok(entries) = fs::read_dir(bin) else {
            continue;
        };
        for entry in entries.flatten() {
            tick();
            let name = entry.file_name();
            let Some(suffix) = name.to_str().and_then(|n| n.strip_prefix("$I")) else {
                continue;
            };
            let info_path = entry.path();
            let Ok(info_meta) = fs::symlink_metadata(&info_path) else {
                continue;
            };
            if !info_meta.is_file() || info_meta.len() > MAX_INFO_BYTES {
                continue;
            }
            let Some((deleted, original)) =
                fs::read(&info_path).ok().and_then(|data| parse_info(&data))
            else {
                continue;
            };
            if now
                .duration_since(deleted)
                .map_or(true, |d| d < RECYCLE_MIN_AGE)
            {
                continue;
            }
            let data_path = bin.join(format!("$R{suffix}"));
            let Ok(data_meta) = fs::symlink_metadata(&data_path) else {
                continue; // Already gone: nothing to free.
            };
            let mut files = Vec::new();
            if data_meta.is_dir() {
                walk(&data_path, &|_| false, &mut |path, meta| {
                    tick();
                    files.push(ScannedFile::new(path, meta));
                });
            } else if data_meta.is_file() {
                files.push(ScannedFile::new(data_path.clone(), &data_meta));
            } else {
                continue;
            }
            let size = files.iter().map(|f| f.size).sum();
            // The info file goes with it, so the Recycle Bin does not list an empty entry.
            files.push(ScannedFile::new(info_path, &info_meta));
            out.push(BinEntry {
                item: FileItem {
                    path: data_path.to_string_lossy().into_owned(),
                    size_bytes: size,
                    // May be a folder: like other folder items, no date.
                    last_used: None,
                    duplicate_of: None,
                    explain_id: None,
                    original_path: Some(original),
                },
                files,
            });
        }
    }
    out
}

/// Reads a Recycle Bin `$I` file: when the item was deleted and where it was.
/// Version 1 (Vista to 8.1) has a fixed 260-character path; version 2 (Windows 10+)
/// stores the path length first.
pub(super) fn parse_info(data: &[u8]) -> Option<(SystemTime, String)> {
    let word = |at: usize| -> Option<u64> {
        Some(u64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?))
    };
    let name = match word(0)? {
        1 => data.get(24..)?,
        2 => data.get(28..)?,
        _ => return None,
    };
    let units: Vec<u16> = name
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    let original = String::from_utf16(&units).ok().filter(|s| !s.is_empty())?;
    // FILETIME: 100 ns steps since 1601-01-01.
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let secs = (word(16)? / 10_000_000).checked_sub(EPOCH_DIFF_SECS)?;
    Some((UNIX_EPOCH + Duration::from_secs(secs), original))
}

/// Every file in the error report and crash dump folders, except very recent ones.
pub(super) fn crash_reports(
    dirs: &[PathBuf],
    older_than: &dyn Fn(SystemTime) -> bool,
    tick: &mut dyn FnMut(),
) -> Vec<Seen> {
    let mut out = Vec::new();
    for dir in dirs {
        walk(dir, &|_| false, &mut |path, meta| {
            tick();
            let seen = Seen::from_meta(path, meta);
            if older_than(seen.modified) {
                out.push(seen);
            }
        });
    }
    out
}

/// The thumbnail cache files (`thumbcache_*.db`) Explorer rebuilds when needed.
pub(super) fn thumbnails(dir: Option<&Path>, tick: &mut dyn FnMut()) -> Vec<Seen> {
    let Some(Ok(entries)) = dir.map(fs::read_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        tick();
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if !(name.starts_with("thumbcache_") && name.ends_with(".db")) {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                out.push(Seen::from_meta(entry.path(), &meta));
            }
        }
    }
    out
}

/// Big folders right inside AppData and ProgramData, with what the app knows about them.
/// Only listed to be explained: nothing here is offered for removal as a group.
pub(super) fn app_folders(
    roots: &[PathBuf],
    known: &KnownFolders,
    tick: &mut dyn FnMut(),
) -> Vec<FileItem> {
    let mut out = Vec::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            // Links and junctions ("Application Data") point at folders measured elsewhere.
            if !entry
                .file_type()
                .is_ok_and(|t| t.is_dir() && !t.is_symlink())
            {
                continue;
            }
            let path = entry.path();
            let found = known.lookup(&path);
            if found.is_some_and(|f| f.hidden) {
                continue;
            }
            let mut size = 0;
            walk(&path, &|_| false, &mut |_, meta| {
                tick();
                size += meta.len();
            });
            if size >= APP_FOLDER_MIN_BYTES {
                out.push(FileItem {
                    path: path.to_string_lossy().into_owned(),
                    size_bytes: size,
                    last_used: None,
                    duplicate_of: None,
                    explain_id: found.map(|f| f.id.clone()),
                    original_path: None,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{scan, FileGroup, Locations, Safety, ScanStep};
    use super::*;
    use std::fs::{File, FileTimes};

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    /// A fresh, empty folder under the system temp dir, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "easy-uninstaller-extra-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        /// Writes `size` bytes at `rel`, changed `age_days` ago.
        fn file(&self, rel: &str, size: usize, age_days: u32) -> PathBuf {
            self.write(rel, &vec![7; size], age_days)
        }

        fn write(&self, rel: &str, data: &[u8], age_days: u32) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, data).unwrap();
            let when = SystemTime::now() - DAY * age_days;
            let times = FileTimes::new().set_accessed(when).set_modified(when);
            File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_times(times)
                .unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A `$I` file as Windows 10+ writes it (version 2).
    fn info_v2(size: u64, deleted: SystemTime, path: &str) -> Vec<u8> {
        let units: Vec<u16> = path.encode_utf16().chain([0]).collect();
        let mut data = Vec::new();
        data.extend(2u64.to_le_bytes());
        data.extend(size.to_le_bytes());
        data.extend(filetime(deleted).to_le_bytes());
        data.extend((units.len() as u32).to_le_bytes());
        data.extend(units.iter().flat_map(|u| u.to_le_bytes()));
        data
    }

    /// A `$I` file as Windows Vista to 8.1 writes it (version 1, fixed 260 characters).
    fn info_v1(size: u64, deleted: SystemTime, path: &str) -> Vec<u8> {
        let mut units: Vec<u16> = path.encode_utf16().collect();
        units.resize(260, 0);
        let mut data = Vec::new();
        data.extend(1u64.to_le_bytes());
        data.extend(size.to_le_bytes());
        data.extend(filetime(deleted).to_le_bytes());
        data.extend(units.iter().flat_map(|u| u.to_le_bytes()));
        data
    }

    fn filetime(t: SystemTime) -> u64 {
        (t.duration_since(UNIX_EPOCH).unwrap().as_secs() + 11_644_473_600) * 10_000_000
    }

    fn days_ago(days: u32) -> SystemTime {
        // Whole seconds, as a FILETIME round trip keeps them.
        let t = SystemTime::now() - DAY * days;
        UNIX_EPOCH + Duration::from_secs(t.duration_since(UNIX_EPOCH).unwrap().as_secs())
    }

    #[test]
    fn reads_both_info_file_versions() {
        let when = days_ago(40);
        let path = r"C:\Users\Ann\Pictures\Holiday photo.jpg";
        assert_eq!(
            parse_info(&info_v2(10, when, path)),
            Some((when, path.to_string()))
        );
        assert_eq!(
            parse_info(&info_v1(10, when, path)),
            Some((when, path.to_string()))
        );
        assert_eq!(parse_info(&[]), None);
        assert_eq!(parse_info(&info_v2(10, when, "")), None);
        let mut odd = info_v2(10, when, path);
        odd[0] = 9;
        assert_eq!(parse_info(&odd), None, "unknown version");
    }

    #[test]
    fn takes_only_old_recycle_bin_items() {
        let t = TempDir::new("bin");
        let bin = t.0.join("bin");
        let old = days_ago(45);
        t.write(
            "bin/$IAB12CD.jpg",
            &info_v2(3000, old, r"C:\Users\Ann\photo.jpg"),
            45,
        );
        t.file("bin/$RAB12CD.jpg", 3000, 45);
        // A deleted folder: every file inside it counts.
        t.write("bin/$IXY34.zip", &info_v1(0, old, r"D:\Projects\old"), 45);
        t.file("bin/$RXY34.zip/a.txt", 100, 45);
        t.file("bin/$RXY34.zip/sub/b.txt", 200, 45);
        // Deleted last week: may still be wanted.
        t.write(
            "bin/$INEW.doc",
            &info_v2(50, days_ago(7), r"C:\x\new.doc"),
            7,
        );
        t.file("bin/$RNEW.doc", 50, 7);
        // Info without its data: nothing to free.
        t.write("bin/$IGONE.txt", &info_v2(9, old, r"C:\x\gone.txt"), 45);
        t.file("bin/desktop.ini", 10, 45);

        let mut entries = recycle_bin(std::slice::from_ref(&bin), SystemTime::now(), &mut || {});
        entries.sort_by(|a, b| a.item.path.cmp(&b.item.path));
        assert_eq!(entries.len(), 2);
        let photo = &entries[0].item;
        assert!(photo.path.ends_with("$RAB12CD.jpg"));
        assert_eq!(
            photo.original_path.as_deref(),
            Some(r"C:\Users\Ann\photo.jpg")
        );
        assert_eq!(photo.size_bytes, 3000);
        assert_eq!(photo.last_used, None);
        assert_eq!(entries[0].files.len(), 2, "data and info file");
        let folder = &entries[1];
        assert_eq!(folder.item.size_bytes, 300);
        assert_eq!(folder.files.len(), 3);
        assert!(folder.files.iter().any(|f| f.path.ends_with("$IXY34.zip")));
    }

    #[test]
    fn new_groups_in_the_scan() {
        let t = TempDir::new("scan");
        let old = days_ago(60);
        t.write("bin/$I1.txt", &info_v2(500, old, r"C:\a.txt"), 60);
        t.file("bin/$R1.txt", 500, 60);
        t.file("local/CrashDumps/app.exe.1234.dmp", 4000, 3);
        t.file("local/CrashDumps/fresh.dmp", 4000, 0); // may still be written
        t.file("local/WER/ReportArchive/AppCrash_x/Report.wer", 100, 10);
        t.file(
            "local/Microsoft/Windows/Explorer/thumbcache_256.db",
            7000,
            0,
        );
        t.file("local/Microsoft/Windows/Explorer/iconcache_32.db", 7000, 0); // icons: left alone
        t.file("local/BigApp/data.bin", APP_FOLDER_MIN_BYTES as usize, 10);
        t.file("local/SmallApp/data.bin", 1000, 10);
        t.file("local/Temp/huge.bin", APP_FOLDER_MIN_BYTES as usize, 10); // hidden: temp

        let local = t.0.join("local");
        let known = KnownFolders::new(|name| (name == "LOCALAPPDATA").then(|| local.clone()));
        let loc = Locations {
            recycle_bins: vec![t.0.join("bin")],
            crash_reports: vec![local.join("CrashDumps"), local.join("WER")],
            thumbnails: Some(local.join("Microsoft/Windows/Explorer")),
            app_data: vec![local.clone()],
            known,
            ..Default::default()
        };
        let mut steps = Vec::new();
        let result = scan(&loc, &[], SystemTime::now(), &mut |p| steps.push(p.step));
        let find = |id: &str| -> &FileGroup { result.groups.iter().find(|g| g.id == id).unwrap() };

        let bin = find("recycle-bin");
        assert_eq!(
            (bin.safety, bin.item_count, bin.size_bytes),
            (Safety::Safe, 1, 500)
        );
        assert_eq!(bin.title_key, "files.groups.recycleBin.title");
        assert_eq!(
            bin.items[0].explain_id, None,
            "no LOCALAPPDATA pattern matches the test bin"
        );

        let crash = find("crash-reports");
        assert_eq!((crash.item_count, crash.size_bytes), (2, 4100));
        assert_eq!(crash.items[0].explain_id.as_deref(), Some("crashDumps"));

        let thumbs = find("thumbnails");
        assert_eq!(thumbs.item_count, 1);
        assert_eq!(thumbs.items[0].explain_id.as_deref(), Some("thumbCache"));

        let unknown = find("unknown-folders");
        assert!(unknown.review_only);
        assert_eq!(unknown.safety, Safety::Check);
        assert_eq!(
            unknown.item_count, 1,
            "small and hidden folders are left out"
        );
        assert!(unknown.items[0].path.ends_with("BigApp"));
        assert_eq!(unknown.items[0].explain_id, None);
        assert!(
            !result.files.contains_key("unknown-folders"),
            "review only: nothing to move"
        );
        assert!(result
            .groups
            .iter()
            .filter(|g| g.id != "unknown-folders")
            .all(|g| !g.review_only));
        assert_eq!(result.files["recycle-bin"].len(), 2);
        assert_eq!(steps.last(), Some(&ScanStep::AppFolders));
    }
}

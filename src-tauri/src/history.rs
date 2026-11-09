//! History: what Easy Uninstaller did on this PC, and how much space it gave back.
//!
//! Every action that removes something (Quick clean, files, leftovers, a folder,
//! an uninstalled app) adds one *record*, saved in `history.json` in the app's data
//! folder. The history never leaves the PC and only describes actions: it holds no
//! file lists, only names (a file group, an app) and sizes.
//!
//! "Space freed" is honest: files moved into a quarantine on the same drive free their
//! space only when the quarantine deletes them for good, so until then they are counted
//! as *pending*. Entries put back with Restore are not counted at all. A record keeps the
//! quarantine entries it made, and their state is read from the quarantine each time
//! the history is shown, so expiry and "Delete now" need no bookkeeping here.

use crate::quarantine::{now_secs, MoveReport, Quarantine, QuarantineEntry, Title};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Manager};

const FILE: &str = "history.json";
/// Older records are folded into the totals, so the file stays small.
const MAX_RECORDS: usize = 1000;

/// What an action was. Each one has its own line of text in the UI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// Quick clean on Home.
    QuickClean,
    /// Whole file groups or files picked one by one, on the Files screen.
    Files,
    /// Leftovers of uninstalled apps, on the Leftovers screen or right after an uninstall.
    Leftovers,
    /// One folder of the "unknown folders" group.
    AppFolder,
    /// An app removed by its own uninstaller (or a Store app).
    Uninstall,
}

/// A quarantine entry made by an action, with its size when it was made.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EntrySize {
    pub id: String,
    pub bytes: u64,
}

/// One action, as saved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    /// Unix seconds.
    pub at: u64,
    pub kind: Kind,
    /// What it was about: file groups (i18n keys) or app and folder names.
    pub names: Vec<Title>,
    pub item_count: u64,
    /// Deleted right away (temporary files and caches).
    #[serde(default)]
    pub deleted_bytes: u64,
    /// Moved to a quarantine on another drive: freed on the original drive right away.
    #[serde(default)]
    pub crossed_bytes: u64,
    /// The quarantine entries it made.
    #[serde(default)]
    pub entries: Vec<EntrySize>,
    /// For an uninstalled app: the size Windows reported for it, an estimate.
    #[serde(default)]
    pub estimated_bytes: u64,
    /// Entries of this record put back with Restore.
    #[serde(default)]
    pub restored: Vec<String>,
    #[serde(default)]
    pub restored_at: Option<u64>,
}

/// What is saved in [`FILE`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
struct Stored {
    version: u32,
    /// Oldest first.
    records: Vec<Record>,
    /// Records dropped to keep the file small, still part of the totals.
    older_count: u64,
    older_freed_bytes: u64,
    /// When the first action was recorded (kept when old records are dropped).
    since: Option<u64>,
}

/// One action, as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    pub at: u64,
    pub kind: Kind,
    pub names: Vec<Title>,
    pub item_count: u64,
    /// Space given back by this action so far.
    pub freed_bytes: u64,
    /// Still in quarantine on the same drive: freed when deleted for good.
    pub pending_bytes: u64,
    /// When the last of that is deleted for good (unix seconds).
    pub pending_until: Option<u64>,
    /// Put back with Restore.
    pub restored_bytes: u64,
    pub restored_at: Option<u64>,
    /// The size is Windows' estimate (uninstalled apps).
    pub estimated: bool,
}

/// The whole history, as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummary {
    /// Space given back since the first action.
    pub freed_bytes: u64,
    /// In quarantine on the same drive: freed when deleted for good.
    pub pending_bytes: u64,
    /// When the first action was recorded (unix seconds).
    pub since: Option<u64>,
    pub action_count: u64,
    /// Newest first.
    pub items: Vec<HistoryItem>,
}

/// A quarantine entry as the history needs it: its size now and when it expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Live {
    pub bytes: u64,
    pub expires_at: u64,
}

impl Record {
    /// A record for a move into quarantine and/or a deletion. `sizes` gives the size of
    /// each entry the move made. `None` when nothing was removed.
    pub fn moved(
        kind: Kind,
        names: Vec<Title>,
        report: &MoveReport,
        sizes: &HashMap<String, Live>,
        now: u64,
    ) -> Option<Record> {
        let item_count = report.moved_count + report.deleted_count;
        if item_count == 0 {
            return None;
        }
        let entries = report
            .entry_ids
            .iter()
            .map(|id| EntrySize {
                id: id.clone(),
                bytes: sizes.get(id).map_or(0, |l| l.bytes),
            })
            .collect();
        Some(Record {
            id: String::new(),
            at: now,
            kind,
            names,
            item_count,
            deleted_bytes: report.deleted_bytes,
            crossed_bytes: report.freed_bytes.saturating_sub(report.deleted_bytes),
            entries,
            estimated_bytes: 0,
            restored: Vec::new(),
            restored_at: None,
        })
    }

    /// A record for an uninstalled app. `size` is what Windows reported for it.
    pub fn uninstalled(name: String, size: u64, now: u64) -> Record {
        Record {
            id: String::new(),
            at: now,
            kind: Kind::Uninstall,
            names: vec![Title::Text(name)],
            item_count: 1,
            deleted_bytes: 0,
            crossed_bytes: 0,
            entries: Vec::new(),
            estimated_bytes: size,
            restored: Vec::new(),
            restored_at: None,
        }
    }

    /// Where this record stands, given the quarantine entries that exist now.
    pub fn item(&self, live: &HashMap<String, Live>) -> HistoryItem {
        let mut in_quarantine = 0;
        let mut restored = 0;
        let mut ended = 0;
        let mut until: Option<u64> = None;
        for e in &self.entries {
            if let Some(l) = live.get(&e.id) {
                // A partial restore leaves a smaller entry behind.
                in_quarantine += e.bytes.min(l.bytes);
                until = Some(until.map_or(l.expires_at, |u| u.max(l.expires_at)));
            } else if self.restored.contains(&e.id) {
                restored += e.bytes;
            } else {
                ended += e.bytes;
            }
        }
        // Moved to another drive: already freed where it was, even while in quarantine.
        let crossed = self.crossed_bytes.min(in_quarantine);
        let pending = in_quarantine - crossed;
        HistoryItem {
            id: self.id.clone(),
            at: self.at,
            kind: self.kind,
            names: self.names.clone(),
            item_count: self.item_count,
            freed_bytes: self.deleted_bytes + self.estimated_bytes + ended + crossed,
            pending_bytes: pending,
            pending_until: if pending > 0 { until } else { None },
            restored_bytes: restored,
            restored_at: self.restored_at,
            estimated: self.estimated_bytes > 0,
        }
    }
}

/// The saved history. All changes go through one lock.
pub struct History {
    path: PathBuf,
    lock: Mutex<Stored>,
    /// [`MAX_RECORDS`], smaller in tests.
    max: usize,
}

impl History {
    /// Reads the history saved at `path`; a missing or unreadable file starts a new one.
    pub fn new(path: PathBuf) -> Self {
        let stored = fs::read(&path)
            .ok()
            .and_then(|text| serde_json::from_slice::<Stored>(&text).ok())
            .unwrap_or_default();
        History {
            path,
            lock: Mutex::new(stored),
            max: MAX_RECORDS,
        }
    }

    fn guard(&self) -> MutexGuard<'_, Stored> {
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Writes to a temporary file first, so a crash never leaves half a file.
    fn save(&self, stored: &Stored) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_vec(stored).map_err(io::Error::other)?;
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, &self.path)
    }

    /// Adds a record. When there are too many, the oldest are folded into the totals.
    pub fn add(&self, mut record: Record) -> io::Result<()> {
        let mut stored = self.guard();
        stored.version = 1;
        let n = stored.older_count + stored.records.len() as u64;
        record.id = format!("{}-{n}", record.at);
        stored.since = Some(stored.since.map_or(record.at, |s| s.min(record.at)));
        stored.records.push(record);
        if stored.records.len() > self.max {
            let extra = stored.records.len() - self.max;
            let dropped: Vec<Record> = stored.records.drain(..extra).collect();
            for r in dropped {
                // So old that its quarantine entries are long gone: count them as freed.
                stored.older_freed_bytes += r.item(&HashMap::new()).freed_bytes;
                stored.older_count += 1;
            }
        }
        self.save(&stored)
    }

    /// Notes that these quarantine entries were put back, so they no longer count as freed.
    pub fn mark_restored(&self, ids: &[String], now: u64) -> io::Result<()> {
        let mut stored = self.guard();
        let mut changed = false;
        for r in &mut stored.records {
            for id in ids {
                if r.entries.iter().any(|e| &e.id == id) && !r.restored.contains(id) {
                    r.restored.push(id.clone());
                    r.restored_at = Some(now);
                    changed = true;
                }
            }
        }
        if changed {
            self.save(&stored)?;
        }
        Ok(())
    }

    /// The whole history, given the quarantine entries that exist now.
    pub fn summary(&self, live: &HashMap<String, Live>) -> HistorySummary {
        let stored = self.guard();
        let items: Vec<HistoryItem> = stored.records.iter().rev().map(|r| r.item(live)).collect();
        HistorySummary {
            freed_bytes: stored.older_freed_bytes
                + items.iter().map(|i| i.freed_bytes).sum::<u64>(),
            pending_bytes: items.iter().map(|i| i.pending_bytes).sum(),
            since: stored.since,
            action_count: stored.older_count + items.len() as u64,
            items,
        }
    }

    /// Forgets every action; the total starts again from zero. No file is touched.
    pub fn clear(&self) -> io::Result<()> {
        let mut stored = self.guard();
        *stored = Stored::default();
        match fs::remove_file(&self.path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// The quarantine entries that exist now, by id.
pub fn live_entries(entries: &[QuarantineEntry]) -> HashMap<String, Live> {
    entries
        .iter()
        .map(|e| {
            (
                e.id.clone(),
                Live {
                    bytes: e.size_bytes,
                    expires_at: e.expires_at,
                },
            )
        })
        .collect()
}

pub fn init(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let path = app.path().app_local_data_dir()?.join(FILE);
    app.manage(History::new(path));
    Ok(())
}

/// Records a move into quarantine or a deletion. The history is a convenience: a
/// failure to save it never fails the action itself.
pub fn record_move(app: &AppHandle, kind: Kind, names: Vec<Title>, report: &MoveReport) {
    let live = if report.entry_ids.is_empty() {
        HashMap::new()
    } else {
        live_entries(&app.state::<Quarantine>().list())
    };
    if let Some(record) = Record::moved(kind, names, report, &live, now_secs()) {
        let _ = app.state::<History>().add(record);
    }
}

/// Records an app that is gone from the app list.
pub fn record_uninstall(app: &AppHandle, name: String, size: u64) {
    let _ = app
        .state::<History>()
        .add(Record::uninstalled(name, size, now_secs()));
}

/// Notes the quarantine entries put back; see [`History::mark_restored`].
pub fn record_restored(app: &AppHandle, ids: &[String]) {
    if !ids.is_empty() {
        let _ = app.state::<History>().mark_restored(ids, now_secs());
    }
}

#[tauri::command]
pub async fn get_history(app: AppHandle) -> Result<HistorySummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let quarantine = app.state::<Quarantine>();
        // Expired entries are deleted first, so their space counts as freed.
        quarantine.purge_expired(now_secs(), false);
        let live = live_entries(&quarantine.list());
        app.state::<History>().summary(&live)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_history(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<History>().clear().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;
    const DAY: u64 = 86_400;

    fn path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("easy-uninstaller-h-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join(FILE)
    }

    fn report(deleted: u64, entries: &[&str], moved: u64, freed: u64) -> MoveReport {
        MoveReport {
            entry_ids: entries.iter().map(|s| s.to_string()).collect(),
            moved_count: entries.len() as u64,
            moved_bytes: moved,
            deleted_count: u64::from(deleted > 0),
            deleted_bytes: deleted,
            freed_bytes: freed,
            ..MoveReport::default()
        }
    }

    fn live(items: &[(&str, u64)]) -> HashMap<String, Live> {
        items
            .iter()
            .map(|(id, bytes)| {
                (
                    id.to_string(),
                    Live {
                        bytes: *bytes,
                        expires_at: NOW + 7 * DAY,
                    },
                )
            })
            .collect()
    }

    fn key(k: &str) -> Vec<Title> {
        vec![Title::Key(k.into())]
    }

    #[test]
    fn nothing_removed_is_not_recorded() {
        let r = Record::moved(
            Kind::Files,
            key("x"),
            &MoveReport::default(),
            &live(&[]),
            NOW,
        );
        assert!(r.is_none());
    }

    #[test]
    fn same_drive_quarantine_is_pending_until_it_ends() {
        let sizes = live(&[("e1", 300)]);
        let r = Record::moved(
            Kind::QuickClean,
            vec![],
            &report(100, &["e1"], 300, 100),
            &sizes,
            NOW,
        )
        .unwrap();
        assert_eq!(
            r.entries,
            [EntrySize {
                id: "e1".into(),
                bytes: 300
            }]
        );

        let now = r.item(&sizes);
        assert_eq!((now.freed_bytes, now.pending_bytes), (100, 300));
        assert_eq!(now.pending_until, Some(NOW + 7 * DAY));

        // Expired or deleted for good: all of it is freed.
        let later = r.item(&live(&[]));
        assert_eq!((later.freed_bytes, later.pending_bytes), (400, 0));
        assert_eq!(later.pending_until, None);
    }

    #[test]
    fn another_drive_is_freed_at_once_and_not_twice() {
        let sizes = live(&[("e1", 300)]);
        let r = Record::moved(
            Kind::Files,
            key("g"),
            &report(0, &["e1"], 300, 300),
            &sizes,
            NOW,
        )
        .unwrap();
        let now = r.item(&sizes);
        assert_eq!((now.freed_bytes, now.pending_bytes), (300, 0));
        let later = r.item(&live(&[]));
        assert_eq!((later.freed_bytes, later.pending_bytes), (300, 0));
    }

    #[test]
    fn restored_entries_do_not_count() {
        let h = History::new(path("restore"));
        let sizes = live(&[("e1", 300), ("e2", 50)]);
        let r = Record::moved(
            Kind::QuickClean,
            vec![],
            &report(100, &["e1", "e2"], 350, 100),
            &sizes,
            NOW,
        )
        .unwrap();
        h.add(r).unwrap();
        h.mark_restored(&["e1".into()], NOW + 60).unwrap();

        // e1 is back where it was, e2 expired since.
        let s = h.summary(&live(&[]));
        let item = &s.items[0];
        assert_eq!(
            (item.freed_bytes, item.pending_bytes, item.restored_bytes),
            (150, 0, 300)
        );
        assert_eq!(item.restored_at, Some(NOW + 60));
        assert_eq!(s.freed_bytes, 150);
        let _ = fs::remove_dir_all(h.path.parent().unwrap());
    }

    #[test]
    fn saves_counts_and_lists_newest_first() {
        let p = path("save");
        let h = History::new(p.clone());
        h.add(Record::uninstalled("Discord".into(), 1000, NOW))
            .unwrap();
        let r = Record::moved(
            Kind::Leftovers,
            vec![Title::Text("Discord".into())],
            &report(0, &["e1"], 40, 0),
            &live(&[("e1", 40)]),
            NOW + 5,
        )
        .unwrap();
        h.add(r).unwrap();

        let again = History::new(p.clone());
        let s = again.summary(&live(&[("e1", 40)]));
        assert_eq!(s.action_count, 2);
        assert_eq!(s.since, Some(NOW));
        assert_eq!((s.freed_bytes, s.pending_bytes), (1000, 40));
        assert_eq!(s.items[0].kind, Kind::Leftovers);
        assert!(s.items[1].estimated);
        assert_ne!(s.items[0].id, s.items[1].id);

        again.clear().unwrap();
        assert!(!p.exists());
        let s = again.summary(&live(&[]));
        assert_eq!((s.action_count, s.freed_bytes, s.since), (0, 0, None));
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn old_records_are_folded_into_the_totals() {
        const MAX: usize = 20;
        let p = path("trim");
        let mut h = History::new(p.clone());
        h.max = MAX;
        for i in 0..(MAX as u64 + 5) {
            h.add(Record::uninstalled(format!("App {i}"), 10, NOW + i))
                .unwrap();
        }
        let s = h.summary(&live(&[]));
        assert_eq!(s.items.len(), MAX);
        assert_eq!(s.action_count, MAX as u64 + 5);
        assert_eq!(s.freed_bytes, (MAX as u64 + 5) * 10);
        assert_eq!(s.since, Some(NOW));
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn serializes_for_the_ui() {
        let item = Record::uninstalled("Discord".into(), 5, NOW).item(&live(&[]));
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["kind"], "uninstall");
        assert_eq!(json["names"][0]["text"], "Discord");
        assert_eq!(json["freedBytes"], 5);
        assert_eq!(json["pendingUntil"], serde_json::Value::Null);
    }
}

//! What the AI is told about a folder, and how its answer is read.
//!
//! Only names and numbers leave the PC: the folder's path (with the user's own
//! part replaced by `%LOCALAPPDATA%` and the like), its size, file count, last
//! change, and some subfolder and file names. Never what is inside a file.
//!
//! The answer must be one JSON object with fixed fields. Anything else in it is
//! dropped, texts are cut to size, and an answer that does not fit is refused.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// At most this many subfolder names and file names are sent.
const MAX_SUBFOLDERS: usize = 12;
const MAX_NAMES: usize = 20;
/// The walk that counts files stops after this many entries.
const MAX_WALK: usize = 200_000;
/// A name longer than this is cut.
const MAX_NAME_CHARS: usize = 60;

/// What is sent about one folder. The UI shows it before sending.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FolderSummary {
    /// `%LOCALAPPDATA%\Name`: never the user's own folder path.
    pub path: String,
    pub size_bytes: u64,
    pub file_count: u64,
    /// The newest change inside, in seconds since 1970.
    pub modified: Option<u64>,
    pub subfolders: Vec<String>,
    pub names: Vec<String>,
}

/// `path`'s root as the variable that names it: `C:\Users\me\AppData\Local\X`
/// -> `%LOCALAPPDATA%\X`. `roots` are `(variable, folder)`.
pub fn shown_path(path: &Path, roots: &[(&str, &Path)]) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    let parent = path.parent()?;
    roots
        .iter()
        .find(|(_, root)| *root == parent)
        .map(|(var, _)| format!("%{var}%\\{name}"))
}

/// A file or folder name as sent: the user name (when it shows up in a name)
/// replaced, control characters dropped, long names cut.
fn clean_name(name: &str, user: &str) -> String {
    let mut n: String = name.chars().filter(|c| !c.is_control()).collect();
    if user.chars().count() >= 3 {
        let lower = n.to_lowercase();
        let u = user.to_lowercase();
        if let Some(i) = lower.find(&u) {
            if lower.len() == n.len() {
                n.replace_range(i..i + u.len(), "%USERNAME%");
            }
        }
    }
    if n.chars().count() > MAX_NAME_CHARS {
        n = n.chars().take(MAX_NAME_CHARS).collect::<String>() + "…";
    }
    n
}

/// Looks at `folder` (names, sizes, dates only) for the summary.
pub fn summarize(folder: &Path, shown: String, user: &str) -> FolderSummary {
    let mut summary = FolderSummary {
        path: shown,
        size_bytes: 0,
        file_count: 0,
        modified: None,
        subfolders: Vec::new(),
        names: Vec::new(),
    };
    // Breadth first, so the names sent come from the top levels.
    let mut queue = std::collections::VecDeque::from([(folder.to_path_buf(), 0usize)]);
    let mut seen = 0usize;
    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            seen += 1;
            if seen > MAX_WALK {
                return summary;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = clean_name(&entry.file_name().to_string_lossy(), user);
            if kind.is_dir() && !kind.is_symlink() {
                if depth == 0 && summary.subfolders.len() < MAX_SUBFOLDERS {
                    summary.subfolders.push(name);
                }
                queue.push_back((entry.path(), depth + 1));
            } else if let Ok(meta) = entry.metadata() {
                if !meta.is_file() {
                    continue;
                }
                summary.file_count += 1;
                summary.size_bytes += meta.len();
                let secs = meta
                    .modified()
                    .ok()
                    .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());
                if let Some(s) = secs {
                    summary.modified = Some(summary.modified.map_or(s, |m| m.max(s)));
                }
                if summary.names.len() < MAX_NAMES && !summary.names.contains(&name) {
                    summary.names.push(name);
                }
            }
        }
    }
    summary
}

/// `YYYY-MM` of a time in seconds since 1970 (UTC).
pub fn year_month(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}")
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn size_text(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= 1024.0 * MB {
        format!("{:.1} GB", bytes as f64 / (1024.0 * MB))
    } else {
        format!("{:.0} MB", (bytes as f64 / MB).max(1.0))
    }
}

/// The language the answer is written in.
pub fn language_name(lang: &str) -> &'static str {
    match lang {
        "it" => "Italian",
        _ => "English",
    }
}

/// The whole text sent for one folder.
pub fn question(s: &FolderSummary, lang: &str) -> String {
    let mut facts = format!(
        "Folder: {}\nSize: {}, {} files\n",
        s.path,
        size_text(s.size_bytes),
        s.file_count
    );
    if let Some(m) = s.modified {
        facts.push_str(&format!("Last modified: {}\n", year_month(m)));
    }
    if !s.subfolders.is_empty() {
        facts.push_str(&format!("Subfolders: {}\n", s.subfolders.join(", ")));
    }
    if !s.names.is_empty() {
        facts.push_str(&format!(
            "Some file names ({} of {}): {}\n",
            s.names.len(),
            s.file_count,
            s.names.join(", ")
        ));
    }
    let language = language_name(lang);
    format!(
        "You help a person who is not technical free up disk space on Windows. \
Below is what is known about one folder in AppData or ProgramData. You cannot see what is inside the files.

{facts}
Reply with one JSON object and nothing else:
{{\"what\": \"...\", \"madeBy\": \"...\", \"verdict\": \"removable\", \"confidence\": \"medium\", \"reason\": \"...\", \"label\": \"...\"}}

- what: one or two plain sentences on what this folder most likely is.
- madeBy: the app or company that most likely made it, or \"\" if you are not sure.
- verdict: \"removable\" only if deleting it loses nothing that matters (a cache, temporary files, leftovers of an app that is gone); \"keep\" if it may hold settings, saved games, documents or data of an app in use; \"unknown\" if you cannot tell.
- confidence: \"low\", \"medium\" or \"high\".
- reason: one short sentence explaining the verdict.
- label: two to four words naming the folder, for a small tag.

Write what, reason and label in {language}, in simple words. If you are not sure, say \"unknown\": do not guess."
    )
}

/// The short question used by the "Test" button.
pub const TEST_QUESTION: &str =
    "Reply with this JSON object and nothing else: {\"verdict\": \"unknown\", \"ok\": true}";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AiVerdict {
    Removable,
    Keep,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// The AI's answer, as checked here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub what: String,
    pub made_by: String,
    pub verdict: AiVerdict,
    pub confidence: Confidence,
    pub reason: String,
    pub label: String,
}

/// Text from the AI: one line, no control characters, at most `max` characters.
fn tidy(text: &str, max: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max {
        flat.chars().take(max).collect::<String>() + "…"
    } else {
        flat
    }
}

/// Removes `<think>…</think>` blocks some local models write first.
fn without_thinking(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Every JSON object in `text`, in order (a model may wrap it in prose or ```).
fn objects(text: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices('{') {
        let mut stream =
            serde_json::Deserializer::from_str(&text[i..]).into_iter::<serde_json::Value>();
        if let Some(Ok(v)) = stream.next() {
            if v.is_object() {
                out.push(v);
            }
        }
    }
    out
}

/// The last JSON object in `text` that has a `verdict`.
pub fn answer_object(text: &str) -> Option<serde_json::Value> {
    objects(&without_thinking(text))
        .into_iter()
        .rev()
        .find(|v| v.get("verdict").is_some())
}

/// Reads the AI's answer. `None` when it is not the JSON asked for.
pub fn parse_reply(text: &str) -> Option<Reply> {
    let v = answer_object(text)?;
    let s = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let verdict = match s("verdict").to_ascii_lowercase().as_str() {
        "removable" => AiVerdict::Removable,
        "keep" => AiVerdict::Keep,
        "unknown" => AiVerdict::Unknown,
        _ => return None,
    };
    let confidence = match s("confidence").to_ascii_lowercase().as_str() {
        "high" => Confidence::High,
        "medium" => Confidence::Medium,
        _ => Confidence::Low,
    };
    let what = tidy(&s("what"), 600);
    if what.is_empty() {
        return None;
    }
    Some(Reply {
        what,
        made_by: tidy(&s("madeBy"), 80),
        verdict,
        confidence,
        reason: tidy(&s("reason"), 300),
        label: tidy(&s("label"), 40),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_users_folder_never_shows() {
        let local = PathBuf::from("/home/u/AppData/Local");
        let roaming = PathBuf::from("/home/u/AppData/Roaming");
        let roots = [
            ("LOCALAPPDATA", local.as_path()),
            ("APPDATA", roaming.as_path()),
        ];
        assert_eq!(
            shown_path(&local.join("ABX_7731"), &roots).as_deref(),
            Some("%LOCALAPPDATA%\\ABX_7731")
        );
        assert_eq!(
            shown_path(&roaming.join("Zoom"), &roots).as_deref(),
            Some("%APPDATA%\\Zoom")
        );
        assert_eq!(shown_path(&local.join("a/b"), &roots), None);
        assert_eq!(
            clean_name("backup-Mario.zip", "mario"),
            "backup-%USERNAME%.zip"
        );
        assert_eq!(clean_name("a\u{7}b", "mario"), "ab");
        assert_eq!(clean_name(&"x".repeat(80), "mario").chars().count(), 61);
    }

    #[test]
    fn a_folder_is_summed_up_by_names_and_numbers() {
        let dir =
            std::env::temp_dir().join(format!("easy-uninstaller-ai-sum-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Cache/deep")).unwrap();
        fs::create_dir_all(dir.join("VideoCache")).unwrap();
        fs::write(dir.join("config.ini"), b"secret=1").unwrap();
        fs::write(dir.join("Cache/data_0001.blk"), [0u8; 100]).unwrap();
        fs::write(dir.join("Cache/deep/x.pak"), [0u8; 50]).unwrap();
        let s = summarize(&dir, "%LOCALAPPDATA%\\T".into(), "me");
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(s.file_count, 3);
        assert_eq!(s.size_bytes, 158);
        assert_eq!(s.subfolders, ["Cache", "VideoCache"]);
        assert_eq!(s.names, ["config.ini", "data_0001.blk", "x.pak"]);
        assert!(s.modified.is_some());

        let q = question(&s, "it");
        assert!(q.contains("Folder: %LOCALAPPDATA%\\T"));
        assert!(q.contains("Subfolders: Cache, VideoCache"));
        assert!(q.contains("Some file names (3 of 3): config.ini, data_0001.blk, x.pak"));
        assert!(q.contains("in Italian"));
        assert!(!q.contains("secret"), "file contents are never read");
    }

    #[test]
    fn dates_are_months() {
        assert_eq!(year_month(0), "1970-01");
        assert_eq!(year_month(1_756_684_800), "2025-09"); // 2025-09-01
        assert_eq!(year_month(1_709_164_800), "2024-02"); // 2024-02-29
    }

    #[test]
    fn only_the_fixed_json_is_taken() {
        let text = "Sure! ```json\n{\"what\": \"Cache of a\\ngame.\", \"madeBy\": \"HoYoverse\", \"verdict\": \"Removable\", \"confidence\": \"medium\", \"reason\": \"Only a cache.\", \"label\": \"Game cache\", \"delete\": true}\n```";
        let r = parse_reply(text).unwrap();
        assert_eq!(r.what, "Cache of a game.");
        assert_eq!(r.made_by, "HoYoverse");
        assert_eq!(r.verdict, AiVerdict::Removable);
        assert_eq!(r.confidence, Confidence::Medium);
        assert_eq!(r.label, "Game cache");

        let thinking = "<think>maybe {\"verdict\": \"removable\", \"what\": \"x\"}</think>{\"what\": \"Settings\", \"verdict\": \"keep\", \"confidence\": \"sure\"}";
        let r = parse_reply(thinking).unwrap();
        assert_eq!(r.verdict, AiVerdict::Keep);
        assert_eq!(
            r.confidence,
            Confidence::Low,
            "an unknown confidence is low"
        );

        assert_eq!(parse_reply("I think it is a cache."), None);
        assert_eq!(
            parse_reply("{\"what\": \"x\", \"verdict\": \"delete\"}"),
            None
        );
        assert_eq!(parse_reply("{\"what\": \"\", \"verdict\": \"keep\"}"), None);
        let long = format!(
            "{{\"what\": \"{}\", \"verdict\": \"keep\"}}",
            "a".repeat(2000)
        );
        assert_eq!(parse_reply(&long).unwrap().what.chars().count(), 601);
    }
}

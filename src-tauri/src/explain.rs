//! "What is this?": explanations for common Windows and app folders, without AI.
//!
//! The list lives in `known_folders.json` (a few KB, compiled into the app). It only
//! says which folder is which and how safe it is to remove; the texts the user reads
//! are i18n keys (`explain.folders.<id>.*` in `src/lib/i18n`), so the UI translates them.
//! Nothing is sent anywhere: matching is a plain path comparison.

use serde::Deserialize;
use std::path::{Path, PathBuf};

const KNOWN_FOLDERS: &str = include_str!("known_folders.json");

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Apps or Windows recreate it: it can go.
    Safe,
    /// It may hold something the user wants: look first.
    Check,
    /// Better keep it: removing it breaks an app or loses data.
    Keep,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownFolder {
    pub id: String,
    /// The UI reads it from the same JSON; parsing it here makes a typo fail the tests.
    #[allow(dead_code)]
    pub verdict: Verdict,
    /// Never listed among the unknown folders: Windows itself, or already in another group.
    #[serde(default)]
    pub hidden: bool,
    /// Patterns like `%LOCALAPPDATA%\NVIDIA`; `*` stands for any one folder name.
    paths: Vec<String>,
}

#[derive(Deserialize)]
struct KnownFile {
    folders: Vec<KnownFolder>,
}

/// The known folders, with their path patterns expanded for this PC.
#[derive(Debug, Default)]
pub struct KnownFolders {
    folders: Vec<KnownFolder>,
    /// (lowercase path parts, index in `folders`).
    patterns: Vec<(Vec<String>, usize)>,
}

impl KnownFolders {
    /// `var` gives the value of an environment variable such as `LOCALAPPDATA`.
    /// Patterns whose variable is missing are left out.
    pub fn new(var: impl Fn(&str) -> Option<PathBuf>) -> Self {
        let file: KnownFile =
            serde_json::from_str(KNOWN_FOLDERS).expect("known_folders.json is valid");
        let mut patterns = Vec::new();
        for (index, folder) in file.folders.iter().enumerate() {
            for pattern in &folder.paths {
                if let Some(expanded) = expand(pattern, &var) {
                    patterns.push((parts(&expanded), index));
                }
            }
        }
        KnownFolders {
            folders: file.folders,
            patterns,
        }
    }

    /// Reads the variables of the running process.
    pub fn from_env() -> Self {
        Self::new(|name| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
    }

    /// The known folder `path` is, or is inside. The most specific pattern wins, so
    /// a Chrome cache is "browser cache" even though Chrome's own folder is "browser data".
    pub fn lookup(&self, path: &Path) -> Option<&KnownFolder> {
        let path = parts(&path.to_string_lossy());
        self.patterns
            .iter()
            .filter(|(pattern, _)| is_prefix(pattern, &path))
            .max_by_key(|(pattern, _)| pattern.len())
            .map(|(_, index)| &self.folders[*index])
    }
}

/// `%NAME%\rest` -> the variable's value joined with `rest`. Patterns without a variable are kept as they are.
fn expand(pattern: &str, var: &impl Fn(&str) -> Option<PathBuf>) -> Option<String> {
    let Some(rest) = pattern.strip_prefix('%') else {
        return Some(pattern.to_string());
    };
    let (name, tail) = rest.split_once('%')?;
    let base = var(name)?;
    Some(format!("{}{tail}", base.to_string_lossy()))
}

/// Lowercase path parts, split on either separator: `C:\Users\Me` -> `["c:", "users", "me"]`.
fn parts(path: &str) -> Vec<String> {
    path.split(['\\', '/'])
        .filter(|p| !p.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn is_prefix(pattern: &[String], path: &[String]) -> bool {
    pattern.len() <= path.len()
        && pattern
            .iter()
            .zip(path)
            .all(|(want, have)| want == "*" || want == have)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn sample() -> KnownFolders {
        KnownFolders::new(|name| match name {
            "LOCALAPPDATA" => Some(PathBuf::from(r"C:\Users\Ann\AppData\Local")),
            "APPDATA" => Some(PathBuf::from(r"C:\Users\Ann\AppData\Roaming")),
            "PROGRAMDATA" => Some(PathBuf::from(r"C:\ProgramData")),
            _ => None,
        })
    }

    fn id(known: &KnownFolders, path: &str) -> Option<String> {
        known.lookup(Path::new(path)).map(|f| f.id.clone())
    }

    #[test]
    fn finds_the_most_specific_folder() {
        let k = sample();
        let local = r"C:\Users\Ann\AppData\Local";
        assert_eq!(
            id(&k, r"c:\programdata\Package Cache").as_deref(),
            Some("packageCache"),
            "case does not matter"
        );
        assert_eq!(
            id(
                &k,
                &format!(r"{local}\Google\Chrome\User Data\Profile 2\Cache")
            )
            .as_deref(),
            Some("browserCache"),
            "* matches any profile"
        );
        assert_eq!(
            id(&k, &format!(r"{local}\Google\Drive")).as_deref(),
            Some("browserData")
        );
        assert_eq!(
            id(
                &k,
                &format!(r"{local}\Microsoft\Windows\INetCache\IE\x.dat")
            )
            .as_deref(),
            Some("inetCache"),
            "files inside a known folder"
        );
        assert_eq!(
            id(&k, &format!(r"{local}\Microsoft\Teams")).as_deref(),
            Some("windowsData")
        );
        assert_eq!(
            id(&k, r"D:\$Recycle.Bin\S-1-5-21\$R1.jpg").as_deref(),
            Some("recycleBin")
        );
        assert_eq!(
            id(&k, "C:/Users/Ann/AppData/Roaming/discord").as_deref(),
            Some("discord"),
            "either separator"
        );
        assert_eq!(id(&k, &format!(r"{local}\ABX_7731")), None);
        assert_eq!(id(&k, &format!(r"{local}\Goog")), None, "whole names only");
        assert_eq!(
            id(&k, r"C:\Windows\Temp"),
            None,
            "patterns with a missing variable are left out"
        );
    }

    #[test]
    fn hidden_and_verdicts() {
        let k = sample();
        let temp = k
            .lookup(Path::new(r"C:\Users\Ann\AppData\Local\Temp"))
            .unwrap();
        assert!(temp.hidden);
        assert_eq!(temp.verdict, Verdict::Safe);
        let pc = k
            .lookup(Path::new(r"C:\ProgramData\Package Cache"))
            .unwrap();
        assert!(!pc.hidden);
        assert_eq!(pc.verdict, Verdict::Keep);
    }

    /// Every known folder must have its texts in every language.
    #[test]
    fn every_folder_is_translated() {
        let file: KnownFile = serde_json::from_str(KNOWN_FOLDERS).unwrap();
        let mut ids: Vec<&str> = file.folders.iter().map(|f| f.id.as_str()).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "ids are unique");
        for (lang, text) in [
            ("en", include_str!("../../src/lib/i18n/en.json")),
            ("it", include_str!("../../src/lib/i18n/it.json")),
        ] {
            let dict: Value = serde_json::from_str(text).unwrap();
            for id in &ids {
                for field in ["label", "what", "ifRemoved"] {
                    let value = &dict["explain"]["folders"][id][field];
                    assert!(
                        value.as_str().is_some_and(|s| !s.is_empty()),
                        "{lang}: explain.folders.{id}.{field} is missing"
                    );
                }
            }
        }
    }
}

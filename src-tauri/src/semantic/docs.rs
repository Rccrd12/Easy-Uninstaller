//! Documents found by what they say, through the index Windows already keeps
//! (Windows Search reads the text inside PDFs and Office files). Nothing new is
//! indexed for them: this only asks Windows, read only, with PowerShell, which
//! every Windows 10 and 11 has.
//!
//! What is searched travels in an environment variable, never inside the script,
//! and is quoted for Windows Search's SQL there.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// At most this many documents, best first.
pub const MAX_DOCS: usize = 500;
const TIME_LIMIT: Duration = Duration::from_secs(25);

/// Asks Windows Search for documents about `text` inside `roots`.
#[cfg_attr(not(windows), allow(dead_code))]
const SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$q = $env:EU_QUERY -replace "'", "''"
$scopes = ($env:EU_SCOPES -split "`n" | Where-Object { $_ } | ForEach-Object { "SCOPE='file:" + ($_ -replace "'", "''") + "'" }) -join ' OR '
$sql = "SELECT TOP $($env:EU_MAX) System.ItemPathDisplay FROM SYSTEMINDEX WHERE FREETEXT('$q') AND System.Kind = 'document' AND ($scopes) ORDER BY System.Search.Rank DESC"
$c = New-Object -ComObject ADODB.Connection
$c.Open("Provider=Search.CollatorDSO;Extended Properties='Application=Windows';")
$r = $c.Execute($sql)
while (-not $r.EOF) { [Console]::Out.WriteLine([string]$r.Fields.Item(0).Value); $r.MoveNext() }
$c.Close()"#;

/// What is searched, safe for the script: one line, no control characters, short.
pub fn clean_query(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect()
}

/// Paths from the script's output: absolute ones only, in their order, no repeats.
pub fn parse_paths(output: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for line in output.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let p = PathBuf::from(line);
        if is_absolute(&p) && !out.contains(&p) {
            out.push(p);
        }
        if out.len() >= MAX_DOCS {
            break;
        }
    }
    out
}

fn is_absolute(p: &Path) -> bool {
    let s = p.to_string_lossy();
    let b = s.as_bytes();
    p.is_absolute()
        || (b.len() > 2
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'\\' || b[2] == b'/'))
}

/// Documents about `text` in `roots`, best first. Empty when Windows Search is off,
/// has not indexed them, or does not answer in time.
pub fn search(text: &str, roots: &[PathBuf]) -> Vec<PathBuf> {
    let query = clean_query(text);
    if query.is_empty() || roots.is_empty() {
        return Vec::new();
    }
    let scopes: Vec<String> = roots
        .iter()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .collect();
    platform::run(&query, &scopes.join("\n"))
        .map(|out| parse_paths(&out))
        .unwrap_or_default()
}

#[cfg(windows)]
mod platform {
    use super::{MAX_DOCS, SCRIPT, TIME_LIMIT};
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn run(query: &str, scopes: &str) -> Option<String> {
        let system = std::env::var_os("SystemRoot")?;
        let exe =
            std::path::Path::new(&system).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        let mut child = Command::new(exe)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                SCRIPT,
            ])
            .env("EU_QUERY", query)
            .env("EU_SCOPES", scopes)
            .env("EU_MAX", MAX_DOCS.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .ok()?;
        let mut stdout = child.stdout.take()?;
        let reader = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stdout.read_to_string(&mut text);
            text
        });
        let deadline = Instant::now() + TIME_LIMIT;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(100))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        }
        reader.join().ok()
    }
}

/// Easy Uninstaller targets Windows; elsewhere there is no Windows Search to ask.
#[cfg(not(windows))]
mod platform {
    pub fn run(_query: &str, _scopes: &str) -> Option<String> {
        let _ = (super::SCRIPT, super::TIME_LIMIT);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_is_one_short_line() {
        assert_eq!(
            clean_query("  bolletta\n della\tluce "),
            "bolletta della luce"
        );
        assert_eq!(clean_query(&"a".repeat(500)).len(), 200);
        assert_eq!(clean_query("\u{0}\u{7}"), "");
    }

    #[test]
    fn only_absolute_paths_are_kept() {
        let out = "C:\\Users\\a\\Documents\\scan_0042.pdf\r\n\r\nnot a path\nC:\\Users\\a\\Documents\\scan_0042.pdf\nD:/Archivio/luce.docx\n";
        assert_eq!(
            parse_paths(out),
            [
                PathBuf::from("C:\\Users\\a\\Documents\\scan_0042.pdf"),
                PathBuf::from("D:/Archivio/luce.docx")
            ]
        );
    }

    #[test]
    fn what_is_searched_never_enters_the_script() {
        assert!(SCRIPT.contains("$env:EU_QUERY -replace \"'\", \"''\""));
        assert!(!SCRIPT.contains("{query}"));
    }
}

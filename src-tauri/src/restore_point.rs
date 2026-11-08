//! Windows System Restore points, made before changes to the system (uninstalling
//! an app). They need administrator rights, so Windows asks the user to allow it.
//!
//! Note: a restore point covers programs, drivers and settings, not personal
//! files. Personal files are protected by the quarantine instead.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RestorePointOutcome {
    /// Created, or Windows already made one in the last 24 hours (it allows one a day).
    Created,
    /// The user said no to the Windows permission prompt.
    Declined,
    /// System Restore is turned off on this PC.
    Disabled,
    Failed,
}

#[tauri::command]
pub async fn create_restore_point() -> RestorePointOutcome {
    tauri::async_runtime::spawn_blocking(platform::create)
        .await
        .unwrap_or(RestorePointOutcome::Failed)
}

/// Exit codes of the elevated script, chosen to match Windows error codes.
const ERROR_SERVICE_DISABLED: i32 = 1058;
const ERROR_CANCELLED: i32 = 1223;

#[cfg_attr(not(windows), allow(dead_code))]
fn outcome(exit_code: Option<i32>) -> RestorePointOutcome {
    match exit_code {
        Some(0) => RestorePointOutcome::Created,
        Some(ERROR_CANCELLED) => RestorePointOutcome::Declined,
        Some(ERROR_SERVICE_DISABLED) => RestorePointOutcome::Disabled,
        _ => RestorePointOutcome::Failed,
    }
}

/// The script run as administrator. `CreateRestorePoint` returns a Windows error
/// code: 0 when done, 1058 when System Restore is off.
/// Type 12 is APPLICATION_UNINSTALL, event 100 is BEGIN_SYSTEM_CHANGE.
#[cfg_attr(not(windows), allow(dead_code))]
const ELEVATED_SCRIPT: &str = "try { \
    $r = (Get-WmiObject -Namespace root/default -List -Class SystemRestore).CreateRestorePoint('Easy Uninstaller', 12, 100); \
    exit [int]$r.ReturnValue \
} catch { exit 1 }";

/// The value for `powershell -EncodedCommand`: Base64 of the UTF-16LE script.
/// Avoids quoting the script through two shells.
#[cfg_attr(not(windows), allow(dead_code))]
fn encode_command(script: &str) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(windows)]
mod platform {
    use super::{encode_command, outcome, RestorePointOutcome, ELEVATED_SCRIPT};
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Starts an elevated, hidden PowerShell (Windows shows its permission prompt)
    /// and waits for it. Saying no to the prompt makes `Start-Process` fail.
    pub fn create() -> RestorePointOutcome {
        let launcher = format!(
            "try {{ $p = Start-Process -FilePath powershell.exe -Verb RunAs -WindowStyle Hidden \
             -Wait -PassThru -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{}'; \
             exit $p.ExitCode }} catch {{ exit 1223 }}",
            encode_command(ELEVATED_SCRIPT)
        );
        let status = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &launcher])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        outcome(status.ok().and_then(|s| s.code()))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::RestorePointOutcome;

    /// System Restore exists only on Windows.
    pub fn create() -> RestorePointOutcome {
        RestorePointOutcome::Disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_utf16_base64() {
        // [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('exit 0'))
        assert_eq!(encode_command("exit 0"), "ZQB4AGkAdAAgADAA");
        assert_eq!(encode_command("a"), "YQA=");
        assert_eq!(encode_command("ab"), "YQBiAA==");
        assert_eq!(encode_command(""), "");
    }

    #[test]
    fn exit_codes() {
        assert_eq!(outcome(Some(0)), RestorePointOutcome::Created);
        assert_eq!(outcome(Some(1223)), RestorePointOutcome::Declined);
        assert_eq!(outcome(Some(1058)), RestorePointOutcome::Disabled);
        assert_eq!(outcome(Some(5)), RestorePointOutcome::Failed);
        assert_eq!(outcome(None), RestorePointOutcome::Failed);
    }
}

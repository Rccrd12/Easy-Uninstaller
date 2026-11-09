//! Doing things with administrator rights.
//!
//! - [`shell_run`] starts a program (an app's own uninstaller) and waits for it,
//!   as administrator when asked: Windows shows its permission prompt.
//! - [`run`] hands a quarantine job to a second copy of Easy Uninstaller started as
//!   administrator (`easy-uninstaller.exe --elevated-job <file>`). Leftovers in
//!   Program Files, ProgramData and `HKLM` need those rights to be moved, put back
//!   or deleted. The app itself never runs as administrator.
//!
//! The job file lives in the app's own data folder. The elevated copy trusts it only
//! as far as it can check it: the quarantine folder must be ours, and every item to
//! move must pass the same rules the leftover search uses (`leftovers::allowed`).

use crate::leftovers;
use crate::quarantine::{now_secs, Item, MoveReport, Quarantine, RestoreReport, Title};
use crate::startup::{self, StartupChange, StartupReport};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// The error text when the user says no to the Windows permission prompt.
pub const DECLINED: &str = "declined";
const FLAG: &str = "--elevated-job";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Job {
    /// Moves groups of items into quarantine, one entry per group.
    Add {
        root: PathBuf,
        groups: Vec<(Title, Vec<Item>)>,
    },
    Restore {
        root: PathBuf,
        ids: Vec<String>,
    },
    Delete {
        root: PathBuf,
        id: String,
    },
    /// Turns programs that start for all users on or off (`startup.rs`).
    Startup {
        changes: Vec<StartupChange>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobResult {
    pub moved: Option<MoveReport>,
    pub restore: Option<RestoreReport>,
    #[serde(default)]
    pub startup: Option<StartupReport>,
    pub error: Option<String>,
}

pub enum LaunchError {
    /// The user said no to the permission prompt.
    #[cfg_attr(not(windows), allow(dead_code))]
    Declined,
    Failed(String),
}

/// Runs `job` as administrator and waits for it.
/// `Ok(None)` when the user said no to the permission prompt.
pub fn run(app: &AppHandle, job: &Job) -> Result<Option<JobResult>, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("jobs");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("{}-{}.json", now_secs(), std::process::id()));
    let result_file = result_path(&file);
    let _ = fs::remove_file(&result_file);
    fs::write(&file, serde_json::to_vec(job).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let params = format!("{FLAG} \"{}\"", file.display());
    let launched = shell_run(&exe.to_string_lossy(), &params, true, main_window(app));
    let result = fs::read(&result_file);
    let _ = fs::remove_file(&file);
    let _ = fs::remove_file(&result_file);
    match launched {
        Err(LaunchError::Declined) => Ok(None),
        Err(LaunchError::Failed(e)) => Err(e),
        Ok(_) => {
            let result: JobResult = serde_json::from_slice(&result.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            match result.error {
                Some(e) => Err(e),
                None => Ok(Some(result)),
            }
        }
    }
}

fn result_path(job_file: &Path) -> PathBuf {
    job_file.with_extension("result.json")
}

/// The main window, so prompts and uninstallers open in front of it.
pub fn main_window(app: &AppHandle) -> Option<isize> {
    #[cfg(windows)]
    {
        app.get_webview_window("main")
            .and_then(|w| w.hwnd().ok())
            .map(|h| h.0 as isize)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        None
    }
}

/// Called first thing in `main`: when started as the elevated helper, runs the job
/// and returns the exit code; otherwise returns `None` and the app starts normally.
pub fn run_from_args() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != FLAG {
        return None;
    }
    let Some(file) = args.next().map(PathBuf::from) else {
        return Some(2);
    };
    let result = match fs::read(&file)
        .and_then(|b| serde_json::from_slice::<Job>(&b).map_err(io::Error::other))
    {
        Ok(job) => execute(job, &leftovers::Roots::from_env()),
        Err(e) => JobResult {
            error: Some(e.to_string()),
            ..JobResult::default()
        },
    };
    let ok = result.error.is_none();
    let written = serde_json::to_vec(&result)
        .map_err(io::Error::other)
        .and_then(|b| fs::write(result_path(&file), b));
    Some(if ok && written.is_ok() { 0 } else { 1 })
}

/// Does the job. Only a folder named `Quarantine` inside this app's data folder is
/// accepted, and only items the leftover rules allow are moved. Startup switches
/// are changed only for entries that exist, and nothing else is touched.
fn execute(job: Job, roots: &leftovers::Roots) -> JobResult {
    let root = match &job {
        Job::Add { root, .. } | Job::Restore { root, .. } | Job::Delete { root, .. } => root,
        Job::Startup { changes } => {
            // Only switches for all users: the rest belong to the user's own session.
            let changes: Vec<StartupChange> = changes
                .iter()
                .filter(|c| startup::parse_id(&c.id).is_some_and(|(l, _)| l.all_users()))
                .cloned()
                .collect();
            return JobResult {
                startup: Some(startup::apply(&changes)),
                ..JobResult::default()
            };
        }
    };
    if !is_quarantine_root(root) {
        return JobResult {
            error: Some("not a quarantine folder".into()),
            ..JobResult::default()
        };
    }
    let quarantine = Quarantine::new(root.clone());
    let now = now_secs();
    // Entries made with administrator rights can only be cleaned up from here.
    quarantine.purge_expired(now, true);
    let mut result = JobResult::default();
    let outcome = match job {
        Job::Add { groups, .. } => {
            let mut report = MoveReport::default();
            let done = groups.into_iter().try_for_each(|(title, items)| {
                let (ok, refused): (Vec<Item>, Vec<Item>) = items
                    .into_iter()
                    .partition(|i| leftovers::allowed(&i.target, roots));
                report.skipped_count += refused.len() as u64;
                quarantine
                    .add(title, &ok, now, true, &mut report)
                    .map(|_| ())
            });
            result.moved = Some(report);
            done
        }
        Job::Restore { ids, .. } => {
            let mut report = RestoreReport::default();
            let done = ids
                .iter()
                .try_for_each(|id| quarantine.restore(id, &mut report));
            result.restore = Some(report);
            done
        }
        Job::Delete { id, .. } => quarantine.delete(&id),
        Job::Startup { .. } => Ok(()),
    };
    if let Err(e) = outcome {
        result.error = Some(e.to_string());
    }
    result
}

/// `...\com.rccrd12.easyuninstaller\Quarantine`.
fn is_quarantine_root(root: &Path) -> bool {
    root.is_absolute()
        && root.file_name().is_some_and(|n| n == "Quarantine")
        && root
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|n| n == "com.rccrd12.easyuninstaller")
}

/// Starts `file` with `params` the way Explorer would and waits until it exits.
/// Returns its exit code when Windows gives it.
#[cfg(windows)]
pub fn shell_run(
    file: &str,
    params: &str,
    admin: bool,
    window: Option<isize>,
) -> Result<Option<u32>, LaunchError> {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HWND};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
        SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let file = HSTRING::from(file);
    let params = HSTRING::from(params);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        hwnd: HWND(window.unwrap_or(0) as *mut _),
        lpVerb: if admin { w!("runas") } else { PCWSTR::null() },
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: `info` is fully set up and the strings outlive the call.
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        return Err(if e.code() == ERROR_CANCELLED.to_hresult() {
            LaunchError::Declined
        } else {
            LaunchError::Failed(e.message())
        });
    }
    if info.hProcess.is_invalid() {
        // Handed to an already running program: nothing to wait for.
        return Ok(None);
    }
    let mut code = 0u32;
    // SAFETY: `hProcess` is a valid process handle we own and close once.
    let got = unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        let got = GetExitCodeProcess(info.hProcess, &mut code).is_ok();
        let _ = CloseHandle(info.hProcess);
        got
    };
    Ok(got.then_some(code))
}

#[cfg(not(windows))]
pub fn shell_run(
    _file: &str,
    _params: &str,
    _admin: bool,
    _window: Option<isize>,
) -> Result<Option<u32>, LaunchError> {
    Err(LaunchError::Failed("only available on Windows".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quarantine::Target;

    #[test]
    fn only_our_quarantine_folder_is_accepted() {
        let ok = std::env::temp_dir()
            .join("com.rccrd12.easyuninstaller")
            .join("Quarantine");
        assert!(is_quarantine_root(&ok));
        assert!(!is_quarantine_root(
            &std::env::temp_dir().join("Quarantine")
        ));
        assert!(!is_quarantine_root(Path::new(
            "com.rccrd12.easyuninstaller/Quarantine"
        )));
        let job = Job::Delete {
            root: std::env::temp_dir(),
            id: "x".into(),
        };
        let result = execute(job, &leftovers::Roots::default());
        assert!(result.error.is_some());
    }

    #[test]
    fn refuses_items_the_leftover_rules_do_not_allow() {
        let base = std::env::temp_dir().join(format!("eu-elevated-{}", std::process::id()));
        let root = base.join("com.rccrd12.easyuninstaller").join("Quarantine");
        let outside = base.join("Documents").join("thesis.docx");
        fs::create_dir_all(outside.parent().unwrap()).unwrap();
        fs::write(&outside, "keep me").unwrap();
        let job = Job::Add {
            root,
            groups: vec![(
                Title::Text("App".into()),
                vec![Item {
                    target: Target::File(outside.clone()),
                    size: 7,
                    modified: None,
                }],
            )],
        };
        let result = execute(job, &leftovers::Roots::default());
        assert_eq!(result.error, None);
        let moved = result.moved.unwrap();
        assert_eq!((moved.moved_count, moved.skipped_count), (0, 1));
        assert!(outside.exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn jobs_survive_the_trip_through_a_file() {
        let job = Job::Restore {
            root: PathBuf::from("/q"),
            ids: vec!["1-a".into()],
        };
        let text = serde_json::to_string(&job).unwrap();
        assert!(text.contains("\"op\":\"restore\""));
        assert_eq!(serde_json::from_str::<Job>(&text).unwrap(), job);
    }
}

//! Reminders: a Windows notification when the system drive is almost full, and
//! one when a month has gone by without opening the app.
//!
//! Nothing stays running. While a reminder is on, a Windows scheduled task starts
//! `easy-uninstaller.exe --reminders <file>` once a day (or as soon as the PC is on
//! again after a missed day). That copy reads the file, checks the drive, shows a
//! notification when one is due and exits, without a window or the webview.
//! Turning both reminders off removes the task; so does uninstalling the app
//! (`windows/hooks.nsh`). Clicking a notification opens the app through its Start
//! menu shortcut, which carries the same app id ([`APP_ID`]).

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const FLAG: &str = "--reminders";
/// The scheduled task, in the Task Scheduler's root folder (users can add tasks there without administrator rights).
#[cfg_attr(not(windows), allow(dead_code))]
const TASK_NAME: &str = "Easy Uninstaller reminders";
/// `identifier` in `tauri.conf.json`: the installer gives the Start menu shortcut this app id,
/// and Windows shows notifications under that shortcut's name and icon.
#[cfg_attr(not(windows), allow(dead_code))]
const APP_ID: &str = "com.rccrd12.easyuninstaller";
/// Mirrored by `REMINDER_THRESHOLDS` in `src/lib/api.ts`.
pub const THRESHOLD_CHOICES: [u8; 4] = [5, 10, 15, 20];
const DAY: u64 = 86_400;
/// A drive that is still almost full is mentioned again after a week, not every day.
const LOW_SPACE_REPEAT: u64 = 7 * DAY;
const MONTH: u64 = 30 * DAY;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReminderSettings {
    /// A notification when less than `threshold` percent of the system drive is free.
    pub low_space: bool,
    pub threshold: u8,
    /// A notification when the app has not been opened for 30 days.
    pub monthly: bool,
}

impl Default for ReminderSettings {
    fn default() -> Self {
        Self {
            low_space: false,
            threshold: 10,
            monthly: false,
        }
    }
}

impl ReminderSettings {
    fn any(&self) -> bool {
        self.low_space || self.monthly
    }
}

/// The notification texts in the user's language. The UI sends them (the strings
/// live in its i18n files) when it opens and whenever the language changes.
/// Placeholders: `{drive}`, `{free}`, `{percent}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ReminderTexts {
    pub low_space_title: String,
    pub low_space_body: String,
    pub monthly_title: String,
    pub monthly_body: String,
    /// The decimal separator for sizes ("," in Italian).
    pub decimal: String,
}

/// What the reminders file holds. Unknown or missing fields fall back to defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
struct Stored {
    settings: ReminderSettings,
    texts: ReminderTexts,
    /// When the app window was last opened (Unix seconds).
    last_opened: u64,
    /// When the last "almost full" notice was shown, while the drive stays full.
    low_space_noticed: Option<u64>,
    monthly_noticed: Option<u64>,
    /// The program the scheduled task starts, to set it up again after the app moves.
    task_exe: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum Notice {
    LowSpace { free: u64, percent: u64 },
    Monthly,
}

/// The notice due now, if any. At most one per check; a full drive comes first.
fn due(stored: &Stored, disk: Option<(u64, u64)>, now: u64) -> Option<Notice> {
    let s = &stored.settings;
    if s.low_space {
        if let Some((total, free)) = disk.filter(|(total, _)| *total > 0) {
            let low = u128::from(free) * 100 < u128::from(total) * u128::from(s.threshold);
            let quiet = stored
                .low_space_noticed
                .is_some_and(|t| now < t.saturating_add(LOW_SPACE_REPEAT));
            if low && !quiet {
                let percent = (u128::from(free) * 100 / u128::from(total)) as u64;
                return Some(Notice::LowSpace { free, percent });
            }
        }
    }
    let month_since = |t: u64| now >= t.saturating_add(MONTH);
    if s.monthly
        && month_since(stored.last_opened)
        && stored.monthly_noticed.is_none_or(month_since)
    {
        return Some(Notice::Monthly);
    }
    None
}

/// Notes what this check saw: the notice it showed, and a drive that has room again
/// (so the next time it fills up, the notice comes right away).
fn record(stored: &mut Stored, disk: Option<(u64, u64)>, shown: Option<&Notice>, now: u64) {
    if let Some((total, free)) = disk {
        let threshold = u128::from(stored.settings.threshold);
        if total > 0 && u128::from(free) * 100 >= u128::from(total) * threshold {
            stored.low_space_noticed = None;
        }
    }
    match shown {
        Some(Notice::LowSpace { .. }) => stored.low_space_noticed = Some(now),
        Some(Notice::Monthly) => stored.monthly_noticed = Some(now),
        None => {}
    }
}

/// "8,2 GB": 1024-based with one decimal, like `formatBytes` in `src/lib/format.ts`.
fn size_text(bytes: u64, decimal: &str) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let number = if value >= 100.0 || unit == 0 {
        format!("{value:.0}")
    } else {
        let one = format!("{value:.1}");
        let one = one.strip_suffix(".0").map(str::to_string).unwrap_or(one);
        one.replace('.', if decimal.is_empty() { "." } else { decimal })
    };
    format!("{number} {}", UNITS[unit])
}

/// The title and text of a notice, in the user's language.
fn notice_text(texts: &ReminderTexts, notice: &Notice, drive: &str) -> (String, String) {
    let (title, body) = match notice {
        Notice::LowSpace { .. } => (&texts.low_space_title, &texts.low_space_body),
        Notice::Monthly => (&texts.monthly_title, &texts.monthly_body),
    };
    let (free, percent) = match notice {
        Notice::LowSpace { free, percent } => (size_text(*free, &texts.decimal), *percent),
        Notice::Monthly => (String::new(), 0),
    };
    let fill = |s: &str| {
        s.replace("{drive}", drive)
            .replace("{free}", &free)
            .replace("{percent}", &percent.to_string())
    };
    (fill(title), fill(body))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// A plain notification: a title and one line. Clicking it opens the app.
#[cfg_attr(not(windows), allow(dead_code))]
fn toast_xml(title: &str, body: &str) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        xml_escape(title),
        xml_escape(body)
    )
}

/// The scheduled task: once a day at 11:00, or as soon as possible after a missed
/// day; also on battery; as the user, without administrator rights; stopped after
/// five minutes at most (a check takes well under a second).
fn task_xml(exe: &str, file: &str, description: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>{}</Description></RegistrationInfo>
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>2026-01-01T11:00:00</StartBoundary>
      <ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay>
    </CalendarTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author"><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <ExecutionTimeLimit>PT5M</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec><Command>{}</Command><Arguments>{FLAG} "{}"</Arguments></Exec>
  </Actions>
</Task>
"#,
        xml_escape(description),
        xml_escape(exe),
        xml_escape(file)
    )
}

fn load(file: &Path) -> Stored {
    fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(file: &Path, stored: &Stored) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(stored).map_err(io::Error::other)?;
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, file)
}

fn now_secs() -> u64 {
    crate::quarantine::now_secs()
}

/// The system drive: its name and (total, free) bytes.
fn system_disk() -> (String, Option<(u64, u64)>) {
    match crate::disk::get_system_disk() {
        Ok(d) => (d.mount_point, Some((d.total_bytes, d.free_bytes))),
        Err(_) => (crate::disk::system_drive(), None),
    }
}

/// Called first thing in `main`: when started by the scheduled task, checks and
/// returns the exit code; otherwise returns `None` and the app starts normally.
pub fn run_from_args() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != FLAG {
        return None;
    }
    let Some(file) = args.next().map(PathBuf::from) else {
        return Some(2);
    };
    Some(match check(&file, now_secs()) {
        Ok(()) => 0,
        Err(_) => 1,
    })
}

/// One check, run by the scheduled task.
fn check(file: &Path, now: u64) -> io::Result<()> {
    check_with(file, now, &System)
}

/// What a check does outside its own file, replaced by a fake in the tests.
trait Outside {
    fn disk(&self) -> (String, Option<(u64, u64)>);
    fn show(&self, title: &str, body: &str) -> Result<(), String>;
    fn delete_task(&self) -> Result<(), String>;
}

struct System;

impl Outside for System {
    fn disk(&self) -> (String, Option<(u64, u64)>) {
        system_disk()
    }
    fn show(&self, title: &str, body: &str) -> Result<(), String> {
        platform::show(title, body, true)
    }
    fn delete_task(&self) -> Result<(), String> {
        platform::delete_task()
    }
}

fn check_with(file: &Path, now: u64, outside: &impl Outside) -> io::Result<()> {
    // The app's data is gone (uninstalled without the task being removed) or both
    // reminders are off: the task has nothing left to do, so it removes itself.
    if !file.exists() {
        return outside.delete_task().map_err(io::Error::other);
    }
    let mut stored = load(file);
    if !stored.settings.any() {
        return outside.delete_task().map_err(io::Error::other);
    }
    let (drive, disk) = outside.disk();
    let notice = due(&stored, disk, now);
    let shown = notice.filter(|n| {
        let (title, body) = notice_text(&stored.texts, n, &drive);
        outside.show(&title, &body).is_ok()
    });
    let before = stored.clone();
    record(&mut stored, disk, shown.as_ref(), now);
    if stored != before {
        save(file, &stored)?;
    }
    Ok(())
}

/// The reminders file, kept by the running app.
pub struct Reminders {
    file: PathBuf,
    lock: Mutex<()>,
}

impl Reminders {
    fn read(&self) -> Stored {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        load(&self.file)
    }

    /// Loads, changes and saves the file under the lock.
    fn update<T>(&self, change: impl FnOnce(&mut Stored) -> T) -> io::Result<T> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut stored = load(&self.file);
        let out = change(&mut stored);
        save(&self.file, &stored)?;
        Ok(out)
    }
}

pub fn init(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let dir = app.path().app_local_data_dir()?;
    fs::create_dir_all(&dir)?;
    app.manage(Reminders {
        file: dir.join("reminders.json"),
        lock: Mutex::new(()),
    });
    Ok(())
}

fn current_exe() -> Result<String, String> {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

/// Sets up the scheduled task for this copy of the app.
fn install_task(reminders: &Reminders, exe: &str) -> Result<(), String> {
    let xml = task_xml(
        exe,
        &reminders.file.to_string_lossy(),
        "Easy Uninstaller: checks once a day whether a reminder is due. It never runs in the background.",
    );
    platform::install_task(&xml, &reminders.file.with_extension("task.xml"))
}

#[tauri::command]
pub fn get_reminders(reminders: tauri::State<'_, Reminders>) -> ReminderSettings {
    reminders.read().settings
}

/// Saves the reminder choices and adds or removes the scheduled task to match.
/// If the task cannot be set up, the previous choices stay.
#[tauri::command]
pub async fn set_reminders(
    app: AppHandle,
    settings: ReminderSettings,
    texts: ReminderTexts,
) -> Result<(), String> {
    if !THRESHOLD_CHOICES.contains(&settings.threshold) {
        return Err(format!("unsupported threshold {}", settings.threshold));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let reminders = app.state::<Reminders>();
        let exe = current_exe()?;
        let previous = reminders.read();
        let task = if settings.any() {
            install_task(&reminders, &exe)
        } else {
            platform::delete_task()
        };
        task?;
        reminders
            .update(|s| {
                // Turning a reminder on starts counting from today.
                if settings.monthly && !previous.settings.monthly {
                    s.monthly_noticed = None;
                }
                if settings.low_space && !previous.settings.low_space {
                    s.low_space_noticed = None;
                }
                s.settings = settings;
                s.texts = texts;
                s.last_opened = now_secs();
                s.task_exe = settings.any().then_some(exe);
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Called each time the app opens and when the language changes: notes the visit
/// (the monthly reminder counts from it), keeps the texts in the user's language and,
/// when a reminder is on, sets the task up again if it went missing or the app moved.
#[tauri::command]
pub async fn reminders_opened(app: AppHandle, texts: ReminderTexts) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let reminders = app.state::<Reminders>();
        let exe = current_exe()?;
        let stored = reminders
            .update(|s| {
                s.texts = texts;
                s.last_opened = now_secs();
                s.clone()
            })
            .map_err(|e| e.to_string())?;
        if stored.settings.any()
            && (stored.task_exe.as_deref() != Some(exe.as_str()) || !platform::task_exists())
        {
            install_task(&reminders, &exe)?;
            reminders
                .update(|s| s.task_exe = Some(exe))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Shows a sample notification right away, so the user can see that Windows lets them through.
#[tauri::command]
pub async fn preview_reminder(title: String, body: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || platform::show(&title, &body, false))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(windows)]
mod platform {
    use super::{toast_xml, APP_ID, TASK_NAME};
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Shows a notification under the app's name and icon. `exiting`: the process
    /// ends right after, so give Windows a moment to take the notification.
    pub fn show(title: &str, body: &str, exiting: bool) -> Result<(), String> {
        let shown = (|| -> windows::core::Result<()> {
            let doc = XmlDocument::new()?;
            doc.LoadXml(&HSTRING::from(toast_xml(title, body)))?;
            let toast = ToastNotification::CreateToastNotification(&doc)?;
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?
                .Show(&toast)
        })();
        if exiting {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        shown.map_err(|e| e.message())
    }

    /// `schtasks.exe` from System32, never one found on the PATH.
    fn schtasks(args: &[&std::ffi::OsStr]) -> std::io::Result<Output> {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        Command::new(PathBuf::from(root).join("System32").join("schtasks.exe"))
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    }

    fn failure(out: std::io::Result<Output>) -> Option<String> {
        match out {
            Ok(o) if o.status.success() => None,
            Ok(o) => Some(String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => Some(e.to_string()),
        }
    }

    /// Creates or replaces the task. `schtasks` reads the definition from a UTF-16 file.
    pub fn install_task(xml: &str, xml_file: &Path) -> Result<(), String> {
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
        std::fs::write(xml_file, bytes).map_err(|e| e.to_string())?;
        let out = schtasks(&[
            "/Create".as_ref(),
            "/TN".as_ref(),
            TASK_NAME.as_ref(),
            "/XML".as_ref(),
            xml_file.as_os_str(),
            "/F".as_ref(),
        ]);
        let _ = std::fs::remove_file(xml_file);
        match failure(out) {
            None => Ok(()),
            Some(e) if e.is_empty() => Err("schtasks failed".into()),
            Some(e) => Err(e),
        }
    }

    pub fn task_exists() -> bool {
        failure(schtasks(&[
            "/Query".as_ref(),
            "/TN".as_ref(),
            TASK_NAME.as_ref(),
        ]))
        .is_none()
    }

    /// Removes the task; no task at all counts as done.
    pub fn delete_task() -> Result<(), String> {
        let out = schtasks(&[
            "/Delete".as_ref(),
            "/TN".as_ref(),
            TASK_NAME.as_ref(),
            "/F".as_ref(),
        ]);
        match failure(out) {
            Some(e) if task_exists() => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    /// Windows notifications and scheduled tasks exist only on Windows.
    pub fn show(_title: &str, _body: &str, _exiting: bool) -> Result<(), String> {
        Err("notifications are only available on Windows".into())
    }

    pub fn install_task(_xml: &str, _xml_file: &Path) -> Result<(), String> {
        Ok(())
    }

    pub fn task_exists() -> bool {
        true
    }

    pub fn delete_task() -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;
    const NOW: u64 = 100 * DAY;

    fn stored(low_space: bool, monthly: bool) -> Stored {
        Stored {
            settings: ReminderSettings {
                low_space,
                threshold: 10,
                monthly,
            },
            last_opened: NOW,
            ..Stored::default()
        }
    }

    #[test]
    fn app_id_matches_tauri_identifier() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], APP_ID);
    }

    #[test]
    fn thresholds_match_the_ui() {
        let api = include_str!("../../src/lib/api.ts");
        let list = THRESHOLD_CHOICES.map(|n| n.to_string()).join(", ");
        assert!(
            api.contains(&format!("REMINDER_THRESHOLDS = [{list}]")),
            "update REMINDER_THRESHOLDS in api.ts"
        );
    }

    #[test]
    fn low_space_below_threshold_only() {
        let s = stored(true, false);
        assert_eq!(due(&s, Some((100 * GB, 20 * GB)), NOW), None);
        assert_eq!(
            due(&s, Some((100 * GB, 10 * GB)), NOW),
            None,
            "exactly 10% is not below"
        );
        assert_eq!(
            due(&s, Some((100 * GB, 8 * GB)), NOW),
            Some(Notice::LowSpace {
                free: 8 * GB,
                percent: 8
            })
        );
        assert_eq!(due(&s, None, NOW), None, "no disk info, no notice");
        assert_eq!(due(&s, Some((0, 0)), NOW), None);
        assert_eq!(due(&stored(false, false), Some((100 * GB, 1)), NOW), None);
    }

    #[test]
    fn low_space_repeats_after_a_week_and_again_once_it_recovers() {
        let mut s = stored(true, false);
        let full = Some((100 * GB, 5 * GB));
        let n = due(&s, full, NOW).unwrap();
        record(&mut s, full, Some(&n), NOW);
        assert_eq!(due(&s, full, NOW + DAY), None, "quiet the next day");
        assert!(due(&s, full, NOW + 7 * DAY).is_some(), "again after a week");

        // Space was freed: the next time it fills up, the notice is immediate.
        record(&mut s, Some((100 * GB, 40 * GB)), None, NOW + DAY);
        assert_eq!(s.low_space_noticed, None);
        assert!(due(&s, full, NOW + 2 * DAY).is_some());
    }

    #[test]
    fn monthly_counts_from_the_last_visit_and_the_last_notice() {
        let mut s = stored(false, true);
        assert_eq!(due(&s, None, NOW + 29 * DAY), None);
        assert_eq!(due(&s, None, NOW + 30 * DAY), Some(Notice::Monthly));
        record(&mut s, None, Some(&Notice::Monthly), NOW + 30 * DAY);
        assert_eq!(due(&s, None, NOW + 45 * DAY), None, "once a month at most");
        assert_eq!(due(&s, None, NOW + 60 * DAY), Some(Notice::Monthly));
        s.last_opened = NOW + 50 * DAY;
        assert_eq!(due(&s, None, NOW + 60 * DAY), None, "opened recently");
    }

    #[test]
    fn full_drive_comes_before_the_monthly_reminder() {
        let s = stored(true, true);
        let n = due(&s, Some((100 * GB, GB)), NOW + 40 * DAY);
        assert!(matches!(n, Some(Notice::LowSpace { .. })));
    }

    #[test]
    fn sizes_like_the_ui() {
        assert_eq!(size_text(8 * GB + GB / 5, ","), "8,2 GB");
        assert_eq!(size_text(8 * GB + GB / 5, "."), "8.2 GB");
        assert_eq!(size_text(8 * GB, ","), "8 GB");
        assert_eq!(size_text(150 * GB, ","), "150 GB");
        assert_eq!(size_text(512, ","), "512 B");
        assert_eq!(size_text(700 << 20, ","), "700 MB");
    }

    #[test]
    fn texts_are_filled_in() {
        let texts = ReminderTexts {
            low_space_title: "{drive} is almost full".into(),
            low_space_body: "Only {free} free ({percent}%).".into(),
            monthly_title: "Time for a cleanup".into(),
            monthly_body: "A month already.".into(),
            decimal: ".".into(),
        };
        let n = Notice::LowSpace {
            free: 3 * GB / 2,
            percent: 4,
        };
        assert_eq!(
            notice_text(&texts, &n, "C:"),
            ("C: is almost full".into(), "Only 1.5 GB free (4%).".into())
        );
        assert_eq!(
            notice_text(&texts, &Notice::Monthly, "C:").0,
            "Time for a cleanup"
        );
    }

    #[test]
    fn xml_is_escaped() {
        let x = toast_xml("A & B", "<b>\"x\"</b>");
        assert!(x.contains("<text>A &amp; B</text>"));
        assert!(x.contains("<text>&lt;b&gt;&quot;x&quot;&lt;/b&gt;</text>"));
        let t = task_xml(r"C:\Apps & Co\app.exe", r"C:\data\reminders.json", "d");
        assert!(t.contains(r"<Command>C:\Apps &amp; Co\app.exe</Command>"));
        assert!(t.contains(r#"<Arguments>--reminders "C:\data\reminders.json"</Arguments>"#));
    }

    /// Records what a check would do on the PC, without notifications or tasks.
    struct Fake {
        disk: Option<(u64, u64)>,
        show_ok: bool,
        shown: std::cell::RefCell<Vec<(String, String)>>,
        deleted: std::cell::Cell<u32>,
    }

    impl Fake {
        fn new(disk: Option<(u64, u64)>, show_ok: bool) -> Self {
            Self {
                disk,
                show_ok,
                shown: Default::default(),
                deleted: Default::default(),
            }
        }
    }

    impl Outside for Fake {
        fn disk(&self) -> (String, Option<(u64, u64)>) {
            ("C:".into(), self.disk)
        }
        fn show(&self, title: &str, body: &str) -> Result<(), String> {
            self.shown.borrow_mut().push((title.into(), body.into()));
            if self.show_ok {
                Ok(())
            } else {
                Err("blocked".into())
            }
        }
        fn delete_task(&self) -> Result<(), String> {
            self.deleted.set(self.deleted.get() + 1);
            Ok(())
        }
    }

    #[test]
    fn check_saves_what_it_saw_and_removes_itself_when_unused() {
        let dir = std::env::temp_dir().join(format!("eu-reminders-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("reminders.json");

        // App data gone: the task removes itself.
        let fake = Fake::new(None, true);
        check_with(&file, NOW, &fake).unwrap();
        assert_eq!(fake.deleted.get(), 1);
        assert!(!file.exists());

        // Both reminders off: same.
        save(&file, &stored(false, false)).unwrap();
        check_with(&file, NOW, &fake).unwrap();
        assert_eq!(fake.deleted.get(), 2);

        // A notice Windows refused is not recorded, so the next check tries again.
        let mut s = stored(false, true);
        s.last_opened = 0;
        save(&file, &s).unwrap();
        let refused = Fake::new(None, false);
        check_with(&file, NOW, &refused).unwrap();
        assert_eq!(refused.shown.borrow().len(), 1);
        assert_eq!(load(&file).monthly_noticed, None);

        // Shown: recorded, and not shown again the next day.
        let fake = Fake::new(None, true);
        check_with(&file, NOW, &fake).unwrap();
        assert_eq!(load(&file).monthly_noticed, Some(NOW));
        check_with(&file, NOW + DAY, &fake).unwrap();
        assert_eq!(fake.shown.borrow().len(), 1);
        assert_eq!(fake.deleted.get(), 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_or_partial_files_fall_back_to_defaults() {
        let s: Stored = serde_json::from_str(
            r#"{"settings":{"lowSpace":true,"threshold":15,"monthly":false},"extra":1}"#,
        )
        .unwrap();
        assert!(s.settings.low_space);
        assert_eq!(s.settings.threshold, 15);
        assert_eq!(s.last_opened, 0);
    }
}

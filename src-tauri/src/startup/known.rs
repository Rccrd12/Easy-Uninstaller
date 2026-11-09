//! Startup programs the app knows: how heavy they are and what to suggest.
//! Windows does not tell other programs how long each one takes at startup, so this
//! short list covers the common ones; the rest get an estimate from their size.

use super::{Advice, Impact};

pub struct Known {
    /// Any of these, in the lowercase "name + command" text, is a match.
    pub patterns: &'static [&'static str],
    /// A clearer name than the one in the program file, when needed.
    pub name: Option<&'static str>,
    pub impact: Impact,
    pub advice: Advice,
    /// Key under `startup.notes` in the i18n files.
    pub note: &'static str,
}

const fn k(
    patterns: &'static [&'static str],
    name: Option<&'static str>,
    impact: Impact,
    advice: Advice,
    note: &'static str,
) -> Known {
    Known {
        patterns,
        name,
        impact,
        advice,
        note,
    }
}

use Advice::{Keep, System, TurnOff};
use Impact::{High, Low, Medium};

/// First match wins: specific entries before broad ones.
pub const KNOWN: &[Known] = &[
    k(
        &["securityhealth", "windowsdefender"],
        Some("Windows Security"),
        Low,
        System,
        "protects",
    ),
    k(&["onedrive"], Some("OneDrive"), Medium, Keep, "sync"),
    k(&["dropbox"], Some("Dropbox"), Medium, Keep, "sync"),
    k(
        &["googledrivefs", "google drive"],
        Some("Google Drive"),
        Medium,
        Keep,
        "sync",
    ),
    k(&["icloud"], None, Medium, Keep, "sync"),
    k(
        &[
            "avast",
            "avgui",
            "\\avg\\",
            "mcafee",
            "norton",
            "kaspersky",
            "bitdefender",
            "\\eset\\",
            "egui.exe",
            "malwarebytes",
        ],
        None,
        Medium,
        Keep,
        "protects",
    ),
    k(
        &["rtkaud", "realtek", "waves maxxaudio", "nahimic"],
        None,
        Low,
        Keep,
        "driver",
    ),
    k(
        &["igfx", "intel graphics", "radeonsoftware", "amd software"],
        None,
        Low,
        Keep,
        "driver",
    ),
    k(
        &["syntp", "synaptics", "etdctrl", "elan smart-pad"],
        None,
        Low,
        Keep,
        "driver",
    ),
    k(
        &[
            "lghub",
            "logioptions",
            "logi options",
            "icue",
            "corsair",
            "razer",
        ],
        None,
        Medium,
        Keep,
        "devices",
    ),
    k(
        &["bitwarden", "1password", "keepass", "lastpass"],
        None,
        Low,
        Keep,
        "passwords",
    ),
    k(&["spotify"], Some("Spotify"), High, TurnOff, "background"),
    k(&["discord"], Some("Discord"), High, TurnOff, "background"),
    k(
        &["ms-teams", "msteams", "teams.exe", "com.squirrel.teams"],
        Some("Microsoft Teams"),
        High,
        TurnOff,
        "chat",
    ),
    k(&["slack"], Some("Slack"), High, TurnOff, "chat"),
    k(&["skype"], Some("Skype"), High, TurnOff, "chat"),
    k(&["zoom"], Some("Zoom"), Medium, TurnOff, "chat"),
    k(&["whatsapp"], Some("WhatsApp"), Medium, TurnOff, "chat"),
    k(&["telegram"], Some("Telegram"), Medium, TurnOff, "chat"),
    k(&["steam"], Some("Steam"), High, TurnOff, "games"),
    k(
        &["epicgameslauncher", "epic games"],
        Some("Epic Games Launcher"),
        High,
        TurnOff,
        "games",
    ),
    k(
        &["eadesktop", "ea desktop", "origin.exe"],
        Some("EA app"),
        High,
        TurnOff,
        "games",
    ),
    k(
        &["battle.net"],
        Some("Battle.net"),
        Medium,
        TurnOff,
        "games",
    ),
    k(
        &["ubisoft", "upc.exe"],
        Some("Ubisoft Connect"),
        Medium,
        TurnOff,
        "games",
    ),
    k(
        &["gog galaxy", "galaxyclient"],
        Some("GOG Galaxy"),
        Medium,
        TurnOff,
        "games",
    ),
    k(
        &["microsoftedgeautolaunch", "msedge"],
        Some("Microsoft Edge"),
        Medium,
        TurnOff,
        "browser",
    ),
    k(
        &["opera.exe", "\\opera\\", "opera browser"],
        None,
        Medium,
        TurnOff,
        "browser",
    ),
    k(
        &["utorrent", "bittorrent", "qbittorrent"],
        None,
        High,
        TurnOff,
        "background",
    ),
    k(
        &["ccleaner"],
        Some("CCleaner"),
        Medium,
        TurnOff,
        "background",
    ),
    k(&["cortana"], Some("Cortana"), Low, TurnOff, "background"),
    k(
        &["yourphone", "phoneexperiencehost", "phone link"],
        Some("Phone Link"),
        Medium,
        TurnOff,
        "background",
    ),
    k(
        &["creative cloud", "adobe", "acrotray", "agcinvoker"],
        None,
        Medium,
        TurnOff,
        "updater",
    ),
    k(&["ituneshelper", "itunes"], None, Low, TurnOff, "updater"),
    k(&["jusched", "java update"], None, Low, TurnOff, "updater"),
];

pub fn find(haystack: &str) -> Option<&'static Known> {
    KNOWN
        .iter()
        .find(|k| k.patterns.iter().any(|p| haystack.contains(p)))
}

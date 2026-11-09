use super::*;

#[test]
fn ids_round_trip() {
    let id = make_id(Location::MachineRun32, "Acrobat Assistant");
    assert_eq!(id, "hklm-run32:Acrobat Assistant");
    assert_eq!(
        parse_id(&id),
        Some((Location::MachineRun32, "Acrobat Assistant"))
    );
    let store = make_id(
        Location::Store,
        r"SpotifyAB.SpotifyMusic_zpdnekdrzrea0\Spotify",
    );
    assert_eq!(
        parse_id(&store),
        Some((
            Location::Store,
            r"SpotifyAB.SpotifyMusic_zpdnekdrzrea0\Spotify"
        ))
    );
    assert_eq!(parse_id("hkcu-run:"), None);
    assert_eq!(parse_id("elsewhere:x"), None);
    assert_eq!(parse_id("nocolon"), None);
}

#[test]
fn only_machine_entries_need_admin() {
    assert!(!Location::UserRun.all_users());
    assert!(!Location::UserFolder.all_users());
    assert!(!Location::Store.all_users());
    assert!(Location::MachineRun.all_users());
    assert!(Location::MachineRun32.all_users());
    assert!(Location::CommonFolder.all_users());
}

#[test]
fn reads_the_program_from_a_command() {
    assert_eq!(
        program_path(r#""C:\Program Files\Spotify\Spotify.exe" --autostart --minimized"#)
            .as_deref(),
        Some(r"C:\Program Files\Spotify\Spotify.exe")
    );
    assert_eq!(
        program_path(r"C:\Tools\tray.exe /min").as_deref(),
        Some(r"C:\Tools\tray.exe")
    );
    assert_eq!(
        program_path(r"C:\Program Files\App Folder\app.EXE -silent").as_deref(),
        Some(r"C:\Program Files\App Folder\app.EXE")
    );
    assert_eq!(
        program_path(r#"rundll32.exe "C:\Drivers\helper.dll",Start"#).as_deref(),
        Some(r"C:\Drivers\helper.dll")
    );
    assert_eq!(program_path("   "), None);
}

#[test]
fn startup_approved_values() {
    assert!(approved_enabled(None));
    assert!(approved_enabled(Some(&[2, 0, 0, 0])));
    assert!(approved_enabled(Some(&[6, 0, 0, 0])));
    assert!(!approved_enabled(Some(&[3, 0, 0, 0])));
    assert!(!approved_enabled(Some(&[7])));
    assert!(approved_enabled(Some(&[])));

    let on = approved_value(true, 1_700_000_000);
    assert_eq!(on, [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let off = approved_value(false, 0);
    assert_eq!(off[0], 3);
    // 1970-01-01 as a FILETIME.
    assert_eq!(
        u64::from_le_bytes(off[4..].try_into().unwrap()),
        116_444_736_000_000_000
    );
    assert!(!approved_enabled(Some(&off)));
}

#[test]
fn store_task_states() {
    assert_eq!(store_state(0), Some((false, false)));
    assert_eq!(store_state(1), Some((false, false)));
    assert_eq!(store_state(2), Some((true, false)));
    assert_eq!(store_state(3), Some((false, true)));
    assert_eq!(store_state(4), Some((true, true)));
    assert_eq!(store_state(9), None);
}

#[test]
fn package_family_from_full_name() {
    assert_eq!(
        package_family("store:SpotifyAB.SpotifyMusic_1.230.1135.0_x64__zpdnekdrzrea0").as_deref(),
        Some("SpotifyAB.SpotifyMusic_zpdnekdrzrea0")
    );
    assert_eq!(package_family("store:"), None);
}

#[test]
fn impact_estimate_from_size() {
    const MB: u64 = 1024 * 1024;
    assert_eq!(impact_from_size(150 * MB), Impact::High);
    assert_eq!(impact_from_size(20 * MB), Impact::Medium);
    assert_eq!(impact_from_size(900 * 1024), Impact::Low);
}

#[test]
fn known_programs() {
    let spotify =
        known::find(r#"spotify "c:\users\a\appdata\roaming\spotify\spotify.exe" --autostart"#)
            .unwrap();
    assert_eq!(
        (spotify.impact, spotify.advice),
        (Impact::High, Advice::TurnOff)
    );
    let security =
        known::find(r"securityhealth %windir%\system32\securityhealthsystray.exe").unwrap();
    assert_eq!(security.advice, Advice::System);
    let onedrive =
        known::find(r#"onedrive "c:\program files\microsoft onedrive\onedrive.exe" /background"#)
            .unwrap();
    assert_eq!(onedrive.advice, Advice::Keep);
    assert!(known::find(r"preset c:\tools\reset.exe").is_none());
    // Every note has a text in the UI.
    let en: serde_json::Value =
        serde_json::from_str(include_str!("../../../src/lib/i18n/en.json")).unwrap();
    for k in known::KNOWN {
        assert!(
            en["startup"]["notes"][k.note].is_string(),
            "missing startup.notes.{}",
            k.note
        );
    }
    assert!(en["startup"]["notes"]["missing"].is_string());
}

#[test]
fn display_names_drop_the_extension() {
    assert_eq!(display_name("Send to OneNote.lnk"), "Send to OneNote");
    assert_eq!(display_name("Spotify"), "Spotify");
}

#[test]
fn on_first_then_heaviest() {
    let app = |name: &str, enabled: bool, impact: Impact| StartupApp {
        id: name.into(),
        name: name.into(),
        publisher: String::new(),
        note: None,
        file_name: String::new(),
        command: String::new(),
        enabled,
        impact,
        advice: Advice::None,
        all_users: false,
        icon_color: String::new(),
        icon: None,
    };
    let mut list = vec![
        app("b", false, Impact::High),
        app("c", true, Impact::Low),
        app("a", true, Impact::High),
        app("d", true, Impact::Low),
    ];
    sort(&mut list);
    let names: Vec<_> = list.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["a", "c", "d", "b"]);
}

#[test]
fn unknown_ids_are_reported_as_failed() {
    let report = apply(&[StartupChange {
        id: "hkcu-run:Nothing here".into(),
        enabled: false,
    }]);
    assert_eq!(report.failed, ["hkcu-run:Nothing here"]);
    assert!(report.changed.is_empty());
}

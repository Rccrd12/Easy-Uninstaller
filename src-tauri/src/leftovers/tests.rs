use super::*;
use std::collections::HashMap;

/// A fake PC in a temp folder: Program Files, AppData, ProgramData, a desktop
/// and a Start menu, plus a fake registry.
struct Pc {
    base: PathBuf,
    roots: Roots,
}

impl Pc {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!("eu-leftovers-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let home = base.join("Users").join("me");
        let dirs = [
            base.join("Program Files"),
            home.join(r"AppData/Local/Programs"),
            home.join("AppData/Roaming"),
            base.join("ProgramData"),
            home.join("Desktop"),
            home.join("AppData/Roaming/Start Menu/Programs"),
        ];
        for d in &dirs {
            fs::create_dir_all(d).unwrap();
        }
        let root = |p: &PathBuf, reason, safety| FolderRoot {
            path: p.clone(),
            reason,
            safety,
        };
        let roots = Roots {
            folders: vec![
                root(&dirs[0], "programFolder", Safety::Safe),
                root(&dirs[1], "programFolder", Safety::Safe),
                root(&home.join("AppData/Local"), "localData", Safety::Safe),
                root(&dirs[2], "userSettings", Safety::Check),
                root(&dirs[3], "sharedData", Safety::Safe),
            ],
            shortcuts: vec![dirs[4].clone(), dirs[5].clone()],
            start_menus: vec![dirs[5].clone()],
            profile: Some(home),
        };
        Pc { base, roots }
    }

    fn dir(&self, rel: &str) -> PathBuf {
        let p = self.base.join(rel);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn file(&self, rel: &str, size: usize) -> PathBuf {
        let p = self.base.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, vec![0u8; size]).unwrap();
        p
    }

    fn link(&self, rel: &str, target: &Path) -> PathBuf {
        let p = self.base.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, lnk::sample(target.to_str().unwrap(), true)).unwrap();
        p
    }
}

impl Drop for Pc {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

#[derive(Default)]
struct FakeReg(HashMap<String, (Vec<String>, usize)>);

impl FakeReg {
    fn key(mut self, path: &str, subkeys: &[&str], values: usize) -> Self {
        self.0.insert(
            path.to_lowercase(),
            (subkeys.iter().map(|s| s.to_string()).collect(), values),
        );
        self
    }
}

impl RegView for FakeReg {
    fn subkeys(&self, key: &RegKeyPath) -> Vec<String> {
        self.0
            .get(&key.to_string().to_lowercase())
            .map(|(s, _)| s.clone())
            .unwrap_or_default()
    }
    fn value_count(&self, key: &RegKeyPath) -> usize {
        self.0
            .get(&key.to_string().to_lowercase())
            .map_or(0, |(_, v)| *v)
    }
}

/// What was found, as short paths below the fake PC (keys as they are), sorted.
fn ends(pc: &Pc, found: &[Found]) -> Vec<String> {
    let mut v: Vec<String> = found
        .iter()
        .map(|f| match &f.target {
            Target::Registry(k) => k.to_string(),
            Target::File(p) | Target::Folder(p) => {
                let rel = p
                    .strip_prefix(&pc.base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                rel.trim_start_matches("Users/me/").to_string()
            }
        })
        .collect();
    v.sort();
    v
}

#[test]
fn names_drop_versions_architecture_and_brackets() {
    let n = |s: &str, p: &str| {
        let mut v: Vec<String> = clean_names(s, p).into_iter().collect();
        v.sort();
        v
    };
    assert_eq!(
        n("Mozilla Firefox (x64 en-US)", "Mozilla"),
        ["firefox", "mozillafirefox"]
    );
    assert_eq!(n("VLC media player 3.0.21", "VideoLAN"), ["vlcmediaplayer"]);
    assert_eq!(n("Discord", "Discord Inc."), ["discord"]);
    assert_eq!(n("7-Zip 23.01 (x64)", "Igor Pavlov"), ["7zip"]);
    assert_eq!(n("Notepad++ v8.6.2", ""), ["notepad"]);
    assert_eq!(n("2048", ""), ["2048"]);
    assert_eq!(n("Microsoft", ""), Vec::<String>::new());
    assert_eq!(n("Go", ""), Vec::<String>::new());
    let mut p: Vec<String> = publisher_names("Discord Inc.").into_iter().collect();
    p.sort();
    assert_eq!(p, ["discord", "discordinc"]);
    assert!(publisher_names("Microsoft Corporation").contains("microsoftcorporation"));
    assert!(!publisher_names("Microsoft Corporation").contains("microsoft"));
}

#[test]
fn finds_folders_keys_and_shortcuts_of_an_uninstalled_app() {
    let pc = Pc::new("app");
    // Zoom: left its data in AppData, a cache in Local, a publisher folder in ProgramData.
    pc.file("Users/me/AppData/Roaming/Zoom/data/settings.ini", 100);
    pc.file("Users/me/AppData/Local/Zoom/cache.bin", 50);
    pc.file("ProgramData/Zoom Video Communications/Zoom/logs/a.log", 10);
    // The install folder, left half empty by the uninstaller.
    let install = pc.dir("Program Files/Zoom App");
    pc.file("Program Files/Zoom App/leftover.log", 5);
    // Things that must stay.
    pc.file("Users/me/AppData/Roaming/Zoomit/keep.ini", 1); // another name
    pc.file("Users/me/AppData/Roaming/Microsoft/Zoom/keep.ini", 1); // generic parent
    let gone = install.join("bin").join("Zoom.exe");
    pc.link("Users/me/Desktop/Zoom.lnk", &gone);
    pc.link(
        "Users/me/Desktop/Other.lnk",
        &pc.file("Program Files/Other/o.exe", 1),
    );
    pc.link(
        "Users/me/AppData/Roaming/Start Menu/Programs/Zoom/Zoom.lnk",
        &gone,
    );
    pc.link(
        "Users/me/AppData/Roaming/Start Menu/Programs/Zoom/Uninstall Zoom.lnk",
        &install.join("uninstall.exe"),
    );

    let reg = FakeReg::default()
        .key(r"HKCU\Software", &["Zoom", "Microsoft", "Zoomit"], 0)
        .key(r"HKLM\Software", &["Zoom Video Communications"], 0)
        .key(r"HKLM\Software\Zoom Video Communications", &["Zoom"], 0);
    let hints = AppHints {
        name: "Zoom".into(),
        publisher: "Zoom Video Communications, Inc.".into(),
        install_dir: Some(install.clone()),
        related_dirs: vec![],
        uninstalled_at: 0,
    };
    let found = find_for_app(&hints, &pc.roots, &InUse::default(), &reg);
    assert_eq!(
        ends(&pc, &found),
        [
            "AppData/Local/Zoom",
            "AppData/Roaming/Start Menu/Programs/Zoom",
            "AppData/Roaming/Zoom",
            "Desktop/Zoom.lnk",
            r"HKCU\Software\Zoom",
            r"HKLM\Software\Zoom Video Communications",
            "Program Files/Zoom App",
            "ProgramData/Zoom Video Communications",
        ]
    );
    let by = |end: &str| {
        found
            .iter()
            .find(|f| {
                f.target
                    .display()
                    .unwrap()
                    .replace('\\', "/")
                    .ends_with(end)
            })
            .unwrap()
    };
    assert_eq!(
        by("Roaming/Zoom").safety,
        Safety::Check,
        "user settings are opt-in"
    );
    assert_eq!(by("Roaming/Zoom").size, 100);
    assert_eq!(by("Local/Zoom").reason, "localData");
    assert_eq!(by("Programs/Zoom").reason, "startMenu");
    assert_eq!(by("Program Files/Zoom App").reason, "programFolder");
    let item = by("ProgramData/Zoom Video Communications")
        .item(&pc.roots)
        .unwrap();
    assert!(item.needs_admin);
    assert_eq!(item.kind, LeftoverKind::Folder);
    let item = by("Roaming/Zoom").item(&pc.roots).unwrap();
    assert!(!item.needs_admin);
    let item = by("Desktop/Zoom.lnk").item(&pc.roots).unwrap();
    assert_eq!(item.kind, LeftoverKind::Shortcut);
    assert_eq!(item.app_name.as_deref(), Some("Zoom"));
}

#[test]
fn never_touches_what_other_apps_use() {
    let pc = Pc::new("in-use");
    pc.file("Users/me/AppData/Roaming/Python/history", 1);
    pc.file(
        "Users/me/AppData/Local/Programs/Python/Python311/python.exe",
        1,
    );
    let left = pc.dir("Users/me/AppData/Local/Programs/Python/Python312");
    pc.file("ProgramData/Acme/Rocket/r.dat", 1);
    pc.file("ProgramData/Acme/Shared/s.dat", 1);
    let reg = FakeReg::default()
        .key(r"HKCU\Software", &["Python"], 0)
        .key(r"HKCU\Software\Python", &["PythonCore"], 0);

    let mut in_use = InUse::default();
    in_use.add(
        "Python 3.11.4 (64-bit)",
        "Python Software Foundation",
        Some(
            &pc.base
                .join("Users/me/AppData/Local/Programs/Python/Python311"),
        ),
        &[],
    );
    let hints = AppHints {
        name: "Python 3.12.1 (64-bit)".into(),
        publisher: "Python Software Foundation".into(),
        install_dir: Some(left.clone()),
        ..AppHints::default()
    };
    let found = find_for_app(&hints, &pc.roots, &in_use, &reg);
    // Only the 3.12 folder: "Python" is still used by 3.11.
    assert_eq!(
        ends(&pc, &found),
        ["AppData/Local/Programs/Python/Python312"]
    );

    // A publisher folder holding another app is never taken whole.
    let rocket = AppHints {
        name: "Rocket".into(),
        publisher: "Acme".into(),
        ..AppHints::default()
    };
    let found = find_for_app(&rocket, &pc.roots, &InUse::default(), &FakeReg::default());
    assert_eq!(ends(&pc, &found), ["ProgramData/Acme/Rocket"]);

    // A folder an installed app points into is never taken.
    let mut in_use = InUse::default();
    in_use.add(
        "Launcher",
        "",
        None,
        &[pc.base.join("ProgramData/Acme/Rocket")],
    );
    let found = find_for_app(&rocket, &pc.roots, &in_use, &FakeReg::default());
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn generic_or_short_names_find_nothing() {
    let pc = Pc::new("generic");
    pc.dir("Users/me/AppData/Roaming/Microsoft");
    pc.dir("Users/me/AppData/Roaming/Go");
    for name in ["Microsoft", "Go", "Update", ""] {
        let h = AppHints {
            name: name.into(),
            publisher: "Microsoft Corporation".into(),
            ..AppHints::default()
        };
        let reg = FakeReg::default().key(r"HKCU\Software", &["Microsoft", "Go"], 0);
        assert!(find_for_app(&h, &pc.roots, &InUse::default(), &reg).is_empty());
    }
}

#[test]
fn finds_broken_shortcuts_and_list_entries() {
    let pc = Pc::new("orphans");
    let gone = pc.base.join("Program Files/Old Game/game.exe");
    pc.link("Users/me/Desktop/Old Game.lnk", &gone);
    let alive = pc.file("Program Files/Editor/editor.exe", 1);
    pc.link("Users/me/Desktop/Editor.lnk", &alive);
    fs::write(pc.base.join("Users/me/Desktop/notes.txt"), "x").unwrap();
    fs::write(
        pc.base.join("Users/me/Desktop/damaged.lnk"),
        "not a shortcut",
    )
    .unwrap();

    let key = |n: &str| {
        RegKeyPath::new(
            Hive::Hklm,
            &format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{n}"),
        )
    };
    let list = [
        ListEntry {
            key: key("OldGame"),
            name: "Old Game".into(),
            uninstall_string: format!(
                "\"{}\" /S",
                pc.base.join("Program Files/Old Game/unins.exe").display()
            ),
            install_location: Some(pc.base.join("Program Files/Old Game").display().to_string()),
            msi: false,
        },
        ListEntry {
            key: key("Editor"),
            name: "Editor".into(),
            uninstall_string: format!(
                "\"{}\"",
                pc.base.join("Program Files/Editor/missing.exe").display()
            ),
            // The uninstaller is gone but the program is still there: not broken.
            install_location: Some(pc.base.join("Program Files/Editor").display().to_string()),
            msi: false,
        },
        ListEntry {
            key: key("{GUID}"),
            name: "Some MSI".into(),
            uninstall_string: "MsiExec.exe /X{GUID}".into(),
            install_location: None,
            msi: true,
        },
    ];
    let found = find_orphans(&pc.roots, &list);
    assert_eq!(
        ends(&pc, &found),
        [
            "Desktop/Old Game.lnk",
            r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\OldGame",
        ]
    );
    assert!(found.iter().all(|f| f.app_name.is_some()));
}

#[test]
fn only_known_places_may_ever_be_moved() {
    let pc = Pc::new("allowed");
    let ok_dir = pc.dir("Users/me/AppData/Roaming/Zoom");
    let deep = pc.dir("ProgramData/Vendor/Rocket");
    assert!(allowed(&Target::Folder(ok_dir), &pc.roots));
    assert!(allowed(&Target::Folder(deep), &pc.roots));
    for bad in [
        pc.base.join("Users/me/AppData/Roaming"),
        pc.base.join("Users/me/AppData/Local/Programs"),
        pc.base.join("Users/me/AppData/Roaming/Microsoft"),
        pc.base.join("Users/me/Documents/Thesis"),
        pc.base.join("Users/me/AppData"),
        pc.base.clone(),
    ] {
        assert!(!allowed(&Target::Folder(bad.clone()), &pc.roots), "{bad:?}");
    }
    let link = pc.base.join("Users/me/Desktop/x.lnk");
    assert!(allowed(&Target::File(link), &pc.roots));
    assert!(!allowed(
        &Target::File(pc.base.join("Users/me/Desktop/x.docx")),
        &pc.roots
    ));
    assert!(!allowed(
        &Target::File(pc.base.join("Users/me/x.lnk")),
        &pc.roots
    ));

    let key = |p: &str| Target::Registry(RegKeyPath::parse(p).unwrap());
    assert!(allowed(&key(r"HKCU\Software\Zoom"), &pc.roots));
    assert!(allowed(
        &key(r"HKLM\Software\WOW6432Node\VideoLAN\VLC"),
        &pc.roots
    ));
    assert!(allowed(
        &key(r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\OldGame"),
        &pc.roots
    ));
    for bad in [
        r"HKCU\Software\Microsoft",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKCU\Software\Classes\.txt",
        r"HKLM\Software\WOW6432Node",
        r"HKLM\Software",
        r"HKLM\System\CurrentControlSet",
    ] {
        assert!(!allowed(&key(bad), &pc.roots), "{bad}");
    }
}

#[test]
fn ids_are_stable_and_distinct() {
    let a = leftover_id(&Target::Folder(PathBuf::from(r"C:\A")));
    assert_eq!(a, leftover_id(&Target::Folder(PathBuf::from(r"c:\a"))));
    assert_ne!(a, leftover_id(&Target::File(PathBuf::from(r"C:\A"))));
    assert_eq!(a.len(), 16);
}

#[test]
fn items_serialize_like_the_typescript_type() {
    let pc = Pc::new("json");
    let f = Found {
        target: Target::Registry(RegKeyPath::parse(r"HKLM\Software\Zoom").unwrap()),
        app_name: Some("Zoom".into()),
        reason: "registry",
        safety: Safety::Safe,
        size: 0,
    };
    let json = serde_json::to_string(&f.item(&pc.roots).unwrap()).unwrap();
    for part in [
        r#""appName":"Zoom""#,
        r#""kind":"registry""#,
        r#""path":"HKLM\\Software\\Zoom""#,
        r#""sizeBytes":0"#,
        r#""reason":"registry""#,
        r#""safety":"safe""#,
        r#""needsAdmin":true"#,
    ] {
        assert!(json.contains(part), "{json}");
    }
}

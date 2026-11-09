//! Apps that come with Windows or with a new PC, and whether people usually keep them.
//! Used by the "New PC" guided cleanup.
//!
//! Only apps on these lists are ever suggested. Windows itself, drivers and the PC
//! maker's support tools (driver and BIOS updates) are never on them.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Advice {
    /// Selected by default in the guided cleanup.
    Remove,
    /// Shown, not selected: some people use it.
    Choose,
}

/// Why an app is suggested. The UI shows `newPc.reasons.<reason>`.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// A game installed with Windows or by the PC maker.
    Game,
    /// A trial of a paid antivirus: Windows Security already protects the PC.
    Trial,
    /// Installed to advertise a service.
    Promo,
    /// A Microsoft extra few people open.
    Rarely,
    /// Microsoft stopped it, or replaced it with a newer app.
    Retired,
    /// Only needed for Xbox games and the Game Bar.
    Xbox,
    /// Links an Android phone or iPhone to the PC.
    Phone,
    /// Chat or social network: keep it if you use it.
    Social,
    /// Films, series or music service.
    Streaming,
    /// A link to Microsoft 365 / Office online.
    Office,
    /// Video editor.
    Video,
    /// Small Microsoft tool some people use (weather, to-do, family safety, ...).
    Tool,
}

/// What the cleanup says about one app. Mirrors `NewPcAdvice` in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NewPcAdvice {
    pub advice: Advice,
    pub reason: Reason,
}

use Advice::{Choose, Remove};
use Reason::*;

/// Microsoft Store packages by package name. A name ending in `*` matches by prefix.
const STORE: &[(&str, Advice, Reason)] = &[
    // Games and game stubs pinned on new PCs.
    ("king.com.*", Remove, Game),
    ("G5Entertainment.*", Remove, Game),
    ("A278AB0D.*", Remove, Game), // Gameloft: March of Empires, Disney Magic Kingdoms, ...
    ("GAMELOFTSA.*", Remove, Game),
    ("ROBLOXCORPORATION.ROBLOX", Choose, Game),
    ("Microsoft.MicrosoftSolitaireCollection", Choose, Game),
    ("Microsoft.MicrosoftMahjong", Choose, Game),
    // Advertised apps.
    ("5A894077.McAfeeSecurity", Remove, Trial),
    ("AD2F1837.HPJumpStarts", Remove, Promo),
    ("AcerIncorporated.AcerCollection*", Remove, Promo),
    ("Amazon.com.Amazon", Remove, Promo),
    ("PricelinePartnerNetwork.Booking.com*", Remove, Promo),
    (
        "AdobeSystemsIncorporated.AdobePhotoshopExpress",
        Remove,
        Promo,
    ),
    ("2FE3CB00.PicsArt-PhotoStudio", Remove, Promo),
    ("Flipboard.Flipboard", Remove, Promo),
    ("ShazamEntertainmentLtd.Shazam", Remove, Promo),
    ("D5EA27B7.Duolingo-LearnLanguagesforFree", Remove, Promo),
    // Microsoft extras.
    ("Microsoft.BingNews", Remove, Rarely),
    ("Microsoft.BingSports", Remove, Rarely),
    ("Microsoft.BingFinance", Remove, Rarely),
    ("Microsoft.BingSearch", Remove, Rarely),
    ("Microsoft.GetHelp", Remove, Rarely),
    ("Microsoft.Getstarted", Remove, Rarely),
    ("Microsoft.WindowsFeedbackHub", Remove, Rarely),
    ("Microsoft.Microsoft3DViewer", Remove, Rarely),
    ("Microsoft.MixedReality.Portal", Remove, Rarely),
    ("Microsoft.Print3D", Remove, Rarely),
    ("Microsoft.Messaging", Remove, Rarely),
    ("Microsoft.OneConnect", Remove, Rarely),
    ("Microsoft.Wallet", Remove, Rarely),
    ("Microsoft.MSPaint", Remove, Retired), // Paint 3D; the classic Paint is another app
    ("Microsoft.SkypeApp", Remove, Retired),
    ("Microsoft.People", Remove, Retired),
    ("Microsoft.549981C3F5F10", Remove, Retired), // Cortana
    ("Microsoft.WindowsMaps", Remove, Retired),
    ("microsoft.windowscommunicationsapps", Choose, Retired), // Mail and Calendar
    ("Microsoft.GamingApp", Choose, Xbox),
    ("Microsoft.XboxApp", Choose, Xbox),
    ("Microsoft.XboxGamingOverlay", Choose, Xbox),
    ("Microsoft.YourPhone", Choose, Phone),
    ("MicrosoftTeams", Choose, Social), // Teams (personal); work Teams is MSTeams
    ("Microsoft.MicrosoftOfficeHub", Choose, Office),
    ("Clipchamp.Clipchamp", Choose, Video),
    ("Microsoft.BingWeather", Choose, Tool),
    ("Microsoft.Todos", Choose, Tool),
    ("Microsoft.PowerAutomateDesktop", Choose, Tool),
    ("MicrosoftCorporationII.MicrosoftFamily", Choose, Tool),
    ("Microsoft.ZuneVideo", Choose, Streaming), // Films & TV
    // Social and streaming apps pinned on new PCs.
    ("BytedancePte.Ltd.TikTok", Choose, Social),
    ("Facebook.Facebook", Choose, Social),
    ("FACEBOOK.317180B0BB486", Choose, Social),
    ("Facebook.Instagram*", Choose, Social),
    ("9E2F88E3.Twitter", Choose, Social),
    ("7EE7776C.LinkedInforWindows", Choose, Social),
    ("5319275A.WhatsAppDesktop", Choose, Social),
    ("SpotifyAB.SpotifyMusic", Choose, Streaming),
    ("4DF9E0F8.Netflix", Choose, Streaming),
    ("Disney.37853FC22B2CE", Choose, Streaming),
    ("AmazonVideo.PrimeVideo", Choose, Streaming),
];

/// Advice for a Microsoft Store app, from its package name (`Microsoft.BingNews`).
pub fn for_store(package_name: &str) -> Option<NewPcAdvice> {
    STORE
        .iter()
        .find(|(pattern, _, _)| match pattern.strip_suffix('*') {
            Some(prefix) => starts_with_ignore_case(package_name, prefix),
            None => package_name.eq_ignore_ascii_case(pattern),
        })
        .map(|&(_, advice, reason)| NewPcAdvice { advice, reason })
}

/// The package name of a Store app from its package full name
/// (`Name_Version_Architecture_ResourceId_PublisherId`).
pub fn package_name(full_name: &str) -> &str {
    full_name.split('_').next().unwrap_or(full_name)
}

/// Advice for a classic program: trial antivirus and game portals installed by PC makers.
pub fn for_program(name: &str, publisher: &str) -> Option<NewPcAdvice> {
    let name = name.to_lowercase();
    let publisher = publisher.to_lowercase();
    let advice = |advice, reason| Some(NewPcAdvice { advice, reason });
    if publisher.starts_with("mcafee") && name.starts_with("mcafee") {
        return advice(Remove, Trial);
    }
    let norton_maker = ["norton", "symantec", "gen digital"]
        .iter()
        .any(|p| publisher.starts_with(p));
    if norton_maker && name.starts_with("norton") {
        return advice(Remove, Trial);
    }
    if publisher.starts_with("wildtangent") {
        return advice(Remove, Game);
    }
    if name.starts_with("booking.com") {
        return advice(Remove, Promo);
    }
    if name.starts_with("dropbox") && (name.contains("promotion") || name.contains("offer")) {
        return advice(Remove, Promo);
    }
    None
}

fn starts_with_ignore_case(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len()
        && s.is_char_boundary(prefix.len())
        && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_apps_match_by_name_or_prefix() {
        let candy = package_name("king.com.CandyCrushSaga_1.2.3.0_x86__kgqvnymyfvs32");
        assert_eq!(candy, "king.com.CandyCrushSaga");
        assert_eq!(
            for_store(candy),
            Some(NewPcAdvice {
                advice: Remove,
                reason: Game
            })
        );
        assert_eq!(for_store("Microsoft.YourPhone").unwrap().advice, Choose);
        assert_eq!(for_store("microsoft.bingnews").unwrap().reason, Rarely);
        // Work Teams, Photos, Calculator, the Store and Paint stay off the list.
        for keep in [
            "MSTeams",
            "Microsoft.Windows.Photos",
            "Microsoft.WindowsCalculator",
            "Microsoft.WindowsStore",
            "Microsoft.Paint",
            "Microsoft.WindowsNotepad",
            "E046963F.LenovoCompanion",
            "AD2F1837.myHP",
            "king.co",
        ] {
            assert_eq!(for_store(keep), None, "{keep}");
        }
    }

    #[test]
    fn every_store_entry_is_reachable() {
        for (i, (pattern, _, _)) in STORE.iter().enumerate() {
            let name = pattern.trim_end_matches('*');
            let first = STORE
                .iter()
                .position(|(p, _, _)| match p.strip_suffix('*') {
                    Some(prefix) => starts_with_ignore_case(name, prefix),
                    None => name.eq_ignore_ascii_case(p),
                });
            assert_eq!(first, Some(i), "{pattern} is shadowed");
        }
    }

    #[test]
    fn trial_antivirus_and_game_portals() {
        assert_eq!(
            for_program("McAfee LiveSafe", "McAfee, LLC")
                .unwrap()
                .reason,
            Trial
        );
        assert_eq!(
            for_program("Norton 360", "Gen Digital Inc.")
                .unwrap()
                .reason,
            Trial
        );
        assert_eq!(
            for_program("WildTangent Games App", "WildTangent")
                .unwrap()
                .reason,
            Game
        );
        assert_eq!(
            for_program("Dropbox 25 GB Promotion", "Dropbox, Inc.")
                .unwrap()
                .reason,
            Promo
        );
        assert_eq!(for_program("Dropbox", "Dropbox, Inc."), None);
        assert_eq!(for_program("VLC media player", "VideoLAN"), None);
        assert_eq!(for_program("Norton Commander", "Someone"), None);
    }

    #[test]
    fn serializes_like_the_typescript_type() {
        let json = serde_json::to_string(&for_store("Microsoft.GetHelp")).unwrap();
        assert_eq!(json, r#"{"advice":"remove","reason":"rarely"}"#);
    }
}

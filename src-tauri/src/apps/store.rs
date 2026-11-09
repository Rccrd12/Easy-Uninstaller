//! Microsoft Store (packaged) apps of the current user, from the Windows PackageManager.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::{Path, PathBuf};

/// A Store app worth showing to the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// `store:<PackageFullName>`: what the PackageManager needs to remove it.
    pub id: String,
    pub name: String,
    pub publisher: String,
    /// "1.2.3.4"; empty when Windows does not say.
    pub version: String,
    pub installed_on: Option<String>,
    /// Folders removed together with the app: the package and its local data.
    pub dirs: Vec<PathBuf>,
    pub logo: Option<PathBuf>,
}

/// Unresolved resource names (`ms-resource:...`) are not readable: use the package name.
fn pick_name(display: &str, package_name: &str) -> String {
    let display = display.trim();
    if display.is_empty() || display.starts_with("ms-resource:") {
        package_name.to_string()
    } else {
        display.to_string()
    }
}

/// `file:///C:/Program%20Files/...` to `C:\Program Files\...`.
fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file:///")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(if bytes[i] == b'/' { b'\\' } else { bytes[i] });
            i += 1;
        }
    }
    String::from_utf8(out).ok().map(PathBuf::from)
}

/// The manifest names `Assets\StoreLogo.png`, but the files on disk usually carry a scale
/// qualifier (`StoreLogo.scale-100.png`). Picks the smallest scale that exists.
fn resolve_logo(manifest_path: &Path) -> Option<PathBuf> {
    if manifest_path.is_file() {
        return Some(manifest_path.to_path_buf());
    }
    let dir = manifest_path.parent()?;
    let stem = manifest_path.file_stem()?.to_str()?.to_lowercase();
    let mut candidates: Vec<(u32, PathBuf)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_lowercase();
            let qualifier = name.strip_prefix(&stem)?.strip_prefix('.')?;
            let qualifier = qualifier.strip_suffix(".png")?;
            Some((logo_rank(qualifier), e.path()))
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next().map(|(_, p)| p)
}

/// Lower is better: plain scale-100 first, then bigger scales, then anything else.
fn logo_rank(qualifier: &str) -> u32 {
    let plain = !qualifier.contains("contrast") && !qualifier.contains("theme");
    let scale = qualifier
        .split('_')
        .find_map(|q| q.strip_prefix("scale-"))
        .and_then(|s| s.parse::<u32>().ok());
    match (plain, scale) {
        (true, Some(s)) => s,
        (true, None) => 1_000,
        (false, _) => 2_000,
    }
}

#[cfg(windows)]
pub fn list() -> Vec<Entry> {
    match read_packages() {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Store apps unavailable: {e}");
            Vec::new()
        }
    }
}

#[cfg(not(windows))]
pub fn list() -> Vec<Entry> {
    Vec::new()
}

#[cfg(windows)]
pub fn is_installed(full_name: &str) -> bool {
    use windows::core::HSTRING;
    use windows::Management::Deployment::PackageManager;
    PackageManager::new()
        .and_then(|m| {
            m.FindPackageByUserSecurityIdPackageFullName(&HSTRING::new(), &HSTRING::from(full_name))
        })
        .is_ok()
}

#[cfg(not(windows))]
pub fn is_installed(_full_name: &str) -> bool {
    false
}

/// Removes the package for the current user (no administrator rights needed).
#[cfg(windows)]
pub fn remove(full_name: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Management::Deployment::PackageManager;
    let result = PackageManager::new()
        .and_then(|m| m.RemovePackageAsync(&HSTRING::from(full_name)))
        .and_then(|op| op.join())
        .map_err(|e| e.message())?;
    match result.ExtendedErrorCode() {
        Ok(code) if code.is_err() => Err(result
            .ErrorText()
            .map(|t| t.to_string())
            .unwrap_or_else(|_| code.message())),
        _ => Ok(()),
    }
}

#[cfg(not(windows))]
pub fn remove(_full_name: &str) -> Result<(), String> {
    Err("only available on Windows".into())
}

#[cfg(windows)]
fn read_packages() -> windows::core::Result<Vec<Entry>> {
    use windows::core::HSTRING;
    use windows::Management::Deployment::{PackageManager, PackageTypes};

    let manager = PackageManager::new()?;
    // Empty SID = the current user; Main = no frameworks, resource or optional packages.
    let packages = manager
        .FindPackagesByUserSecurityIdWithPackageTypes(&HSTRING::new(), PackageTypes::Main)?;
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let mut out = Vec::new();
    let it = packages.First()?;
    while it.HasCurrent()? {
        if let Some(e) = to_entry(&it.Current()?, local.as_deref()) {
            out.push(e);
        }
        it.MoveNext()?;
    }
    Ok(out)
}

#[cfg(windows)]
fn to_entry(pkg: &windows::ApplicationModel::Package, local: Option<&Path>) -> Option<Entry> {
    use windows::ApplicationModel::PackageSignatureKind;

    if pkg.IsFramework().unwrap_or(true)
        || pkg.IsResourcePackage().unwrap_or(true)
        || pkg.IsOptional().unwrap_or(false)
        // Parts of Windows itself (Start, Settings, ...): cannot be removed.
        || pkg.SignatureKind().ok()? == PackageSignatureKind::System
    {
        return None;
    }
    // Background helpers with nothing in the Start menu are hidden, as in Settings.
    if pkg.GetAppListEntries().and_then(|e| e.Size()).ok() == Some(0) {
        return None;
    }
    let id = pkg.Id().ok()?;
    let full_name = id.FullName().ok()?.to_string();
    let family = id.FamilyName().ok()?.to_string();
    let display = pkg.DisplayName().map(|s| s.to_string()).unwrap_or_default();
    let name = pick_name(&display, &id.Name().ok()?.to_string());

    let mut dirs = Vec::new();
    if let Ok(path) = pkg.InstalledPath() {
        dirs.push(PathBuf::from(path.to_string()));
    }
    if let Some(local) = local {
        dirs.push(local.join("Packages").join(&family));
    }
    Some(Entry {
        id: format!("store:{full_name}"),
        name,
        publisher: pkg
            .PublisherDisplayName()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        version: id
            .Version()
            .map(|v| format!("{}.{}.{}.{}", v.Major, v.Minor, v.Build, v.Revision))
            .unwrap_or_default(),
        installed_on: pkg
            .InstalledDate()
            .ok()
            .and_then(|d| super::filetime_to_date(d.UniversalTime)),
        dirs,
        logo: pkg
            .Logo()
            .and_then(|uri| uri.RawUri())
            .ok()
            .and_then(|uri| file_uri_to_path(&uri.to_string()))
            .and_then(|p| resolve_logo(&p)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_fall_back_to_the_package_name() {
        assert_eq!(
            pick_name("Calculator", "Microsoft.WindowsCalculator"),
            "Calculator"
        );
        assert_eq!(pick_name("ms-resource:AppName", "Pkg.Name"), "Pkg.Name");
        assert_eq!(pick_name("  ", "Pkg.Name"), "Pkg.Name");
    }

    #[test]
    fn file_uris_become_windows_paths() {
        assert_eq!(
            file_uri_to_path("file:///C:/Program%20Files/WindowsApps/A_1.0/Assets/Logo.png"),
            Some(PathBuf::from(
                r"C:\Program Files\WindowsApps\A_1.0\Assets\Logo.png"
            ))
        );
        assert_eq!(file_uri_to_path("ms-appx:///Assets/Logo.png"), None);
        assert_eq!(file_uri_to_path("file:///bad%2"), None);
    }

    #[test]
    fn picks_the_smallest_plain_scaled_logo() {
        let dir = std::env::temp_dir().join("eu-store-test-logo");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in [
            "StoreLogo.scale-200.png",
            "StoreLogo.scale-100_contrast-black.png",
            "StoreLogo.scale-100.png",
            "Other.scale-100.png",
        ] {
            std::fs::write(dir.join(f), b"png").unwrap();
        }
        let got = resolve_logo(&dir.join("StoreLogo.png")).unwrap();
        assert_eq!(got.file_name().unwrap(), "StoreLogo.scale-100.png");
        assert_eq!(resolve_logo(&dir.join("Missing.png")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn logo_ranking() {
        assert!(logo_rank("scale-100") < logo_rank("scale-200"));
        assert!(logo_rank("scale-400") < logo_rank("targetsize-48"));
        assert!(logo_rank("targetsize-48") < logo_rank("scale-100_contrast-white"));
    }
}

//! Registry keys as quarantine items: a key is saved to a file with all its
//! values and subkeys, then removed; putting it back recreates it from that file.
//!
//! Only keys under `Software` with at least one more level (`Software\Vendor`)
//! can ever be removed, so a bad path can never touch the rest of the registry.
#![cfg_attr(not(windows), allow(dead_code))]

use serde::{Deserialize, Serialize};
use std::fmt;
use std::io;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Hive {
    /// `HKEY_CURRENT_USER`: settings of the current user.
    Hkcu,
    /// `HKEY_LOCAL_MACHINE`: settings for every user; changing them needs administrator rights.
    Hklm,
}

/// A key such as `HKCU\Software\Vendor\App`. Always read in the 64-bit view, so
/// 32-bit programs' keys are reached through their real `WOW6432Node` path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RegKeyPath {
    pub hive: Hive,
    /// Below the hive, `\`-separated, without leading or trailing `\`.
    pub path: String,
}

impl RegKeyPath {
    pub fn new(hive: Hive, path: &str) -> Self {
        RegKeyPath {
            hive,
            path: path.trim_matches('\\').to_string(),
        }
    }

    /// Parses `HKCU\...`, `HKEY_CURRENT_USER\...`, `HKLM\...` or `HKEY_LOCAL_MACHINE\...`.
    pub fn parse(s: &str) -> Option<Self> {
        let (hive, rest) = s.split_once('\\')?;
        let hive = match hive.to_ascii_uppercase().as_str() {
            "HKCU" | "HKEY_CURRENT_USER" => Hive::Hkcu,
            "HKLM" | "HKEY_LOCAL_MACHINE" => Hive::Hklm,
            _ => return None,
        };
        Some(RegKeyPath::new(hive, rest))
    }

    /// The key's own name (last part of the path).
    pub fn name(&self) -> &str {
        self.path.rsplit('\\').next().unwrap_or(&self.path)
    }

    /// True for keys this app may ever remove: below `Software\<something>`,
    /// with no empty or relative parts.
    pub fn removable(&self) -> bool {
        let parts: Vec<&str> = self.path.split('\\').collect();
        parts.len() >= 2
            && parts[0].eq_ignore_ascii_case("software")
            && parts
                .iter()
                .all(|p| !p.trim().is_empty() && *p != "." && *p != "..")
    }

    pub fn needs_admin(&self) -> bool {
        self.hive == Hive::Hklm
    }
}

impl fmt::Display for RegKeyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hive = match self.hive {
            Hive::Hkcu => "HKCU",
            Hive::Hklm => "HKLM",
        };
        write!(f, "{hive}\\{}", self.path)
    }
}

/// A saved key: its values and, recursively, its subkeys.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegTree {
    pub values: Vec<RegValueData>,
    pub keys: Vec<(String, RegTree)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegValueData {
    pub name: String,
    /// The Windows value type (`REG_SZ` = 1, `REG_DWORD` = 4, ...).
    pub kind: u32,
    pub data: Vec<u8>,
}

#[cfg(test)]
impl RegTree {
    /// Number of values in the key and all its subkeys.
    pub fn value_count(&self) -> usize {
        self.values.len()
            + self
                .keys
                .iter()
                .map(|(_, k)| k.value_count())
                .sum::<usize>()
    }
}

fn refuse(key: &RegKeyPath) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("{key} is not a key this app may remove"),
    )
}

pub use platform::{delete, exists, read, write_missing};

#[cfg(windows)]
mod platform {
    use super::{refuse, Hive, RegKeyPath, RegTree, RegValueData};
    use std::io;
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    fn hive(h: Hive) -> RegKey {
        RegKey::predef(match h {
            Hive::Hkcu => HKEY_CURRENT_USER,
            Hive::Hklm => HKEY_LOCAL_MACHINE,
        })
    }

    pub fn exists(key: &RegKeyPath) -> bool {
        hive(key.hive)
            .open_subkey_with_flags(&key.path, KEY_READ | KEY_WOW64_64KEY)
            .is_ok()
    }

    /// Reads the whole key. Fails if any part cannot be read, so a key is never
    /// removed after being saved only in part.
    pub fn read(key: &RegKeyPath) -> io::Result<RegTree> {
        let k = hive(key.hive).open_subkey_with_flags(&key.path, KEY_READ | KEY_WOW64_64KEY)?;
        read_tree(&k, 0)
    }

    fn read_tree(k: &RegKey, depth: usize) -> io::Result<RegTree> {
        if depth > 32 {
            return Err(io::Error::other("registry key nested too deep"));
        }
        let mut tree = RegTree::default();
        for v in k.enum_values() {
            let (name, value) = v?;
            tree.values.push(RegValueData {
                name,
                kind: value.vtype.clone() as u32,
                data: value.bytes.into_owned(),
            });
        }
        for name in k.enum_keys() {
            let name = name?;
            let sub = k.open_subkey_with_flags(&name, KEY_READ | KEY_WOW64_64KEY)?;
            tree.keys.push((name, read_tree(&sub, depth + 1)?));
        }
        Ok(tree)
    }

    pub fn delete(key: &RegKeyPath) -> io::Result<()> {
        if !key.removable() {
            return Err(refuse(key));
        }
        let (parent, name) = key.path.rsplit_once('\\').ok_or_else(|| refuse(key))?;
        let parent =
            hive(key.hive).open_subkey_with_flags(parent, KEY_ALL_ACCESS | KEY_WOW64_64KEY)?;
        parent.delete_subkey_all(name)
    }

    /// Recreates a saved key. Values and subkeys that exist already are kept as
    /// they are: nothing is ever overwritten.
    pub fn write_missing(key: &RegKeyPath, tree: &RegTree) -> io::Result<()> {
        if !key.removable() {
            return Err(refuse(key));
        }
        let (k, _) =
            hive(key.hive).create_subkey_with_flags(&key.path, KEY_ALL_ACCESS | KEY_WOW64_64KEY)?;
        write_tree(&k, tree)
    }

    fn write_tree(k: &RegKey, tree: &RegTree) -> io::Result<()> {
        for v in &tree.values {
            if k.get_raw_value(&v.name).is_ok() {
                continue;
            }
            let vtype = reg_type(v.kind).ok_or_else(|| io::Error::other("unknown value type"))?;
            k.set_raw_value(
                &v.name,
                &RegValue {
                    bytes: v.data.as_slice().into(),
                    vtype,
                },
            )?;
        }
        for (name, sub) in &tree.keys {
            let (sk, _) = k.create_subkey_with_flags(name, KEY_ALL_ACCESS | KEY_WOW64_64KEY)?;
            write_tree(&sk, sub)?;
        }
        Ok(())
    }

    fn reg_type(kind: u32) -> Option<winreg::enums::RegType> {
        use winreg::enums::RegType;
        [
            RegType::REG_NONE,
            RegType::REG_SZ,
            RegType::REG_EXPAND_SZ,
            RegType::REG_BINARY,
            RegType::REG_DWORD,
            RegType::REG_DWORD_BIG_ENDIAN,
            RegType::REG_LINK,
            RegType::REG_MULTI_SZ,
            RegType::REG_RESOURCE_LIST,
            RegType::REG_FULL_RESOURCE_DESCRIPTOR,
            RegType::REG_RESOURCE_REQUIREMENTS_LIST,
            RegType::REG_QWORD,
        ]
        .into_iter()
        .find(|t| t.clone() as u32 == kind)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{RegKeyPath, RegTree};
    use std::io;

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "the registry exists only on Windows",
        )
    }

    pub fn exists(_key: &RegKeyPath) -> bool {
        false
    }

    pub fn read(_key: &RegKeyPath) -> io::Result<RegTree> {
        Err(unsupported())
    }

    pub fn delete(key: &RegKeyPath) -> io::Result<()> {
        if !key.removable() {
            return Err(super::refuse(key));
        }
        Err(unsupported())
    }

    pub fn write_missing(_key: &RegKeyPath, _tree: &RegTree) -> io::Result<()> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_paths() {
        let k = RegKeyPath::parse(r"HKEY_CURRENT_USER\Software\Zoom\").unwrap();
        assert_eq!(k, RegKeyPath::new(Hive::Hkcu, r"Software\Zoom"));
        assert_eq!(k.to_string(), r"HKCU\Software\Zoom");
        assert_eq!(k.name(), "Zoom");
        let m = RegKeyPath::parse(r"hklm\SOFTWARE\WOW6432Node\VideoLAN\VLC").unwrap();
        assert_eq!(m.hive, Hive::Hklm);
        assert!(m.needs_admin());
        assert_eq!(RegKeyPath::parse(r"HKCR\.txt"), None);
        assert_eq!(RegKeyPath::parse("HKCU"), None);
    }

    #[test]
    fn only_keys_below_software_vendor_are_removable() {
        let ok = |p: &str| RegKeyPath::new(Hive::Hkcu, p).removable();
        assert!(ok(r"Software\Zoom"));
        assert!(ok(r"SOFTWARE\WOW6432Node\VideoLAN\VLC"));
        assert!(!ok("Software"));
        assert!(!ok(""));
        assert!(!ok(r"System\CurrentControlSet\Services"));
        assert!(!ok(r"Software\..\System"));
        assert!(!ok(r"Software\\Zoom"));
        assert!(delete(&RegKeyPath::new(Hive::Hklm, "Software"))
            .is_err_and(|e| e.kind() == io::ErrorKind::PermissionDenied));
    }

    #[test]
    fn counts_values_in_subkeys() {
        let v = |n: &str| RegValueData {
            name: n.into(),
            kind: 1,
            data: vec![],
        };
        let tree = RegTree {
            values: vec![v("a")],
            keys: vec![(
                "sub".into(),
                RegTree {
                    values: vec![v("b"), v("c")],
                    keys: vec![],
                },
            )],
        };
        assert_eq!(tree.value_count(), 3);
        let json = serde_json::to_string(&tree).unwrap();
        assert_eq!(serde_json::from_str::<RegTree>(&json).unwrap(), tree);
    }

    /// Saves, removes and recreates a real key under HKCU. Runs on Windows (CI).
    #[cfg(windows)]
    #[test]
    fn roundtrip_on_the_real_registry() {
        use winreg::enums::*;
        use winreg::RegKey;
        let path = format!(r"Software\EasyUninstallerTest-{}", std::process::id());
        let key = RegKeyPath::new(Hive::Hkcu, &path);
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (k, _) = hkcu.create_subkey(&path).unwrap();
        k.set_value("Name", &"value").unwrap();
        k.set_value("Count", &7u32).unwrap();
        let (sub, _) = k.create_subkey(r"Nested\Deeper").unwrap();
        sub.set_value("Path", &r"C:\x").unwrap();
        drop((k, sub));

        let tree = read(&key).unwrap();
        assert_eq!(tree.value_count(), 3);
        delete(&key).unwrap();
        assert!(!exists(&key));

        // Something recreated one value meanwhile: it must be kept.
        let (k, _) = hkcu.create_subkey(&path).unwrap();
        k.set_value("Name", &"newer").unwrap();
        drop(k);
        write_missing(&key, &tree).unwrap();
        let k = hkcu.open_subkey(&path).unwrap();
        assert_eq!(k.get_value::<String, _>("Name").unwrap(), "newer");
        assert_eq!(k.get_value::<u32, _>("Count").unwrap(), 7);
        let sub = k.open_subkey(r"Nested\Deeper").unwrap();
        assert_eq!(sub.get_value::<String, _>("Path").unwrap(), r"C:\x");
        delete(&key).unwrap();
    }
}

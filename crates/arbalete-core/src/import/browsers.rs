//! Finds the browsers installed for a user and their profiles. Only macOS paths for now.
//!
//! | Family | Profiles | Bookmarks | History |
//! |---|---|---|---|
//! | Chromium | `Local State` (`profile.info_cache`) | `Bookmarks` (JSON) | `History` (SQLite, µs since 1601) |
//! | Firefox | `profiles.ini` | `places.sqlite` (`moz_bookmarks`) | `places.sqlite` (`moz_places`, `moz_historyvisits`) |
//! | Safari | none | `Bookmarks.plist` | `History.db` (SQLite, seconds since 2001) |
//!
//! SQLite databases are copied before they are read, as a running browser locks them. Safari's
//! data needs Full Disk Access, which macOS asks the user for. Passwords and cookies are left out:
//! the browsers encrypt them with the system keychain.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Browsers of a family store their data the same way, so one importer reads them all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Chromium,
    /// A Chromium browser whose sidebar holds what other browsers keep in bookmarks.
    Arc,
    Firefox,
    Safari,
}

#[derive(Debug, PartialEq)]
pub struct Browser {
    pub name: &'static str,
    pub family: Family,
    pub profiles: Vec<Profile>,
}

#[derive(Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    /// The directory holding the profile's bookmarks and history.
    pub path: PathBuf,
}

/// Where each browser keeps its data, relative to the home directory. Arc comes first: Arbalete
/// users most likely come from it.
const BROWSERS: [(&str, Family, &str); 11] = [
    (
        "Arc",
        Family::Arc,
        "Library/Application Support/Arc/User Data",
    ),
    (
        "Chrome",
        Family::Chromium,
        "Library/Application Support/Google/Chrome",
    ),
    (
        "Edge",
        Family::Chromium,
        "Library/Application Support/Microsoft Edge",
    ),
    (
        "Brave",
        Family::Chromium,
        "Library/Application Support/BraveSoftware/Brave-Browser",
    ),
    (
        "Vivaldi",
        Family::Chromium,
        "Library/Application Support/Vivaldi",
    ),
    (
        "Opera",
        Family::Chromium,
        "Library/Application Support/com.operasoftware.Opera",
    ),
    (
        "Dia",
        Family::Chromium,
        "Library/Application Support/Dia/User Data",
    ),
    (
        "Firefox",
        Family::Firefox,
        "Library/Application Support/Firefox",
    ),
    ("Zen", Family::Firefox, "Library/Application Support/zen"),
    (
        "LibreWolf",
        Family::Firefox,
        "Library/Application Support/librewolf",
    ),
    ("Safari", Family::Safari, "Library/Safari"),
];

/// The browsers found under `home`, in the order above, with their profiles. A browser whose data
/// directory exists but lists no profile is left out: there would be nothing to import.
pub fn detect(home: &Path) -> Vec<Browser> {
    BROWSERS
        .iter()
        .filter_map(|&(name, family, dir)| {
            let dir = home.join(dir);
            let profiles = match family {
                Family::Chromium | Family::Arc => chromium_profiles(&dir),
                Family::Firefox => firefox_profiles(&dir),
                // Safari's data is behind Full Disk Access: its presence is all that can be seen.
                Family::Safari => vec![Profile {
                    name: "Default".to_string(),
                    path: dir,
                }],
            };
            let profiles = profiles.into_iter().filter(|p| p.path.is_dir());
            let profiles: Vec<_> = profiles.collect();
            (!profiles.is_empty()).then_some(Browser {
                name,
                family,
                profiles,
            })
        })
        .collect()
}

/// The profiles in `Local State`'s `profile.info_cache`, by directory. Opera keeps a single
/// profile in the data directory itself and lists none.
fn chromium_profiles(dir: &Path) -> Vec<Profile> {
    let Ok(local_state) = fs::read_to_string(dir.join("Local State")) else {
        return Vec::new();
    };
    let json: Value = serde_json::from_str(&local_state).unwrap_or_default();
    let Some(cache) = json["profile"]["info_cache"].as_object() else {
        return vec![Profile {
            name: "Default".to_string(),
            path: dir.to_path_buf(),
        }];
    };
    cache
        .iter()
        .map(|(directory, info)| Profile {
            name: info["name"].as_str().unwrap_or(directory).to_string(),
            path: dir.join(directory),
        })
        .collect()
}

/// The `[Profile…]` sections of `profiles.ini`, whose paths are relative to it unless
/// `IsRelative=0`.
fn firefox_profiles(dir: &Path) -> Vec<Profile> {
    let Ok(ini) = fs::read_to_string(dir.join("profiles.ini")) else {
        return Vec::new();
    };
    let mut profiles = Vec::new();
    let sections = ini
        .split("\n[")
        .filter(|s| s.trim_start_matches('[').starts_with("Profile"));
    for section in sections {
        let value = |key: &str| {
            section
                .lines()
                .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
                .map(str::trim)
        };
        let Some(path) = value("Path") else {
            continue;
        };
        let path = match value("IsRelative") {
            Some("0") => PathBuf::from(path),
            _ => dir.join(path),
        };
        let name = value("Name").unwrap_or("Default").to_string();
        profiles.push(Profile { name, path });
    }
    profiles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::TestDir as Home;

    fn summary(browsers: &[Browser]) -> Vec<(&str, Family, Vec<&str>)> {
        browsers
            .iter()
            .map(|b| {
                let profiles = b.profiles.iter().map(|p| p.name.as_str()).collect();
                (b.name, b.family, profiles)
            })
            .collect()
    }

    const SUPPORT: &str = "Library/Application Support";

    #[test]
    fn finds_nothing_in_an_empty_home() {
        let home = Home::new("browsers-empty");
        assert!(detect(&home.0).is_empty());
    }

    #[test]
    fn finds_chromium_profiles_in_local_state() {
        let home = Home::new("browsers-chromium");
        let local_state = r#"{ "profile": { "info_cache": {
            "Default": { "name": "Personal" },
            "Profile 1": { "name": "Work" },
            "Profile 2": { "name": "Removed" },
            "Profile 3": {}
        } } }"#;
        home.write(&format!("{SUPPORT}/Google/Chrome/Local State"), local_state);
        for profile in ["Default", "Profile 1", "Profile 3"] {
            home.dir(&format!("{SUPPORT}/Google/Chrome/{profile}"));
        }
        home.write(&format!("{SUPPORT}/Arc/User Data/Local State"), "{}");
        home.dir(&format!("{SUPPORT}/Arc/User Data/Default"));
        let browsers = detect(&home.0);
        assert_eq!(
            summary(&browsers),
            [
                ("Arc", Family::Arc, vec!["Default"]),
                (
                    "Chrome",
                    Family::Chromium,
                    vec!["Personal", "Work", "Profile 3"]
                ),
            ]
        );
        let chrome = home.0.join(SUPPORT).join("Google/Chrome");
        assert_eq!(browsers[1].profiles[1].path, chrome.join("Profile 1"));
    }

    #[test]
    fn finds_opera_s_single_profile_in_its_data_directory() {
        let home = Home::new("browsers-opera");
        home.write(
            &format!("{SUPPORT}/com.operasoftware.Opera/Local State"),
            r#"{ "browser": {} }"#,
        );
        let browsers = detect(&home.0);
        assert_eq!(
            summary(&browsers),
            [("Opera", Family::Chromium, vec!["Default"])]
        );
        let opera = home.0.join(SUPPORT).join("com.operasoftware.Opera");
        assert_eq!(browsers[0].profiles[0].path, opera);
    }

    #[test]
    fn finds_firefox_profiles_in_profiles_ini() {
        let home = Home::new("browsers-firefox");
        let elsewhere = home.0.join("elsewhere");
        let ini = format!(
            "[General]\nStartWithLastProfile=1\n\n\
             [Profile1]\nName=work\nIsRelative=0\nPath={}\n\n\
             [Profile0]\nName=default-release\nIsRelative=1\nPath=Profiles/abc.default-release\nDefault=1\n\n\
             [Install4F96D1932A9F858E]\nDefault=Profiles/abc.default-release\n",
            elsewhere.display()
        );
        home.write(&format!("{SUPPORT}/Firefox/profiles.ini"), &ini);
        home.dir(&format!("{SUPPORT}/Firefox/Profiles/abc.default-release"));
        home.dir("elsewhere");
        home.write(
            &format!("{SUPPORT}/zen/profiles.ini"),
            "[Profile0]\nName=missing\nIsRelative=1\nPath=Profiles/gone\n",
        );
        let browsers = detect(&home.0);
        assert_eq!(
            summary(&browsers),
            [("Firefox", Family::Firefox, vec!["work", "default-release"])]
        );
        assert_eq!(browsers[0].profiles[0].path, elsewhere);
    }

    #[test]
    fn finds_safari_when_its_data_directory_exists() {
        let home = Home::new("browsers-safari");
        home.dir("Library/Safari");
        let browsers = detect(&home.0);
        assert_eq!(
            summary(&browsers),
            [("Safari", Family::Safari, vec!["Default"])]
        );
        assert_eq!(browsers[0].profiles[0].path, home.0.join("Library/Safari"));
    }
}

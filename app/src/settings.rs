//! The app's settings, the same for every profile, kept in a file of their own. A profile's own
//! settings, like its search engine, are kept with it in the sidebar.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::Path};

/// The settings' file name in the data directory.
pub const FILE_NAME: &str = "settings.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// ⌘Q asks before quitting.
    pub warn_before_quit: bool,
    /// ⌥⌘N opens Little Arbalete from any app, like Arc's Little Arc.
    pub little_hotkey: bool,
    /// The "Today" tabs open at launch are those left open at the last quit; without, every
    /// Space starts with none, its pinned tabs and favorites kept.
    pub restore_session: bool,
    /// A bar above the page holds the navigation buttons and the address, which the sidebar no
    /// longer shows (View › Show Toolbar).
    pub show_toolbar: bool,
    /// The pages hidden for long are frozen, then unloaded, to save memory and energy.
    pub suspend_tabs: bool,
    /// The sites never suspended, by host (and port), like `mail.google.com`.
    pub awake_sites: Vec<String>,
    /// The sidebar's width, in points, which dragging its edge changes.
    pub sidebar_width: i32,
    /// The sites turned in or out of developer mode, by host (and port), unlike their default
    /// (developer_mode in app/src/shell/settings_ui.rs).
    pub developer_sites: BTreeMap<String, bool>,
    /// The shortcuts changed in the settings, by menu action (mac::apply_shortcuts).
    pub shortcuts: BTreeMap<String, ShortcutOverride>,
    /// Where the recent items over the library's button come from, like Arc's "Show media from:":
    /// "documents", "desktop", "screenshots", "captures" and "downloads". The folders ask macOS
    /// for access, so only the app's own are on at first.
    pub media_sources: Vec<String>,
}

/// A shortcut changed from its default, like Arc's: other keys, none, or who takes them first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShortcutOverride {
    pub keys: Option<Keys>,
    pub priority: Option<Priority>,
}

/// A menu item's key equivalent and its modifiers, AppKit's mask; no key for none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keys {
    pub key: String,
    pub modifiers: u64,
}

/// A shortcut's priority unless the settings change it: Arbalete's for the shortcuts Chrome keeps
/// from pages, like ⌘W and ⌘T, the page's for the others, like ⌘F in a document editor.
pub fn default_priority(id: &str) -> Priority {
    match id {
        "closeTab" | "newTab" | "newWindow" | "newIncognito" | "reopenTab" | "performClose"
        | "terminate" => Priority::App,
        _ => Priority::Websites,
    }
}

/// Who takes a shortcut a page wants too, like Arc's: the page, the page then Arbalete on a second
/// press, or Arbalete.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Priority {
    Websites,
    Both,
    #[default]
    App,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            warn_before_quit: false,
            little_hotkey: true,
            restore_session: true,
            show_toolbar: false,
            suspend_tabs: true,
            awake_sites: Vec::new(),
            sidebar_width: 260,
            developer_sites: BTreeMap::new(),
            shortcuts: BTreeMap::new(),
            media_sources: vec!["captures".into(), "downloads".into()],
        }
    }
}

/// Writes the settings atomically: a crash while saving leaves the previous file intact.
pub fn save(settings: &Settings, path: &Path) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(settings).map_err(io::Error::other)?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, json)?;
    fs::rename(temporary, path)
}

/// Loads the settings, the default ones when there are none or they cannot be read, in which case
/// the file is set aside so the next save does not overwrite it.
pub fn load(path: &Path) -> Settings {
    if !path.exists() {
        return Settings::default();
    }
    let settings = fs::read(path)
        .ok()
        .and_then(|json| serde_json::from_slice(&json).ok());
    settings.unwrap_or_else(|| {
        crate::shell::keep_broken(path);
        Settings::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_settings_and_defaults_what_is_missing() {
        let dir = std::env::temp_dir().join(format!("arbalete-settings-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        assert_eq!(load(&path), Settings::default());
        let settings = Settings {
            warn_before_quit: true,
            little_hotkey: false,
            restore_session: false,
            show_toolbar: true,
            suspend_tabs: false,
            awake_sites: vec!["mail.google.com".into()],
            sidebar_width: 320,
            developer_sites: BTreeMap::from([("example.com".into(), true)]),
            media_sources: vec!["desktop".into()],
            shortcuts: BTreeMap::from([(
                "newTab".into(),
                ShortcutOverride {
                    keys: Some(Keys {
                        key: "y".into(),
                        modifiers: 1 << 20,
                    }),
                    priority: Some(Priority::Websites),
                },
            )]),
        };
        save(&settings, &path).unwrap();
        assert_eq!(load(&path), settings);
        fs::write(&path, r#"{ "warnBeforeQuit": true }"#).unwrap();
        assert!(load(&path).restore_session && load(&path).suspend_tabs);
        assert_eq!(load(&path).sidebar_width, 260);
        fs::write(&path, "not json").unwrap();
        assert_eq!(load(&path), Settings::default());
        // The unreadable file is kept aside, so the next save does not overwrite it.
        assert!(!path.exists());
        let aside: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().contains("settings.broken-"))
            .collect();
        assert_eq!(aside.len(), 1);
        assert_eq!(fs::read(&aside[0]).unwrap(), b"not json");
        fs::remove_dir_all(dir).unwrap();
    }
}

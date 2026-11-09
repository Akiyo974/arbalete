//! Saves the sidebar, and what the windows show of it and where they are, as a JSON file and loads
//! them back.
//!
//! The file carries a format version, so a later version of Arbalete can migrate older files.

use crate::sidebar::{Sidebar, View};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const VERSION: u32 = 1;

/// The session's file name in the data directory.
pub const FILE_NAME: &str = "session.json";

#[derive(Serialize, Deserialize)]
struct Saved {
    version: u32,
    sidebar: Sidebar,
    /// Where the focused window is, whose view the sidebar holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    place: Option<Place>,
    /// The windows but the focused one.
    #[serde(default)]
    windows: Vec<Window>,
}

/// A saved session: the sidebar, where the focused window is, and the other windows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub sidebar: Sidebar,
    pub place: Option<Place>,
    pub windows: Vec<Window>,
}

/// A window but the focused one: what it shows of the sidebar, and where it is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Window {
    #[serde(flatten)]
    pub view: View,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
}

/// Where a window is: its screen, by the display's UUID, which outlives reconnecting it, and its
/// frame from that screen's top left corner, in points; full screen, its frame once it leaves it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub screen: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub full_screen: bool,
}

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Invalid(serde_json::Error),
    UnsupportedVersion(u32),
    /// The sidebar breaks a rule its operations rely on (`Sidebar::check_invariants`).
    Inconsistent(String),
}

/// Writes the session atomically: a crash while saving leaves the previous file intact.
pub fn save(session: &Session, path: &Path) -> io::Result<()> {
    let saved = Saved {
        version: VERSION,
        sidebar: session.sidebar.saved(),
        place: session.place.clone(),
        windows: session.windows.clone(),
    };
    let json = serde_json::to_vec_pretty(&saved).map_err(io::Error::other)?;
    write_atomically(path, &json)
}

/// Replaces the file at `path` with `bytes`: a crash or a power loss while saving leaves either
/// the previous file or the new one, never a truncated one.
///
/// The data reaches the disk before the rename (on macOS `sync_all` is `F_FULLFSYNC`), since the
/// file system may otherwise persist the rename first. The directory is not synced: a power loss
/// right after the rename can at worst bring back the previous file, which saves rewrite anyway.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)
}

/// Loads the session, or `None` when nothing was saved yet.
pub fn load(path: &Path) -> Result<Option<Session>, LoadError> {
    let json = match fs::read(path) {
        Ok(json) => json,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(LoadError::Io(error)),
    };
    let saved: Saved = serde_json::from_slice(&json).map_err(LoadError::Invalid)?;
    if saved.version != VERSION {
        return Err(LoadError::UnsupportedVersion(saved.version));
    }
    saved
        .sidebar
        .check_invariants()
        .map_err(LoadError::Inconsistent)?;
    Ok(Some(Session {
        sidebar: saved.sidebar,
        place: saved.place,
        windows: saved.windows,
    }))
}

/// Moves a file that cannot be loaded out of the way, so the next save does not overwrite it:
/// `session.json` becomes `session.broken-<seconds since 1970>.json` next to it.
pub fn set_aside(path: &Path) -> io::Result<PathBuf> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let mut name = format!("{stem}.broken-{seconds}");
    if let Some(extension) = path.extension() {
        name = format!("{name}.{}", extension.to_string_lossy());
    }
    let aside = path.with_file_name(name);
    fs::rename(path, &aside)?;
    Ok(aside)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::TestDir;
    use crate::sidebar::Color;

    #[test]
    fn saves_and_loads_the_sidebar() {
        let dir = TestDir::new("store-round-trip");
        let path = dir.0.join("sidebar.json");
        let mut sidebar = Sidebar::default();
        let home = sidebar.spaces()[0].clone();
        let folder = sidebar.add_folder(home.pinned, "Docs").unwrap();
        let tab = sidebar.open_tab(home.id, "https://example.com/").unwrap();
        sidebar.move_node(tab, folder, 0).unwrap();
        let work = sidebar.add_space("Work", home.profile).unwrap();
        sidebar.display(work).unwrap();
        sidebar.set_space_emoji(work, "💼").unwrap();
        sidebar.set_space_theme(work, Color::Green.into()).unwrap();

        let place = |screen: &str| Place {
            screen: screen.into(),
            x: 10.0,
            y: 20.0,
            width: 1280.0,
            height: 820.0,
            full_screen: screen == "B",
        };
        let session = Session {
            place: Some(place("A")),
            windows: vec![Window {
                view: sidebar.new_window_view(),
                place: Some(place("B")),
            }],
            sidebar,
        };
        save(&session, &path).unwrap();
        assert_eq!(load(&path).unwrap(), Some(session));
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn loads_the_windows_of_a_file_without_places() {
        let dir = TestDir::new("store-without-places");
        let path = dir.0.join("sidebar.json");
        let sidebar = Sidebar::default();
        let view = sidebar.new_window_view();
        let json =
            serde_json::json!({ "version": 1, "sidebar": sidebar.saved(), "windows": [view] });
        fs::write(&path, json.to_string()).unwrap();
        let session = load(&path).unwrap().unwrap();
        assert_eq!(session.place, None);
        assert_eq!(session.windows, vec![Window { view, place: None }]);
    }

    #[test]
    fn replaces_the_previous_file() {
        let dir = TestDir::new("store-replace");
        let path = dir.0.join("file.json");
        write_atomically(&path, b"old").unwrap();
        write_atomically(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn loads_nothing_before_the_first_save() {
        let dir = TestDir::new("store-missing");
        let path = dir.0.join("sidebar.json");
        assert!(matches!(load(&path), Ok(None)));
    }

    #[test]
    fn rejects_an_unknown_version() {
        let dir = TestDir::new("store-version");
        let path = dir.0.join("sidebar.json");
        save(&Session::default(), &path).unwrap();
        let json = fs::read_to_string(&path)
            .unwrap()
            .replace("\"version\": 1", "\"version\": 99");
        fs::write(&path, json).unwrap();
        assert!(matches!(
            load(&path),
            Err(LoadError::UnsupportedVersion(99))
        ));
    }

    #[test]
    fn rejects_an_invalid_file() {
        let dir = TestDir::new("store-invalid");
        let path = dir.0.join("sidebar.json");
        fs::write(&path, "not json").unwrap();
        assert!(matches!(load(&path), Err(LoadError::Invalid(_))));
    }

    #[test]
    fn rejects_a_sidebar_with_a_dangling_child() {
        let dir = TestDir::new("store-dangling");
        let path = dir.0.join("sidebar.json");
        let mut sidebar = Sidebar::default();
        let home = sidebar.spaces()[0].clone();
        let tab = sidebar.open_tab(home.id, "https://example.com/").unwrap();
        save(
            &Session {
                sidebar,
                ..Session::default()
            },
            &path,
        )
        .unwrap();
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let tab = serde_json::to_value(tab).unwrap().to_string();
        json["sidebar"]["nodes"]
            .as_object_mut()
            .unwrap()
            .remove(&tab);
        fs::write(&path, json.to_string()).unwrap();
        assert!(matches!(load(&path), Err(LoadError::Inconsistent(_))));
    }

    #[test]
    fn sets_a_broken_file_aside_with_its_bytes() {
        let dir = TestDir::new("store-set-aside");
        let path = dir.0.join(FILE_NAME);
        fs::write(&path, "not json").unwrap();
        let aside = set_aside(&path).unwrap();
        assert!(!path.exists());
        assert_eq!(aside.parent(), path.parent());
        let name = aside.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("session.broken-") && name.ends_with(".json"));
        assert_eq!(fs::read(&aside).unwrap(), b"not json");
    }
}

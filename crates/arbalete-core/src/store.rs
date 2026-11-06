//! Saves the sidebar, and what the other windows show of it, as a JSON file and loads them back.
//!
//! The file carries a format version, so a later version of Arbalete can migrate older files.

use crate::sidebar::{Sidebar, View};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
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
    /// What the windows but the focused one show, whose view the sidebar holds.
    #[serde(default)]
    windows: Vec<View>,
}

/// A saved session: the sidebar, and what the other windows show of it.
pub type Session = (Sidebar, Vec<View>);

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Invalid(serde_json::Error),
    UnsupportedVersion(u32),
    /// The sidebar breaks a rule its operations rely on (`Sidebar::check_invariants`).
    Inconsistent(String),
}

/// Writes the sidebar and the other windows' views atomically: a crash while saving leaves the
/// previous file intact.
pub fn save(sidebar: &Sidebar, windows: &[View], path: &Path) -> io::Result<()> {
    let saved = Saved {
        version: VERSION,
        sidebar: sidebar.saved(),
        windows: windows.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&saved).map_err(io::Error::other)?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, json)?;
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
    Ok(Some((saved.sidebar, saved.windows)))
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

        let windows = vec![sidebar.new_window_view()];
        save(&sidebar, &windows, &path).unwrap();
        assert_eq!(load(&path).unwrap(), Some((sidebar, windows)));
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
        save(&Sidebar::default(), &[], &path).unwrap();
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
        save(&sidebar, &[], &path).unwrap();
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

//! Saves the sidebar, and what the other windows show of it, as a JSON file and loads them back.
//!
//! The file carries a format version, so a later version of Arbalete can migrate older files.

use crate::sidebar::{Sidebar, View};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

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
    Ok(Some((saved.sidebar, saved.windows)))
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
}

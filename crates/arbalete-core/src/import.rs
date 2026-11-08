//! Turns other browsers' data into Arbalete's.

pub mod arc;
pub mod bookmarks;
pub mod browsers;
pub mod chromium;
pub mod firefox;
pub mod safari;

use crate::history::Entry;
use crate::sidebar::{ProfileId, Sidebar, SpaceId};
use crate::url_routing::UrlRouting;
use bookmarks::{Bar, Bookmarks};
use browsers::{Browser, Family};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{env, fs, io, process};

/// A browser profile to import, as picked in the import assistant: `browser` and `profile` are
/// indexes in the browsers detected. It goes into the existing profile `into`, or a new one.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Choice {
    pub browser: usize,
    pub profile: usize,
    #[serde(default)]
    pub into: Option<ProfileId>,
    /// Ignored for Arc, which has no bookmarks bar.
    pub bar: Bar,
}

/// A chosen browser profile, read: what it adds to the sidebar and its history, or why it could
/// not be read. Reading takes long, a large history above all, and touches no state of the app:
/// it can run on a thread of its own.
#[derive(Debug)]
pub struct Source {
    choice: Choice,
    /// The name of a new profile, or Space, for it.
    name: String,
    /// The browser and profile, for errors.
    label: String,
    data: Result<(Data, Vec<Entry>, Favicons), String>,
}

/// The icons found for pages, by url, as PNG images.
type Favicons = HashMap<String, Vec<u8>>;

#[derive(Debug)]
enum Data {
    /// Arc's sidebar, if any, the directory of the profile in `User Data`, Arc's archive and its
    /// URL routing, if any.
    Arc(Option<String>, String, Option<String>, Option<String>),
    Bookmarks(Bookmarks),
}

/// What is left to the app once the sidebar has its imports.
#[derive(Debug, Default)]
pub struct Imported {
    /// The visits to add to each profile's history.
    pub histories: Vec<(ProfileId, Vec<Entry>)>,
    /// The icons to cache for each profile (crate::favicon), by source, as PNG images: the
    /// imported tabs show them.
    pub favicons: Vec<(ProfileId, Favicons)>,
    /// One message per browser profile that could not be read, fully or in part.
    pub errors: Vec<String>,
    /// Each browser profile imported, as chosen, and the profile it went into.
    pub targets: Vec<(Choice, ProfileId)>,
}

/// Reads each chosen browser profile. Choices that name no known browser profile are skipped.
pub fn read(browsers: &[Browser], choices: &[Choice]) -> Vec<Source> {
    let read = |choice: &Choice| {
        let browser = browsers.get(choice.browser)?;
        let source = browser.profiles.get(choice.profile)?;
        Some(Source {
            choice: choice.clone(),
            name: profile_name(browser, choice.profile),
            label: format!("{} {}", browser.name, source.name),
            data: read_profile(browser.family, &source.path),
        })
    };
    choices.iter().filter_map(read).collect()
}

/// Adds the browser profiles read to `sidebar`: Arc's Spaces and favorites, other browsers'
/// bookmarks into the profile's first Space. Every profile imported into has a Space, even when
/// its browser's data cannot be read, so that it shows. Sources going into an Arbalete profile
/// the sidebar no longer has are skipped. Quick: it only changes the sidebar in memory.
pub fn apply(sidebar: &mut Sidebar, sources: Vec<Source>) -> Imported {
    let mut imported = Imported::default();
    let mut routes = ArcRoutes::default();
    for source in sources {
        let profile = match source.choice.into {
            None => sidebar.add_profile(&source.name),
            Some(id) if sidebar.profiles().iter().any(|p| p.id == id) => id,
            Some(_) => continue,
        };
        imported.targets.push((source.choice.clone(), profile));
        let target = Target {
            profile,
            name: &source.name,
        };
        let added = source.data.and_then(|(data, history, favicons)| {
            add(sidebar, data, target, source.choice.bar, &mut routes)?;
            Ok((history, show_favicons(sidebar, profile, favicons)))
        });
        match added {
            Ok((history, favicons)) => {
                imported.histories.push((profile, history));
                imported.favicons.push((profile, favicons));
            }
            Err(error) => imported.errors.push(format!("{}: {error}", source.label)),
        }
        target.space(sidebar);
    }
    if let Err(error) = routes.apply(sidebar) {
        imported.errors.push(format!("Arc's URL routing: {error}"));
    }
    imported
}

/// Gives the profile's tabs without an icon the one found for their url, the url naming its
/// source, and returns the icons they show.
fn show_favicons(sidebar: &mut Sidebar, profile: ProfileId, mut found: Favicons) -> Favicons {
    let tabs: Vec<_> = sidebar
        .tabs()
        .filter(|(id, tab)| {
            tab.favicon.is_none()
                && found.contains_key(&tab.url)
                && sidebar.profile_of(*id) == Some(profile)
        })
        .map(|(id, _)| id)
        .collect();
    let mut shown = Favicons::new();
    for id in tabs {
        let Some(tab) = sidebar.tab_mut(id) else {
            continue;
        };
        tab.favicon = Some(tab.url.clone());
        if let Some(png) = found.remove(&tab.url) {
            shown.insert(tab.url.clone(), png);
        }
    }
    shown
}

/// A new profile is named after its browser, and its browser profile when there are several.
fn profile_name(browser: &Browser, profile: usize) -> String {
    match browser.profiles.as_slice() {
        [_] => browser.name.to_string(),
        profiles => format!("{} ({})", profiles[profile].name, browser.name),
    }
}

#[derive(Clone, Copy)]
struct Target<'a> {
    profile: ProfileId,
    /// The name of a Space created for the profile.
    name: &'a str,
}

impl Target<'_> {
    /// The profile's first Space, created when it has none.
    fn space(self, sidebar: &mut Sidebar) -> SpaceId {
        match sidebar.spaces().iter().find(|s| s.profile == self.profile) {
            Some(space) => space.id,
            None => sidebar
                .add_space(self.name, self.profile)
                .expect("a profile of the sidebar"),
        }
    }
}

/// Reads a browser profile's sidebar or bookmarks, its history and the icons of its tabs.
fn read_profile(family: Family, path: &Path) -> Result<(Data, Vec<Entry>, Favicons), String> {
    let mut favicons = Favicons::new();
    let data = match family {
        Family::Arc => {
            // Arc's data directory holds `User Data`, which holds its profiles.
            let dir = path.parent().and_then(Path::parent).unwrap_or(path);
            let directory = path.file_name().and_then(|name| name.to_str());
            let sidebar = arc::read(dir).map_err(debug)?;
            if let Some(json) = &sidebar {
                favicons = arc::favicons(dir, path, json);
            }
            // An archive or routes that cannot be read are not worth failing the import for.
            let archive = arc::read_archive(dir).ok().flatten();
            let routing = arc::read_url_routing(dir).ok().flatten();
            Data::Arc(
                sidebar,
                directory.unwrap_or_default().to_string(),
                archive,
                routing,
            )
        }
        Family::Chromium => Data::Bookmarks(chromium::read_bookmarks(path).map_err(debug)?),
        Family::Firefox => Data::Bookmarks(firefox::read_bookmarks(path).map_err(debug)?),
        Family::Safari => Data::Bookmarks(safari::read_bookmarks(path).map_err(debug)?),
    };
    let history = match family {
        Family::Arc | Family::Chromium => chromium::read_history(path).map_err(debug),
        Family::Firefox => firefox::read_history(path).map_err(debug),
        Family::Safari => safari::read_history(path).map_err(debug),
    };
    Ok((data, history?, favicons))
}

/// Arc's URL routing, which every Arc profile reads the same, and the Spaces imported from all of
/// them, by their Arc id, which its routes name.
#[derive(Default)]
struct ArcRoutes {
    json: Option<String>,
    spaces: HashMap<String, SpaceId>,
}

impl ArcRoutes {
    /// Adds Arc's routes to the sidebar's, once every Arc profile is imported; Arc's default
    /// destination takes over.
    fn apply(self, sidebar: &mut Sidebar) -> Result<(), String> {
        let Some(json) = self.json else { return Ok(()) };
        let imported = arc::url_routing(&json, &self.spaces).map_err(debug)?;
        let mut routes = sidebar.url_routing().routes.clone();
        routes.extend(imported.routes);
        sidebar.set_url_routing(UrlRouting {
            routes,
            default: imported.default,
        });
        Ok(())
    }
}

/// Adds a browser profile's sidebar or bookmarks to `target`; Arc's routes wait in `routes`.
fn add(
    sidebar: &mut Sidebar,
    data: Data,
    target: Target,
    bar: Bar,
    routes: &mut ArcRoutes,
) -> Result<(), String> {
    match data {
        Data::Arc(None, ..) => Ok(()),
        Data::Arc(Some(json), directory, archive, routing) => {
            let spaces = arc::add(sidebar, &json, &directory, target.profile).map_err(debug)?;
            if let Some(archive) = archive {
                arc::add_archive(sidebar, &archive, &spaces).map_err(debug)?;
            }
            routes.json = routing.or(routes.json.take());
            routes.spaces.extend(spaces);
            Ok(())
        }
        Data::Bookmarks(bookmarks) => {
            let space = target.space(sidebar);
            bookmarks::add(sidebar, space, &bookmarks, bar).map_err(debug)
        }
    }
}

fn debug(error: impl Debug) -> String {
    format!("{error:?}")
}

/// A copy of a SQLite database, with its write-ahead log, in a temporary directory removed when
/// dropped. The browser keeps its database locked while it runs, and the log holds its latest
/// changes.
pub(crate) struct Copy(PathBuf);

impl Copy {
    /// `None` when there is no database to copy.
    pub fn of(database: &Path) -> io::Result<Option<Self>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("arbalete-import-{}-{n}", process::id()));
        fs::create_dir_all(&dir)?;
        let copy = Self(dir);
        match fs::copy(database, copy.file()) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            result => result?,
        };
        let mut wal = database.as_os_str().to_owned();
        wal.push("-wal");
        match fs::copy(wal, copy.0.join("database-wal")) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            result => _ = result?,
        }
        Ok(Some(copy))
    }

    pub fn file(&self) -> PathBuf {
        self.0.join("database")
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A directory under the system's temporary one for a test's files, removed when dropped.
#[cfg(test)]
pub(crate) struct TestDir(pub std::path::PathBuf);

#[cfg(test)]
impl TestDir {
    /// `name` keeps tests running in parallel apart.
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("arbalete-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn write(&self, path: &str, content: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    pub fn dir(&self, path: &str) {
        std::fs::create_dir_all(self.0.join(path)).unwrap();
    }
}

#[cfg(test)]
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::NodeKind;
    use browsers::Profile;
    use rusqlite::Connection;

    fn profile(name: &str, path: PathBuf) -> Profile {
        Profile {
            name: name.to_string(),
            path,
        }
    }

    /// Arc's fixture, with two profiles, and a Chrome profile in `chrome` with a bookmark in its
    /// bar and a visit.
    fn browsers(chrome: &TestDir) -> Vec<Browser> {
        let arc = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/arc/User Data");
        chrome.write(
            "Bookmarks",
            r#"{ "roots": { "bookmark_bar": { "children": [
                { "name": "Mail", "type": "url", "url": "https://mail.example.com/" }
            ] } } }"#,
        );
        let db = Connection::open(chrome.0.join("History")).unwrap();
        db.execute_batch(
            "CREATE TABLE urls (url TEXT, title TEXT, visit_count INTEGER,
                last_visit_time INTEGER, hidden INTEGER);
             INSERT INTO urls VALUES ('https://news.example.net/', 'News', 2, 13350000000000000, 0);",
        )
        .unwrap();
        vec![
            Browser {
                name: "Arc",
                family: Family::Arc,
                profiles: vec![
                    profile("Personal", arc.join("Default")),
                    profile("Work", arc.join("Profile 1")),
                ],
            },
            Browser {
                name: "Chrome",
                family: Family::Chromium,
                profiles: vec![profile("Default", chrome.0.clone())],
            },
        ]
    }

    fn choice(browser: usize, profile: usize, into: Option<ProfileId>, bar: Bar) -> Choice {
        Choice {
            browser,
            profile,
            into,
            bar,
        }
    }

    fn urls(sidebar: &Sidebar, container: crate::sidebar::NodeId) -> Vec<String> {
        let children = &sidebar.node(container).unwrap().children;
        let nodes = children.iter().map(|&id| &sidebar.node(id).unwrap().kind);
        nodes
            .map(|kind| match kind {
                NodeKind::Tab(tab) => tab.url.clone(),
                NodeKind::Folder { title, .. } => title.clone(),
                NodeKind::Container | NodeKind::Split => panic!("no container or split imported"),
            })
            .collect()
    }

    #[test]
    fn imports_each_browser_profile_into_a_new_profile() {
        let chrome = TestDir::new("import-new");
        let browsers = browsers(&chrome);
        let mut sidebar = Sidebar::empty();
        let choices = [
            choice(0, 0, None, Bar::Favorites),
            choice(0, 1, None, Bar::Favorites),
            choice(1, 0, None, Bar::Favorites),
        ];
        let imported = apply(&mut sidebar, read(&browsers, &choices));
        assert!(imported.errors.is_empty(), "{:?}", imported.errors);
        let names: Vec<_> = sidebar.profiles().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Personal (Arc)", "Work (Arc)", "Chrome"]);
        let spaces: Vec<_> = sidebar.spaces().iter().map(|s| s.title.as_str()).collect();
        assert_eq!(spaces, ["Home", "Space", "Office", "Chrome"]);
        let favorites = |i: usize| urls(&sidebar, sidebar.profiles()[i].favorites);
        assert_eq!(favorites(1), ["https://tracker.example.com/"]);
        assert_eq!(favorites(2), ["https://mail.example.com/"]);
        let chrome_profile = sidebar.profiles()[2].id;
        let histories: Vec<_> = imported
            .histories
            .iter()
            .map(|(profile, entries)| (*profile, entries.len()))
            .collect();
        let profiles: Vec<_> = sidebar.profiles().iter().map(|p| p.id).collect();
        assert_eq!(
            histories,
            [(profiles[0], 0), (profiles[1], 0), (chrome_profile, 1)]
        );
    }

    #[test]
    fn merges_a_browser_profile_into_an_existing_one() {
        let chrome = TestDir::new("import-merge");
        let browsers = browsers(&chrome);
        let mut sidebar = Sidebar::default();
        let default = sidebar.profiles()[0].id;
        let choices = [
            choice(1, 0, Some(default), Bar::PinnedFolder),
            choice(0, 1, Some(default), Bar::Favorites),
        ];
        let imported = apply(&mut sidebar, read(&browsers, &choices));
        assert_eq!(sidebar.profiles().len(), 1);
        let spaces: Vec<_> = sidebar.spaces().iter().map(|s| s.title.as_str()).collect();
        assert_eq!(spaces, ["Home", "Office"]);
        assert_eq!(
            urls(&sidebar, sidebar.spaces()[0].pinned),
            ["Bookmarks bar"]
        );
        let favorites = urls(&sidebar, sidebar.profiles()[0].favorites);
        assert_eq!(favorites, ["https://tracker.example.com/"]);
        assert_eq!(imported.histories[0].0, default);
        let targets: Vec<_> = imported
            .targets
            .iter()
            .map(|(c, p)| (c.browser, *p))
            .collect();
        assert_eq!(targets, [(1, default), (0, default)]);
    }

    #[test]
    fn gives_a_space_to_a_profile_whose_data_cannot_be_read() {
        let chrome = TestDir::new("import-invalid");
        let browsers = browsers(&chrome);
        chrome.write("Bookmarks", "not json");
        let mut sidebar = Sidebar::empty();
        let sources = read(&browsers, &[choice(1, 0, None, Bar::Favorites)]);
        let imported = apply(&mut sidebar, sources);
        assert_eq!(imported.errors.len(), 1);
        assert!(imported.errors[0].starts_with("Chrome Default: Invalid"));
        assert_eq!(sidebar.spaces()[0].title, "Chrome");
        assert!(imported.histories.is_empty());
    }

    #[test]
    fn skips_unknown_browser_profiles_and_arbalete_profiles() {
        let chrome = TestDir::new("import-unknown");
        let browsers = browsers(&chrome);
        let mut sidebar = Sidebar::default();
        let unknown = Sidebar::default().add_profile("Elsewhere");
        let choices = [
            choice(2, 0, None, Bar::Favorites),
            choice(1, 1, None, Bar::Favorites),
            choice(1, 0, Some(unknown), Bar::Favorites),
        ];
        let imported = apply(&mut sidebar, read(&browsers, &choices));
        assert_eq!(sidebar, Sidebar::default());
        assert!(imported.histories.is_empty() && imported.errors.is_empty());
    }

    #[test]
    fn applies_what_was_read_to_the_sidebar_as_it_is_then() {
        let chrome = TestDir::new("import-later");
        let browsers = browsers(&chrome);
        let mut sidebar = Sidebar::default();
        let default = sidebar.profiles()[0].id;
        let sources = read(&browsers, &[choice(1, 0, Some(default), Bar::Favorites)]);
        // The files are gone, and the sidebar changed, before the read is applied.
        drop(chrome);
        let today = sidebar.spaces()[0].today;
        sidebar
            .add_tab(today, "https://later.example.com/", "Later")
            .unwrap();
        let imported = apply(&mut sidebar, sources);
        assert!(imported.errors.is_empty(), "{:?}", imported.errors);
        assert_eq!(urls(&sidebar, today), ["https://later.example.com/"]);
        let favorites = urls(&sidebar, sidebar.profiles()[0].favorites);
        assert_eq!(favorites, ["https://mail.example.com/"]);
        assert_eq!(imported.histories.len(), 1);
        assert_eq!(imported.histories[0].1.len(), 1);
    }

    #[test]
    fn gives_the_tabs_of_the_profile_the_icon_found_for_their_url() {
        let mut sidebar = Sidebar::default();
        let profile = sidebar.profiles()[0].id;
        let other = sidebar.add_profile("Other");
        sidebar.add_space("Other", other).unwrap();
        let [today, other_today] = [0, 1].map(|i| sidebar.spaces()[i].today);
        let favorites = sidebar.profiles()[0].favorites;
        let tabs = [today, favorites, other_today]
            .map(|parent| sidebar.add_tab(parent, "https://a.example/", "A").unwrap());
        let found = Favicons::from([
            ("https://a.example/".to_string(), b"a".to_vec()),
            ("https://unused.example/".to_string(), b"b".to_vec()),
        ]);
        let shown = show_favicons(&mut sidebar, profile, found);
        assert_eq!(
            shown,
            Favicons::from([("https://a.example/".to_string(), b"a".to_vec())])
        );
        let favicons = tabs.map(|tab| sidebar.tab_mut(tab).unwrap().favicon.clone());
        let a = Some("https://a.example/".to_string());
        assert_eq!(favicons, [a.clone(), a, None]);
    }

    #[test]
    fn reads_choices_from_the_assistant() {
        let choice: Choice = serde_json::from_str(
            r#"{ "browser": 1, "profile": 0, "into": null, "bar": "pinnedFolder" }"#,
        )
        .unwrap();
        assert_eq!(choice, self::choice(1, 0, None, Bar::PinnedFolder));
    }
}

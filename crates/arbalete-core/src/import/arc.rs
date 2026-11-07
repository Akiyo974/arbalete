//! Reads Arc's sidebar, `StorableSidebar.json`, into Spaces and tabs, with its archive and its
//! URL routing.
//!
//! Arc keeps them in `~/Library/Application Support/Arc/`, documented here only as far as reading
//! the user's own data needs:
//!
//! - **`StorableSidebar.json`**: in `sidebar.containers`, the entry with `items`, `spaces` and
//!   `topAppsContainerIDs`, each a flat array alternating keys (ids) and values. Items are linked
//!   by `parentID` / `childrenIds` and hold one kind in `data`: `tab` (`savedURL`, `savedTitle`;
//!   the item's `title` when renamed), `list` (a folder), `splitView` (its tabs as children) or
//!   `itemContainer` (a root). A Space's `containerIDs` alternates `pinned` / `unpinned` and their
//!   root; its `profile` is `{"default": true}` or `{"custom": {"_0": {"directoryBasename": …}}}`,
//!   the profile's directory in `User Data`. Its `customInfo` holds `iconType` (`emoji_v2`, or a
//!   named `icon`) and `windowTheme`: under `background.single._0.style.color._0`, one of
//!   `basicColor`, `blendedSingleColor` (`color`) or `blendedGradient` (`baseColors`, then
//!   `overlayColors` whose first is transparent), each with `translucencyStyle` (`light`, `dark`,
//!   `auto`) and `modifiers` (`intensityFactor`, `noiseFactor`, `overlay`); colors are
//!   `extendedSRGB` components, a little beyond 0 to 1. A Space without colors has no
//!   `windowTheme`. `topAppsContainerIDs` alternates a profile and its Favorites root. `spaces` is
//!   read in its own order; `sidebarSyncState` and `firebaseSyncState` are sync copies.
//! - **`StorableArchiveItems.json`**: the archived tabs, each with its Space and `archivedAt`.
//! - **`StorableLinkRouting.json`**: the routes and the default destination.
//! - **`User Data/`**: a Chromium data directory, whose `Local State` names the profiles
//!   (`profile.info_cache.<directory>.name`, no colors) and whose profiles hold the history.
//!
//! Items Arbalete has no equivalent for, or that are malformed, are skipped: one odd item must not
//! stop the import of the others.

use super::{Copy, chromium};
use crate::sidebar::{
    Archived, MAX_THEME_COLORS, Mode, NodeId, ProfileId, Sidebar, SpaceId, Tab, Theme,
};
use crate::url_routing::{Destination, Match, Route, UrlRouting};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::{fs, io, path::Path};

/// Reads the sidebar in Arc's data directory `dir`, `StorableSidebar.json`. `None` when Arc saved
/// none there.
pub fn read(dir: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(dir.join("StorableSidebar.json")) {
        Ok(json) => Ok(Some(json)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Reads the tabs Arc archived, in its data directory `dir`, `StorableArchiveItems.json`. `None`
/// when Arc saved none there.
pub fn read_archive(dir: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(dir.join("StorableArchiveItems.json")) {
        Ok(json) => Ok(Some(json)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Reads where Arc opens the links of other apps, in its data directory `dir`,
/// `StorableLinkRouting.json`. `None` when Arc saved none there.
pub fn read_url_routing(dir: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(dir.join("StorableLinkRouting.json")) {
        Ok(json) => Ok(Some(json)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Arc's routes and default destination, from the content of `StorableLinkRouting.json`: a route
/// to a Space not imported, `spaces` holding those imported by their Arc id, goes to the Space
/// shown last; a route without a url condition is left out.
pub fn url_routing(
    storable_link_routing: &str,
    spaces: &HashMap<String, SpaceId>,
) -> Result<UrlRouting, serde_json::Error> {
    let json: Value = serde_json::from_str(storable_link_routing)?;
    let destination = |value: &Value| {
        if value.get("littleArc").is_some() {
            return Destination::Little;
        }
        value["space"]["_0"]["space"]["_0"]
            .as_str()
            .and_then(|arc_id| spaces.get(arc_id))
            .map_or(Destination::MostRecentSpace, |&space| {
                Destination::Space(space)
            })
    };
    let routes = json["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|rule| {
            let condition = &rule["sourceComponents"][0]["componentType"]["urlMatch"];
            let text = condition["_1"].as_str()?.to_string();
            let matching = if condition["_0"].get("matches").is_some() {
                Match::IsEqualTo
            } else {
                Match::Contains
            };
            Some(Route {
                matching,
                text,
                destination: destination(&rule["destination"]),
            })
        });
    Ok(UrlRouting {
        routes: routes.collect(),
        default: destination(&json["defaultDestination"]),
    })
}

/// Arc counts time in seconds from 2001-01-01, Apple's reference date: that date in milliseconds
/// since the Unix epoch.
const APPLE_EPOCH_MILLIS: f64 = 978_307_200_000.0;

/// Adds the tabs Arc archived from the Spaces just imported, `spaces`, by their Arc id (add),
/// from the content of `StorableArchiveItems.json`: those of other profiles' Spaces, or of a
/// Little Arc, are left out.
pub fn add_archive(
    sidebar: &mut Sidebar,
    storable_archive: &str,
    spaces: &HashMap<String, SpaceId>,
) -> Result<(), serde_json::Error> {
    let json: Value = serde_json::from_str(storable_archive)?;
    let archived = pairs(&json["items"]).filter_map(|(_, item)| {
        let space = *spaces.get(item["source"]["space"]["_0"].as_str()?)?;
        let tab = &item["sidebarItem"]["data"]["tab"];
        let url = tab["savedURL"].as_str()?.to_string();
        let title = text(&tab["savedTitle"]).unwrap_or(&url).to_string();
        let archived_at = item["archivedAt"].as_f64()? * 1000.0 + APPLE_EPOCH_MILLIS;
        Some(Archived {
            tab: Tab {
                url,
                title,
                favicon: None,
                opener: None,
                saved_url: None,
                name: None,
                muted: false,
                last_shown: None,
            },
            space,
            archived_at: archived_at as u64,
            manual: false,
        })
    });
    sidebar.add_archived(archived.collect());
    Ok(())
}

/// The icons of the tabs in `StorableSidebar.json`, by saved url: the one Arc cached for the
/// sidebar item in its data directory `dir`, `SidebarItemsFaviconCache/<md5 of the item's id in
/// uppercase>`, else the largest one the profile's `Favicons` database, in `profile`, has for the
/// page. Icons are a bonus: what cannot be read is skipped.
pub fn favicons(dir: &Path, profile: &Path, storable_sidebar: &str) -> HashMap<String, Vec<u8>> {
    let mut favicons = HashMap::new();
    let Ok(json) = serde_json::from_str::<Value>(storable_sidebar) else {
        return favicons;
    };
    let copy = Copy::of(&profile.join("Favicons")).ok().flatten();
    let db = copy
        .as_ref()
        .and_then(|copy| Connection::open(copy.file()).ok());
    let containers = json["sidebar"]["containers"].as_array();
    let items = containers
        .into_iter()
        .flatten()
        .flat_map(|c| pairs(&c["items"]));
    for (id, item) in items {
        let (Some(id), Some(url)) = (id.as_str(), item["data"]["tab"]["savedURL"].as_str()) else {
            continue;
        };
        if favicons.contains_key(url) {
            continue;
        }
        let key = format!("{:x}", md5::compute(id.to_uppercase()));
        let cached = fs::read(dir.join("SidebarItemsFaviconCache").join(key)).ok();
        let png = cached.or_else(|| chromium::favicon(db.as_ref()?, url));
        if let Some(png) = png {
            favicons.insert(url.to_string(), png);
        }
    }
    favicons
}

/// Adds the Spaces and the favorites of one Arc profile, the one in `User Data/<directory>`, to
/// `profile`, from the content of `StorableSidebar.json`. Returns the Spaces added by their Arc
/// id, which the archive names them by (add_archive).
pub fn add(
    sidebar: &mut Sidebar,
    storable_sidebar: &str,
    directory: &str,
    profile: ProfileId,
) -> Result<HashMap<String, SpaceId>, serde_json::Error> {
    let json: Value = serde_json::from_str(storable_sidebar)?;
    let containers = json["sidebar"]["containers"].as_array();
    let Some(container) = containers
        .into_iter()
        .flatten()
        .find(|c| c.get("spaces").is_some())
    else {
        return Ok(HashMap::new());
    };
    let mut import = Import {
        sidebar,
        items: pairs(&container["items"])
            .filter_map(|(id, item)| Some((id.as_str()?, item)))
            .collect(),
        directory,
        profile,
        imported: HashSet::new(),
    };
    let mut spaces = HashMap::new();
    for (arc_id, space) in pairs(&container["spaces"]) {
        if let (Some(id), Some(arc_id)) = (import.space(space), arc_id.as_str()) {
            spaces.insert(arc_id.to_string(), id);
        }
    }
    for (arc_profile, favorites) in pairs(&container["topAppsContainerIDs"]) {
        import.favorites(arc_profile, favorites);
    }
    Ok(spaces)
}

struct Import<'a> {
    sidebar: &'a mut Sidebar,
    items: HashMap<&'a str, &'a Value>,
    /// The directory of the Arc profile imported, and the profile it goes into.
    directory: &'a str,
    profile: ProfileId,
    /// Guards against an item listed twice, or inside itself.
    imported: HashSet<&'a str>,
}

impl<'a> Import<'a> {
    /// The Spaces of other Arc profiles are left out.
    fn space(&mut self, space: &'a Value) -> Option<SpaceId> {
        if profile_directory(&space["profile"])? != self.directory {
            return None;
        }
        let containers: HashMap<_, _> = pairs(&space["containerIDs"])
            .filter_map(|(kind, id)| Some((kind.as_str()?, id.as_str()?)))
            .collect();
        let (arc_pinned, arc_today) = (*containers.get("pinned")?, *containers.get("unpinned")?);
        let title = text(&space["title"]).unwrap_or("Space");
        let id = self.sidebar.add_space(title, self.profile).ok()?;
        let custom = &space["customInfo"];
        if let Some(emoji) = custom["iconType"]["emoji_v2"].as_str() {
            self.sidebar.set_space_emoji(id, emoji).ok()?;
        }
        self.sidebar
            .set_space_theme(id, theme(&custom["windowTheme"]))
            .ok()?;
        let space = self.sidebar.spaces().last()?;
        let (pinned, today) = (space.pinned, space.today);
        self.children(arc_pinned, pinned);
        self.children(arc_today, today);
        Some(id)
    }

    fn favorites(&mut self, arc_profile: &Value, container: &'a Value) -> Option<()> {
        if profile_directory(arc_profile)? != self.directory {
            return None;
        }
        let favorites = self
            .sidebar
            .profiles()
            .iter()
            .find(|p| p.id == self.profile)?;
        let favorites = favorites.favorites;
        self.children(container.as_str()?, favorites);
        Some(())
    }

    /// Imports the children of Arc's item `id` at the end of `parent`.
    fn children(&mut self, id: &str, parent: NodeId) {
        let Some(item) = self.items.get(id).copied() else {
            return;
        };
        let children = item["childrenIds"].as_array().into_iter().flatten();
        for child in children.filter_map(Value::as_str) {
            self.item(child, parent);
        }
    }

    fn item(&mut self, id: &'a str, parent: NodeId) {
        let Some(item) = self.items.get(id).copied() else {
            return;
        };
        if !self.imported.insert(id) {
            return;
        }
        let data = &item["data"];
        if let Some(url) = data["tab"]["savedURL"].as_str() {
            // The title the user gave the tab, else the page's.
            let title = text(&item["title"]).or(text(&data["tab"]["savedTitle"]));
            // The parent is a container or a folder: only full favorites refuse the tab, which is
            // then left out.
            let _ = self.sidebar.add_tab(parent, url, title.unwrap_or(url));
        } else if data.get("list").is_some() {
            let title = text(&item["title"]).unwrap_or("Folder");
            // Favorites hold only tabs: a folder among them hands them its tabs.
            let folder = self.sidebar.add_folder(parent, title).unwrap_or(parent);
            self.children(id, folder);
        } else if data.get("splitView").is_some() {
            // Arbalete has no split views yet: their tabs take their place.
            self.children(id, parent);
        }
    }
}

/// Arc stores its maps as arrays alternating keys and values.
fn pairs(array: &Value) -> impl Iterator<Item = (&Value, &Value)> {
    let array = array.as_array().map(Vec::as_slice).unwrap_or_default();
    array
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[key, value]| (key, value))
}

/// The directory of an Arc profile in `User Data`, as Chromium names it.
fn profile_directory(profile: &Value) -> Option<String> {
    if profile["default"] == true {
        return Some("Default".to_string());
    }
    let directory = profile["custom"]["_0"]["directoryBasename"].as_str()?;
    Some(directory.to_string())
}

fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.trim().is_empty())
}

/// A Space's theme from Arc's `windowTheme`: its single color, or its gradient's base colors then
/// its overlay's visible ones, with its translucency style and modifiers. Without one, the Space
/// has no color, as in Arc.
fn theme(window_theme: &Value) -> Theme {
    let style = &window_theme["background"]["single"]["_0"]["style"]["color"]["_0"];
    let (kind, config) = ["blendedGradient", "blendedSingleColor", "basicColor"]
        .into_iter()
        .map(|kind| (kind, &style[kind]["_0"]))
        .find(|(_, config)| config.is_object())
        .unwrap_or(("", &Value::Null));
    let colors: Vec<_> = match kind {
        "blendedGradient" => {
            let list = |key: &str| config[key].as_array().cloned().unwrap_or_default();
            let mut colors = list("baseColors");
            colors.extend(list("overlayColors"));
            colors
        }
        _ => vec![config["color"].clone()],
    };
    let colors = colors
        .iter()
        .filter(|color| color["alpha"].as_f64().unwrap_or(1.0) > 0.0)
        .filter_map(hex)
        .take(MAX_THEME_COLORS)
        .collect();
    let mode = match config["translucencyStyle"].as_str() {
        Some("light") => Mode::Light,
        Some("dark") => Mode::Dark,
        _ => Mode::Auto,
    };
    let factor = |key: &str, default: f64| {
        let value = config["modifiers"][key].as_f64().unwrap_or(default);
        value.clamp(0.0, 1.0)
    };
    Theme {
        colors,
        mode,
        intensity: factor("intensityFactor", 1.0),
        noise: factor("noiseFactor", 0.0),
    }
}

/// An Arc color as `#rrggbb`. Its components go from 0 to 1, a little beyond in extended sRGB.
fn hex(color: &Value) -> Option<String> {
    let rgb = ["red", "green", "blue"].map(|c| color[c].as_f64());
    let [Some(red), Some(green), Some(blue)] = rgb else {
        return None;
    };
    let byte = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(format!(
        "#{:02x}{:02x}{:02x}",
        byte(red),
        byte(green),
        byte(blue)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::NodeKind;
    use serde_json::json;

    const SIDEBAR: &str = include_str!("../../tests/fixtures/arc/StorableSidebar.json");

    #[test]
    fn reads_arcs_routes_and_default_destination() {
        let sidebar = Sidebar::default();
        let home = sidebar.spaces()[0].id;
        let spaces = HashMap::from([("space-home".to_string(), home)]);
        let json = json!({
            "rules": [
                { "sourceComponents": [{ "componentType": { "urlMatch": { "_0": { "contains": {} }, "_1": "meet.google.com" } } }],
                  "destination": { "space": { "_0": { "mostRecent": {} } } } },
                { "sourceComponents": [{ "componentType": { "urlMatch": { "_0": { "matches": {} }, "_1": "test.com" } } }],
                  "destination": { "littleArc": { "_0": { "mostRecent": {} } } } },
                { "sourceComponents": [{ "componentType": { "urlMatch": { "_0": { "contains": {} }, "_1": "foo.bar" } } }],
                  "destination": { "space": { "_0": { "space": { "_0": "space-other" } } } } },
                { "sourceComponents": [], "destination": { "littleArc": {} } }
            ],
            "version": 1,
            "defaultDestination": { "space": { "_0": { "space": { "_0": "space-home" } } } }
        });
        let routing = url_routing(&json.to_string(), &spaces).unwrap();
        let routes: Vec<_> = routing
            .routes
            .iter()
            .map(|r| (r.matching, r.text.as_str(), r.destination))
            .collect();
        assert_eq!(
            routes,
            [
                (
                    Match::Contains,
                    "meet.google.com",
                    Destination::MostRecentSpace
                ),
                (Match::IsEqualTo, "test.com", Destination::Little),
                // A Space not imported.
                (Match::Contains, "foo.bar", Destination::MostRecentSpace),
            ]
        );
        assert_eq!(routing.default, Destination::Space(home));
    }
    /// The fixture's two Arc profiles, each into a profile of its own.
    fn fixture() -> Sidebar {
        let mut sidebar = Sidebar::empty();
        for (directory, name) in [("Default", "Personal"), ("Profile 1", "Work")] {
            let profile = sidebar.add_profile(name);
            add(&mut sidebar, SIDEBAR, directory, profile).unwrap();
        }
        sidebar
    }

    /// A node's content, as urls and `folder[...]`, for comparisons that read like the sidebar.
    fn content(sidebar: &Sidebar, id: NodeId) -> Vec<String> {
        let node = sidebar.node(id).unwrap();
        node.children
            .iter()
            .map(|&child| match &sidebar.node(child).unwrap().kind {
                NodeKind::Tab(tab) => tab.url.clone(),
                NodeKind::Folder { title, .. } => {
                    format!("{title}[{}]", content(sidebar, child).join(", "))
                }
                NodeKind::Container | NodeKind::Split => panic!("no container or split imported"),
            })
            .collect()
    }

    #[test]
    fn imports_only_the_spaces_and_favorites_of_one_arc_profile_into_another() {
        let mut sidebar = Sidebar::default();
        let profile = sidebar.profiles()[0].id;
        add(&mut sidebar, SIDEBAR, "Profile 1", profile).unwrap();
        let titles: Vec<_> = sidebar.spaces().iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Home", "Office"]);
        assert_eq!(sidebar.spaces()[1].profile, profile);
        let favorites = sidebar.profiles()[0].favorites;
        assert_eq!(
            content(&sidebar, favorites),
            ["https://tracker.example.com/"]
        );
    }

    #[test]
    fn imports_spaces_in_order_with_their_profile_emoji_and_theme() {
        let sidebar = fixture();
        let [personal, work] = [0, 1].map(|i| sidebar.profiles()[i].id);
        let spaces: Vec<_> = sidebar
            .spaces()
            .iter()
            .map(|s| (s.title.as_str(), s.emoji.as_deref(), &s.theme, s.profile))
            .collect();
        let home = Theme {
            colors: vec!["#e64d52".into()],
            mode: Mode::Light,
            intensity: 1.0,
            noise: 0.5,
        };
        let none = Theme {
            colors: Vec::new(),
            mode: Mode::Auto,
            intensity: 1.0,
            noise: 0.0,
        };
        // The gradient's base colors, then its overlay's visible one; extended components are
        // brought back between 0 and 1.
        let office = Theme {
            colors: vec!["#407af7".into(), "#ff8000".into(), "#33cc66".into()],
            mode: Mode::Dark,
            intensity: 0.8,
            noise: 0.0,
        };
        assert_eq!(
            spaces,
            [
                ("Home", Some("🏡"), &home, personal),
                ("Space", None, &none, personal),
                ("Office", Some("💼"), &office, work),
            ]
        );
        assert_eq!(sidebar.displayed().title, "Home");
    }

    #[test]
    fn imports_the_favorites_of_each_profile() {
        let sidebar = fixture();
        let [personal, work] = [0, 1].map(|i| sidebar.profiles()[i].favorites);
        assert_eq!(
            content(&sidebar, personal),
            ["https://mail.example.com/", "https://calendar.example.com/"]
        );
        assert_eq!(content(&sidebar, work), ["https://tracker.example.com/"]);
        let tracker = sidebar.node(work).unwrap().children[0];
        let NodeKind::Tab(tab) = &sidebar.node(tracker).unwrap().kind else {
            panic!("not a tab")
        };
        assert_eq!(tab.title, "Tracker");
    }

    #[test]
    fn imports_pinned_tabs_and_nested_folders_with_their_saved_urls() {
        let mut sidebar = fixture();
        let home = sidebar.spaces()[0].clone();
        assert_eq!(
            content(&sidebar, home.pinned),
            [
                "Docs[https://docs.example.com/guide, Recipes[https://recipes.example.org/bread]]",
                "https://news.example.net/",
            ]
        );
        let news = sidebar.node(home.pinned).unwrap().children[1];
        let tab = sidebar.tab_mut(news).unwrap();
        assert_eq!(tab.title, "https://news.example.net/");
        assert_eq!(tab.saved_url.as_deref(), Some("https://news.example.net/"));
        let office = sidebar.spaces()[2].clone();
        assert_eq!(
            content(&sidebar, office.pinned),
            ["https://wiki.example.com/"]
        );
    }

    #[test]
    fn imports_today_tabs_with_split_views_as_separate_tabs() {
        let sidebar = fixture();
        let [home, reading] = [0, 1].map(|i| sidebar.spaces()[i].today);
        assert_eq!(
            content(&sidebar, home),
            [
                "https://search.example.com/?q=arbalete",
                "https://left.example.com/",
                "https://right.example.com/",
            ]
        );
        assert_eq!(
            content(&sidebar, reading),
            ["https://blog.example.org/post"]
        );
    }

    #[test]
    fn imports_a_folder_listed_inside_itself_once() {
        let json = json!({ "sidebar": { "containers": [{
            "topAppsContainerIDs": [{ "default": true }, "favorites"],
            "spaces": ["s", { "profile": { "default": true },
                "containerIDs": ["pinned", "pinned", "unpinned", "today"] }],
            "items": [
                "pinned", { "childrenIds": ["loop"] },
                "loop", { "title": "Loop", "childrenIds": ["loop", "pinned"], "data": { "list": {} } },
                "favorites", { "childrenIds": ["folder"] },
                "folder", { "childrenIds": ["tab"], "data": { "list": {} } },
                "tab", { "data": { "tab": { "savedURL": "https://example.com/" } } },
            ],
        }]}});
        let mut sidebar = Sidebar::empty();
        let profile = sidebar.add_profile("Default");
        add(&mut sidebar, &json.to_string(), "Default", profile).unwrap();
        let space = &sidebar.spaces()[0];
        assert_eq!(content(&sidebar, space.pinned), ["Loop[]"]);
        assert!(content(&sidebar, space.today).is_empty());
        let favorites = sidebar.profiles()[0].favorites;
        assert_eq!(content(&sidebar, favorites), ["https://example.com/"]);
    }

    #[test]
    fn finds_the_icon_arc_cached_for_an_item_else_the_largest_of_the_page() {
        let dir = crate::import::TestDir::new("arc-favicons");
        let json = json!({ "sidebar": { "containers": [{ "items": [
            "cached-tab", { "data": { "tab": { "savedURL": "https://a.example/" } } },
            "other-tab", { "data": { "tab": { "savedURL": "https://b.example/" } } },
            "unknown-tab", { "data": { "tab": { "savedURL": "https://c.example/" } } },
        ] }] } });
        // The md5 of "CACHED-TAB".
        dir.write(
            "SidebarItemsFaviconCache/0e0a3fea1eaecfa272e5ea443ca2f915",
            "cached",
        );
        dir.dir("User Data/Default");
        let db = Connection::open(dir.0.join("User Data/Default/Favicons")).unwrap();
        db.execute_batch(
            "CREATE TABLE icon_mapping (id INTEGER PRIMARY KEY, page_url TEXT, icon_id INTEGER);
             CREATE TABLE favicon_bitmaps (id INTEGER PRIMARY KEY, icon_id INTEGER, width INTEGER,
                 height INTEGER, image_data BLOB);
             INSERT INTO icon_mapping (page_url, icon_id) VALUES
                 ('https://a.example/', 1), ('https://b.example/', 2);
             INSERT INTO favicon_bitmaps (icon_id, width, height, image_data) VALUES
                 (1, 16, 16, CAST('a' AS BLOB)), (2, 16, 16, CAST('small' AS BLOB)),
                 (2, 32, 32, CAST('large' AS BLOB));",
        )
        .unwrap();
        drop(db);
        let favicons = favicons(&dir.0, &dir.0.join("User Data/Default"), &json.to_string());
        let mut favicons: Vec<_> = favicons.into_iter().collect();
        favicons.sort();
        assert_eq!(
            favicons,
            [
                ("https://a.example/".to_string(), b"cached".to_vec()),
                ("https://b.example/".to_string(), b"large".to_vec()),
            ]
        );
    }

    #[test]
    fn reads_the_sidebar_in_arc_s_data_directory() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/arc");
        assert_eq!(read(&dir).unwrap().as_deref(), Some(SIDEBAR));
        assert_eq!(read(&dir.join("missing")).unwrap(), None);
    }

    #[test]
    fn imports_the_tabs_archived_from_the_spaces_imported() {
        let mut sidebar = Sidebar::default();
        let profile = sidebar.profiles()[0].id;
        let spaces = add(&mut sidebar, SIDEBAR, "Profile 1", profile).unwrap();
        let office = spaces["space-office"];
        let archive = json!({ "version": 1, "items": [
            "a", { "archivedAt": 815_002_210.5, "source": { "space": { "_0": "space-office" } },
                   "sidebarItem": { "data": { "tab": { "savedURL": "https://a.example/", "savedTitle": "A" } } } },
            "b", { "archivedAt": 1.0, "source": { "space": { "_0": "space-home" } },
                   "sidebarItem": { "data": { "tab": { "savedURL": "https://b.example/" } } } },
            "c", { "archivedAt": 2.0, "source": { "littleArc": {} },
                   "sidebarItem": { "data": { "tab": { "savedURL": "https://c.example/" } } } },
        ] });
        add_archive(&mut sidebar, &archive.to_string(), &spaces).unwrap();
        let archived = sidebar.archive();
        assert_eq!(archived.len(), 1);
        assert_eq!(
            (archived[0].tab.url.as_str(), archived[0].tab.title.as_str()),
            ("https://a.example/", "A")
        );
        assert_eq!(archived[0].space, office);
        // 2026-10-29 at 23:30:10.5 UTC.
        assert_eq!(archived[0].archived_at, 1_793_309_410_500);
        assert!(add_archive(&mut sidebar, "not json", &spaces).is_err());
    }

    #[test]
    fn imports_nothing_from_a_file_without_spaces() {
        let mut sidebar = Sidebar::default();
        let profile = sidebar.profiles()[0].id;
        let invalid = add(&mut sidebar, "not json", "Default", profile);
        assert!(invalid.is_err());
        let empty = json!({ "sidebar": { "containers": [{ "global": {} }, { "spaces": [] }] } });
        for json in ["{}".to_string(), empty.to_string()] {
            add(&mut sidebar, &json, "Default", profile).unwrap();
        }
        assert_eq!(sidebar, Sidebar::default());
    }
}

//! The sidebar: profiles, Spaces and the tree of tabs and folders each Space holds.
//!
//! Every profile has a root container for its favorites, shared by the Spaces attached to it. Every
//! Space has two root containers: its pinned tabs and its "Today" tabs. Tabs, folders and splits are
//! nodes of a single tree, stored flat and linked by `parent` and `children`.

use crate::search::SearchEngine;
use crate::url_routing::{Destination, UrlRouting};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProfileId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpaceId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(u64);

/// Isolated web data (cookies, history, extensions), shared by the Spaces attached to it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    #[serde(default)]
    pub color: Color,
    pub favorites: NodeId,
    /// The user dismissed the box inviting to drag tabs into the empty favorites; adding a
    /// favorite brings it back for when they are empty again.
    #[serde(default)]
    pub favorites_hint_hidden: bool,
    /// What the Command Bar searches with, in this profile's Spaces.
    #[serde(default)]
    pub search_engine: SearchEngine,
    /// The Command Bar suggests searches as text is typed, from the search engine: what is typed
    /// goes to it.
    #[serde(default = "yes")]
    pub search_suggestions: bool,
    /// The extensions whose buttons show by the address, by id, in the order they were pinned.
    #[serde(default)]
    pub pinned_extensions: Vec<String>,
    /// How long its "Today" tabs stay unshown before they are archived.
    #[serde(default)]
    pub archive_after: ArchiveAfter,
    /// An incognito window's, with its one Space: listed nowhere, never saved, and gone with
    /// the window (`Sidebar::open_incognito`).
    #[serde(skip)]
    pub incognito: bool,
}

/// A setting on by default, for profiles saved before it existed.
fn yes() -> bool {
    true
}

/// How many favorites a profile holds at most: the grid keeps 3 rows of 4 tiles.
pub const MAX_FAVORITES: usize = 12;
/// A split shows at most this many tabs side by side, like Arc's Split View.
pub const MAX_SPLIT_TABS: usize = 4;

/// The colors a profile or a Space can take: a small palette, which the UI draws (ui/src/bridge.ts).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    #[default]
    Purple,
    Blue,
    Green,
    Yellow,
    Orange,
    Red,
    Pink,
    Gray,
}

impl Color {
    /// Its value as `#rrggbb`, which the UI's palette draws too (COLORS in ui/src/bridge.ts).
    pub fn hex(self) -> &'static str {
        match self {
            Color::Purple => "#6e56cf",
            Color::Blue => "#3e7bfa",
            Color::Green => "#30a46c",
            Color::Yellow => "#e5b800",
            Color::Orange => "#f76b15",
            Color::Red => "#e5484d",
            Color::Pink => "#d6409f",
            Color::Gray => "#8b8d98",
        }
    }
}

/// How a Space colors the window, like Arc's: up to three colors blended into a gradient, shown
/// over light or dark neutrals at an intensity, with grain over them. Without colors, the window
/// keeps its neutrals, without grain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    /// The first one leads, as `#rrggbb`.
    pub colors: Vec<String>,
    pub mode: Mode,
    /// How much the colors show over the neutrals, from 0 to 1.
    pub intensity: f64,
    /// How much grain shows, from 0 to 1.
    pub noise: f64,
}

/// Whether a theme takes the system's appearance or forces a light or dark one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Auto,
    Light,
    Dark,
}

/// A theme holds at most this many colors.
pub const MAX_THEME_COLORS: usize = 3;

impl Default for Theme {
    fn default() -> Self {
        Color::default().into()
    }
}

/// A palette color shows faintly, as Spaces showed their color before themes.
impl From<Color> for Theme {
    fn from(color: Color) -> Self {
        Theme {
            colors: vec![color.hex().to_string()],
            mode: Mode::Auto,
            intensity: 0.25,
            noise: 0.0,
        }
    }
}

impl Theme {
    /// Whether the UI can paint it: at most three `#rrggbb` colors, and factors from 0 to 1.
    fn is_valid(&self) -> bool {
        let hex = |color: &String| {
            color.len() == 7
                && color.starts_with('#')
                && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        };
        self.colors.len() <= MAX_THEME_COLORS
            && self.colors.iter().all(hex)
            && (0.0..=1.0).contains(&self.intensity)
            && (0.0..=1.0).contains(&self.noise)
    }
}

/// A theme, or a palette color, which Spaces saved before themes had in `color`.
fn theme_or_color<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Theme, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Saved {
        Color(Color),
        Theme(Theme),
    }
    Ok(match Saved::deserialize(deserializer)? {
        Saved::Color(color) => color.into(),
        Saved::Theme(theme) => theme,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Space {
    pub id: SpaceId,
    pub title: String,
    /// Shown in the Space switcher instead of the title's initial.
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default, alias = "color", deserialize_with = "theme_or_color")]
    pub theme: Theme,
    pub profile: ProfileId,
    pub pinned: NodeId,
    pub today: NodeId,
    /// The tab shown when the Space is displayed.
    pub selected: Option<NodeId>,
    /// The sidebar hides the Space's pinned tabs, like Arc's; "Today" tabs stay.
    #[serde(default)]
    pub collapsed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    /// A root: a profile's favorites, or a Space's pinned tabs or "Today" tabs.
    Container,
    Folder {
        title: String,
        expanded: bool,
    },
    Tab(Tab),
    /// From two to `MAX_SPLIT_TABS` tabs side by side in one row, its children; it never holds
    /// fewer. Like Arc's, it may be a favorite. Its layout is apart (`Sidebar::split_layout`).
    Split,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
    /// The tab selected when this one opened: closing this one returns to it.
    pub opener: Option<NodeId>,
    /// The url a pinned tab goes back to when it is closed.
    #[serde(default)]
    pub saved_url: Option<String>,
    /// The name the user gave the tab, shown instead of its page's title.
    #[serde(default)]
    pub name: Option<String>,
    /// The tab's page plays no sound.
    #[serde(default)]
    pub muted: bool,
    /// When the tab last showed, in milliseconds since the Unix epoch: unshown long enough, a
    /// "Today" tab is archived (Sidebar::archive_due).
    #[serde(default)]
    pub last_shown: Option<u64>,
}

/// How long a profile's "Today" tabs stay unshown before they are archived.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArchiveAfter {
    Hour,
    SixHours,
    #[default]
    TwelveHours,
    Day,
    Week,
    Month,
    Never,
}

impl ArchiveAfter {
    pub const ALL: [Self; 7] = [
        Self::Hour,
        Self::SixHours,
        Self::TwelveHours,
        Self::Day,
        Self::Week,
        Self::Month,
        Self::Never,
    ];

    /// The delay, in milliseconds, if tabs are archived at all.
    pub fn millis(self) -> Option<u64> {
        const HOUR: u64 = 60 * 60 * 1000;
        match self {
            Self::Hour => Some(HOUR),
            Self::SixHours => Some(6 * HOUR),
            Self::TwelveHours => Some(12 * HOUR),
            Self::Day => Some(24 * HOUR),
            Self::Week => Some(7 * 24 * HOUR),
            Self::Month => Some(30 * 24 * HOUR),
            Self::Never => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Hour => "1 hour",
            Self::SixHours => "6 hours",
            Self::TwelveHours => "12 hours",
            Self::Day => "24 hours",
            Self::Week => "7 days",
            Self::Month => "30 days",
            Self::Never => "Never",
        }
    }
}

/// A tab archived, which may be restored to its Space.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Archived {
    pub tab: Tab,
    pub space: SpaceId,
    /// When it was archived, in milliseconds since the Unix epoch.
    pub archived_at: u64,
}

/// How many archived tabs are kept: the oldest go first.
pub const ARCHIVE_SIZE: usize = 1000;

impl Tab {
    /// What the sidebar calls the tab: its name, or its page's title.
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.title)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    UnknownProfile,
    UnknownSpace,
    UnknownNode,
    /// Only containers and folders hold other nodes.
    NotAContainer,
    NotATab,
    NotAFolder,
    /// Only pinned tabs have a url to go back to.
    NotPinned,
    /// Folders, profiles and Spaces keep a title to be told apart.
    EmptyTitle,
    /// A folder cannot move inside itself or one of its descendants.
    MoveIntoItself,
    /// Root containers cannot be moved or removed.
    RootNode,
    /// The browser always keeps a Space to open tabs in.
    LastSpace,
    /// A split joins different tabs, none a favorite, and a tab joins one split at a time.
    NotSplittable,
    /// A split holds at most `MAX_SPLIT_TABS` tabs.
    SplitFull,
    /// A profile holds at most `MAX_FAVORITES` favorites.
    FavoritesFull,
    /// A theme holds at most `MAX_THEME_COLORS` `#rrggbb` colors, and factors from 0 to 1.
    InvalidTheme,
}

/// What a window shows of the sidebar: its displayed Space, each Space's selected tab and the
/// folders it collapsed, which another window, showing the same Spaces and tabs, keeps apart.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct View {
    displayed: Option<SpaceId>,
    selected: Vec<(SpaceId, Option<NodeId>)>,
    #[serde(default)]
    collapsed: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sidebar {
    profiles: Vec<Profile>,
    spaces: Vec<Space>,
    /// The Space the sidebar shows, `None` for the first one.
    #[serde(default)]
    displayed: Option<SpaceId>,
    nodes: HashMap<NodeId, Node>,
    next_id: u64,
    /// The tabs archived, the most recent first.
    #[serde(default)]
    archive: Vec<Archived>,
    /// Where the links other apps open go.
    #[serde(default)]
    url_routing: UrlRouting,
    /// How each split lays its tabs out, by split, when it differs from side by side in equal
    /// parts.
    #[serde(default)]
    splits: HashMap<NodeId, Layout>,
}

/// How a split lays its tabs out: along one axis, each taking its factor of the room.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Layout {
    /// One above the other, rather than side by side.
    stacked: bool,
    /// Each tab's part of the room, by tab, which `split_layout` scales to add up to 1.
    factors: HashMap<NodeId, f64>,
}

/// How a split shows its tabs, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitLayout {
    /// One above the other, rather than side by side.
    pub stacked: bool,
    /// Each tab's part of the room, adding up to 1.
    pub factors: Vec<f64>,
}

impl Default for Sidebar {
    /// A first profile and a first Space, so the browser always has somewhere to open tabs.
    fn default() -> Self {
        let mut sidebar = Self::empty();
        let profile = sidebar.add_profile("Default");
        sidebar
            .add_space("Home", profile)
            .expect("the profile exists");
        sidebar
    }
}

impl Sidebar {
    /// A sidebar without profiles or Spaces, for importers to fill: it must have a Space before
    /// it is used.
    pub fn empty() -> Self {
        Self {
            profiles: Vec::new(),
            spaces: Vec::new(),
            displayed: None,
            nodes: HashMap::new(),
            next_id: 1,
            archive: Vec::new(),
            url_routing: UrlRouting::default(),
            splits: HashMap::new(),
        }
    }

    /// Where the links other apps open go.
    pub fn url_routing(&self) -> &UrlRouting {
        &self.url_routing
    }

    /// Replaces the routes and the default destination; a Space it does not know becomes the
    /// Space shown last.
    pub fn set_url_routing(&mut self, mut routing: UrlRouting) {
        let known: Vec<_> = self.spaces.iter().map(|s| s.id).collect();
        let destinations = routing
            .routes
            .iter_mut()
            .map(|route| &mut route.destination);
        for destination in destinations.chain([&mut routing.default]) {
            if let Destination::Space(space) = destination
                && !known.contains(space)
            {
                *destination = Destination::MostRecentSpace;
            }
        }
        self.url_routing = routing;
    }

    /// The profiles, but the incognito windows'.
    pub fn profiles(&self) -> &[Profile] {
        &self.profiles[..self.listed_profiles()]
    }

    /// Spaces in sidebar order, but the incognito windows'.
    pub fn spaces(&self) -> &[Space] {
        &self.spaces[..self.listed_spaces()]
    }

    /// How many profiles are listed: the incognito ones come after them.
    fn listed_profiles(&self) -> usize {
        self.profiles
            .iter()
            .position(|p| p.incognito)
            .unwrap_or(self.profiles.len())
    }

    /// How many Spaces are listed: the incognito ones come after them.
    fn listed_spaces(&self) -> usize {
        self.spaces
            .iter()
            .position(|s| self.is_incognito(s.id))
            .unwrap_or(self.spaces.len())
    }

    /// Whether a Space is an incognito window's.
    pub fn is_incognito(&self, space: SpaceId) -> bool {
        self.space(space)
            .ok()
            .and_then(|s| self.profiles.iter().find(|p| p.id == s.profile))
            .is_some_and(|p| p.incognito)
    }

    /// Whether a window showing the Space `shown` may show the tab or Space `space`: an
    /// incognito window only its own, another none of the incognito windows'.
    pub fn reaches(&self, shown: SpaceId, space: SpaceId) -> bool {
        space == shown || !(self.is_incognito(shown) || self.is_incognito(space))
    }

    /// Opens an incognito window's profile and Space, after the others: the profile's web data
    /// is the app's to keep off the record.
    pub fn open_incognito(&mut self) -> SpaceId {
        let profile = ProfileId(self.next_id());
        let favorites = self.insert(None, NodeKind::Container);
        self.profiles.push(Profile {
            id: profile,
            name: "Incognito".to_string(),
            color: Color::Gray,
            favorites,
            favorites_hint_hidden: true,
            search_engine: SearchEngine::default(),
            search_suggestions: false,
            pinned_extensions: Vec::new(),
            archive_after: ArchiveAfter::Never,
            incognito: true,
        });
        let space = SpaceId(self.next_id());
        let pinned = self.insert(None, NodeKind::Container);
        let today = self.insert(None, NodeKind::Container);
        self.spaces.push(Space {
            id: space,
            title: "Incognito".to_string(),
            emoji: None,
            theme: Theme {
                colors: vec!["#2b2b2e".to_string()],
                mode: Mode::Dark,
                intensity: 0.8,
                noise: 0.0,
            },
            profile,
            pinned,
            today,
            selected: None,
            collapsed: false,
        });
        space
    }

    /// Forgets an incognito window's Space and profile, with their tabs, as the window closes.
    pub fn close_incognito(&mut self, space: SpaceId) -> Result<(), Error> {
        if !self.is_incognito(space) {
            return Err(Error::UnknownSpace);
        }
        let index = self.spaces.iter().position(|s| s.id == space);
        let space = self.spaces.remove(index.expect("a known Space"));
        let profile = self.profiles.iter().position(|p| p.id == space.profile);
        let profile = self.profiles.remove(profile.expect("a known profile"));
        if self.displayed == Some(space.id) {
            self.displayed = None;
        }
        let mut pending = vec![space.pinned, space.today, profile.favorites];
        while let Some(id) = pending.pop() {
            if let Some(node) = self.nodes.remove(&id) {
                self.splits.remove(&id);
                pending.extend(node.children);
            }
        }
        Ok(())
    }

    /// The sidebar to save: without the incognito windows' profiles, Spaces and tabs.
    pub fn saved(&self) -> Sidebar {
        let mut saved = self.clone();
        let incognito: Vec<_> = saved.spaces[saved.listed_spaces()..]
            .iter()
            .map(|s| s.id)
            .collect();
        for space in incognito {
            saved.close_incognito(space).expect("an incognito Space");
        }
        saved
    }

    /// The Space the sidebar shows: the first one until another is displayed.
    pub fn displayed(&self) -> &Space {
        self.displayed
            .and_then(|id| self.space(id).ok())
            .unwrap_or(&self.spaces[0])
    }

    /// Shows a Space in the sidebar, with the tab it had selected.
    pub fn display(&mut self, space: SpaceId) -> Result<(), Error> {
        self.space(space)?;
        self.displayed = Some(space);
        Ok(())
    }

    /// What the sidebar shows now, to show again with `set_view`.
    pub fn view(&self) -> View {
        View {
            displayed: self.displayed,
            selected: self.spaces.iter().map(|s| (s.id, s.selected)).collect(),
            collapsed: self
                .nodes
                .iter()
                .filter(|(_, node)| {
                    matches!(
                        node.kind,
                        NodeKind::Folder {
                            expanded: false,
                            ..
                        }
                    )
                })
                .map(|(id, _)| *id)
                .collect(),
        }
    }

    /// Shows what `view` holds, but the Spaces and tabs gone since; a Space it did not know
    /// shows no tab, a folder it did not know stays expanded.
    pub fn set_view(&mut self, view: &View) {
        for (id, node) in &mut self.nodes {
            if let NodeKind::Folder { expanded, .. } = &mut node.kind {
                *expanded = !view.collapsed.contains(id);
            }
        }
        self.displayed = view.displayed.filter(|&id| self.space(id).is_ok());
        let selected: HashMap<_, _> = view.selected.iter().copied().collect();
        let tabs: Vec<_> = self
            .spaces
            .iter()
            .map(|s| selected.get(&s.id).copied().flatten())
            .collect();
        for (space, tab) in self.spaces.iter_mut().zip(tabs) {
            space.selected = tab.filter(|tab| self.nodes.contains_key(tab));
        }
    }

    /// What a new window shows: the displayed Space, without a selected tab.
    pub fn new_window_view(&self) -> View {
        let mut view = self.view();
        let displayed = self.displayed().id;
        for (space, tab) in &mut view.selected {
            if *space == displayed {
                *tab = None;
            }
        }
        view
    }

    /// The profile a Space is attached to.
    pub fn profile_of_space(&self, space: SpaceId) -> Option<ProfileId> {
        self.space(space).ok().map(|s| s.profile)
    }

    /// What a window showing `space` shows: it, without a selected tab.
    pub fn view_showing(&self, space: SpaceId) -> View {
        let mut view = self.view();
        view.displayed = Some(space);
        for (id, tab) in &mut view.selected {
            if *id == space {
                *tab = None;
            }
        }
        view
    }

    /// The Space at `number` in sidebar order, counted from 1.
    pub fn space_at(&self, number: usize) -> Option<SpaceId> {
        self.spaces().get(number.checked_sub(1)?).map(|s| s.id)
    }

    /// The Space after the displayed one in sidebar order, or before it; none past the first or
    /// the last, as a swipe stops there.
    pub fn next_space(&self, forward: bool) -> Option<SpaceId> {
        // An incognito window shows only its Space.
        let index = self
            .spaces()
            .iter()
            .position(|s| s.id == self.displayed().id)?;
        let next = if forward {
            index + 1
        } else {
            index.checked_sub(1)?
        };
        self.spaces().get(next).map(|s| s.id)
    }

    /// The Space whose pinned or "Today" tabs hold a node; favorites belong to no Space.
    pub fn space_of(&self, id: NodeId) -> Option<SpaceId> {
        let root = self.ancestors(id).last()?;
        let space = self
            .spaces
            .iter()
            .find(|s| s.pinned == root || s.today == root);
        space.map(|s| s.id)
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Every tab of every profile, in no order.
    pub fn tabs(&self) -> impl Iterator<Item = (NodeId, &Tab)> {
        self.nodes.values().filter_map(|node| match &node.kind {
            NodeKind::Tab(tab) => Some((node.id, tab)),
            _ => None,
        })
    }

    pub fn tab_mut(&mut self, id: NodeId) -> Option<&mut Tab> {
        match &mut self.nodes.get_mut(&id)?.kind {
            NodeKind::Tab(tab) => Some(tab),
            _ => None,
        }
    }

    pub fn add_profile(&mut self, name: &str) -> ProfileId {
        let id = ProfileId(self.next_id());
        let favorites = self.insert(None, NodeKind::Container);
        // The listed profiles come first.
        let listed = self.listed_profiles();
        self.profiles.insert(
            listed,
            Profile {
                id,
                name: name.to_string(),
                color: Color::default(),
                favorites,
                favorites_hint_hidden: false,
                search_engine: SearchEngine::default(),
                search_suggestions: true,
                pinned_extensions: Vec::new(),
                archive_after: ArchiveAfter::default(),
                incognito: false,
            },
        );
        id
    }

    /// Creates a profile with a Space of the same name, so the profile has somewhere to open tabs.
    pub fn create_profile(&mut self, name: &str) -> Result<ProfileId, Error> {
        if name.trim().is_empty() {
            return Err(Error::EmptyTitle);
        }
        let profile = self.add_profile(name);
        self.add_space(name, profile)?;
        Ok(profile)
    }

    pub fn rename_profile(&mut self, id: ProfileId, name: &str) -> Result<(), Error> {
        if name.trim().is_empty() {
            return Err(Error::EmptyTitle);
        }
        self.profile_mut(id)?.name = name.to_string();
        Ok(())
    }

    pub fn set_profile_color(&mut self, id: ProfileId, color: Color) -> Result<(), Error> {
        self.profile_mut(id)?.color = color;
        Ok(())
    }

    pub fn set_search_engine(&mut self, id: ProfileId, engine: SearchEngine) -> Result<(), Error> {
        self.profile_mut(id)?.search_engine = engine;
        Ok(())
    }

    /// Lets a profile's Command Bar suggest searches from its search engine, or not.
    pub fn set_search_suggestions(&mut self, id: ProfileId, on: bool) -> Result<(), Error> {
        self.profile_mut(id)?.search_suggestions = on;
        Ok(())
    }

    /// Pins an extension of a profile, its button then showing by the address, or unpins it.
    pub fn pin_extension(
        &mut self,
        id: ProfileId,
        extension: &str,
        pinned: bool,
    ) -> Result<(), Error> {
        let list = &mut self.profile_mut(id)?.pinned_extensions;
        list.retain(|pinned| pinned != extension);
        if pinned {
            list.push(extension.to_string());
        }
        Ok(())
    }

    pub fn add_space(&mut self, title: &str, profile: ProfileId) -> Result<SpaceId, Error> {
        if !self.profiles.iter().any(|p| p.id == profile) {
            return Err(Error::UnknownProfile);
        }
        let id = SpaceId(self.next_id());
        let pinned = self.insert(None, NodeKind::Container);
        let today = self.insert(None, NodeKind::Container);
        // The listed Spaces come first.
        let listed = self.listed_spaces();
        self.spaces.insert(
            listed,
            Space {
                id,
                title: title.to_string(),
                emoji: None,
                theme: Theme::default(),
                profile,
                pinned,
                today,
                selected: None,
                collapsed: false,
            },
        );
        Ok(id)
    }

    /// Creates a Space at the end of the switcher and displays it.
    pub fn create_space(&mut self, title: &str, profile: ProfileId) -> Result<SpaceId, Error> {
        if title.trim().is_empty() {
            return Err(Error::EmptyTitle);
        }
        let space = self.add_space(title, profile)?;
        self.display(space)?;
        Ok(space)
    }

    pub fn rename_space(&mut self, id: SpaceId, title: &str) -> Result<(), Error> {
        if title.trim().is_empty() {
            return Err(Error::EmptyTitle);
        }
        self.space_mut(id)?.title = title.to_string();
        Ok(())
    }

    /// Sets the Space's emoji; a blank one removes it, so the switcher shows the initial again.
    pub fn set_space_emoji(&mut self, id: SpaceId, emoji: &str) -> Result<(), Error> {
        let emoji = emoji.trim();
        self.space_mut(id)?.emoji = (!emoji.is_empty()).then(|| emoji.to_string());
        Ok(())
    }

    pub fn set_space_theme(&mut self, id: SpaceId, theme: Theme) -> Result<(), Error> {
        if !theme.is_valid() {
            return Err(Error::InvalidTheme);
        }
        self.space_mut(id)?.theme = theme;
        Ok(())
    }

    /// Hides the Space's pinned tabs in the sidebar, or shows them again.
    pub fn toggle_space(&mut self, id: SpaceId) -> Result<(), Error> {
        let space = self.space_mut(id)?;
        space.collapsed = !space.collapsed;
        Ok(())
    }

    /// Attaches a Space to another profile, whose web data and favorites it then shows.
    pub fn set_space_profile(&mut self, id: SpaceId, profile: ProfileId) -> Result<(), Error> {
        self.profile_mut(profile)?;
        self.space_mut(id)?.profile = profile;
        Ok(())
    }

    /// Moves a Space to `index` in sidebar order, counted once it has left its place; past the
    /// end, it goes last. The displayed Space stays displayed, though it may no longer be first.
    pub fn move_space(&mut self, id: SpaceId, index: usize) -> Result<(), Error> {
        self.displayed = Some(self.displayed().id);
        let from = self
            .spaces
            .iter()
            .position(|s| s.id == id)
            .ok_or(Error::UnknownSpace)?;
        if self.is_incognito(id) {
            return Err(Error::UnknownSpace);
        }
        let space = self.spaces.remove(from);
        self.spaces.insert(index.min(self.listed_spaces()), space);
        Ok(())
    }

    /// Deletes a Space with its tabs and folders, but not the last one. Its neighbor, the next one
    /// or else the previous one, is displayed instead.
    pub fn delete_space(&mut self, id: SpaceId) -> Result<(), Error> {
        let index = self
            .spaces
            .iter()
            .position(|s| s.id == id)
            .ok_or(Error::UnknownSpace)?;
        if self.is_incognito(id) {
            return Err(Error::UnknownSpace);
        }
        if self.listed_spaces() == 1 {
            return Err(Error::LastSpace);
        }
        if self.displayed().id == id {
            let next = if index + 1 < self.listed_spaces() {
                index + 1
            } else {
                index - 1
            };
            self.displayed = Some(self.spaces[next].id);
        }
        let space = self.spaces.remove(index);
        self.url_routing.forget_space(id);
        let mut pending = vec![space.pinned, space.today];
        while let Some(id) = pending.pop() {
            let node = self.nodes.remove(&id).expect("children are known nodes");
            pending.extend(node.children);
        }
        Ok(())
    }

    /// Opens a tab at the end of the Space's "Today" tabs, from the Space's selected tab.
    pub fn open_tab(&mut self, space: SpaceId, url: &str) -> Result<NodeId, Error> {
        let space = self.space(space)?;
        let today = space.today;
        let tab = Tab {
            url: url.to_string(),
            title: url.to_string(),
            favicon: None,
            opener: space.selected,
            saved_url: None,
            name: None,
            muted: false,
            last_shown: None,
        };
        Ok(self.insert(Some(today), NodeKind::Tab(tab)))
    }

    /// Shows `tab` in `space`. A favorite is shown in whichever Space it is selected from.
    pub fn select(&mut self, space: SpaceId, tab: NodeId) -> Result<(), Error> {
        self.check_tab(tab)?;
        let space = self.spaces.iter_mut().find(|s| s.id == space);
        space.ok_or(Error::UnknownSpace)?.selected = Some(tab);
        Ok(())
    }

    /// Adds a tab at the end of `parent`; a pinned tab saves its url, as when it is pinned.
    pub fn add_tab(&mut self, parent: NodeId, url: &str, title: &str) -> Result<NodeId, Error> {
        self.check_container(parent)?;
        self.check_room(parent)?;
        let tab = Tab {
            url: url.to_string(),
            title: title.to_string(),
            favicon: None,
            opener: None,
            saved_url: self.is_pinned(parent).then(|| url.to_string()),
            name: None,
            muted: false,
            last_shown: None,
        };
        self.show_favorites_hint(parent);
        Ok(self.insert(Some(parent), NodeKind::Tab(tab)))
    }

    /// Adds a new tab with `tab`'s url, title and icon at `index` among `parent`'s children, as
    /// `add_tab` adds one.
    pub fn duplicate(
        &mut self,
        tab: NodeId,
        parent: NodeId,
        index: usize,
    ) -> Result<NodeId, Error> {
        let Some(NodeKind::Tab(original)) = self.nodes.get(&tab).map(|node| &node.kind) else {
            return Err(Error::NotATab);
        };
        let Tab {
            url,
            title,
            favicon,
            ..
        } = original.clone();
        let copy = self.add_tab(parent, &url, &title)?;
        self.tab_mut(copy).expect("just added").favicon = favicon;
        let children = self.children_mut(parent);
        children.pop();
        children.insert(index.min(children.len()), copy);
        Ok(copy)
    }

    /// Adds a folder at the end of `parent`, which cannot be favorites: they hold only tabs.
    pub fn add_folder(&mut self, parent: NodeId, title: &str) -> Result<NodeId, Error> {
        self.check_container(parent)?;
        if self.profiles.iter().any(|p| p.favorites == parent) {
            return Err(Error::NotATab);
        }
        let folder = NodeKind::Folder {
            title: title.to_string(),
            expanded: true,
        };
        Ok(self.insert(Some(parent), folder))
    }

    pub fn rename_folder(&mut self, id: NodeId, new_title: &str) -> Result<(), Error> {
        if new_title.trim().is_empty() {
            return Err(Error::EmptyTitle);
        }
        let (title, _) = self.folder_mut(id)?;
        *title = new_title.to_string();
        Ok(())
    }

    /// Renames a folder, or a tab: a blank name gives a tab its page's title back.
    pub fn rename(&mut self, id: NodeId, name: &str) -> Result<(), Error> {
        if self.folder_mut(id).is_ok() {
            return self.rename_folder(id, name);
        }
        let tab = self.tab_mut(id).ok_or(Error::NotATab)?;
        let name = name.trim();
        tab.name = (!name.is_empty()).then(|| name.to_string());
        Ok(())
    }

    /// Changes the url a pinned tab goes back to when it is closed.
    pub fn set_saved_url(&mut self, tab: NodeId, url: &str) -> Result<(), Error> {
        self.check_tab(tab)?;
        if !self.is_pinned(tab) {
            return Err(Error::NotPinned);
        }
        let url = url.trim();
        if url.is_empty() {
            return Err(Error::EmptyTitle);
        }
        self.tab_mut(tab).expect("a tab").saved_url = Some(url.to_string());
        Ok(())
    }

    /// Mutes a tab, or gives it its sound back, and returns whether it is muted.
    pub fn toggle_mute(&mut self, tab: NodeId) -> Result<bool, Error> {
        let tab = self.tab_mut(tab).ok_or(Error::NotATab)?;
        tab.muted = !tab.muted;
        Ok(tab.muted)
    }

    /// Collapses an expanded folder, or expands a collapsed one.
    pub fn toggle_folder(&mut self, id: NodeId) -> Result<(), Error> {
        let (_, expanded) = self.folder_mut(id)?;
        *expanded = !*expanded;
        Ok(())
    }

    /// Moves a tab or a folder, with its content, to `index` among `parent`'s children. Tabs that
    /// become pinned save their url, tabs that stop being pinned forget it. Favorites hold only
    /// tabs and splits.
    pub fn move_node(&mut self, id: NodeId, parent: NodeId, index: usize) -> Result<(), Error> {
        let old_parent = self.movable(id)?;
        self.check_container(parent)?;
        if self.ancestors(parent).any(|ancestor| ancestor == id) {
            return Err(Error::MoveIntoItself);
        }
        if !self.is_tab(id)
            && !self.is_split(id)
            && self.profiles.iter().any(|p| p.favorites == parent)
        {
            return Err(Error::NotATab);
        }
        if parent != old_parent {
            self.check_room(parent)?;
        }
        self.place(id, old_parent, parent, index);
        Ok(())
    }

    /// Hides the box inviting to drag tabs into the empty favorites of the Space's profile.
    pub fn hide_favorites_hint(&mut self, space: SpaceId) -> Result<(), Error> {
        let profile = self.space(space)?.profile;
        self.profile_mut(profile)?.favorites_hint_hidden = true;
        Ok(())
    }

    /// Puts `tab` beside `target` in a split taking `target`'s place, `target` first, or right
    /// after `target` in the split it is in, with an equal part of the room, like Arc's. A tab
    /// moved out of another split leaves it, and a split left with one tab becomes that tab.
    pub fn split(&mut self, tab: NodeId, target: NodeId) -> Result<NodeId, Error> {
        self.split_beside(tab, target, false)
    }

    /// Puts `tab` beside `target` in a split as `split` does, but before it with `before`.
    pub fn split_beside(
        &mut self,
        tab: NodeId,
        target: NodeId,
        before: bool,
    ) -> Result<NodeId, Error> {
        self.check_tab(tab)?;
        self.check_tab(target)?;
        let parent = self.movable(target)?;
        if tab == target || self.is_favorite(target) || self.row_of(tab) == self.row_of(target) {
            return Err(Error::NotSplittable);
        }
        if self.is_split(parent) {
            return self.join(tab, parent, target, before);
        }
        let index = self.nodes[&parent]
            .children
            .iter()
            .position(|&id| id == target);
        let split = self.insert(None, NodeKind::Split);
        self.nodes.get_mut(&split).expect("just inserted").parent = Some(parent);
        self.children_mut(parent)
            .insert(index.expect("a child of its parent"), split);
        self.place(target, parent, split, 0);
        let from = self.movable(tab)?;
        self.place(tab, from, split, usize::from(!before));
        Ok(split)
    }

    /// Adds `tab` to `split`, right after `target`, or right before it with `before`, with an
    /// equal part of the room: the other tabs give it up in proportion to theirs.
    fn join(
        &mut self,
        tab: NodeId,
        split: NodeId,
        target: NodeId,
        before: bool,
    ) -> Result<NodeId, Error> {
        let count = self.nodes[&split].children.len() + 1;
        if count > MAX_SPLIT_TABS {
            return Err(Error::SplitFull);
        }
        let factors = self.split_layout(split).factors;
        let index = self.nodes[&split]
            .children
            .iter()
            .position(|&id| id == target);
        let from = self.movable(tab)?;
        let index = index.expect("a tab of the split") + usize::from(!before);
        self.place(tab, from, split, index);
        let share = 1.0 / count as f64;
        let children = self.nodes[&split].children.clone();
        let mut others = factors.into_iter();
        let layout = self.splits.entry(split).or_default();
        layout.factors = children
            .into_iter()
            .map(|id| {
                let factor = if id == tab {
                    share
                } else {
                    others.next().unwrap_or(share) * (1.0 - share)
                };
                (id, factor)
            })
            .collect();
        Ok(split)
    }

    /// How `split` lays its tabs out, in order: side by side in equal parts unless changed.
    pub fn split_layout(&self, split: NodeId) -> SplitLayout {
        let children = self
            .nodes
            .get(&split)
            .map_or(&[][..], |node| &node.children[..]);
        let layout = self.splits.get(&split);
        let equal = 1.0 / children.len().max(1) as f64;
        let factors: Vec<f64> = children
            .iter()
            .map(|id| {
                layout
                    .and_then(|layout| layout.factors.get(id))
                    .copied()
                    .filter(|factor| factor.is_finite() && *factor > 0.0)
                    .unwrap_or(equal)
            })
            .collect();
        let sum: f64 = factors.iter().sum();
        SplitLayout {
            stacked: layout.is_some_and(|layout| layout.stacked),
            factors: factors.iter().map(|factor| factor / sum).collect(),
        }
    }

    /// Gives `split`'s tabs these parts of the room, in order, scaled to add up to 1.
    pub fn set_split_factors(&mut self, split: NodeId, factors: &[f64]) -> Result<(), Error> {
        let children = self.split_children(split)?;
        if factors.len() != children.len() || factors.iter().any(|f| !f.is_finite() || *f <= 0.0) {
            return Err(Error::NotSplittable);
        }
        let sum: f64 = factors.iter().sum();
        let layout = self.splits.entry(split).or_default();
        layout.factors = children
            .into_iter()
            .zip(factors.iter().map(|f| f / sum))
            .collect();
        Ok(())
    }

    /// Lays `split`'s tabs out one above the other, or side by side again.
    pub fn set_split_stacked(&mut self, split: NodeId, stacked: bool) -> Result<(), Error> {
        self.split_children(split)?;
        self.splits.entry(split).or_default().stacked = stacked;
        Ok(())
    }

    fn split_children(&self, split: NodeId) -> Result<Vec<NodeId>, Error> {
        if !self.is_split(split) {
            return Err(Error::UnknownNode);
        }
        Ok(self.nodes[&split].children.clone())
    }

    /// Puts a copy of `tab` beside `target` in a split, as `split` puts `tab` itself.
    pub fn split_duplicate(
        &mut self,
        tab: NodeId,
        target: NodeId,
        before: bool,
    ) -> Result<NodeId, Error> {
        let parent = self.movable(target)?;
        let copy = self.duplicate(tab, parent, usize::MAX)?;
        self.split_beside(copy, target, before).inspect_err(|_| {
            self.remove(copy).expect("just added");
        })
    }

    /// The row a tab shows in: its split, if any, else the tab itself.
    pub fn row_of(&self, tab: NodeId) -> NodeId {
        let parent = self.nodes.get(&tab).and_then(|node| node.parent);
        parent.filter(|&p| self.is_split(p)).unwrap_or(tab)
    }

    /// The tab of `tab`'s split that `split_beside` puts it back beside, once it has left: the
    /// tab before it, or the one after it, `true`, when it comes first. None outside a split.
    pub fn split_neighbor(&self, tab: NodeId) -> Option<(NodeId, bool)> {
        let split = self.row_of(tab);
        let tabs = &self.nodes.get(&split).filter(|_| split != tab)?.children;
        let index = tabs.iter().position(|&id| id == tab)?;
        match index.checked_sub(1) {
            Some(before) => Some((tabs[before], false)),
            None => tabs.get(1).map(|&after| (after, true)),
        }
    }

    /// The tab to show once `tab` is closed: the tab it was opened from if still open, else the
    /// next tab in its container, else the previous one.
    pub fn next_after_close(&self, tab: NodeId) -> Option<NodeId> {
        if let Some(opener) = self.open_opener(tab) {
            return Some(opener);
        }
        let siblings = &self.nodes.get(&self.nodes.get(&tab)?.parent?)?.children;
        let tabs: Vec<_> = siblings
            .iter()
            .copied()
            .filter(|&id| id == tab || self.is_tab(id))
            .collect();
        let index = tabs.iter().position(|&id| id == tab)?;
        tabs.get(index + 1)
            .or(index.checked_sub(1).and_then(|i| tabs.get(i)))
            .copied()
    }

    /// The tab after the Space's selected one, or before it, in the order the sidebar shows tabs:
    /// pinned tabs, those of expanded folders included, then "Today" tabs, wrapping around at the
    /// ends. Favorites and tabs hidden in collapsed folders are left out; from one of them, or
    /// from no selection, the first tab, or the last.
    pub fn next_tab(&self, space: SpaceId, forward: bool) -> Option<NodeId> {
        let space = self.space(space).ok()?;
        let mut tabs = Vec::new();
        self.push_visible_tabs(space.pinned, &mut tabs);
        self.push_visible_tabs(space.today, &mut tabs);
        if !forward {
            tabs.reverse();
        }
        let selected = space.selected;
        let next = tabs
            .iter()
            .position(|&id| Some(id) == selected)
            .map_or(0, |i| i + 1);
        tabs.get(next).or(tabs.first()).copied()
    }

    /// The favorite at `number`, counted from 1, of the Space's profile; 9 is the last one, as
    /// ⌘9 is in browsers.
    pub fn favorite(&self, space: SpaceId, number: usize) -> Option<NodeId> {
        let favorites = &self.nodes[&self.favorites_of(space)?].children;
        if number == 9 {
            favorites.last().copied()
        } else {
            favorites.get(number.checked_sub(1)?).copied()
        }
    }

    /// The favorites of the Space's profile, which its Space shows.
    pub fn favorites_of(&self, space: SpaceId) -> Option<NodeId> {
        let profile = self.space(space).ok()?.profile;
        let profile = self.profiles.iter().find(|p| p.id == profile)?;
        Some(profile.favorites)
    }

    /// The profile whose web data shows a node: the profile of its favorites, or of its Space.
    pub fn profile_of(&self, id: NodeId) -> Option<ProfileId> {
        let root = self.ancestors(id).last()?;
        if let Some(profile) = self.profiles.iter().find(|p| p.favorites == root) {
            return Some(profile.id);
        }
        Some(self.space(self.space_of(id)?).ok()?.profile)
    }

    pub fn is_favorite(&self, id: NodeId) -> bool {
        // A tab of a favorite split is a favorite too.
        let parent = self
            .nodes
            .get(&self.row_of(id))
            .and_then(|node| node.parent);
        self.profiles.iter().any(|p| Some(p.favorites) == parent)
    }

    /// Whether `id` is among a Space's pinned tabs, directly or in a folder.
    pub fn is_pinned(&self, id: NodeId) -> bool {
        self.spaces
            .iter()
            .any(|space| self.ancestors(id).any(|ancestor| ancestor == space.pinned))
    }

    /// Pins a "Today" tab at the end of the Space's pinned tabs, or moves a pinned tab back to the
    /// top of "Today". Favorites stay where they are.
    pub fn toggle_pin(&mut self, space: SpaceId, tab: NodeId) -> Result<(), Error> {
        let (pinned, today) = self.space(space).map(|s| (s.pinned, s.today))?;
        if !self.is_tab(tab) {
            return Err(Error::NotATab);
        }
        let row = self.row_of(tab);
        if self.ancestors(tab).any(|id| id == today) {
            self.move_node(row, pinned, usize::MAX)
        } else if self.ancestors(tab).any(|id| id == pinned) {
            self.move_node(row, today, 0)
        } else {
            Ok(())
        }
    }

    /// Moves a tab, with the split it is in, to the end of another Space's "Today" tabs, unpinned,
    /// or a folder, with what it holds, to the end of its pinned tabs, and deselects what moved
    /// in the Space it leaves. Favorites belong to no Space: they stay where they are, like a
    /// node moved to its own Space.
    pub fn move_to_space(&mut self, id: NodeId, space: SpaceId) -> Result<(), Error> {
        let (pinned, today) = self.space(space).map(|s| (s.pinned, s.today))?;
        // Nothing goes in or out of an incognito window.
        let from = self.space_of(id);
        if self.is_incognito(space) || from.is_some_and(|from| self.is_incognito(from)) {
            return Err(Error::UnknownSpace);
        }
        let (row, to) = if self.folder_mut(id).is_ok() {
            (id, pinned)
        } else {
            self.check_tab(id)?;
            (self.row_of(id), today)
        };
        let Some(from) = self.space_of(id).filter(|&from| from != space) else {
            return Ok(());
        };
        self.move_node(row, to, usize::MAX)?;
        let selected = self.space(from)?.selected;
        if selected.is_some_and(|selected| self.ancestors(selected).any(|id| id == row)) {
            self.space_mut(from)?.selected = None;
        }
        Ok(())
    }

    /// The tabs inside a folder, those of the folders it holds included, in the sidebar's order.
    pub fn folder_tabs(&self, folder: NodeId) -> Result<Vec<NodeId>, Error> {
        match self.nodes.get(&folder).map(|n| &n.kind) {
            None => return Err(Error::UnknownNode),
            Some(NodeKind::Folder { .. }) => {}
            Some(_) => return Err(Error::NotAFolder),
        }
        let mut tabs = Vec::new();
        let mut pending = vec![folder];
        while let Some(id) = pending.pop() {
            let node = &self.nodes[&id];
            if self.is_tab(id) {
                tabs.push(id);
            }
            pending.extend(node.children.iter().rev());
        }
        Ok(tabs)
    }

    /// Favorites and pinned tabs stay when closed: closing one deselects it in `space`, takes it
    /// out of its split, if any, and back to its saved url, if any. Returns the tab to show instead: the tab it was opened
    /// from if still open, else the Space's last "Today" tab, rather than a neighbor it stays with.
    pub fn close_in_place(&mut self, space: SpaceId, tab: NodeId) -> Result<Option<NodeId>, Error> {
        self.space(space)?;
        let opener = self.open_opener(tab);
        self.separate(tab)?;
        let space = self.spaces.iter_mut().find(|s| s.id == space);
        let space = space.ok_or(Error::UnknownSpace)?;
        if space.selected == Some(tab) {
            space.selected = None;
        }
        let today = space.today;
        // Like Arc's, it keeps its title until its page shows again.
        if let Some(tab) = self.tab_mut(tab)
            && let Some(saved_url) = &tab.saved_url
        {
            tab.url = saved_url.clone();
        }
        let today = &self.nodes[&today].children;
        Ok(opener.or_else(|| today.iter().rev().copied().find(|&id| self.is_tab(id))))
    }

    /// Removes everything among a Space's "Today" tabs, folders and splits included, and returns
    /// the tabs removed, in the order the sidebar shows them, so they may be reopened.
    pub fn clear_today(&mut self, space: SpaceId) -> Result<Vec<Tab>, Error> {
        let today = self.space(space)?.today;
        let rows = self.nodes[&today].children.clone();
        self.remove_rows(&rows)
    }

    /// Notes that a tab showed at `now`, in milliseconds since the Unix epoch.
    pub fn touch(&mut self, tab: NodeId, now: u64) {
        if let Some(tab) = self.tab_mut(tab) {
            tab.last_shown = Some(now);
        }
    }

    /// Archives the "Today" tabs left unshown longer than their profile allows, at `now`: the
    /// tab a Space shows stays, and so do folders and splits. A tab never shown since it was
    /// opened or loaded counts from now. Returns how many were archived.
    pub fn archive_due(&mut self, now: u64) -> usize {
        let mut due = Vec::new();
        let mut unseen = Vec::new();
        for space in &self.spaces {
            let profile = self.profiles.iter().find(|p| p.id == space.profile);
            let Some(delay) = profile.and_then(|p| p.archive_after.millis()) else {
                continue;
            };
            for &row in &self.nodes[&space.today].children {
                let NodeKind::Tab(tab) = &self.nodes[&row].kind else {
                    continue;
                };
                match tab.last_shown {
                    _ if space.selected == Some(row) => {}
                    None => unseen.push(row),
                    Some(shown) if now.saturating_sub(shown) >= delay => due.push((space.id, row)),
                    Some(_) => {}
                }
            }
        }
        for tab in unseen {
            self.touch(tab, now);
        }
        for &(space, row) in &due {
            let Some(tab) = self.tab_mut(row).map(|tab| tab.clone()) else {
                continue;
            };
            if self.remove(row).is_ok() {
                self.archive.insert(
                    0,
                    Archived {
                        tab,
                        space,
                        archived_at: now,
                    },
                );
            }
        }
        self.archive.truncate(ARCHIVE_SIZE);
        due.len()
    }

    /// The tabs archived, the most recent first.
    pub fn archive(&self) -> &[Archived] {
        &self.archive
    }

    /// Adds tabs archived elsewhere, like in Arc, keeping the most recent first.
    pub fn add_archived(&mut self, items: Vec<Archived>) {
        self.archive.extend(items);
        self.archive
            .sort_by_key(|archived| std::cmp::Reverse(archived.archived_at));
        self.archive.truncate(ARCHIVE_SIZE);
    }

    /// Opens the archived tab at `index` again at the end of its Space's "Today" tabs, or of the
    /// displayed Space's when its own is gone, as shown at `now`. Returns its Space and its id.
    pub fn restore(&mut self, index: usize, now: u64) -> Result<(SpaceId, NodeId), Error> {
        if index >= self.archive.len() {
            return Err(Error::UnknownNode);
        }
        let archived = self.archive.remove(index);
        let space = match self.space(archived.space) {
            Ok(space) => space.id,
            Err(_) => self.displayed().id,
        };
        let today = self.space(space)?.today;
        let tab = Tab {
            last_shown: Some(now),
            opener: None,
            ..archived.tab
        };
        Ok((space, self.insert(Some(today), NodeKind::Tab(tab))))
    }

    /// Changes how long a profile's "Today" tabs stay unshown before they are archived.
    pub fn set_archive_after(&mut self, id: ProfileId, after: ArchiveAfter) -> Result<(), Error> {
        self.profile_mut(id)?.archive_after = after;
        Ok(())
    }

    /// Removes the rows after `tab`'s among its parent's, as `clear_today` removes them.
    pub fn remove_below(&mut self, tab: NodeId) -> Result<Vec<Tab>, Error> {
        self.check_tab(tab)?;
        let row = self.row_of(tab);
        let siblings = &self.nodes[&self.movable(row)?].children;
        let index = siblings.iter().position(|&id| id == row);
        let rows = siblings[index.expect("a child of its parent") + 1..].to_vec();
        self.remove_rows(&rows)
    }

    /// Removes a split with its tabs, as `clear_today` removes rows.
    pub fn remove_split(&mut self, split: NodeId) -> Result<Vec<Tab>, Error> {
        self.split_children(split)?;
        self.remove_rows(&[split])
    }

    /// Takes every tab out of a split, as rows in its place, in its order.
    pub fn separate_all(&mut self, split: NodeId) -> Result<(), Error> {
        for tab in self.split_children(split)?.into_iter().rev() {
            self.separate(tab)?;
        }
        Ok(())
    }

    /// Removes a folder with what it holds, as `clear_today` removes rows.
    pub fn remove_folder(&mut self, folder: NodeId) -> Result<Vec<Tab>, Error> {
        self.folder_mut(folder)?;
        self.remove_rows(&[folder])
    }

    /// Makes a folder a Space of its own, named after it, in its Space's profile, and displays
    /// it: what the folder held becomes the new Space's pinned tabs, the tab selected among them
    /// its selected one.
    pub fn folder_to_space(&mut self, folder: NodeId) -> Result<SpaceId, Error> {
        let title = self.folder_mut(folder)?.0.clone();
        let from = self.space_of(folder).ok_or(Error::UnknownNode)?;
        let profile = self.space(from)?.profile;
        let space = self.create_space(&title, profile)?;
        let pinned = self.space(space)?.pinned;
        for id in self.nodes[&folder].children.clone() {
            self.move_node(id, pinned, usize::MAX)?;
        }
        let selected = self.space(from)?.selected;
        if selected.is_some_and(|tab| self.space_of(tab) == Some(space)) {
            self.space_mut(from)?.selected = None;
            self.space_mut(space)?.selected = selected;
        }
        self.remove(folder)?;
        Ok(space)
    }

    /// Copies a tab, a folder with what it holds, or a tab's split, right after it, and returns
    /// the copy. A tab of a split is copied alone, after its split.
    pub fn duplicate_below(&mut self, id: NodeId) -> Result<NodeId, Error> {
        let row = if self.is_tab(id) { self.row_of(id) } else { id };
        let parent = self.movable(row)?;
        if self.is_tab(id) {
            self.check_room(parent)?;
        }
        let copy = self.copy_into(id, parent);
        let children = self.children_mut(parent);
        children.pop();
        let index = children.iter().position(|&child| child == row);
        children.insert(index.expect("a child of its parent") + 1, copy);
        Ok(copy)
    }

    /// Adds a copy of a node and what it holds at the end of `parent`, its tabs opened from none.
    fn copy_into(&mut self, id: NodeId, parent: NodeId) -> NodeId {
        let mut kind = self.nodes[&id].kind.clone();
        if let NodeKind::Tab(tab) = &mut kind {
            tab.opener = None;
        }
        let copy = self.insert(Some(parent), kind);
        for child in self.nodes[&id].children.clone() {
            self.copy_into(child, copy);
        }
        copy
    }

    /// Removes rows with what they hold, and returns their tabs, in the order the sidebar shows
    /// them, so they may be reopened.
    fn remove_rows(&mut self, rows: &[NodeId]) -> Result<Vec<Tab>, Error> {
        let mut tabs = Vec::new();
        for &row in rows {
            if self.is_tab(row) {
                tabs.push(row);
            } else {
                self.push_tabs(row, &mut tabs);
            }
        }
        let removed = tabs
            .iter()
            .filter_map(|&id| match &self.nodes[&id].kind {
                NodeKind::Tab(tab) => Some(tab.clone()),
                _ => None,
            })
            .collect();
        for &row in rows {
            self.remove(row)?;
        }
        Ok(removed)
    }

    /// Removes a tab, a folder or a split with everything it contains; the other tab of a split
    /// left alone takes the split's place.
    pub fn remove(&mut self, id: NodeId) -> Result<(), Error> {
        let parent = self.movable(id)?;
        self.children_mut(parent).retain(|child| *child != id);
        self.dissolve(parent);
        if let Some(layout) = self.splits.get_mut(&parent) {
            layout.factors.remove(&id);
        }
        let mut pending = vec![id];
        while let Some(id) = pending.pop() {
            let node = self.nodes.remove(&id).expect("children are known nodes");
            self.splits.remove(&id);
            pending.extend(node.children);
        }
        for space in &mut self.spaces {
            if space
                .selected
                .is_some_and(|tab| !self.nodes.contains_key(&tab))
            {
                space.selected = None;
            }
        }
        Ok(())
    }

    /// Moves `id` from `from` to `index` among `to`'s children, saving or forgetting the urls of
    /// the tabs that become pinned or stop being pinned.
    fn place(&mut self, id: NodeId, from: NodeId, to: NodeId, index: usize) {
        let was_pinned = self.is_pinned(id);
        self.children_mut(from).retain(|child| *child != id);
        if let Some(layout) = self.splits.get_mut(&from) {
            layout.factors.remove(&id);
        }
        let children = self.children_mut(to);
        children.insert(index.min(children.len()), id);
        self.nodes.get_mut(&id).expect("a known node").parent = Some(to);
        self.show_favorites_hint(to);
        self.dissolve(from);
        let pinned = self.is_pinned(id);
        if pinned != was_pinned {
            self.save_urls(id, pinned);
        }
    }

    /// Moves a tab out of its split, as a row right after it; the split then dissolves, its
    /// other tab staying as a row in its place. A tab in no split stays where it is.
    pub fn separate(&mut self, tab: NodeId) -> Result<(), Error> {
        self.check_tab(tab)?;
        let split = self.row_of(tab);
        if split == tab {
            return Ok(());
        }
        let parent = self.nodes[&split].parent.expect("a split has a parent");
        let index = self.nodes[&parent]
            .children
            .iter()
            .position(|&id| id == split);
        self.place(
            tab,
            split,
            parent,
            index.expect("a child of its parent") + 1,
        );
        Ok(())
    }

    /// Replaces a split holding fewer than two tabs by the tab it has left, if any.
    fn dissolve(&mut self, split: NodeId) {
        if !self.is_split(split) || self.nodes[&split].children.len() > 1 {
            return;
        }
        let node = self.nodes.remove(&split).expect("a known split");
        self.splits.remove(&split);
        let parent = node.parent.expect("a split has a parent");
        for &child in &node.children {
            self.nodes.get_mut(&child).expect("a known tab").parent = Some(parent);
        }
        let siblings = self.children_mut(parent);
        let index = siblings.iter().position(|&id| id == split);
        let index = index.expect("a child of its parent");
        siblings.splice(index..=index, node.children);
    }

    /// Refuses a tab more into favorites already full.
    fn check_room(&self, parent: NodeId) -> Result<(), Error> {
        let favorites = self.profiles.iter().any(|p| p.favorites == parent);
        if favorites && self.nodes[&parent].children.len() >= MAX_FAVORITES {
            return Err(Error::FavoritesFull);
        }
        Ok(())
    }

    /// A favorite added to a profile brings back its hint for when its favorites are empty again.
    fn show_favorites_hint(&mut self, parent: NodeId) {
        if let Some(profile) = self.profiles.iter_mut().find(|p| p.favorites == parent) {
            profile.favorites_hint_hidden = false;
        }
    }

    fn is_split(&self, id: NodeId) -> bool {
        matches!(self.nodes.get(&id).map(|n| &n.kind), Some(NodeKind::Split))
    }

    fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id - 1
    }

    fn insert(&mut self, parent: Option<NodeId>, kind: NodeKind) -> NodeId {
        let id = NodeId(self.next_id());
        self.nodes.insert(
            id,
            Node {
                id,
                parent,
                children: Vec::new(),
                kind,
            },
        );
        if let Some(parent) = parent {
            self.children_mut(parent).push(id);
        }
        id
    }

    /// Saves the url of every tab in `id`, or forgets it.
    fn save_urls(&mut self, id: NodeId, save: bool) {
        let mut pending = vec![id];
        while let Some(id) = pending.pop() {
            let node = self.nodes.get_mut(&id).expect("children are known nodes");
            if let NodeKind::Tab(tab) = &mut node.kind {
                tab.saved_url = save.then(|| tab.url.clone());
            }
            pending.extend(node.children.iter().copied());
        }
    }

    /// Appends every tab inside `parent`, collapsed folders included, depth first.
    fn push_tabs(&self, parent: NodeId, tabs: &mut Vec<NodeId>) {
        for &id in &self.nodes[&parent].children {
            match &self.nodes[&id].kind {
                NodeKind::Tab(_) => tabs.push(id),
                _ => self.push_tabs(id, tabs),
            }
        }
    }

    /// Appends the tabs inside `parent` the sidebar shows, depth first.
    fn push_visible_tabs(&self, parent: NodeId, tabs: &mut Vec<NodeId>) {
        for &id in &self.nodes[&parent].children {
            match &self.nodes[&id].kind {
                NodeKind::Tab(_) => tabs.push(id),
                NodeKind::Folder { expanded: true, .. } | NodeKind::Split => {
                    self.push_visible_tabs(id, tabs)
                }
                _ => {}
            }
        }
    }

    /// The tab `tab` was opened from, if it is still open.
    fn open_opener(&self, tab: NodeId) -> Option<NodeId> {
        match &self.nodes.get(&tab)?.kind {
            NodeKind::Tab(Tab { opener, .. }) => opener.filter(|&id| self.is_tab(id)),
            _ => None,
        }
    }

    fn folder_mut(&mut self, id: NodeId) -> Result<(&mut String, &mut bool), Error> {
        match &mut self.nodes.get_mut(&id).ok_or(Error::UnknownNode)?.kind {
            NodeKind::Folder { title, expanded } => Ok((title, expanded)),
            _ => Err(Error::NotAFolder),
        }
    }

    fn check_tab(&self, id: NodeId) -> Result<(), Error> {
        match self.nodes.get(&id).map(|n| &n.kind) {
            None => Err(Error::UnknownNode),
            Some(NodeKind::Tab(_)) => Ok(()),
            Some(_) => Err(Error::NotATab),
        }
    }

    fn is_tab(&self, id: NodeId) -> bool {
        matches!(self.nodes.get(&id).map(|n| &n.kind), Some(NodeKind::Tab(_)))
    }

    fn profile_mut(&mut self, id: ProfileId) -> Result<&mut Profile, Error> {
        self.profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or(Error::UnknownProfile)
    }

    fn space_mut(&mut self, id: SpaceId) -> Result<&mut Space, Error> {
        self.spaces
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(Error::UnknownSpace)
    }

    fn space(&self, id: SpaceId) -> Result<&Space, Error> {
        self.spaces
            .iter()
            .find(|s| s.id == id)
            .ok_or(Error::UnknownSpace)
    }

    fn check_container(&self, id: NodeId) -> Result<(), Error> {
        match self.nodes.get(&id).map(|n| &n.kind) {
            None => Err(Error::UnknownNode),
            Some(NodeKind::Tab(_) | NodeKind::Split) => Err(Error::NotAContainer),
            Some(_) => Ok(()),
        }
    }

    /// The parent of a node that may be moved or removed: any node but a root container.
    fn movable(&self, id: NodeId) -> Result<NodeId, Error> {
        self.nodes
            .get(&id)
            .ok_or(Error::UnknownNode)?
            .parent
            .ok_or(Error::RootNode)
    }

    /// `id` itself, then its parent, up to the root.
    fn ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(Some(id), |id| self.nodes.get(id).and_then(|n| n.parent))
    }

    fn children_mut(&mut self, id: NodeId) -> &mut Vec<NodeId> {
        &mut self
            .nodes
            .get_mut(&id)
            .expect("callers check the node exists")
            .children
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(sidebar: &Sidebar) -> Space {
        sidebar.spaces()[0].clone()
    }

    fn children(sidebar: &Sidebar, id: NodeId) -> Vec<NodeId> {
        sidebar.node(id).unwrap().children.clone()
    }

    #[test]
    fn duplicates_a_tab_a_folder_or_a_tab_of_a_split_right_after_it() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let a = sidebar.open_tab(home.id, "https://a.test/").unwrap();
        let b = sidebar.open_tab(home.id, "https://b.test/").unwrap();
        let copy = sidebar.duplicate_below(a).unwrap();
        assert_eq!(children(&sidebar, home.today), [a, copy, b]);
        let NodeKind::Tab(tab) = &sidebar.node(copy).unwrap().kind else {
            panic!("a tab");
        };
        assert_eq!((tab.url.as_str(), tab.opener), ("https://a.test/", None));
        // A folder is copied with what it holds.
        let folder = sidebar.add_folder(home.pinned, "Work").unwrap();
        let inner = sidebar.add_tab(folder, "https://w.test/", "W").unwrap();
        let folder_copy = sidebar.duplicate_below(folder).unwrap();
        assert_eq!(children(&sidebar, home.pinned), [folder, folder_copy]);
        let copied = children(&sidebar, folder_copy);
        assert_eq!(copied.len(), 1);
        assert_ne!(copied[0], inner);
        assert_eq!(sidebar.folder_tabs(folder_copy).unwrap(), copied);
        // A tab of a split is copied alone, after the split.
        let split = sidebar.split(b, copy).unwrap();
        let alone = sidebar.duplicate_below(b).unwrap();
        assert_eq!(children(&sidebar, home.today), [a, split, alone]);
        assert_eq!(sidebar.duplicate_below(home.today), Err(Error::RootNode));
    }

    #[test]
    fn full_favorites_take_no_copy() {
        let mut sidebar = Sidebar::default();
        let favorites = sidebar.profiles()[0].favorites;
        let mut last = None;
        for i in 0..MAX_FAVORITES {
            last = Some(
                sidebar
                    .add_tab(favorites, &format!("https://{i}.test/"), "")
                    .unwrap(),
            );
        }
        assert_eq!(
            sidebar.duplicate_below(last.unwrap()),
            Err(Error::FavoritesFull)
        );
    }

    #[test]
    fn removes_the_rows_below_a_tab_and_returns_their_tabs() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let a = sidebar.open_tab(home.id, "https://a.test/").unwrap();
        let b = sidebar.open_tab(home.id, "https://b.test/").unwrap();
        let folder = sidebar.add_folder(home.today, "Below").unwrap();
        sidebar.add_tab(folder, "https://c.test/", "C").unwrap();
        let removed = sidebar.remove_below(a).unwrap();
        let urls: Vec<_> = removed.iter().map(|tab| tab.url.as_str()).collect();
        assert_eq!(urls, ["https://b.test/", "https://c.test/"]);
        assert_eq!(children(&sidebar, home.today), [a]);
        assert!(sidebar.node(b).is_none());
        assert_eq!(sidebar.remove_below(a).unwrap(), []);
        assert_eq!(sidebar.remove_below(folder), Err(Error::UnknownNode));
    }

    #[test]
    fn removes_a_folder_and_returns_its_tabs() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let folder = sidebar.add_folder(home.pinned, "Work").unwrap();
        let tab = sidebar.add_tab(folder, "https://w.test/", "W").unwrap();
        sidebar.select(home.id, tab).unwrap();
        let removed = sidebar.remove_folder(folder).unwrap();
        assert_eq!(removed.len(), 1);
        assert!(children(&sidebar, home.pinned).is_empty());
        assert_eq!(sidebar.displayed().selected, None);
        let today = sidebar.open_tab(home.id, "https://t.test/").unwrap();
        assert_eq!(sidebar.remove_folder(today), Err(Error::NotAFolder));
    }

    #[test]
    fn moves_a_folder_to_another_spaces_pinned_tabs() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let work = sidebar.add_space("Work", home.profile).unwrap();
        let folder = sidebar.add_folder(home.today, "Docs").unwrap();
        let tab = sidebar.add_tab(folder, "https://d.test/", "D").unwrap();
        sidebar.select(home.id, tab).unwrap();
        sidebar.move_to_space(folder, work).unwrap();
        let work = sidebar.spaces()[1].clone();
        assert_eq!(children(&sidebar, work.pinned), [folder]);
        assert!(children(&sidebar, home.today).is_empty());
        assert_eq!(sidebar.displayed().selected, None);
        // Pinned, its tabs keep their url.
        assert!(sidebar.is_pinned(tab));
    }

    #[test]
    fn turns_a_folder_into_a_space_of_the_same_profile() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let folder = sidebar.add_folder(home.pinned, "Research").unwrap();
        let tab = sidebar.add_tab(folder, "https://r.test/", "R").unwrap();
        let nested = sidebar.add_folder(folder, "Papers").unwrap();
        sidebar.select(home.id, tab).unwrap();
        let space = sidebar.folder_to_space(folder).unwrap();
        let created = sidebar.displayed().clone();
        assert_eq!(created.id, space);
        assert_eq!(
            (created.title.as_str(), created.profile),
            ("Research", home.profile)
        );
        assert_eq!(children(&sidebar, created.pinned), [tab, nested]);
        assert_eq!(created.selected, Some(tab));
        assert_eq!(sidebar.spaces()[0].selected, None);
        assert!(sidebar.node(folder).is_none());
        assert_eq!(sidebar.folder_to_space(tab), Err(Error::NotAFolder));
    }

    #[test]
    fn names_a_tab_until_its_name_is_blanked() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let tab = sidebar.open_tab(home.id, "https://a.test/").unwrap();
        sidebar.tab_mut(tab).unwrap().title = "Page".to_string();
        sidebar.rename(tab, "  Mine ").unwrap();
        assert_eq!(sidebar.tab_mut(tab).unwrap().label(), "Mine");
        sidebar.rename(tab, " ").unwrap();
        assert_eq!(sidebar.tab_mut(tab).unwrap().label(), "Page");
        let folder = sidebar.add_folder(home.pinned, "Folder").unwrap();
        sidebar.rename(folder, "Work").unwrap();
        assert_eq!(sidebar.rename(folder, " "), Err(Error::EmptyTitle));
        assert_eq!(sidebar.rename(home.today, "x"), Err(Error::NotATab));
    }

    #[test]
    fn changes_the_url_a_pinned_tab_goes_back_to() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let pinned = sidebar
            .add_tab(home.pinned, "https://a.test/", "A")
            .unwrap();
        sidebar.set_saved_url(pinned, " https://b.test/ ").unwrap();
        assert_eq!(
            sidebar.tab_mut(pinned).unwrap().saved_url.as_deref(),
            Some("https://b.test/")
        );
        assert_eq!(sidebar.set_saved_url(pinned, " "), Err(Error::EmptyTitle));
        let today = sidebar.open_tab(home.id, "https://t.test/").unwrap();
        assert_eq!(
            sidebar.set_saved_url(today, "https://b.test/"),
            Err(Error::NotPinned)
        );
    }

    #[test]
    fn mutes_and_unmutes_a_tab() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let tab = sidebar.open_tab(home.id, "https://a.test/").unwrap();
        assert_eq!(sidebar.toggle_mute(tab), Ok(true));
        assert_eq!(sidebar.toggle_mute(tab), Ok(false));
        assert_eq!(sidebar.toggle_mute(home.today), Err(Error::NotATab));
    }

    #[test]
    fn starts_with_a_profile_and_an_empty_space() {
        let sidebar = Sidebar::default();
        assert_eq!(sidebar.profiles().len(), 1);
        let home = home(&sidebar);
        assert_eq!(home.profile, sidebar.profiles()[0].id);
        assert!(children(&sidebar, home.pinned).is_empty());
        assert!(children(&sidebar, home.today).is_empty());
    }

    #[test]
    fn a_new_profile_has_no_favorites() {
        let mut sidebar = Sidebar::default();
        sidebar.add_profile("Work");
        for profile in sidebar.profiles() {
            assert!(children(&sidebar, profile.favorites).is_empty());
        }
        assert_ne!(
            sidebar.profiles()[0].favorites,
            sidebar.profiles()[1].favorites
        );
    }

    #[test]
    fn creates_a_profile_with_a_space_of_its_own() {
        let mut sidebar = Sidebar::default();
        let work = sidebar.create_profile("Work").unwrap();
        let profile = &sidebar.profiles()[1];
        assert_eq!((profile.id, profile.name.as_str()), (work, "Work"));
        assert_eq!(profile.color, Color::Purple);
        let space = &sidebar.spaces()[1];
        assert_eq!((space.profile, space.title.as_str()), (work, "Work"));
        assert_eq!(sidebar.create_profile(" "), Err(Error::EmptyTitle));
        assert_eq!(sidebar.profiles().len(), 2);
        assert_eq!(sidebar.spaces().len(), 2);
    }

    #[test]
    fn archives_the_today_tabs_left_unshown_too_long() {
        const HOUR: u64 = 60 * 60 * 1000;
        let mut sidebar = Sidebar::default();
        let space = sidebar.displayed().id;
        let profile = sidebar.profiles()[0].id;
        let old = sidebar.open_tab(space, "https://old.example/").unwrap();
        let shown = sidebar.open_tab(space, "https://shown.example/").unwrap();
        let recent = sidebar.open_tab(space, "https://recent.example/").unwrap();
        let unseen = sidebar.open_tab(space, "https://unseen.example/").unwrap();
        sidebar.touch(old, 0);
        sidebar.touch(shown, 0);
        sidebar.touch(recent, 11 * HOUR);
        sidebar.select(space, shown).unwrap();
        // The default keeps them 12 hours.
        assert_eq!(sidebar.archive_due(12 * HOUR), 1);
        assert_eq!(sidebar.archive()[0].tab.url, "https://old.example/");
        assert_eq!(sidebar.archive()[0].archived_at, 12 * HOUR);
        assert!(sidebar.node(old).is_none() && sidebar.node(shown).is_some());
        // The tab never shown counts from the first check.
        assert_eq!(sidebar.archive_due(23 * HOUR), 1);
        assert!(sidebar.node(recent).is_none() && sidebar.node(unseen).is_some());
        assert_eq!(sidebar.archive_due(24 * HOUR), 1);
        assert_eq!(sidebar.archive().len(), 3);
        sidebar
            .set_archive_after(profile, ArchiveAfter::Never)
            .unwrap();
        sidebar.touch(shown, 0);
        sidebar.select(space, old).ok();
        assert_eq!(sidebar.archive_due(1000 * HOUR), 0);
    }

    #[test]
    fn restores_an_archived_tab_to_its_space_or_the_displayed_one() {
        let mut sidebar = Sidebar::default();
        let space = sidebar.displayed().id;
        let tab = sidebar.open_tab(space, "https://old.example/").unwrap();
        sidebar.touch(tab, 0);
        sidebar.archive_due(u64::MAX / 2);
        let (to, restored) = sidebar.restore(0, 5).unwrap();
        assert_eq!(to, space);
        assert!(sidebar.archive().is_empty());
        let NodeKind::Tab(tab) = sidebar.node(restored).unwrap().kind.clone() else {
            panic!("a tab")
        };
        assert_eq!(
            (tab.url.as_str(), tab.last_shown),
            ("https://old.example/", Some(5))
        );
        assert_eq!(sidebar.restore(0, 5), Err(Error::UnknownNode));
        let gone = serde_json::from_str("999").unwrap();
        let archived = Archived {
            tab,
            space: gone,
            archived_at: 1,
        };
        sidebar.add_archived(vec![archived]);
        assert_eq!(sidebar.restore(0, 6).unwrap().0, space);
    }

    #[test]
    fn pins_extensions_in_the_order_they_are_pinned() {
        let mut sidebar = Sidebar::default();
        let id = sidebar.profiles()[0].id;
        sidebar.pin_extension(id, "dark", true).unwrap();
        sidebar.pin_extension(id, "block", true).unwrap();
        sidebar.pin_extension(id, "dark", true).unwrap();
        assert_eq!(sidebar.profiles()[0].pinned_extensions, ["block", "dark"]);
        sidebar.pin_extension(id, "block", false).unwrap();
        assert_eq!(sidebar.profiles()[0].pinned_extensions, ["dark"]);
        let unknown = serde_json::from_str("999").unwrap();
        assert_eq!(
            sidebar.pin_extension(unknown, "dark", true),
            Err(Error::UnknownProfile)
        );
    }

    #[test]
    fn a_profile_suggests_searches_until_told_not_to() {
        let mut sidebar = Sidebar::default();
        let id = sidebar.profiles()[0].id;
        assert!(sidebar.profiles()[0].search_suggestions);
        sidebar.set_search_suggestions(id, false).unwrap();
        assert!(!sidebar.profiles()[0].search_suggestions);
        let mut saved = serde_json::to_value(&sidebar.profiles()[0]).unwrap();
        saved.as_object_mut().unwrap().remove("search_suggestions");
        let profile: Profile = serde_json::from_value(saved).unwrap();
        assert!(profile.search_suggestions);
    }

    #[test]
    fn a_profile_searches_with_google_until_another_engine_is_picked() {
        let mut sidebar = Sidebar::default();
        let id = sidebar.profiles()[0].id;
        assert_eq!(sidebar.profiles()[0].search_engine, SearchEngine::Google);
        sidebar.set_search_engine(id, SearchEngine::Ecosia).unwrap();
        assert_eq!(sidebar.profiles()[0].search_engine, SearchEngine::Ecosia);
        let unknown = serde_json::from_str("999").unwrap();
        assert_eq!(
            sidebar.set_search_engine(unknown, SearchEngine::Bing),
            Err(Error::UnknownProfile)
        );
    }

    #[test]
    fn renames_and_recolors_a_profile() {
        let mut sidebar = Sidebar::default();
        let id = sidebar.profiles()[0].id;
        sidebar.rename_profile(id, "Personal").unwrap();
        sidebar.set_profile_color(id, Color::Green).unwrap();
        assert_eq!(sidebar.rename_profile(id, " "), Err(Error::EmptyTitle));
        let profile = &sidebar.profiles()[0];
        assert_eq!(
            (profile.name.as_str(), profile.color),
            ("Personal", Color::Green)
        );
        let unknown = ProfileId(999);
        assert_eq!(
            sidebar.rename_profile(unknown, "a"),
            Err(Error::UnknownProfile)
        );
        assert_eq!(
            sidebar.set_profile_color(unknown, Color::Red),
            Err(Error::UnknownProfile)
        );
    }

    #[test]
    fn a_profile_saved_without_a_color_is_purple() {
        let json = r#"{ "id": 1, "name": "Default", "favorites": 2 }"#;
        let profile: Profile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.color, Color::Purple);
    }

    #[test]
    fn moves_a_tab_into_the_favorites() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        let tab = sidebar.open_tab(home.id, "https://example.com/").unwrap();
        sidebar.move_node(tab, favorites, 0).unwrap();
        assert!(children(&sidebar, home.today).is_empty());
        assert_eq!(children(&sidebar, favorites), [tab]);
        assert_eq!(sidebar.node(tab).unwrap().parent, Some(favorites));
    }

    #[test]
    fn refuses_a_favorite_beyond_the_limit_but_reorders_them() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        for _ in 0..MAX_FAVORITES {
            sidebar.add_tab(favorites, "https://a.test/", "a").unwrap();
        }
        let tab = sidebar.open_tab(home.id, "https://b.test/").unwrap();
        assert_eq!(
            sidebar.move_node(tab, favorites, 0),
            Err(Error::FavoritesFull)
        );
        assert_eq!(children(&sidebar, home.today), [tab]);
        assert_eq!(
            sidebar.add_tab(favorites, "https://c.test/", "c"),
            Err(Error::FavoritesFull)
        );
        let first = children(&sidebar, favorites)[0];
        sidebar.move_node(first, favorites, usize::MAX).unwrap();
        assert_eq!(children(&sidebar, favorites).last(), Some(&first));
        assert_eq!(children(&sidebar, favorites).len(), MAX_FAVORITES);
    }

    #[test]
    fn hides_the_favorites_hint_until_a_favorite_is_added() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        sidebar.hide_favorites_hint(home.id).unwrap();
        assert!(sidebar.profiles()[0].favorites_hint_hidden);
        let tab = sidebar.open_tab(home.id, "https://a.test/").unwrap();
        sidebar.move_node(tab, favorites, 0).unwrap();
        assert!(!sidebar.profiles()[0].favorites_hint_hidden);
        sidebar.hide_favorites_hint(home.id).unwrap();
        sidebar.add_tab(favorites, "https://b.test/", "b").unwrap();
        assert!(!sidebar.profiles()[0].favorites_hint_hidden);
        assert_eq!(
            sidebar.hide_favorites_hint(SpaceId(999)),
            Err(Error::UnknownSpace)
        );
    }

    #[test]
    fn a_profile_saved_without_the_hint_flag_shows_the_hint() {
        let json = r#"{ "id": 1, "name": "Default", "favorites": 2 }"#;
        let profile: Profile = serde_json::from_str(json).unwrap();
        assert!(!profile.favorites_hint_hidden);
    }

    #[test]
    fn clears_the_today_tabs_of_a_space_and_returns_them() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let a = sidebar.open_tab(home.id, "https://a.test/").unwrap();
        let folder = sidebar.add_folder(home.today, "Folder").unwrap();
        let b = sidebar.add_tab(folder, "https://b.test/", "b").unwrap();
        sidebar.toggle_folder(folder).unwrap();
        let c = sidebar.open_tab(home.id, "https://c.test/").unwrap();
        let d = sidebar.open_tab(home.id, "https://d.test/").unwrap();
        sidebar.split(d, c).unwrap();
        let pinned = sidebar
            .add_tab(home.pinned, "https://p.test/", "p")
            .unwrap();
        sidebar.select(home.id, a).unwrap();
        let removed = sidebar.clear_today(home.id).unwrap();
        let urls: Vec<_> = removed.iter().map(|tab| tab.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://a.test/",
                "https://b.test/",
                "https://c.test/",
                "https://d.test/"
            ]
        );
        assert!(children(&sidebar, home.today).is_empty());
        assert!(
            [a, b, c, d, folder]
                .iter()
                .all(|&id| sidebar.node(id).is_none())
        );
        assert_eq!(children(&sidebar, home.pinned), [pinned]);
        assert_eq!(sidebar.displayed().selected, None);
        assert_eq!(sidebar.clear_today(home.id).unwrap(), []);
        assert_eq!(sidebar.clear_today(SpaceId(999)), Err(Error::UnknownSpace));
    }

    #[test]
    fn keeps_the_favorites_root() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        assert_eq!(sidebar.remove(favorites), Err(Error::RootNode));
        assert_eq!(
            sidebar.move_node(favorites, home.pinned, 0),
            Err(Error::RootNode)
        );
    }

    #[test]
    fn selects_a_favorite_in_any_space_of_its_profile() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let work = sidebar.add_space("Work", home.profile).unwrap();
        let favorites = sidebar.profiles()[0].favorites;
        let a = sidebar.open_tab(home.id, "a").unwrap();
        let b = sidebar.open_tab(home.id, "b").unwrap();
        sidebar.move_node(a, favorites, usize::MAX).unwrap();
        sidebar.move_node(b, favorites, usize::MAX).unwrap();
        sidebar.select(home.id, a).unwrap();
        sidebar.select(work, a).unwrap();
        assert_eq!(sidebar.spaces()[1].selected, Some(a));
        sidebar.remove(a).unwrap();
        assert_eq!(sidebar.spaces()[0].selected, None);
        assert_eq!(sidebar.spaces()[1].selected, None);
    }

    #[test]
    fn closing_a_favorite_deselects_it_and_keeps_it() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        let a = sidebar.open_tab(home.id, "a").unwrap();
        let b = sidebar.open_tab(home.id, "b").unwrap();
        let favorite = sidebar.open_tab(home.id, "favorite").unwrap();
        let other = sidebar.open_tab(home.id, "other").unwrap();
        sidebar.move_node(favorite, favorites, usize::MAX).unwrap();
        sidebar.move_node(other, favorites, usize::MAX).unwrap();
        assert!(sidebar.is_favorite(favorite));
        assert!(!sidebar.is_favorite(a));
        sidebar.select(home.id, favorite).unwrap();
        assert_eq!(sidebar.close_in_place(home.id, favorite), Ok(Some(b)));
        assert_eq!(sidebar.spaces()[0].selected, None);
        assert_eq!(children(&sidebar, favorites), [favorite, other]);
        sidebar.remove(a).unwrap();
        sidebar.remove(b).unwrap();
        assert_eq!(sidebar.close_in_place(home.id, favorite), Ok(None));
    }

    #[test]
    fn closing_a_favorite_returns_to_its_opener() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        let opener = sidebar.open_tab(home.id, "opener").unwrap();
        sidebar.open_tab(home.id, "last").unwrap();
        sidebar.select(home.id, opener).unwrap();
        let favorite = sidebar.open_tab(home.id, "favorite").unwrap();
        sidebar.move_node(favorite, favorites, 0).unwrap();
        assert_eq!(sidebar.close_in_place(home.id, favorite), Ok(Some(opener)));
        assert_eq!(
            sidebar.close_in_place(SpaceId(999), favorite),
            Err(Error::UnknownSpace)
        );
    }

    #[test]
    fn pins_a_tab_with_its_url_and_unpins_it_to_the_top_of_today() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let first = sidebar.add_folder(home.pinned, "Docs").unwrap();
        let a = sidebar.open_tab(home.id, "a").unwrap();
        let b = sidebar.open_tab(home.id, "b").unwrap();
        sidebar.toggle_pin(home.id, b).unwrap();
        assert_eq!(children(&sidebar, home.pinned), [first, b]);
        assert!(sidebar.is_pinned(b));
        assert!(!sidebar.is_pinned(a));
        assert_eq!(sidebar.tab_mut(b).unwrap().saved_url.as_deref(), Some("b"));
        sidebar.toggle_pin(home.id, b).unwrap();
        assert_eq!(children(&sidebar, home.pinned), [first]);
        assert_eq!(children(&sidebar, home.today), [b, a]);
        assert_eq!(sidebar.tab_mut(b).unwrap().saved_url, None);
        assert_eq!(sidebar.toggle_pin(home.id, first), Err(Error::NotATab));
    }

    #[test]
    fn unpins_a_tab_from_a_pinned_folder() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let folder = sidebar.add_folder(home.pinned, "Docs").unwrap();
        let tab = sidebar.open_tab(home.id, "a").unwrap();
        sidebar.move_node(tab, folder, 0).unwrap();
        assert!(sidebar.is_pinned(tab));
        sidebar.toggle_pin(home.id, tab).unwrap();
        assert!(children(&sidebar, folder).is_empty());
        assert_eq!(children(&sidebar, home.today), [tab]);
    }

    #[test]
    fn leaves_favorites_where_they_are_on_pin() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        let tab = sidebar.open_tab(home.id, "a").unwrap();
        sidebar.move_node(tab, favorites, 0).unwrap();
        sidebar.toggle_pin(home.id, tab).unwrap();
        assert_eq!(children(&sidebar, favorites), [tab]);
        assert!(!sidebar.is_pinned(tab));
    }

    fn saved_url(sidebar: &mut Sidebar, tab: NodeId) -> Option<String> {
        sidebar.tab_mut(tab).unwrap().saved_url.clone()
    }

    #[test]
    fn moving_a_tab_into_or_out_of_the_pinned_tabs_pins_or_unpins_it() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        let folder = sidebar.add_folder(home.pinned, "Docs").unwrap();
        let tab = sidebar.open_tab(home.id, "a").unwrap();
        sidebar.move_node(tab, folder, 0).unwrap();
        assert_eq!(saved_url(&mut sidebar, tab).as_deref(), Some("a"));
        sidebar.tab_mut(tab).unwrap().url = "b".to_string();
        sidebar.move_node(tab, home.pinned, 0).unwrap();
        assert_eq!(saved_url(&mut sidebar, tab).as_deref(), Some("a"));
        sidebar.move_node(tab, home.today, 0).unwrap();
        assert_eq!(saved_url(&mut sidebar, tab), None);
        sidebar.move_node(tab, home.pinned, 0).unwrap();
        sidebar.move_node(tab, favorites, 0).unwrap();
        assert!(sidebar.is_favorite(tab));
        assert_eq!(saved_url(&mut sidebar, tab), None);
    }

    #[test]
    fn moving_a_folder_pins_or_unpins_its_tabs() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let outer = sidebar.add_folder(home.today, "Outer").unwrap();
        let inner = sidebar.add_folder(outer, "Inner").unwrap();
        let tab = sidebar.open_tab(home.id, "a").unwrap();
        sidebar.move_node(tab, inner, 0).unwrap();
        assert_eq!(saved_url(&mut sidebar, tab), None);
        sidebar.move_node(outer, home.pinned, 0).unwrap();
        assert_eq!(saved_url(&mut sidebar, tab).as_deref(), Some("a"));
        sidebar.move_node(outer, home.today, 0).unwrap();
        assert_eq!(saved_url(&mut sidebar, tab), None);
    }

    #[test]
    fn favorites_hold_only_tabs() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        let folder = sidebar.add_folder(home.pinned, "Docs").unwrap();
        assert_eq!(sidebar.move_node(folder, favorites, 0), Err(Error::NotATab));
        assert_eq!(children(&sidebar, home.pinned), [folder]);
    }

    #[test]
    fn closing_a_pinned_tab_keeps_it_and_its_title_and_takes_it_back_to_its_saved_url() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let today = sidebar.open_tab(home.id, "today").unwrap();
        let pinned = sidebar.open_tab(home.id, "saved").unwrap();
        sidebar.toggle_pin(home.id, pinned).unwrap();
        sidebar.select(home.id, pinned).unwrap();
        let tab = sidebar.tab_mut(pinned).unwrap();
        tab.url = "elsewhere".to_string();
        tab.title = "Elsewhere".to_string();
        assert_eq!(sidebar.close_in_place(home.id, pinned), Ok(Some(today)));
        assert_eq!(sidebar.spaces()[0].selected, None);
        assert_eq!(children(&sidebar, home.pinned), [pinned]);
        let tab = sidebar.tab_mut(pinned).unwrap();
        assert_eq!(
            (tab.url.as_str(), tab.title.as_str()),
            ("saved", "Elsewhere")
        );
    }

    #[test]
    fn closing_an_unselected_pinned_tab_keeps_the_selection() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let today = sidebar.open_tab(home.id, "today").unwrap();
        let pinned = sidebar.open_tab(home.id, "saved").unwrap();
        sidebar.toggle_pin(home.id, pinned).unwrap();
        sidebar.select(home.id, today).unwrap();
        sidebar.close_in_place(home.id, pinned).unwrap();
        assert_eq!(sidebar.spaces()[0].selected, Some(today));
    }

    #[test]
    fn spaces_can_share_a_profile() {
        let mut sidebar = Sidebar::default();
        let profile = sidebar.profiles()[0].id;
        let work = sidebar.add_space("Work", profile).unwrap();
        assert_eq!(sidebar.spaces()[1].id, work);
        assert_eq!(sidebar.spaces()[1].profile, profile);
    }

    #[test]
    fn creates_a_space_and_displays_it() {
        let mut sidebar = Sidebar::default();
        let work = sidebar.create_profile("Work").unwrap();
        let space = sidebar.create_space("Mail", work).unwrap();
        let created = sidebar.displayed();
        assert_eq!(created.id, space);
        assert_eq!((created.title.as_str(), created.profile), ("Mail", work));
        assert_eq!(
            (created.emoji.as_deref(), &created.theme),
            (None, &Theme::from(Color::Purple))
        );
        assert_eq!(sidebar.spaces().last().unwrap().id, space);
        assert_eq!(sidebar.create_space(" ", work), Err(Error::EmptyTitle));
        assert_eq!(
            sidebar.create_space("a", ProfileId(999)),
            Err(Error::UnknownProfile)
        );
        assert_eq!(sidebar.spaces().len(), 3);
        assert_eq!(sidebar.displayed().id, space);
    }

    #[test]
    fn renames_a_space_and_sets_its_emoji_and_theme() {
        let mut sidebar = Sidebar::default();
        let id = home(&sidebar).id;
        sidebar.rename_space(id, "Personal").unwrap();
        sidebar.set_space_emoji(id, " 🏡 ").unwrap();
        let theme = Theme {
            colors: vec!["#6daee9".into(), "#f47a78".into(), "#dfe56b".into()],
            mode: Mode::Light,
            intensity: 0.8,
            noise: 0.0,
        };
        sidebar.set_space_theme(id, theme.clone()).unwrap();
        assert_eq!(sidebar.rename_space(id, " "), Err(Error::EmptyTitle));
        let space = home(&sidebar);
        assert_eq!(
            (space.title.as_str(), space.emoji.as_deref(), &space.theme),
            ("Personal", Some("🏡"), &theme)
        );
        // No color leaves the window's neutrals.
        let neutral = Theme {
            colors: Vec::new(),
            ..theme.clone()
        };
        sidebar.set_space_theme(id, neutral.clone()).unwrap();
        assert_eq!(home(&sidebar).theme, neutral);
        let four = Theme {
            colors: vec!["#000000".into(); 4],
            ..theme.clone()
        };
        let named = Theme {
            colors: vec!["blue".into()],
            ..theme.clone()
        };
        let strong = Theme {
            intensity: 1.5,
            ..theme.clone()
        };
        for invalid in [four, named, strong] {
            assert_eq!(
                sidebar.set_space_theme(id, invalid),
                Err(Error::InvalidTheme)
            );
        }
        assert_eq!(home(&sidebar).theme, neutral);
        sidebar.set_space_emoji(id, " ").unwrap();
        assert_eq!(home(&sidebar).emoji, None);
        let unknown = SpaceId(999);
        assert_eq!(sidebar.rename_space(unknown, "a"), Err(Error::UnknownSpace));
        assert_eq!(
            sidebar.set_space_emoji(unknown, "a"),
            Err(Error::UnknownSpace)
        );
        assert_eq!(
            sidebar.set_space_theme(unknown, Theme::default()),
            Err(Error::UnknownSpace)
        );
    }

    #[test]
    fn a_space_saved_without_an_emoji_or_a_color_has_none_and_is_purple() {
        let json = r#"{ "id": 3, "title": "Home", "profile": 1, "pinned": 4, "today": 5,
            "selected": null }"#;
        let space: Space = serde_json::from_str(json).unwrap();
        assert_eq!((space.emoji, space.theme), (None, Color::Purple.into()));
    }

    #[test]
    fn a_space_saved_with_a_color_takes_it_as_its_theme() {
        let json = r#"{ "id": 3, "title": "Home", "color": "green", "profile": 1, "pinned": 4,
            "today": 5, "selected": null }"#;
        let space: Space = serde_json::from_str(json).unwrap();
        assert_eq!(space.theme, Color::Green.into());
        let saved = serde_json::to_value(&space).unwrap();
        assert_eq!(saved["theme"]["colors"], serde_json::json!(["#30a46c"]));
        assert_eq!(saved.get("color"), None);
    }

    #[test]
    fn displays_the_first_space_until_another_is_displayed() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let work = sidebar.add_space("Work", home.profile).unwrap();
        assert_eq!(sidebar.displayed().id, home.id);
        sidebar.display(work).unwrap();
        assert_eq!(sidebar.displayed().id, work);
        assert_eq!(sidebar.display(SpaceId(999)), Err(Error::UnknownSpace));
        assert_eq!(sidebar.displayed().id, work);
    }

    #[test]
    fn a_view_shows_its_space_and_tabs_again_but_those_gone() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let work = sidebar.add_space("Work", home.profile).unwrap();
        let first = sidebar.add_tab(home.today, "https://a.test/", "a").unwrap();
        let second = sidebar.add_tab(home.today, "https://b.test/", "b").unwrap();
        sidebar.select(home.id, first).unwrap();
        sidebar.display(work).unwrap();
        let view = sidebar.view();
        // A new window shows the same Space, without a tab.
        let new = sidebar.new_window_view();
        sidebar.display(home.id).unwrap();
        sidebar.select(home.id, second).unwrap();
        sidebar.set_view(&new);
        assert_eq!(sidebar.displayed().id, work);
        sidebar.set_view(&view);
        assert_eq!(sidebar.displayed().id, work);
        assert_eq!(sidebar.space(home.id).unwrap().selected, Some(first));
        // Each view keeps its folders collapsed or not.
        let folder = sidebar.add_folder(home.pinned, "Work").unwrap();
        sidebar.toggle_folder(folder).unwrap();
        let collapsed = sidebar.view();
        sidebar.set_view(&view);
        assert!(matches!(
            sidebar.node(folder).unwrap().kind,
            NodeKind::Folder { expanded: true, .. }
        ));
        sidebar.set_view(&collapsed);
        assert!(matches!(
            sidebar.node(folder).unwrap().kind,
            NodeKind::Folder {
                expanded: false,
                ..
            }
        ));
        // A tab closed meanwhile is no longer selected.
        sidebar.remove(first).unwrap();
        sidebar.select(home.id, second).unwrap();
        sidebar.set_view(&view);
        assert_eq!(sidebar.space(home.id).unwrap().selected, None);
    }

    #[test]
    fn url_routing_sends_links_of_unknown_or_deleted_spaces_to_the_space_shown_last() {
        use crate::url_routing::{Match, Route};
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let work = sidebar.add_space("Work", home.profile).unwrap();
        let route = |text: &str, destination| Route {
            matching: Match::Contains,
            text: text.to_string(),
            destination,
        };
        sidebar.set_url_routing(UrlRouting {
            routes: vec![
                route("a", Destination::Space(work)),
                route("b", Destination::Space(SpaceId(999))),
            ],
            default: Destination::Space(work),
        });
        assert_eq!(
            sidebar.url_routing().routes[0].destination,
            Destination::Space(work)
        );
        assert_eq!(
            sidebar.url_routing().routes[1].destination,
            Destination::MostRecentSpace
        );
        sidebar.delete_space(work).unwrap();
        assert_eq!(
            sidebar.url_routing().routes[0].destination,
            Destination::MostRecentSpace
        );
        assert_eq!(sidebar.url_routing().default, Destination::MostRecentSpace);
    }

    #[test]
    fn finds_a_space_by_number() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar).id;
        let work = sidebar.add_space("Work", sidebar.profiles()[0].id).unwrap();
        assert_eq!(sidebar.space_at(1), Some(home));
        assert_eq!(sidebar.space_at(2), Some(work));
        assert_eq!(sidebar.space_at(3), None);
        assert_eq!(sidebar.space_at(0), None);
    }

    #[test]
    fn finds_the_next_and_previous_space_without_wrapping_around() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar).id;
        let work = sidebar.add_space("Work", sidebar.profiles()[0].id).unwrap();
        let read = sidebar.add_space("Read", sidebar.profiles()[0].id).unwrap();
        assert_eq!(sidebar.next_space(true), Some(work));
        assert_eq!(sidebar.next_space(false), None);
        sidebar.display(work).unwrap();
        assert_eq!(sidebar.next_space(true), Some(read));
        assert_eq!(sidebar.next_space(false), Some(home));
        sidebar.display(read).unwrap();
        assert_eq!(sidebar.next_space(true), None);
    }

    #[test]
    fn each_space_keeps_its_selected_tab() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar).id;
        let work = sidebar.add_space("Work", sidebar.profiles()[0].id).unwrap();
        let read = sidebar.open_tab(home, "read").unwrap();
        let mail = sidebar.open_tab(work, "mail").unwrap();
        sidebar.select(home, read).unwrap();
        sidebar.select(work, mail).unwrap();
        sidebar.display(work).unwrap();
        sidebar.display(home).unwrap();
        assert_eq!(sidebar.displayed().selected, Some(read));
        assert_eq!(sidebar.spaces()[1].selected, Some(mail));
    }

    #[test]
    fn a_node_belongs_to_the_space_of_its_pinned_or_today_tabs() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let work = sidebar.add_space("Work", home.profile).unwrap();
        let folder = sidebar.add_folder(home.pinned, "Docs").unwrap();
        let pinned = sidebar.open_tab(home.id, "pinned").unwrap();
        sidebar.move_node(pinned, folder, 0).unwrap();
        let today = sidebar.open_tab(work, "today").unwrap();
        let favorite = sidebar.open_tab(work, "favorite").unwrap();
        let favorites = sidebar.profiles()[0].favorites;
        sidebar.move_node(favorite, favorites, 0).unwrap();
        assert_eq!(sidebar.space_of(pinned), Some(home.id));
        assert_eq!(sidebar.space_of(today), Some(work));
        assert_eq!(sidebar.space_of(favorite), None);
        assert_eq!(sidebar.space_of(NodeId(999)), None);
    }

    #[test]
    fn a_node_shows_in_the_profile_of_its_favorites_or_its_space() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let work = sidebar.add_profile("Work");
        let office = sidebar.add_space("Office", work).unwrap();
        let favorite = sidebar.open_tab(office, "favorite").unwrap();
        sidebar.move_node(favorite, home.today, 0).unwrap();
        let favorites = sidebar.profiles()[1].favorites;
        let folder = sidebar
            .add_folder(sidebar.spaces()[1].pinned, "Docs")
            .unwrap();
        let pinned = sidebar.open_tab(home.id, "pinned").unwrap();
        sidebar.move_node(pinned, folder, 0).unwrap();
        assert_eq!(sidebar.profile_of(favorite), Some(home.profile));
        sidebar.move_node(favorite, favorites, 0).unwrap();
        assert_eq!(sidebar.profile_of(favorite), Some(work));
        assert_eq!(sidebar.profile_of(pinned), Some(work));
        assert_eq!(sidebar.profile_of(home.today), Some(home.profile));
        assert_eq!(sidebar.profile_of(NodeId(999)), None);
    }

    #[test]
    fn a_space_needs_a_known_profile() {
        let mut sidebar = Sidebar::default();
        assert_eq!(
            sidebar.add_space("Work", ProfileId(999)),
            Err(Error::UnknownProfile)
        );
    }

    #[test]
    fn opens_tabs_at_the_end_of_today() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let first = sidebar.open_tab(home.id, "https://example.com/").unwrap();
        let second = sidebar.open_tab(home.id, "https://example.org/").unwrap();
        assert_eq!(children(&sidebar, home.today), [first, second]);
        let NodeKind::Tab(tab) = &sidebar.node(first).unwrap().kind else {
            panic!("not a tab")
        };
        assert_eq!(tab.url, "https://example.com/");
    }

    #[test]
    fn updates_a_tab() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let tab = sidebar.open_tab(home.id, "https://example.com/").unwrap();
        sidebar.tab_mut(tab).unwrap().title = "Example".to_string();
        let NodeKind::Tab(tab) = &sidebar.node(tab).unwrap().kind else {
            panic!("not a tab")
        };
        assert_eq!(tab.title, "Example");
        assert_eq!(sidebar.tab_mut(home.today), None);
    }

    #[test]
    fn selects_a_tab_until_it_is_removed() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let tab = sidebar.open_tab(home.id, "https://example.com/").unwrap();
        sidebar.select(home.id, tab).unwrap();
        assert_eq!(sidebar.spaces()[0].selected, Some(tab));
        assert_eq!(sidebar.select(home.id, home.today), Err(Error::NotATab));
        assert_eq!(
            sidebar.select(home.id, NodeId(999)),
            Err(Error::UnknownNode)
        );
        sidebar.remove(tab).unwrap();
        assert_eq!(sidebar.spaces()[0].selected, None);
    }

    #[test]
    fn shows_the_next_tab_after_closing_one() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let a = sidebar.open_tab(home.id, "a").unwrap();
        let b = sidebar.open_tab(home.id, "b").unwrap();
        let c = sidebar.open_tab(home.id, "c").unwrap();
        assert_eq!(sidebar.next_after_close(b), Some(c));
        assert_eq!(sidebar.next_after_close(c), Some(b));
        sidebar.remove(b).unwrap();
        sidebar.remove(c).unwrap();
        assert_eq!(sidebar.next_after_close(a), None);
    }

    #[test]
    fn returns_to_the_opener_while_it_is_open() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let first = sidebar.open_tab(home.id, "first").unwrap();
        let opener = sidebar.open_tab(home.id, "opener").unwrap();
        sidebar.select(home.id, opener).unwrap();
        let opened = sidebar.open_tab(home.id, "opened").unwrap();
        sidebar.move_node(opened, home.today, 0).unwrap();
        assert_eq!(sidebar.next_after_close(opened), Some(opener));
        sidebar.remove(opener).unwrap();
        assert_eq!(sidebar.next_after_close(opened), Some(first));
    }

    #[test]
    fn moves_a_tab_into_a_folder() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let tab = sidebar.open_tab(home.id, "https://example.com/").unwrap();
        let folder = sidebar.add_folder(home.pinned, "Docs").unwrap();
        sidebar.move_node(tab, folder, 0).unwrap();
        assert!(children(&sidebar, home.today).is_empty());
        assert_eq!(children(&sidebar, folder), [tab]);
        assert_eq!(sidebar.node(tab).unwrap().parent, Some(folder));
    }

    #[test]
    fn moves_to_a_position_clamped_to_the_end() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let a = sidebar.open_tab(home.id, "a").unwrap();
        let b = sidebar.open_tab(home.id, "b").unwrap();
        let c = sidebar.open_tab(home.id, "c").unwrap();
        sidebar.move_node(c, home.today, 0).unwrap();
        assert_eq!(children(&sidebar, home.today), [c, a, b]);
        sidebar.move_node(c, home.today, 99).unwrap();
        assert_eq!(children(&sidebar, home.today), [a, b, c]);
    }

    #[test]
    fn refuses_impossible_moves() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let outer = sidebar.add_folder(home.pinned, "Outer").unwrap();
        let inner = sidebar.add_folder(outer, "Inner").unwrap();
        let tab = sidebar.open_tab(home.id, "https://example.com/").unwrap();
        assert_eq!(
            sidebar.move_node(outer, inner, 0),
            Err(Error::MoveIntoItself)
        );
        assert_eq!(
            sidebar.move_node(outer, outer, 0),
            Err(Error::MoveIntoItself)
        );
        assert_eq!(sidebar.move_node(outer, tab, 0), Err(Error::NotAContainer));
        assert_eq!(
            sidebar.move_node(home.today, outer, 0),
            Err(Error::RootNode)
        );
    }

    fn folder_title(sidebar: &Sidebar, id: NodeId) -> String {
        match &sidebar.node(id).unwrap().kind {
            NodeKind::Folder { title, .. } => title.clone(),
            _ => panic!("not a folder"),
        }
    }

    fn folder_expanded(sidebar: &Sidebar, id: NodeId) -> bool {
        matches!(
            sidebar.node(id).unwrap().kind,
            NodeKind::Folder { expanded: true, .. }
        )
    }

    #[test]
    fn adds_a_tab_that_saves_its_url_when_pinned() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let folder = sidebar.add_folder(home.pinned, "Docs").unwrap();
        let pinned = sidebar.add_tab(folder, "a", "A").unwrap();
        let today = sidebar.add_tab(home.today, "b", "B").unwrap();
        assert_eq!(children(&sidebar, folder), [pinned]);
        assert_eq!(children(&sidebar, home.today), [today]);
        let tab = sidebar.tab_mut(pinned).unwrap().clone();
        assert_eq!(
            (
                tab.url.as_str(),
                tab.title.as_str(),
                tab.saved_url.as_deref()
            ),
            ("a", "A", Some("a"))
        );
        assert_eq!(saved_url(&mut sidebar, today), None);
        assert_eq!(sidebar.add_tab(today, "c", "C"), Err(Error::NotAContainer));
        assert_eq!(
            sidebar.add_tab(NodeId(999), "c", "C"),
            Err(Error::UnknownNode)
        );
    }

    #[test]
    fn adds_no_folder_to_the_favorites() {
        let mut sidebar = Sidebar::default();
        let favorites = sidebar.profiles()[0].favorites;
        assert_eq!(sidebar.add_folder(favorites, "Docs"), Err(Error::NotATab));
        assert!(children(&sidebar, favorites).is_empty());
    }

    #[test]
    fn renames_a_folder_to_a_title_that_is_not_blank() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let folder = sidebar.add_folder(home.pinned, "New Folder").unwrap();
        sidebar.rename_folder(folder, "Reading").unwrap();
        assert_eq!(sidebar.rename_folder(folder, " "), Err(Error::EmptyTitle));
        assert_eq!(folder_title(&sidebar, folder), "Reading");
        let tab = sidebar.open_tab(home.id, "a").unwrap();
        assert_eq!(sidebar.rename_folder(tab, "b"), Err(Error::NotAFolder));
        assert_eq!(
            sidebar.rename_folder(home.pinned, "b"),
            Err(Error::NotAFolder)
        );
        assert_eq!(
            sidebar.rename_folder(NodeId(999), "b"),
            Err(Error::UnknownNode)
        );
    }

    #[test]
    fn collapses_and_expands_a_folder() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let folder = sidebar.add_folder(home.pinned, "Docs").unwrap();
        assert!(folder_expanded(&sidebar, folder));
        sidebar.toggle_folder(folder).unwrap();
        assert!(!folder_expanded(&sidebar, folder));
        sidebar.toggle_folder(folder).unwrap();
        assert!(folder_expanded(&sidebar, folder));
        let tab = sidebar.open_tab(home.id, "a").unwrap();
        assert_eq!(sidebar.toggle_folder(tab), Err(Error::NotAFolder));
    }

    #[test]
    fn removes_a_folder_with_its_content() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let folder = sidebar.add_folder(home.pinned, "Docs").unwrap();
        let tab = sidebar.open_tab(home.id, "https://example.com/").unwrap();
        sidebar.move_node(tab, folder, 0).unwrap();
        sidebar.remove(folder).unwrap();
        assert!(children(&sidebar, home.pinned).is_empty());
        assert_eq!(sidebar.node(folder), None);
        assert_eq!(sidebar.node(tab), None);
        assert_eq!(sidebar.remove(home.pinned), Err(Error::RootNode));
    }

    #[test]
    fn moves_between_the_tabs_the_sidebar_shows_and_wraps_around() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        let open = folder_with_tab(&mut sidebar, &home, "open");
        let shut = folder_with_tab(&mut sidebar, &home, "shut");
        sidebar.toggle_folder(shut.0).unwrap();
        let pinned = sidebar.open_tab(home.id, "pinned").unwrap();
        sidebar.move_node(pinned, home.pinned, usize::MAX).unwrap();
        let today = sidebar.open_tab(home.id, "today").unwrap();
        let favorite = sidebar.open_tab(home.id, "favorite").unwrap();
        sidebar.move_node(favorite, favorites, 0).unwrap();
        assert_eq!(sidebar.next_tab(home.id, true), Some(open.1));
        assert_eq!(sidebar.next_tab(home.id, false), Some(today));
        sidebar.select(home.id, open.1).unwrap();
        assert_eq!(sidebar.next_tab(home.id, true), Some(pinned));
        assert_eq!(sidebar.next_tab(home.id, false), Some(today));
        sidebar.select(home.id, today).unwrap();
        assert_eq!(sidebar.next_tab(home.id, true), Some(open.1));
        assert_eq!(sidebar.next_tab(home.id, false), Some(pinned));
        sidebar.select(home.id, favorite).unwrap();
        assert_eq!(sidebar.next_tab(home.id, true), Some(open.1));
    }

    #[test]
    fn moves_nowhere_without_tabs() {
        let sidebar = Sidebar::default();
        assert_eq!(sidebar.next_tab(home(&sidebar).id, true), None);
        assert_eq!(sidebar.next_tab(SpaceId(999), true), None);
    }

    /// A pinned folder holding one tab: (folder, tab).
    fn folder_with_tab(sidebar: &mut Sidebar, space: &Space, title: &str) -> (NodeId, NodeId) {
        let folder = sidebar.add_folder(space.pinned, title).unwrap();
        let tab = sidebar.open_tab(space.id, title).unwrap();
        sidebar.move_node(tab, folder, 0).unwrap();
        (folder, tab)
    }

    #[test]
    fn finds_a_favorite_by_number_and_the_last_one_with_9() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        assert_eq!(sidebar.favorite(home.id, 1), None);
        assert_eq!(sidebar.favorite(home.id, 9), None);
        let tabs: Vec<_> = (0..3)
            .map(|i| sidebar.open_tab(home.id, &i.to_string()).unwrap())
            .collect();
        for &tab in &tabs {
            sidebar.move_node(tab, favorites, usize::MAX).unwrap();
        }
        assert_eq!(sidebar.favorite(home.id, 1), Some(tabs[0]));
        assert_eq!(sidebar.favorite(home.id, 3), Some(tabs[2]));
        assert_eq!(sidebar.favorite(home.id, 4), None);
        assert_eq!(sidebar.favorite(home.id, 9), Some(tabs[2]));
        assert_eq!(sidebar.favorite(home.id, 0), None);
    }

    #[test]
    fn numbers_the_favorites_of_each_profile() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let work = sidebar.create_profile("Work").unwrap();
        let office = sidebar.spaces()[1].id;
        let personal = sidebar.open_tab(home.id, "personal").unwrap();
        let job = sidebar.open_tab(office, "job").unwrap();
        let favorites = sidebar.favorites_of(home.id).unwrap();
        let work_favorites = sidebar.favorites_of(office).unwrap();
        assert_eq!(favorites, sidebar.profiles()[0].favorites);
        assert_eq!(work_favorites, sidebar.profiles()[1].favorites);
        sidebar.move_node(personal, favorites, 0).unwrap();
        sidebar.move_node(job, work_favorites, 0).unwrap();
        assert_eq!(sidebar.favorite(home.id, 1), Some(personal));
        assert_eq!(sidebar.favorite(office, 1), Some(job));
        assert_eq!(sidebar.profile_of(job), Some(work));
        assert_eq!(sidebar.favorites_of(SpaceId(999)), None);
    }
    #[test]
    fn moves_a_tab_to_the_end_of_another_spaces_today_tabs() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let work = sidebar.add_space("Work", home.profile).unwrap();
        let other = sidebar.open_tab(work, "other").unwrap();
        let tab = sidebar.open_tab(home.id, "tab").unwrap();
        sidebar.toggle_pin(home.id, tab).unwrap();
        sidebar.select(home.id, tab).unwrap();
        sidebar.move_to_space(tab, work).unwrap();
        let work = sidebar.spaces()[1].clone();
        assert_eq!(children(&sidebar, work.today), [other, tab]);
        assert!(children(&sidebar, home.pinned).is_empty());
        assert!(!sidebar.is_pinned(tab));
        assert_eq!(saved_url(&mut sidebar, tab), None);
        assert_eq!(sidebar.spaces()[0].selected, None);
        assert_eq!(sidebar.space_of(tab), Some(work.id));
    }

    #[test]
    fn leaves_a_tab_in_its_own_space_and_favorites_where_they_are() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let work = sidebar.add_space("Work", home.profile).unwrap();
        let favorites = sidebar.profiles()[0].favorites;
        let pinned = sidebar.add_tab(home.pinned, "pinned", "pinned").unwrap();
        let favorite = sidebar.add_tab(favorites, "favorite", "favorite").unwrap();
        sidebar.move_to_space(pinned, home.id).unwrap();
        sidebar.move_to_space(favorite, work).unwrap();
        assert_eq!(children(&sidebar, home.pinned), [pinned]);
        assert_eq!(children(&sidebar, favorites), [favorite]);
        assert_eq!(
            sidebar.move_to_space(home.pinned, work),
            Err(Error::NotATab)
        );
        assert_eq!(
            sidebar.move_to_space(NodeId(999), work),
            Err(Error::UnknownNode)
        );
        assert_eq!(
            sidebar.move_to_space(pinned, SpaceId(999)),
            Err(Error::UnknownSpace)
        );
    }

    #[test]
    fn lists_the_tabs_of_a_folder_and_of_the_folders_inside_it() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let (folder, first) = folder_with_tab(&mut sidebar, &home, "a");
        let inner = sidebar.add_folder(folder, "Inner").unwrap();
        let nested = sidebar.add_tab(inner, "b", "b").unwrap();
        let last = sidebar.add_tab(folder, "c", "c").unwrap();
        sidebar.toggle_folder(inner).unwrap();
        assert_eq!(sidebar.folder_tabs(folder), Ok(vec![first, nested, last]));
        assert_eq!(sidebar.folder_tabs(inner), Ok(vec![nested]));
        assert_eq!(sidebar.folder_tabs(first), Err(Error::NotAFolder));
        assert_eq!(sidebar.folder_tabs(NodeId(999)), Err(Error::UnknownNode));
    }
    #[test]
    fn collapses_a_space_and_expands_it_again() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar).id;
        assert!(!sidebar.spaces()[0].collapsed);
        sidebar.toggle_space(home).unwrap();
        assert!(sidebar.spaces()[0].collapsed);
        sidebar.toggle_space(home).unwrap();
        assert!(!sidebar.spaces()[0].collapsed);
        assert_eq!(sidebar.toggle_space(SpaceId(999)), Err(Error::UnknownSpace));
    }

    #[test]
    fn a_space_saved_without_the_flag_is_expanded() {
        let json = r#"{ "id": 1, "title": "Home", "profile": 2, "pinned": 3, "today": 4,
            "selected": null }"#;
        let space: Space = serde_json::from_str(json).unwrap();
        assert!(!space.collapsed);
    }

    #[test]
    fn attaches_a_space_to_another_profile_and_its_favorites() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar).id;
        let work = sidebar.add_profile("Work");
        sidebar.set_space_profile(home, work).unwrap();
        assert_eq!(sidebar.spaces()[0].profile, work);
        assert_eq!(
            sidebar.favorites_of(home),
            Some(sidebar.profiles()[1].favorites)
        );
        assert_eq!(
            sidebar.set_space_profile(home, ProfileId(999)),
            Err(Error::UnknownProfile)
        );
        assert_eq!(
            sidebar.set_space_profile(SpaceId(999), work),
            Err(Error::UnknownSpace)
        );
    }

    #[test]
    fn moves_a_space_in_sidebar_order_keeping_the_displayed_one() {
        let mut sidebar = Sidebar::default();
        let profile = sidebar.profiles()[0].id;
        let home = sidebar.spaces()[0].id;
        let work = sidebar.add_space("Work", profile).unwrap();
        let play = sidebar.add_space("Play", profile).unwrap();
        let order = |sidebar: &Sidebar| sidebar.spaces().iter().map(|s| s.id).collect::<Vec<_>>();
        sidebar.move_space(home, 2).unwrap();
        assert_eq!(order(&sidebar), [work, play, home]);
        sidebar.move_space(play, 0).unwrap();
        assert_eq!(order(&sidebar), [play, work, home]);
        sidebar.move_space(play, usize::MAX).unwrap();
        assert_eq!(order(&sidebar), [work, home, play]);
        assert_eq!(sidebar.displayed().id, home);
        assert_eq!(
            sidebar.move_space(SpaceId(999), 0),
            Err(Error::UnknownSpace)
        );
    }

    #[test]
    fn deletes_a_space_with_its_tabs_and_displays_its_neighbor() {
        let mut sidebar = Sidebar::default();
        let profile = sidebar.profiles()[0].id;
        let home = home(&sidebar).id;
        let work = sidebar.add_space("Work", profile).unwrap();
        let books = sidebar.add_space("Books", profile).unwrap();
        let tab = sidebar.open_tab(work, "a").unwrap();
        let work_space = sidebar.spaces()[1].clone();
        let (folder, pinned) = folder_with_tab(&mut sidebar, &work_space, "f");
        let nodes = sidebar.nodes.len();
        sidebar.display(work).unwrap();
        sidebar.delete_space(work).unwrap();
        assert_eq!(sidebar.displayed().id, books);
        assert_eq!(sidebar.spaces().len(), 2);
        assert_eq!(sidebar.nodes.len(), nodes - 5);
        for id in [tab, folder, pinned] {
            assert!(sidebar.node(id).is_none());
        }
        sidebar.delete_space(books).unwrap();
        assert_eq!(sidebar.displayed().id, home);
        assert_eq!(sidebar.delete_space(home), Err(Error::LastSpace));
        assert_eq!(sidebar.delete_space(work), Err(Error::UnknownSpace));
    }

    #[test]
    fn deleting_another_space_keeps_the_displayed_one() {
        let mut sidebar = Sidebar::default();
        let profile = sidebar.profiles()[0].id;
        let work = sidebar.add_space("Work", profile).unwrap();
        sidebar.display(work).unwrap();
        sidebar.delete_space(home(&sidebar).id).unwrap();
        assert_eq!(sidebar.displayed().id, work);
    }

    #[test]
    fn splits_a_tab_with_another_in_its_place() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let [a, b, c] = ["a", "b", "c"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        let split = sidebar.split(c, a).unwrap();
        assert_eq!(children(&sidebar, home.today), [split, b]);
        assert_eq!(children(&sidebar, split), [a, c]);
        assert_eq!(sidebar.node(c).unwrap().parent, Some(split));
        assert_eq!((sidebar.row_of(a), sidebar.row_of(b)), (split, b));
        assert_eq!(sidebar.next_tab(home.id, true), Some(a));
        assert_eq!(sidebar.split(a, c), Err(Error::NotSplittable));
        assert_eq!(sidebar.split(b, b), Err(Error::NotSplittable));
        assert_eq!(sidebar.split(split, b), Err(Error::NotATab));
    }

    #[test]
    fn a_split_takes_up_to_four_tabs_each_with_an_equal_part() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let [a, b, c, d, e] =
            ["a", "b", "c", "d", "e"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        let split = sidebar.split(b, a).unwrap();
        assert_eq!(sidebar.split_layout(split).factors, [0.5, 0.5]);
        sidebar.set_split_factors(split, &[3.0, 1.0]).unwrap();
        assert_eq!(sidebar.split(c, a), Ok(split));
        assert_eq!(children(&sidebar, split), [a, c, b]);
        let factors = sidebar.split_layout(split).factors;
        assert!((factors[1] - 1.0 / 3.0).abs() < 1e-9);
        assert!((factors[0] / factors[2] - 3.0).abs() < 1e-9);
        sidebar.split(d, b).unwrap();
        assert_eq!(sidebar.split(e, d), Err(Error::SplitFull));
        assert_eq!(children(&sidebar, split), [a, c, b, d]);
        assert_eq!(sidebar.split_layout(split).factors[3], 0.25);
        sidebar.separate(a).unwrap();
        let factors = sidebar.split_layout(split).factors;
        assert!((factors.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        assert_eq!(factors.len(), 3);
    }

    #[test]
    fn a_tab_joins_a_split_before_its_target_or_after() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let [a, b, c] = ["a", "b", "c"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        let split = sidebar.split_beside(b, a, true).unwrap();
        assert_eq!(children(&sidebar, split), [b, a]);
        sidebar.split_beside(c, a, true).unwrap();
        assert_eq!(children(&sidebar, split), [b, c, a]);
    }

    #[test]
    fn separates_or_removes_a_whole_split() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let [a, b, c, d] = ["a", "b", "c", "d"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        let split = sidebar.split(b, a).unwrap();
        sidebar.split(c, b).unwrap();
        sidebar.separate_all(split).unwrap();
        assert_eq!(children(&sidebar, home.today), [a, b, c, d]);
        assert!(sidebar.node(split).is_none());
        let split = sidebar.split(b, a).unwrap();
        let removed = sidebar.remove_split(split).unwrap();
        assert_eq!(removed.len(), 2);
        assert_eq!(children(&sidebar, home.today), [c, d]);
        assert_eq!(sidebar.remove_split(c), Err(Error::UnknownNode));
    }

    #[test]
    fn keeps_a_split_stacked_and_its_parts_across_sessions() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let [a, b] = ["a", "b"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        let split = sidebar.split(b, a).unwrap();
        sidebar.set_split_stacked(split, true).unwrap();
        sidebar.set_split_factors(split, &[1.0, 3.0]).unwrap();
        assert_eq!(
            sidebar.set_split_factors(split, &[1.0]),
            Err(Error::NotSplittable)
        );
        assert_eq!(sidebar.set_split_stacked(a, true), Err(Error::UnknownNode));
        let saved: Sidebar =
            serde_json::from_str(&serde_json::to_string(&sidebar).unwrap()).unwrap();
        assert_eq!(
            saved.split_layout(split),
            SplitLayout {
                stacked: true,
                factors: vec![0.25, 0.75]
            }
        );
        sidebar.separate(b).unwrap();
        assert!(sidebar.splits.is_empty());
    }

    #[test]
    fn makes_a_split_a_favorite_but_splits_no_favorite_nor_moves_into_a_split() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        let [a, b, c] = ["a", "b", "c"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        sidebar.move_node(a, favorites, 0).unwrap();
        assert_eq!(sidebar.split(b, a), Err(Error::NotSplittable));
        let split = sidebar.split(a, b).unwrap();
        assert!(children(&sidebar, favorites).is_empty());
        assert_eq!(sidebar.move_node(c, split, 0), Err(Error::NotAContainer));
        assert_eq!(sidebar.add_tab(split, "d", "d"), Err(Error::NotAContainer));
        sidebar.move_node(split, favorites, 0).unwrap();
        assert_eq!(children(&sidebar, favorites), [split]);
        assert!(sidebar.is_favorite(a) && sidebar.is_favorite(b));
        let folder = sidebar.add_folder(home.pinned, "f").unwrap();
        assert_eq!(sidebar.move_node(folder, favorites, 0), Err(Error::NotATab));
    }

    #[test]
    fn a_split_left_with_one_tab_becomes_that_tab() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let [a, b, c, d] = ["a", "b", "c", "d"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        let first = sidebar.split(b, a).unwrap();
        assert_eq!(sidebar.next_after_close(a), Some(b));
        sidebar.remove(a).unwrap();
        assert_eq!(children(&sidebar, home.today), [b, c, d]);
        assert_eq!(sidebar.node(b).unwrap().parent, Some(home.today));
        assert!(sidebar.node(first).is_none());
        let second = sidebar.split(d, c).unwrap();
        sidebar.split(c, b).unwrap();
        assert_eq!(children(&sidebar, home.today)[1], d);
        assert!(sidebar.node(second).is_none());
    }

    #[test]
    fn moves_pins_and_closes_a_split_as_one_row() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let work = sidebar.add_space("Work", home.profile).unwrap();
        let [a, b, c] = ["a", "b", "c"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        let split = sidebar.split(b, a).unwrap();
        sidebar.toggle_pin(home.id, b).unwrap();
        assert_eq!(children(&sidebar, home.pinned), [split]);
        assert_eq!(saved_url(&mut sidebar, a).as_deref(), Some("a"));
        assert_eq!(sidebar.close_in_place(home.id, a), Ok(Some(c)));
        assert_eq!(children(&sidebar, home.pinned), [b, a]);
        let split = sidebar.split(a, b).unwrap();
        sidebar.select(home.id, a).unwrap();
        sidebar.move_to_space(b, work).unwrap();
        assert_eq!(children(&sidebar, sidebar.spaces()[1].today), [split]);
        assert_eq!(saved_url(&mut sidebar, a), None);
        assert_eq!(sidebar.spaces()[0].selected, None);
    }

    #[test]
    fn duplicates_a_tab_at_an_index_and_keeps_the_original() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let [a, b] = ["a", "b"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        sidebar.tab_mut(a).unwrap().title = "A".into();
        sidebar.tab_mut(a).unwrap().favicon = Some("a.ico".into());
        let copy = sidebar.duplicate(a, home.today, 1).unwrap();
        assert_eq!(children(&sidebar, home.today), [a, copy, b]);
        let tab = sidebar.tab_mut(copy).unwrap();
        assert_eq!((tab.url.as_str(), tab.title.as_str()), ("a", "A"));
        assert_eq!(tab.favicon.as_deref(), Some("a.ico"));
        let pinned = sidebar.duplicate(b, home.pinned, usize::MAX).unwrap();
        assert_eq!(children(&sidebar, home.pinned), [pinned]);
        assert_eq!(saved_url(&mut sidebar, pinned).as_deref(), Some("b"));
        assert_eq!(saved_url(&mut sidebar, b), None);
        let split = sidebar.split(b, a).unwrap();
        assert_eq!(sidebar.duplicate(split, home.today, 0), Err(Error::NotATab));
        assert_eq!(sidebar.duplicate(a, split, 0), Err(Error::NotAContainer));
    }

    #[test]
    fn splits_a_copy_of_a_tab_with_another_and_keeps_the_original() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let favorites = sidebar.profiles()[0].favorites;
        let [a, b, c] = ["a", "b", "c"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        let split = sidebar.split_duplicate(c, a, false).unwrap();
        let copy = children(&sidebar, split)[1];
        assert_eq!(children(&sidebar, home.today), [split, b, c]);
        assert_eq!(children(&sidebar, split), [a, copy]);
        assert_eq!(sidebar.tab_mut(copy).unwrap().url, "c");
        sidebar.move_node(b, favorites, 0).unwrap();
        let count = sidebar.tabs().count();
        assert_eq!(
            sidebar.split_duplicate(c, b, false),
            Err(Error::NotSplittable)
        );
        assert_eq!(
            sidebar.split_duplicate(c, a, false),
            Err(Error::NotAContainer)
        );
        assert_eq!(sidebar.tabs().count(), count);
    }

    #[test]
    fn separates_a_tab_from_its_split_as_the_row_after_it() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let [a, b, c] = ["a", "b", "c"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        let split = sidebar.split(c, a).unwrap();
        sidebar.separate(a).unwrap();
        assert_eq!(children(&sidebar, home.today), [c, a, b]);
        assert!(sidebar.node(split).is_none());
        sidebar.separate(b).unwrap();
        assert_eq!(children(&sidebar, home.today), [c, a, b]);
        assert_eq!(sidebar.separate(home.today), Err(Error::NotATab));
    }

    #[test]
    fn a_tab_removed_from_its_split_goes_back_to_its_place_beside_its_neighbor() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let [a, b, c] = ["a", "b", "c"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        let split = sidebar.split(b, a).unwrap();
        sidebar.split(c, b).unwrap();
        assert_eq!(sidebar.split_neighbor(a), Some((b, true)));
        assert_eq!(sidebar.split_neighbor(b), Some((a, false)));
        assert_eq!(sidebar.split_neighbor(split), None);
        // The middle tab, then the first: the split keeps its order.
        for tab in [b, a] {
            let (target, before) = sidebar.split_neighbor(tab).unwrap();
            sidebar.remove(tab).unwrap();
            let again = sidebar.open_tab(home.id, "again").unwrap();
            sidebar.split_beside(again, target, before).unwrap();
            assert_eq!(sidebar.split_neighbor(again), Some((target, before)));
        }
        // A split left with one tab comes back around it.
        let [x, y] = ["x", "y"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        sidebar.split(y, x).unwrap();
        let (target, before) = sidebar.split_neighbor(y).unwrap();
        sidebar.remove(y).unwrap();
        assert_eq!(sidebar.split_neighbor(x), None);
        let again = sidebar.open_tab(home.id, "y").unwrap();
        let split = sidebar.split_beside(again, target, before).unwrap();
        assert_eq!(children(&sidebar, split), [x, again]);
    }

    #[test]
    fn a_split_survives_saving_and_older_sessions_still_load() {
        let mut sidebar = Sidebar::default();
        let home = home(&sidebar);
        let [a, b] = ["a", "b"].map(|url| sidebar.open_tab(home.id, url).unwrap());
        let before = serde_json::to_string(&sidebar).unwrap();
        assert_eq!(serde_json::from_str::<Sidebar>(&before).unwrap(), sidebar);
        sidebar.split(b, a).unwrap();
        let json = serde_json::to_string(&sidebar).unwrap();
        assert_eq!(serde_json::from_str::<Sidebar>(&json).unwrap(), sidebar);
    }

    #[test]
    fn keeps_an_incognito_space_unlisted_unsaved_and_apart() {
        let mut sidebar = Sidebar::default();
        let home = sidebar.spaces()[0].id;
        let tab = sidebar.open_tab(home, "https://a.test").unwrap();
        let incognito = sidebar.open_incognito();
        let secret = sidebar.open_tab(incognito, "https://secret.test").unwrap();
        assert!(sidebar.is_incognito(incognito));
        assert_eq!(sidebar.spaces().len(), 1);
        assert_eq!(sidebar.profiles().len(), 1);
        // A Space created later is listed before it.
        let work = sidebar
            .create_space("Work", sidebar.profiles()[0].id)
            .unwrap();
        assert_eq!(
            sidebar.spaces().iter().map(|s| s.id).collect::<Vec<_>>(),
            [home, work]
        );
        // Shown in its window, it has no neighbor and takes nothing in or out.
        sidebar.display(incognito).unwrap();
        assert_eq!(sidebar.next_space(false), None);
        assert_eq!(sidebar.space_at(1), Some(home));
        assert!(sidebar.move_to_space(tab, incognito).is_err());
        assert!(sidebar.move_to_space(secret, home).is_err());
        assert!(sidebar.delete_space(incognito).is_err());
        assert!(sidebar.reaches(incognito, incognito));
        assert!(!sidebar.reaches(incognito, home));
        assert!(!sidebar.reaches(home, incognito));
        assert!(sidebar.reaches(home, work));
        // Never saved.
        let saved = sidebar.saved();
        assert!(
            saved
                .tabs()
                .all(|(_, tab)| tab.url != "https://secret.test")
        );
        assert_eq!(saved.spaces().len(), 2);
        assert!(saved.space(incognito).is_err());
        // Gone with its window.
        sidebar.close_incognito(incognito).unwrap();
        assert!(sidebar.node(secret).is_none());
        assert_eq!(sidebar.displayed().id, home);
        assert_eq!(sidebar, saved);
    }
}

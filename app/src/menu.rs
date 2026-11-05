//! The native menus of the sidebar's rows and Spaces, built from the sidebar: the app shows them
//! (app/src/mac.rs) and runs the item picked (shell::perform).

use crate::command::{Command, SiteData, Target};
use arbalete_core::sidebar::{
    MAX_FAVORITES, MAX_SPLIT_TABS, NodeId, NodeKind, Sidebar, Space, Tab,
};
use serde_json::json;

pub enum Entry {
    Item(Item),
    Submenu(String, Vec<Entry>),
    Separator,
}

pub struct Item {
    /// Names the item for the end-to-end checks (Command::ChooseMenu).
    pub id: String,
    pub title: String,
    pub checked: bool,
    pub enabled: bool,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Run(Command),
    /// Runs commands one after the other, like closing each of several tabs.
    RunEach(Vec<Command>),
    /// Asks the sidebar to do what only it does, like editing a name: a DOM event and its detail.
    Ask(&'static str, serde_json::Value),
    CopyLink(String),
    /// Shows the system's share menu for a link, at the pointer.
    Share(String),
    /// Asks for the url a pinned tab goes back to, in a dialog holding the current one.
    EditPinnedUrl {
        tab: NodeId,
        url: String,
    },
    /// Runs a command once the user confirms it, with a button named `button`.
    Confirm {
        question: String,
        detail: &'static str,
        button: &'static str,
        command: Command,
    },
}

/// The menu of what was right-clicked, if it has one.
pub fn build(sidebar: &Sidebar, target: Target) -> Option<Vec<Entry>> {
    match target {
        Target::Node(id) => match &sidebar.node(id)?.kind {
            NodeKind::Tab(tab) if sidebar.is_favorite(id) => Some(favorite(sidebar, id, &tab.url)),
            NodeKind::Tab(tab) => Some(tab_menu(sidebar, id, tab)),
            NodeKind::Folder { title, .. } => Some(folder(sidebar, id, title)),
            NodeKind::Split => Some(split_menu(sidebar, id)),
            NodeKind::Container => None,
        },
        Target::Space(id) => {
            let space = sidebar.spaces().iter().find(|space| space.id == id)?;
            Some(space_menu(sidebar, space))
        }
        Target::SpaceProfile(id) => {
            let space = sidebar.spaces().iter().find(|space| space.id == id)?;
            Some(profiles(sidebar, space))
        }
        Target::Site => {
            let tab = sidebar.displayed().selected?;
            match &sidebar.node(tab)?.kind {
                NodeKind::Tab(tab) if is_web(&tab.url) => Some(site_menu(&tab.url)),
                _ => None,
            }
        }
        // Its menu needs the extension's own state, which the shell knows (show_menu).
        Target::Pinned(_) => None,
    }
}

/// The selected tab's site controls, like Arc's: its link, and what the site keeps.
fn site_menu(url: &str) -> Vec<Entry> {
    vec![
        copy_link(url),
        item("share", "Share…", Action::Share(url.to_string())),
        Entry::Separator,
        item(
            "clear-cache",
            "Clear Cache",
            Action::Run(Command::ClearSiteData(SiteData::Cache)),
        ),
        item(
            "clear-cookies",
            "Clear Cookies",
            Action::Run(Command::ClearSiteData(SiteData::Cookies)),
        ),
    ]
}

/// An extension's menu, from its button or its tile in the site controls, like Arc's: its name,
/// which opens its Web Store page, its options page, removing it, pinning it by the address or
/// unpinning it, and the library to manage it.
pub fn extension_menu(id: &str, name: &str, options: bool, pinned: bool) -> Vec<Entry> {
    let store = format!("https://chromewebstore.google.com/detail/{id}");
    let mut menu = vec![
        item("store", name, Action::Run(Command::Open(store))),
        Entry::Separator,
    ];
    if options {
        menu.push(item(
            "options",
            "Options",
            Action::Run(Command::ExtensionOptions(id.to_string())),
        ));
    }
    // The app or Chromium asks first (Command::RemoveExtension).
    menu.push(item(
        "remove",
        "Remove from Arbalete…",
        Action::Run(Command::RemoveExtension(id.to_string())),
    ));
    let pin = if pinned { "Unpin" } else { "Pin" };
    menu.push(item(
        "pin",
        pin,
        Action::Run(Command::PinExtension(id.to_string(), !pinned)),
    ));
    // Like Arc's, Chromium's page of the extension: what it may read and change, and its
    // permissions.
    let page = format!("chrome://extensions/?id={id}");
    menu.push(Entry::Separator);
    menu.push(item(
        "manage",
        "Manage Extension",
        Action::Run(Command::Open(page.clone())),
    ));
    menu.push(item(
        "permissions",
        "View Web Permissions",
        Action::Run(Command::Open(page)),
    ));
    menu
}

/// Only web pages have a site to control.
fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// The action of the item named `id`, in submenus too.
pub fn find(entries: Vec<Entry>, id: &str) -> Option<Action> {
    entries.into_iter().find_map(|entry| match entry {
        Entry::Item(item) if item.id == id => Some(item.action),
        Entry::Submenu(_, entries) => find(entries, id),
        _ => None,
    })
}

/// The menu of several tabs selected at once, like Arc's: their links, moving and closing them all.
pub fn tabs_menu(sidebar: &Sidebar, ids: &[NodeId]) -> Vec<Entry> {
    let urls: Vec<&str> = ids
        .iter()
        .filter_map(|&id| match &sidebar.node(id)?.kind {
            NodeKind::Tab(tab) => Some(tab.url.as_str()),
            _ => None,
        })
        .collect();
    let spaces = sidebar
        .spaces()
        .iter()
        .map(|space| {
            Entry::Item(Item {
                id: format!("move/{}", serde_json::to_string(&space.id).expect("an id")),
                title: match &space.emoji {
                    // A symbol shows only in the sidebar.
                    Some(emoji) if !emoji.starts_with("symbol:") => {
                        format!("{emoji}  {}", space.title)
                    }
                    _ => space.title.clone(),
                },
                checked: false,
                enabled: true,
                action: Action::RunEach(
                    ids.iter()
                        .map(|&id| Command::MoveToSpace(id, space.id))
                        .collect(),
                ),
            })
        })
        .collect();
    vec![
        item(
            "copy-links",
            "Copy Links",
            Action::CopyLink(urls.join("\n")),
        ),
        Entry::Separator,
        Entry::Submenu("Move to".to_string(), spaces),
        Entry::Separator,
        item(
            "close",
            &format!("Close {} Tabs", urls.len()),
            Action::RunEach(ids.iter().map(|&id| Command::Close(Some(id))).collect()),
        ),
    ]
}

fn tab_menu(sidebar: &Sidebar, id: NodeId, tab: &Tab) -> Vec<Entry> {
    let pinned = sidebar.is_pinned(id);
    let pin = if pinned { "Unpin Tab" } else { "Pin Tab" };
    let mute = if tab.muted { "Unmute Tab" } else { "Mute Tab" };
    // In Arc's order: the link first, then the tab's own settings.
    let mut menu = vec![
        copy_link(&tab.url),
        item(
            "copy-markdown",
            "Copy Link as Markdown",
            Action::CopyLink(format!("[{}]({})", tab.label(), tab.url)),
        ),
        item("share", "Share…", Action::Share(tab.url.clone())),
        Entry::Separator,
        item(
            "rename",
            "Rename Tab",
            Action::Ask("arbalete:rename", json!(id)),
        ),
        item("mute", mute, Action::Run(Command::ToggleMute(id))),
        Entry::Separator,
        item("pin", pin, Action::Run(Command::Pin(Some(id)))),
    ];
    if let Some(saved) = tab.saved_url.as_deref().filter(|_| pinned) {
        menu.push(Entry::Submenu(
            "Edit Pinned Page".to_string(),
            vec![
                Entry::Item(Item {
                    id: "pinned-url/current".to_string(),
                    title: "Replace Pinned URL with Current".to_string(),
                    checked: false,
                    enabled: saved != tab.url,
                    action: Action::Run(Command::SetPinnedUrl(id, tab.url.clone())),
                }),
                item(
                    "pinned-url/edit",
                    "Edit…",
                    Action::EditPinnedUrl {
                        tab: id,
                        url: saved.to_string(),
                    },
                ),
            ],
        ));
    }
    let tab = id;
    menu.extend([
        Entry::Separator,
        duplicate(tab),
        move_to(sidebar, tab),
        Entry::Separator,
        item("close", "Close Tab", Action::Run(Command::Close(Some(tab)))),
    ]);
    // Pinned tabs stay: only "Today" tabs close those below them.
    if !pinned {
        menu.push(Entry::Item(Item {
            id: "close-below".to_string(),
            title: "Close Tabs Below".to_string(),
            checked: false,
            enabled: has_rows_below(sidebar, sidebar.row_of(tab)),
            action: Action::Run(Command::CloseTabsBelow(tab)),
        }));
    }
    menu
}

/// A split row's menu, like Arc's: the split's own items, then each tab's as a submenu.
fn split_menu(sidebar: &Sidebar, split: NodeId) -> Vec<Entry> {
    let tabs: Vec<(NodeId, &Tab)> = sidebar
        .node(split)
        .map(|node| node.children.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|&id| match &sidebar.node(id)?.kind {
            NodeKind::Tab(tab) => Some((id, tab)),
            _ => None,
        })
        .collect();
    let (Some(&(first, _)), Some(&(last, _))) = (tabs.first(), tabs.last()) else {
        return Vec::new();
    };
    let flip = if sidebar.split_layout(split).stacked {
        "Convert to Horizontal Split View"
    } else {
        "Convert to Vertical Split View"
    };
    let mut menu = vec![
        item(
            "separate-all",
            "Separate All Tabs",
            Action::Run(Command::SeparateAll(split)),
        ),
        Entry::Item(Item {
            id: "add-tab".to_string(),
            title: "Add Tab…".to_string(),
            checked: false,
            enabled: tabs.len() < MAX_SPLIT_TABS,
            action: Action::Run(Command::AddPane(Some(last))),
        }),
        item("flip", flip, Action::Run(Command::FlipPanes(Some(split)))),
        Entry::Separator,
        Entry::Item(Item {
            id: "tab-options".to_string(),
            title: "Tab Options".to_string(),
            checked: false,
            enabled: false,
            action: Action::Run(Command::Select(first)),
        }),
    ];
    menu.extend(tabs.iter().map(|&(id, tab)| {
        let mute = if tab.muted { "Unmute" } else { "Mute" };
        Entry::Submenu(
            tab.label().to_string(),
            vec![
                item(
                    "separate",
                    "Separate from Split View",
                    Action::Run(Command::Separate(id)),
                ),
                Entry::Separator,
                item("share", "Share…", Action::Share(tab.url.clone())),
                item("mute", mute, Action::Run(Command::ToggleMute(id))),
                copy_link(&tab.url),
                Entry::Separator,
                item("close", "Close Tab", Action::Run(Command::Close(Some(id)))),
            ],
        )
    }));
    menu.extend([
        Entry::Separator,
        duplicate(split),
        move_to(sidebar, first),
        Entry::Separator,
        item(
            "close-split",
            "Close Split View",
            Action::Run(Command::CloseSplit(split)),
        ),
        Entry::Item(Item {
            id: "close-below".to_string(),
            title: "Close Tabs Below".to_string(),
            checked: false,
            enabled: has_rows_below(sidebar, split),
            action: Action::Run(Command::CloseTabsBelow(first)),
        }),
    ]);
    menu
}

/// The Spaces a tab or a folder may move to, its own checked, and a new one.
fn move_to(sidebar: &Sidebar, id: NodeId) -> Entry {
    let current = sidebar.space_of(id);
    let mut spaces: Vec<_> = sidebar
        .spaces()
        .iter()
        .map(|space| {
            let space_id = serde_json::to_string(&space.id).expect("an id");
            Entry::Item(Item {
                id: format!("move/{space_id}"),
                title: match &space.emoji {
                    // A symbol shows only in the sidebar.
                    Some(emoji) if !emoji.starts_with("symbol:") => {
                        format!("{emoji}  {}", space.title)
                    }
                    _ => space.title.clone(),
                },
                checked: Some(space.id) == current,
                enabled: true,
                action: Action::Run(Command::MoveToSpace(id, space.id)),
            })
        })
        .collect();
    spaces.extend([
        Entry::Separator,
        item(
            "new-space",
            "New Space…",
            Action::Ask("arbalete:new-space", json!(id)),
        ),
    ]);
    Entry::Submenu("Move to".to_string(), spaces)
}

fn duplicate(id: NodeId) -> Entry {
    item(
        "duplicate",
        "Duplicate",
        Action::Run(Command::DuplicateRow(id)),
    )
}

/// Rows follow `row` in its parent.
fn has_rows_below(sidebar: &Sidebar, row: NodeId) -> bool {
    let parent = sidebar.node(row).and_then(|node| node.parent);
    let siblings = parent.and_then(|parent| sidebar.node(parent));
    siblings.is_some_and(|parent| parent.children.last() != Some(&row))
}

fn space_menu(sidebar: &Sidebar, space: &Space) -> Vec<Entry> {
    let id = space.id;
    let edit = |field| {
        Action::Ask(
            "arbalete:edit-space",
            json!({ "space": id, "field": field }),
        )
    };
    // Like Arc's, at once, with no confirmation.
    let delete = Action::Run(Command::DeleteSpace(id));
    vec![
        item("icon", "Change Space Icon…", edit("emoji")),
        item("rename", "Rename Space…", edit("name")),
        item(
            "color",
            "Edit Theme Color…",
            Action::Run(Command::ShowThemePicker(id)),
        ),
        Entry::Submenu("Set Profile".to_string(), profiles(sidebar, space)),
        Entry::Separator,
        item(
            "new-folder",
            "New Folder",
            Action::Run(Command::NewFolder(Some(space.pinned))),
        ),
        Entry::Separator,
        item(
            "manage",
            "Manage Spaces…",
            Action::Ask("arbalete:library", json!(null)),
        ),
        Entry::Separator,
        Entry::Item(Item {
            id: "delete".to_string(),
            title: "Delete Space".to_string(),
            checked: false,
            // The browser always keeps a Space.
            enabled: sidebar.spaces().len() > 1,
            action: delete,
        }),
    ]
}

/// The profiles a Space may be attached to, its own checked, and a new one.
fn profiles(sidebar: &Sidebar, space: &Space) -> Vec<Entry> {
    let mut profiles: Vec<_> = sidebar
        .profiles()
        .iter()
        .map(|profile| {
            let profile_id = serde_json::to_string(&profile.id).expect("an id");
            Entry::Item(Item {
                id: format!("profile/{profile_id}"),
                title: profile.name.clone(),
                checked: profile.id == space.profile,
                enabled: true,
                action: Action::Run(Command::SetSpaceProfile(space.id, profile.id)),
            })
        })
        .collect();
    profiles.extend([
        Entry::Separator,
        item(
            "new-profile",
            "New Profile…",
            Action::Run(Command::NewSpaceProfile(space.id)),
        ),
    ]);
    profiles
}

fn folder(sidebar: &Sidebar, folder: NodeId, title: &str) -> Vec<Entry> {
    let delete = Action::Confirm {
        question: format!("Delete the folder “{title}”?"),
        detail: "Its tabs will be closed.",
        button: "Delete",
        command: Command::DeleteFolder(folder),
    };
    vec![
        item(
            "rename",
            "Rename Folder",
            Action::Ask("arbalete:rename", json!(folder)),
        ),
        item(
            "new-folder",
            "New Folder",
            Action::Run(Command::NewFolder(Some(folder))),
        ),
        item(
            "to-space",
            &format!("Turn “{title}” into Space"),
            Action::Run(Command::FolderToSpace(folder)),
        ),
        Entry::Separator,
        duplicate(folder),
        move_to(sidebar, folder),
        Entry::Separator,
        item(
            "close-tabs",
            "Close Tabs in Folder",
            Action::Run(Command::CloseFolderTabs(folder)),
        ),
        item("delete", "Delete Folder", delete),
    ]
}

fn favorite(sidebar: &Sidebar, tab: NodeId, url: &str) -> Vec<Entry> {
    let parent = sidebar.node(tab).and_then(|node| node.parent);
    let count = parent.and_then(|parent| sidebar.node(parent));
    vec![
        copy_link(url),
        Entry::Item(Item {
            id: "duplicate".to_string(),
            title: "Duplicate".to_string(),
            checked: false,
            // The favorites hold at most MAX_FAVORITES tabs.
            enabled: count.is_some_and(|favorites| favorites.children.len() < MAX_FAVORITES),
            action: Action::Run(Command::DuplicateRow(tab)),
        }),
        Entry::Separator,
        item(
            "remove",
            "Remove from Favorites",
            Action::Run(Command::RemoveFavorite(tab)),
        ),
    ]
}

/// The developer bar's extensions menu, like Arc's: the profile's extensions turned on, by id and
/// name, whose popup each opens, then adding and managing them.
pub fn extensions_menu(extensions: &[(String, String)]) -> Vec<Entry> {
    let mut menu: Vec<Entry> = extensions
        .iter()
        .map(|(id, name)| {
            item(
                id,
                name,
                Action::Run(Command::OpenExtensionPopup(id.clone())),
            )
        })
        .collect();
    if !menu.is_empty() {
        menu.push(Entry::Separator);
    }
    menu.push(item(
        "add",
        "Add Extension…",
        Action::Run(Command::ShowStore),
    ));
    menu.push(item(
        "manage",
        "Manage Extensions…",
        Action::Run(Command::ManageExtensions),
    ));
    menu
}

fn copy_link(url: &str) -> Entry {
    item("copy-link", "Copy Link", Action::CopyLink(url.to_string()))
}

fn item(id: &str, title: &str, action: Action) -> Entry {
    Entry::Item(Item {
        id: id.to_string(),
        title: title.to_string(),
        checked: false,
        enabled: true,
        action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_menu_is_arcs() {
        let menu = extension_menu("a", "Dark Reader", true, true);
        assert_eq!(
            titles(&menu),
            [
                "Dark Reader",
                "-",
                "Options",
                "Remove from Arbalete…",
                "Unpin",
                "-",
                "Manage Extension",
                "View Web Permissions"
            ]
        );
        assert!(matches!(
            find(extension_menu("a", "Dark Reader", true, true), "manage"),
            Some(Action::Run(Command::Open(url))) if url == "chrome://extensions/?id=a"
        ));
        assert!(matches!(
            find(menu, "store"),
            Some(Action::Run(Command::Open(url)))
                if url == "https://chromewebstore.google.com/detail/a"
        ));
        let menu = extension_menu("a", "Dark Reader", false, false);
        assert!(!titles(&menu).contains(&"Options".to_string()));
        assert!(matches!(
            find(menu, "pin"),
            Some(Action::Run(Command::PinExtension(_, true)))
        ));
    }

    #[test]
    fn extensions_menu_opens_their_popups_then_adds_and_manages() {
        let menu = extensions_menu(&[("a".into(), "Dark Reader".into())]);
        assert_eq!(
            titles(&menu),
            ["Dark Reader", "-", "Add Extension…", "Manage Extensions…"]
        );
        assert!(matches!(
            find(menu, "a"),
            Some(Action::Run(Command::OpenExtensionPopup(id))) if id == "a"
        ));
        assert_eq!(
            titles(&extensions_menu(&[])),
            ["Add Extension…", "Manage Extensions…"]
        );
    }

    /// The titles of a menu, a submenu's items after its title, `-` for a separator, `✓` before
    /// a checked item.
    fn titles(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .flat_map(|entry| match entry {
                Entry::Item(item) if item.checked => vec![format!("✓ {}", item.title)],
                Entry::Item(item) if !item.enabled => vec![format!("({})", item.title)],
                Entry::Item(item) => vec![item.title.clone()],
                Entry::Submenu(title, entries) => {
                    let mut titles = vec![format!("{title} ▸")];
                    titles.extend(titles_of(entries));
                    titles
                }
                Entry::Separator => vec!["-".to_string()],
            })
            .collect()
    }

    fn titles_of(entries: &[Entry]) -> Vec<String> {
        titles(entries)
            .into_iter()
            .map(|t| format!("  {t}"))
            .collect()
    }

    fn choose(sidebar: &Sidebar, id: NodeId, item: &str) -> Option<Action> {
        find(build(sidebar, Target::Node(id))?, item)
    }

    #[test]
    fn a_split_row_has_arcs_menu_with_one_submenu_per_tab() {
        let mut sidebar = Sidebar::default();
        let home = sidebar.spaces()[0].id;
        let [a, b] = ["https://a.test/", "https://b.test/"].map(|url| {
            let tab = sidebar.open_tab(home, url).unwrap();
            sidebar.tab_mut(tab).unwrap().title = url.trim_end_matches('/')[8..].to_string();
            tab
        });
        let split = sidebar.split(b, a).unwrap();
        let menu = build(&sidebar, Target::Node(split)).unwrap();
        assert_eq!(
            titles(&menu)[..6],
            [
                "Separate All Tabs",
                "Add Tab…",
                "Convert to Vertical Split View",
                "-",
                "(Tab Options)",
                "a.test ▸",
            ]
        );
        assert_eq!(
            choose(&sidebar, split, "separate-all"),
            Some(Action::Run(Command::SeparateAll(split)))
        );
        assert_eq!(
            choose(&sidebar, split, "separate"),
            Some(Action::Run(Command::Separate(a)))
        );
        assert_eq!(
            choose(&sidebar, split, "close-split"),
            Some(Action::Run(Command::CloseSplit(split)))
        );
        for url in ["c", "d"] {
            let tab = sidebar.open_tab(home, url).unwrap();
            sidebar.split(tab, b).unwrap();
        }
        let menu = build(&sidebar, Target::Node(split)).unwrap();
        assert_eq!(titles(&menu)[1], "(Add Tab…)");
    }

    #[test]
    fn a_tab_moves_to_any_space_its_own_checked() {
        let mut sidebar = Sidebar::default();
        let home = sidebar.spaces()[0].id;
        let work = sidebar.add_space("Work", sidebar.profiles()[0].id).unwrap();
        sidebar.set_space_emoji(work, "💼").unwrap();
        let tab = sidebar.open_tab(home, "https://example.com/").unwrap();
        let menu = build(&sidebar, Target::Node(tab)).unwrap();
        assert_eq!(
            titles(&menu),
            [
                "Copy Link",
                "Copy Link as Markdown",
                "Share…",
                "-",
                "Rename Tab",
                "Mute Tab",
                "-",
                "Pin Tab",
                "-",
                "Duplicate",
                "Move to ▸",
                "  ✓ Home",
                "  💼  Work",
                "  -",
                "  New Space…",
                "-",
                "Close Tab",
                "(Close Tabs Below)",
            ]
        );
        let work_id = serde_json::to_string(&work).unwrap();
        assert_eq!(
            choose(&sidebar, tab, &format!("move/{work_id}")),
            Some(Action::Run(Command::MoveToSpace(tab, work)))
        );
        assert_eq!(
            choose(&sidebar, tab, "copy-link"),
            Some(Action::CopyLink("https://example.com/".into()))
        );
        assert_eq!(
            choose(&sidebar, tab, "new-space"),
            Some(Action::Ask("arbalete:new-space", json!(tab)))
        );
        assert_eq!(
            choose(&sidebar, tab, "close"),
            Some(Action::Run(Command::Close(Some(tab))))
        );
        assert_eq!(
            choose(&sidebar, tab, "duplicate"),
            Some(Action::Run(Command::DuplicateRow(tab)))
        );
        assert_eq!(
            choose(&sidebar, tab, "rename"),
            Some(Action::Ask("arbalete:rename", json!(tab)))
        );
        assert_eq!(
            choose(&sidebar, tab, "mute"),
            Some(Action::Run(Command::ToggleMute(tab)))
        );
        sidebar.toggle_mute(tab).unwrap();
        let menu = build(&sidebar, Target::Node(tab)).unwrap();
        assert_eq!(titles(&menu)[5], "Unmute Tab");
        assert_eq!(choose(&sidebar, tab, "unknown"), None);
    }

    #[test]
    fn a_today_tab_closes_the_tabs_below_it_when_there_are_some() {
        let mut sidebar = Sidebar::default();
        let home = sidebar.spaces()[0].id;
        let tab = sidebar.open_tab(home, "https://a.test/").unwrap();
        sidebar.open_tab(home, "https://b.test/").unwrap();
        let menu = build(&sidebar, Target::Node(tab)).unwrap();
        assert_eq!(titles(&menu).last().unwrap(), "Close Tabs Below");
        assert_eq!(
            choose(&sidebar, tab, "close-below"),
            Some(Action::Run(Command::CloseTabsBelow(tab)))
        );
    }

    #[test]
    fn a_pinned_tab_unpins() {
        let mut sidebar = Sidebar::default();
        let pinned = sidebar.spaces()[0].pinned;
        let tab = sidebar.add_tab(pinned, "a", "a").unwrap();
        let menu = build(&sidebar, Target::Node(tab)).unwrap();
        assert_eq!(titles(&menu)[7], "Unpin Tab");
        // Its saved url is the current one: there is nothing to replace it with yet.
        assert_eq!(
            titles(&menu)[8..11],
            [
                "Edit Pinned Page ▸",
                "  (Replace Pinned URL with Current)",
                "  Edit…",
            ]
        );
        assert_eq!(
            choose(&sidebar, tab, "pinned-url/edit"),
            Some(Action::EditPinnedUrl {
                tab,
                url: "a".into()
            })
        );
        sidebar.tab_mut(tab).unwrap().url = "b".to_string();
        assert_eq!(
            choose(&sidebar, tab, "pinned-url/current"),
            Some(Action::Run(Command::SetPinnedUrl(tab, "b".into())))
        );
        // Pinned tabs stay: none closes those below it.
        assert_eq!(titles(&menu).last().unwrap(), "Close Tab");
        assert_eq!(
            choose(&sidebar, tab, "pin"),
            Some(Action::Run(Command::Pin(Some(tab))))
        );
    }

    #[test]
    fn a_folder_renames_holds_a_new_folder_moves_and_closes_its_tabs() {
        let mut sidebar = Sidebar::default();
        let pinned = sidebar.spaces()[0].pinned;
        let folder = sidebar.add_folder(pinned, "Folder").unwrap();
        let menu = build(&sidebar, Target::Node(folder)).unwrap();
        assert_eq!(
            titles(&menu),
            [
                "Rename Folder",
                "New Folder",
                "Turn “Folder” into Space",
                "-",
                "Duplicate",
                "Move to ▸",
                "  ✓ Home",
                "  -",
                "  New Space…",
                "-",
                "Close Tabs in Folder",
                "Delete Folder",
            ]
        );
        assert_eq!(
            choose(&sidebar, folder, "to-space"),
            Some(Action::Run(Command::FolderToSpace(folder)))
        );
        assert_eq!(
            choose(&sidebar, folder, "duplicate"),
            Some(Action::Run(Command::DuplicateRow(folder)))
        );
        let home = sidebar.spaces()[0].id;
        let home_id = serde_json::to_string(&home).unwrap();
        assert_eq!(
            choose(&sidebar, folder, &format!("move/{home_id}")),
            Some(Action::Run(Command::MoveToSpace(folder, home)))
        );
        assert_eq!(
            choose(&sidebar, folder, "delete"),
            Some(Action::Confirm {
                question: "Delete the folder “Folder”?".into(),
                detail: "Its tabs will be closed.",
                button: "Delete",
                command: Command::DeleteFolder(folder),
            })
        );
        assert_eq!(
            choose(&sidebar, folder, "rename"),
            Some(Action::Ask("arbalete:rename", json!(folder)))
        );
        assert_eq!(
            choose(&sidebar, folder, "new-folder"),
            Some(Action::Run(Command::NewFolder(Some(folder))))
        );
        assert_eq!(
            choose(&sidebar, folder, "close-tabs"),
            Some(Action::Run(Command::CloseFolderTabs(folder)))
        );
    }

    #[test]
    fn a_favorite_copies_its_link_or_leaves_the_favorites() {
        let mut sidebar = Sidebar::default();
        let favorites = sidebar.profiles()[0].favorites;
        let tab = sidebar.add_tab(favorites, "https://a.test/", "a").unwrap();
        let menu = build(&sidebar, Target::Node(tab)).unwrap();
        assert_eq!(
            titles(&menu),
            ["Copy Link", "Duplicate", "-", "Remove from Favorites"]
        );
        assert_eq!(
            choose(&sidebar, tab, "remove"),
            Some(Action::Run(Command::RemoveFavorite(tab)))
        );
    }

    #[test]
    fn a_space_edits_itself_changes_its_profile_and_deletes_itself() {
        let mut sidebar = Sidebar::default();
        let work = sidebar.create_profile("Work").unwrap();
        let home = sidebar.spaces()[0].clone();
        let target = Target::Space(home.id);
        let menu = build(&sidebar, target).unwrap();
        assert_eq!(
            titles(&menu),
            [
                "Change Space Icon…",
                "Rename Space…",
                "Edit Theme Color…",
                "Set Profile ▸",
                "  ✓ Default",
                "  Work",
                "  -",
                "  New Profile…",
                "-",
                "New Folder",
                "-",
                "Manage Spaces…",
                "-",
                "Delete Space",
            ]
        );
        let choose = |item: &str| find(build(&sidebar, target).unwrap(), item);
        let edit = |field| {
            Action::Ask(
                "arbalete:edit-space",
                json!({ "space": home.id, "field": field }),
            )
        };
        assert_eq!(choose("icon"), Some(edit("emoji")));
        assert_eq!(choose("rename"), Some(edit("name")));
        assert_eq!(
            choose("color"),
            Some(Action::Run(Command::ShowThemePicker(home.id)))
        );
        let work_id = serde_json::to_string(&work).unwrap();
        assert_eq!(
            choose(&format!("profile/{work_id}")),
            Some(Action::Run(Command::SetSpaceProfile(home.id, work)))
        );
        assert_eq!(
            choose("new-profile"),
            Some(Action::Run(Command::NewSpaceProfile(home.id)))
        );
        assert_eq!(
            choose("new-folder"),
            Some(Action::Run(Command::NewFolder(Some(home.pinned))))
        );
        assert_eq!(
            choose("manage"),
            Some(Action::Ask("arbalete:library", json!(null)))
        );
        assert_eq!(
            choose("delete"),
            Some(Action::Run(Command::DeleteSpace(home.id)))
        );
    }

    #[test]
    fn a_spaces_profile_pill_lists_the_profiles_its_own_checked() {
        let mut sidebar = Sidebar::default();
        let work = sidebar.create_profile("Work").unwrap();
        let home = sidebar.spaces()[0].id;
        let target = Target::SpaceProfile(home);
        let menu = build(&sidebar, target).unwrap();
        assert_eq!(titles(&menu), ["✓ Default", "Work", "-", "New Profile…"]);
        let work_id = serde_json::to_string(&work).unwrap();
        assert_eq!(
            find(menu, &format!("profile/{work_id}")),
            Some(Action::Run(Command::SetSpaceProfile(home, work)))
        );
    }

    #[test]
    fn the_selected_web_page_s_site_copies_shares_and_clears_its_data() {
        let mut sidebar = Sidebar::default();
        let home = sidebar.spaces()[0].id;
        assert!(build(&sidebar, Target::Site).is_none());
        let tab = sidebar.open_tab(home, "https://a.test/page").unwrap();
        sidebar.select(home, tab).unwrap();
        let menu = build(&sidebar, Target::Site).unwrap();
        assert_eq!(
            titles(&menu),
            ["Copy Link", "Share…", "-", "Clear Cache", "Clear Cookies"]
        );
        let choose = |item: &str| find(build(&sidebar, Target::Site).unwrap(), item);
        assert_eq!(
            choose("share"),
            Some(Action::Share("https://a.test/page".into()))
        );
        assert_eq!(
            choose("clear-cookies"),
            Some(Action::Run(Command::ClearSiteData(SiteData::Cookies)))
        );
        // A page that is not on the web has no site.
        let blank = sidebar.open_tab(home, "about:blank").unwrap();
        sidebar.select(home, blank).unwrap();
        assert!(build(&sidebar, Target::Site).is_none());
    }

    #[test]
    fn the_last_space_cannot_be_deleted() {
        let sidebar = Sidebar::default();
        let menu = build(&sidebar, Target::Space(sidebar.spaces()[0].id)).unwrap();
        assert_eq!(titles(&menu).last().unwrap(), "(Delete Space)");
    }

    #[test]
    fn roots_and_unknown_nodes_and_spaces_have_no_menu() {
        let sidebar = Sidebar::default();
        let today = sidebar.spaces()[0].today;
        assert!(build(&sidebar, Target::Node(today)).is_none());
        let unknown = serde_json::from_str("999").unwrap();
        assert!(build(&sidebar, Target::Node(unknown)).is_none());
        let unknown = serde_json::from_str("999").unwrap();
        assert!(build(&sidebar, Target::Space(unknown)).is_none());
    }
}

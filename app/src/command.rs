//! Commands the interface sends to the app as JSON (native::send): an object with the command's
//! name, and its arguments unless it takes none, like `{"name": "back"}`,
//! `{"name": "navigate", "args": "https://example.com/"}` or
//! `{"name": "rename", "args": [12, "Work"]}`, a struct-like command's as an object.

use arbalete_core::import::Choice;
use arbalete_core::search::SearchEngine;
use arbalete_core::sidebar::{
    ArchiveAfter, Color, DownloadLocation, NodeId, ProfileId, SpaceId, Theme,
};
use arbalete_core::url_routing::UrlRouting;
use serde::Deserialize;

/// What a site keeps that its site controls clear (Command::ClearSiteData).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SiteData {
    /// Its cache storage and service workers, the page then reloading from the network.
    Cache,
    Cookies,
}

/// What a Little Arbalete window's bar asks for (Command::Little): `"ready"`, `"focused"`,
/// `{"navigate": <url>}`, `{"open-in-space": <space id or null>}` or
/// `{"set-profile": <profile id>}`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LittleAction {
    /// The bar is listening: send it its page's address.
    Ready,
    /// The window came in front: its page is its profile's active tab for the extensions.
    Focused,
    /// Loads an address in the window's page.
    Navigate(String),
    /// Moves the page into a Space, the displayed one without, as a tab, and closes the window.
    OpenInSpace(Option<SpaceId>),
    /// Opens the page again in another profile.
    SetProfile(ProfileId),
}

/// What Peek's bar asks of its page (Command::PeekPage).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PeekAction {
    Back,
    Forward,
    Reload,
    Stop,
}

/// The UI view a button shows in (Command::ShowSiteControls).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ButtonView {
    Docked,
    Revealed,
    Toolbar,
}

/// Where a button is, in CSS pixels from its view's top left corner.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Button {
    pub view: ButtonView,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// A change to a shortcut, from the settings' Shortcuts pane: `{"set": [<id>, <key>,
/// <modifiers>]}`, `{"remove": <id>}`, `{"reset": <id>}`, `{"priority": [<id>, <priority>]}` or
/// `"reset-all"`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShortcutEdit {
    /// Its keys: the key equivalent and AppKit's modifier mask.
    Set(String, String, u64),
    Remove(String),
    Reset(String),
    Priority(String, crate::settings::Priority),
    ResetAll,
}

/// The app's settings the settings window turns on or off (app/src/settings.rs).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Setting {
    WarnBeforeQuit,
    LittleHotkey,
    RestoreSession,
    SuspendTabs,
}

/// A panel of the page's developer tools (Command::DevToolsPanel).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InspectorPanel {
    Console,
    Network,
    Elements,
}

impl InspectorPanel {
    /// Its name in the developer tools.
    pub fn name(self) -> &'static str {
        match self {
            Self::Console => "console",
            Self::Network => "network",
            Self::Elements => "elements",
        }
    }
}

/// A command, named like its variant in kebab case, `ShowSiteControls` as `show-site-controls`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "name", content = "args", rename_all = "kebab-case")]
pub enum Command {
    /// The UI is listening: send it the current state.
    Ready,
    Back,
    Forward,
    Reload,
    /// Reloads the page without its cache, from the menu or the sidebar's "⋯".
    ForceReload,
    /// Stops loading the page, from the menu.
    Stop,
    /// Zooms the page, from the menu.
    Zoom(Zoom),
    /// Prints the page, from the menu.
    Print,
    /// Clears what the selected tab's site keeps, then reloads it, from its site controls.
    ClearSiteData(SiteData),
    /// Opens the page's source in a new tab, from the menu.
    ViewSource,
    /// Shows Chromium's developer tools for the page in a window of their own, or closes them,
    /// from the menu; from the page's menu, on the element at a point of the page.
    DevTools(Option<(i32, i32)>),
    /// Shows the page's developer tools on a panel, from the developer bar.
    DevToolsPanel(InspectorPanel),
    /// Captures the shown page, like Arc's: framed on a color, from the developer bar; or to pick
    /// a part of, which the interface then copies, saves or edits.
    Capture,
    CaptureArea,
    /// Keeps a capture, a picture saved at a temporary path, in the library's Media, like Arc's
    /// Save to Library.
    SaveCaptureToLibrary(std::path::PathBuf),
    /// Copies the whole page, from the site controls.
    CaptureFull,
    /// Shows the selected tab's site's settings, Chromium's page, like Arc's.
    ShowSiteSettings,
    /// Scrolls the page under a capture's veil, by `dx` and `dy` at `x` and `y`, in its points.
    ScrollCapture(f64, f64, f64, f64),
    /// Shows the find bar above the page, from the menu.
    ShowFind,
    /// Finds text in the page, forward or backward. The same text again goes to the next match.
    /// From the menu, the last text, `None`.
    Find {
        text: Option<String>,
        forward: bool,
    },
    /// Hides the find bar and stops finding.
    HideFind,
    /// Shows the selected tab's site controls in a panel under their button, like Arc's.
    ShowSiteControls(Button),
    HideSiteControls,
    /// Shows a Space's theme picker, beside the sidebar; or, from the Spaces view, beside the
    /// Space's column, between its left and right edges from the window's left edge.
    ShowThemePicker(SpaceId, Option<(i32, i32)>),
    HideThemePicker,
    /// The site controls' panel takes this height.
    SiteControlsHeight(i32),
    /// Answers a site's permission prompt, by its id: allowed or not.
    AnswerPermission(u64, bool),
    /// The permission prompt takes this height.
    PermissionPromptHeight(i32),
    /// Allows or blocks a permission for the selected tab's site, from its site controls, by the
    /// content setting that keeps it.
    SetSitePermission(u32, bool),
    /// Forgets what the selected tab's site was allowed or blocked.
    ResetSitePermissions,
    /// Answers a site's or a proxy's request to sign in, by its id, with a user name and a
    /// password, or turns it down.
    AnswerAuth(u64, Option<(String, String)>),
    /// The sign-in prompt takes this height.
    AuthPromptHeight(i32),
    /// Opens an extension's popup from the site controls, under their button, by its id.
    OpenExtensionPopup(String),
    CloseExtensionPopup,
    /// Opens the Chrome Web Store, to install extensions.
    ShowStore,
    /// Shows the shown page's certificate, like Arc's.
    ShowCertificate,
    /// Opens Chromium's extensions page, like Arc's Manage Extensions.
    ManageExtensions,
    /// Where the site controls' panel shows its extensions, where a right-click opens their menu,
    /// like `[{"id":"abc","x":12,"y":80,"width":64,"height":40}]`.
    SiteTargets(Vec<ExtensionTarget>),
    /// Removes an extension from the displayed Space's profile, from its menu.
    RemoveExtension(String),
    /// Opens an extension's options page in a tab, from its menu.
    ExtensionOptions(String),
    /// Turns an extension of the displayed Space's profile on or off, from its menu.
    SetExtensionEnabled(String, bool),
    /// Pins an extension of the displayed Space's profile by the address, or unpins it, from its
    /// menu.
    PinExtension(String, bool),
    /// Opens a pinned extension's popup under its button.
    OpenPinnedExtension(String, Button),
    /// Shows an extension's menu at the pointer, from a right click on its button or its tile.
    ExtensionMenu(String),
    /// Shows the developer bar's extensions menu, whose popups open under its button.
    ExtensionsMenu(Button),
    /// Acts on a download from the sidebar, by its id.
    Download(DownloadAction, u32),
    /// Opens the library on the downloads, from the menu.
    ShowDownloads,
    /// Opens the library on the history, from the menu.
    ShowHistory,
    /// Opens the library on the section it showed last, from the menu.
    ShowLibraryPage,
    /// Picks the next tab selected before, or the previous one, in the tab switcher, which ⌃⇥
    /// shows: from the app's keyboard monitor.
    SwitchTab(bool),
    /// Shows the tab picked in the switcher once ⌃ is released: from the keyboard monitor.
    EndTabSwitch,
    /// Shows a tab in the content view.
    Select(NodeId),
    /// Shows the next tab in the sidebar, or the previous one.
    NextTab,
    PreviousTab,
    /// Shows a favorite by its number, from 1 to 9.
    Favorite(usize),
    ShowOverlay,
    HideOverlay,
    /// Shows the Command Bar holding the selected tab's address, to edit it.
    EditUrl,
    /// Asks for the Command Bar's results for a text.
    SearchOverlay(String),
    /// Fits the Command Bar to its results, in points.
    OverlayHeight(u32),
    /// Loads an address in the selected tab, or a new one without.
    Navigate(String),
    /// Hides the sidebar, or shows it again.
    ToggleSidebar,
    /// Shows the bar above the page, or hides it.
    ToggleToolbar,
    /// Opens a Little Arbalete window on a link another app asks to open, or the Command Bar
    /// that opens one, from the menu.
    NewLittle(Option<String>),
    /// Asks for the results of the Command Bar opening a Little Arbalete window, for a text.
    SearchLittle(String),
    /// Opens another window, on the same Space, without a tab.
    NewWindow,
    /// Opens an incognito window, whose browsing leaves nothing behind.
    NewIncognitoWindow,
    /// Acts in a Little Arbalete window, by its id, from its bar.
    Little(u32, LittleAction),
    /// Moves the page of the Little Arbalete window that has the keyboard into the displayed
    /// Space, from the menu.
    OpenLittleInSpace,
    /// Shows the hidden sidebar above the page, or hides it again.
    RevealSidebar,
    ConcealSidebar,
    /// Moves the page's card right of where it rests beside the sidebar, by a number of points,
    /// as the sidebar pages towards the library.
    CardOffset(u32),
    /// Widens the sidebar for the library, the card resting beside it. Any card offset gives the
    /// sidebar its own width back.
    ShowLibrary,
    /// The library fills the window, its Spaces shown, or rests beside the card.
    WideLibrary(bool),
    /// Opens a tab on an address.
    Open(String),
    /// Opens a tab on an address behind the one shown.
    OpenInBackground(String),
    /// Opens a tab on an address beside the selected one, in a split.
    OpenInSplit(String),
    /// Opens a Little Arbalete window on an address.
    OpenInLittle(String),
    /// Closes a tab, or the selected one.
    Close(Option<NodeId>),
    /// Closes a tab's page's developer tools, like their window's close button.
    CloseInspector(NodeId),
    /// Reopens a closed tab, by its rank from the last closed one, 0.
    Reopen(usize),
    /// Pins or unpins a tab, or the selected one.
    Pin(Option<NodeId>),
    /// Creates a folder for the UI to rename, at the end of the displayed Space's pinned tabs,
    /// or of a folder, from its menu.
    NewFolder(Option<NodeId>),
    /// Closes every tab of a folder, from its menu.
    CloseFolderTabs(NodeId),
    /// Removes a tab from the favorites, from its menu.
    RemoveFavorite(NodeId),
    /// Hides the box inviting to drag tabs into the empty favorites of a Space's profile.
    HideFavoritesHint(SpaceId),
    /// Closes every "Today" tab of the displayed Space.
    ClearToday,
    /// Puts the url of the displayed Space's selected tab on the clipboard.
    CopyLink,
    /// Puts it there as a Markdown link, its title and url.
    CopyMarkdownLink,
    /// Shows the recent items of a source over the library's button, or hides them.
    MediaSource(String, bool),
    /// Changes a shortcut.
    Shortcut(ShortcutEdit),
    /// Moves a tab to another Space's "Today" tabs, or a folder to its pinned tabs, from its menu.
    MoveToSpace(NodeId, SpaceId),
    /// Copies a tab or a folder right after it, from its menu.
    DuplicateRow(NodeId),
    /// Closes the rows after a tab's, which may be reopened, from its menu.
    CloseTabsBelow(NodeId),
    /// Removes a folder and closes its tabs, which may be reopened, from its menu.
    DeleteFolder(NodeId),
    /// Makes a folder a Space of its own and shows it, from its menu.
    FolderToSpace(NodeId),
    /// Opens the native menu of a sidebar row, a Space's header, or another target, at the
    /// pointer, like `{"node": 12}` or `"site"`.
    Menu(Target),
    /// Shows the menu of several tabs selected at once, by their ids.
    TabsMenu(Vec<NodeId>),
    /// The rows, Space header and pinned extensions a view shows, where a right-click opens
    /// their menu, like `[{"target":{"node":12},"x":7,"y":180,"width":250,"height":41}]`.
    MenuTargets {
        view: ButtonView,
        targets: Vec<HitTarget>,
    },
    /// Runs an item of a menu without showing it, by the item's id (app/src/menu.rs), as the
    /// end-to-end checks cannot pick in native menus.
    ChooseMenu(Target, String),
    /// Collapses or expands a folder.
    ToggleFolder(NodeId),
    /// Renames a folder or a tab, a blank name giving a tab its page's title back.
    Rename(NodeId, String),
    /// Changes the url a pinned tab goes back to, from its menu.
    SetPinnedUrl(NodeId, String),
    /// Mutes a tab or gives it its sound back, from its menu.
    ToggleMute(NodeId),
    /// Moves a tab or a folder to an index among a container's or a folder's children, as the
    /// app counts them once the node has left: the node, the parent and the index.
    Move(NodeId, NodeId, usize),
    /// Puts a tab beside another in a split row, in the other's place, after it or before it:
    /// the tab, the other and whether before.
    Split(NodeId, NodeId, bool),
    /// Adds a copy of a tab at an index among a container's or a folder's children: the tab, the
    /// parent and the index.
    Duplicate(NodeId, NodeId, usize),
    /// Puts a copy of a tab beside another in a split row, as `Split` puts the tab.
    SplitDuplicate(NodeId, NodeId, bool),
    /// Takes a tab out of its split row, as the row after it.
    Separate(NodeId),
    /// Moves the handle after a split's page at an index to a point along the split, in the
    /// window's points from its left or top edge.
    ResizePane(usize, f64),
    /// Gives the sidebar the width its edge was dragged to, in the window's points from its left
    /// edge.
    ResizeSidebar(f64),
    /// Keeps the sizes a drag of the sidebar's edge or of a split's handle gave, once it ends.
    Resized,
    /// Gives the pages on either side of a split's handle at an index equal parts.
    EvenPanes(usize),
    /// Opens the Command Bar for a page to show beside the selected tab, or a given one, in its
    /// split.
    AddPane(Option<NodeId>),
    /// Closes the selected tab of a split.
    ClosePane,
    /// Selects the next page of the split shown, or the previous one, wrapping around, like Arc's.
    SwitchPane(bool),
    /// Shows a link in Peek, over the window (from the app itself).
    Peek(String),
    ClosePeek,
    /// Moves Peek's page back or forward, reloads or stops it, from its bar.
    PeekPage(PeekAction),
    /// Opens Peek's page in a tab of its own, or beside the selected tab in a split.
    KeepPeek {
        split: bool,
    },
    /// Lays the pages of the split shown, or a given one, out one above the other, or side by
    /// side again.
    FlipPanes(Option<NodeId>),
    /// While a tab is dragged over the page shown alone, makes room for it before the page or
    /// after it, one above the other when stacked, like Arc's, or gives the page its room back:
    /// `[<before>, <stacked>]` or nothing.
    PreviewDrop(Option<(bool, bool)>),
    /// While a tab is dragged over the split shown, opens an empty page at an index among its
    /// pages, which share the room evenly with it, like Arc's, or closes it again.
    PreviewPane(Option<usize>),
    /// Takes every tab out of a split, as rows in its place.
    SeparateAll(NodeId),
    /// Closes a split's tabs, which may be reopened.
    CloseSplit(NodeId),
    /// Asks the UI for the Create a new Profile dialog, like Arc's.
    NewProfile,
    /// Creates a profile used by some Spaces, from that dialog.
    CreateProfile {
        name: String,
        spaces: Vec<SpaceId>,
    },
    RenameProfile(ProfileId, String),
    /// Deletes a profile no Space uses, with its web data.
    DeleteProfile(ProfileId),
    /// Shows one of Chromium's own pages in a profile, by its url, from the settings, which
    /// close.
    ProfilePage(ProfileId, String),
    /// Changes how long a profile's "Today" tabs stay unshown before they are archived, like
    /// `"twelve-hours"` or `"never"`.
    SetArchiveAfter(ProfileId, ArchiveAfter),
    /// Changes where a profile's downloads go, like `"ask"` or `{"folder": "/Users/me/Work"}`.
    SetDownloadLocation(ProfileId, DownloadLocation),
    /// Sends the library the archived tabs.
    SendArchive,
    /// Opens an archived tab again, by its rank in the archive, and shows it.
    RestoreArchived(usize),
    /// Changes what a profile's Command Bar searches with, like `"duckduckgo"`.
    SetSearchEngine(ProfileId, SearchEngine),
    /// Lets a profile's Command Bar suggest searches, or not.
    SetSearchSuggestions(ProfileId, bool),
    /// Shows the settings window, from the menu.
    ShowSettings,
    HideSettings,
    /// The settings window is listening: send it the settings.
    SettingsReady,
    /// Keeps a site's pages from being suspended, or no longer: its host, and whether.
    KeepAwake(String, bool),
    /// Turns a site's developer mode on or off: its host, and whether.
    DeveloperMode(String, bool),
    /// Turns a setting on or off.
    SetSetting(Setting, bool),
    /// Shows a Space in the sidebar, and its selected tab in the content view.
    ShowSpace(SpaceId),
    /// Shows a Space by its number in sidebar order, from 1 to 9.
    SpaceNumber(usize),
    /// Shows the Space after the displayed one, or before it.
    NextSpace,
    PreviousSpace,
    /// Creates a Space in the displayed Space's profile and displays it, an empty emoji for none,
    /// moving a tab into it if given.
    CreateSpace {
        title: String,
        emoji: String,
        color: Color,
        tab: Option<NodeId>,
    },
    RenameSpace(SpaceId, String),
    /// Sets a Space's emoji, or removes it when empty.
    SetSpaceEmoji(SpaceId, String),
    /// Changes a Space's theme.
    SetSpaceTheme(SpaceId, Theme),
    /// Replaces the URL Routing rules and default.
    SetUrlRouting(UrlRouting),
    /// Hides a Space's pinned tabs in the sidebar, or shows them again.
    ToggleSpace(SpaceId),
    /// Deletes a Space with its tabs.
    DeleteSpace(SpaceId),
    /// Moves a Space to an index in sidebar order, as the app counts them once the Space has
    /// left.
    MoveSpace(SpaceId, usize),
    /// Attaches a Space to another profile.
    SetSpaceProfile(SpaceId, ProfileId),
    /// Opens the import assistant.
    ShowImport,
    /// The import assistant is listening: send it the browsers found.
    ImportReady,
    /// Closes the import assistant without importing.
    CloseImport,
    /// Imports browser profiles.
    Import(Vec<Choice>),
    /// Opens the System Settings pane where the user gives Arbalete Full Disk Access.
    OpenFullDiskAccess,
    /// Paints the window around the sidebar and the card like the sidebar, and an empty Space's
    /// card, with colors as opaque ARGB: the card's, the grain from 0 to 1, and the gradient's
    /// one to three colors.
    WindowColors {
        card: u32,
        grain: f64,
        colors: Vec<u32>,
    },
}

/// What to do with a download (Command::Download).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DownloadAction {
    /// Stops a download in progress.
    Cancel,
    /// Opens a finished download's file.
    Open,
    /// Shows a finished download's file in the file manager.
    Show,
    /// Removes a download from the list, leaving its file.
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Zoom {
    In,
    Out,
    /// Back to the page's actual size.
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    /// A tab, a folder or a favorite.
    Node(NodeId),
    /// A Space, from its header.
    Space(SpaceId),
    /// The profiles a Space may be attached to, from its card in the library.
    SpaceProfile(SpaceId),
    /// The site of the selected tab, from the url bar's site controls.
    Site,
    /// A pinned extension, by its rank among the displayed Space's profile's, from its button.
    Pinned(usize),
}

/// Where an extension shows in the site controls' panel, in CSS pixels from its top left corner.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ExtensionTarget {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Where a menu's target shows in a sidebar, in CSS pixels from the view's top left corner.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct HitTarget {
    pub target: Target,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl HitTarget {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        (self.x..self.x + self.width).contains(&x) && (self.y..self.y + self.height).contains(&y)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Button, ButtonView, Command, InspectorPanel, LittleAction, PeekAction, Setting,
        ShortcutEdit, Target,
    };
    use arbalete_core::sidebar::DownloadLocation;

    fn parse(json: &str) -> Option<Command> {
        serde_json::from_str(json).ok()
    }

    #[test]
    fn parses_commands_without_arguments() {
        assert_eq!(parse(r#"{"name":"back"}"#), Some(Command::Back));
        assert_eq!(
            parse(r#"{"name":"force-reload"}"#),
            Some(Command::ForceReload)
        );
        assert_eq!(parse(r#"{"name":"close"}"#), Some(Command::Close(None)));
        assert_eq!(
            parse(r#"{"name":"new-little"}"#),
            Some(Command::NewLittle(None))
        );
        assert_eq!(
            parse(r#"{"name":"preview-drop"}"#),
            Some(Command::PreviewDrop(None))
        );
    }

    #[test]
    fn parses_commands_with_one_argument() {
        assert_eq!(
            parse(r#"{"name":"navigate","args":"https://example.com/a b"}"#),
            Some(Command::Navigate("https://example.com/a b".into()))
        );
        assert!(matches!(
            parse(r#"{"name":"close","args":3}"#),
            Some(Command::Close(Some(_)))
        ));
        assert_eq!(
            parse(r#"{"name":"overlay-height","args":326}"#),
            Some(Command::OverlayHeight(326))
        );
        assert_eq!(parse(r#"{"name":"overlay-height","args":-1}"#), None);
        assert_eq!(
            parse(r#"{"name":"dev-tools-panel","args":"network"}"#),
            Some(Command::DevToolsPanel(InspectorPanel::Network))
        );
        assert_eq!(
            parse(r#"{"name":"peek-page","args":"stop"}"#),
            Some(Command::PeekPage(PeekAction::Stop))
        );
        assert!(matches!(
            parse(r#"{"name":"tabs-menu","args":[1,2]}"#),
            Some(Command::TabsMenu(tabs)) if tabs.len() == 2
        ));
    }

    #[test]
    fn parses_commands_with_several_arguments() {
        assert!(matches!(
            parse(r#"{"name":"rename","args":[12,"Work"]}"#),
            Some(Command::Rename(_, title)) if title == "Work"
        ));
        assert!(matches!(
            parse(r#"{"name":"split","args":[1,2,true]}"#),
            Some(Command::Split(_, _, true))
        ));
        assert_eq!(parse(r#"{"name":"rename","args":[12]}"#), None);
        assert_eq!(
            parse(r#"{"name":"answer-auth","args":[3,["me@home","p/ss word"]]}"#),
            Some(Command::AnswerAuth(
                3,
                Some(("me@home".into(), "p/ss word".into()))
            ))
        );
        assert_eq!(
            parse(r#"{"name":"answer-auth","args":[3,null]}"#),
            Some(Command::AnswerAuth(3, None))
        );
        assert_eq!(
            parse(r#"{"name":"preview-drop","args":[true,false]}"#),
            Some(Command::PreviewDrop(Some((true, false))))
        );
        assert_eq!(
            parse(r#"{"name":"set-setting","args":["warn-before-quit",true]}"#),
            Some(Command::SetSetting(Setting::WarnBeforeQuit, true))
        );
    }

    #[test]
    fn parses_struct_like_commands() {
        assert_eq!(
            parse(r#"{"name":"find","args":{"text":"café","forward":false}}"#),
            Some(Command::Find {
                text: Some("café".into()),
                forward: false
            })
        );
        assert_eq!(
            parse(r#"{"name":"keep-peek","args":{"split":true}}"#),
            Some(Command::KeepPeek { split: true })
        );
        assert!(matches!(
            parse(r#"{"name":"create-space","args":{"title":"Work","emoji":"","color":"purple"}}"#),
            Some(Command::CreateSpace { tab: None, .. })
        ));
    }

    #[test]
    fn parses_nested_arguments() {
        assert_eq!(
            parse(
                r#"{"name":"show-site-controls","args":{"view":"toolbar","x":10,"y":4,"width":28,"height":28}}"#
            ),
            Some(Command::ShowSiteControls(Button {
                view: ButtonView::Toolbar,
                x: 10,
                y: 4,
                width: 28,
                height: 28
            }))
        );
        assert_eq!(
            parse(r#"{"name":"little","args":[3,"focused"]}"#),
            Some(Command::Little(3, LittleAction::Focused))
        );
        assert_eq!(
            parse(r#"{"name":"little","args":[3,{"navigate":"example.com/a"}]}"#),
            Some(Command::Little(
                3,
                LittleAction::Navigate("example.com/a".into())
            ))
        );
        assert_eq!(
            parse(r#"{"name":"little","args":[3,{"open-in-space":null}]}"#),
            Some(Command::Little(3, LittleAction::OpenInSpace(None)))
        );
        assert_eq!(
            parse(r#"{"name":"shortcut","args":{"set":["togglePin","k",1179648]}}"#),
            Some(Command::Shortcut(ShortcutEdit::Set(
                "togglePin".into(),
                "k".into(),
                1179648
            )))
        );
        assert_eq!(
            parse(r#"{"name":"shortcut","args":{"priority":["find","websites"]}}"#),
            Some(Command::Shortcut(ShortcutEdit::Priority(
                "find".into(),
                crate::settings::Priority::Websites
            )))
        );
        assert_eq!(
            parse(r#"{"name":"shortcut","args":"reset-all"}"#),
            Some(Command::Shortcut(ShortcutEdit::ResetAll))
        );
        assert_eq!(
            parse(r#"{"name":"choose-menu","args":["site","copy-link"]}"#),
            Some(Command::ChooseMenu(Target::Site, "copy-link".into()))
        );
        assert!(matches!(
            parse(r#"{"name":"menu","args":{"space-profile":4}}"#),
            Some(Command::Menu(Target::SpaceProfile(_)))
        ));
        assert!(matches!(
            parse(r#"{"name":"set-download-location","args":[1,{"folder":"/Users/me/Work"}]}"#),
            Some(Command::SetDownloadLocation(_, DownloadLocation::Folder(_)))
        ));
    }

    #[test]
    fn ignores_unknown_commands() {
        assert_eq!(parse(r#"{"name":"launch-rockets"}"#), None);
        assert_eq!(parse(r#"{"name":"back","args":1}"#), None);
        assert_eq!(parse(r#"{"name":"navigate"}"#), None);
        assert_eq!(parse("back"), None);
    }
}

//! Commands the UI sends to the app, as `arbalete://ui/command/<name>` requests.

use arbalete_core::import::Choice;
use arbalete_core::search::SearchEngine;
use arbalete_core::sidebar::{ArchiveAfter, Color, NodeId, ProfileId, SpaceId, Theme};
use arbalete_core::url_routing::UrlRouting;
use serde::Deserialize;

/// What a site keeps that its site controls clear (Command::ClearSiteData).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SiteData {
    /// Its cache storage and service workers, the page then reloading from the network.
    Cache,
    Cookies,
}

/// What a Little Arbalete window's bar asks for (Command::Little).
#[derive(Clone, Debug, PartialEq)]
pub enum LittleAction {
    /// The bar is listening: send it its page's address.
    Ready,
    /// The window came in front: its page is its profile's active tab for the extensions.
    Focused,
    /// Loads an address in the window's page.
    Navigate(String),
    /// Moves the page into a Space, the displayed one without, as a tab, and closes the window:
    /// `open` or `open/<space id>`.
    OpenInSpace(Option<SpaceId>),
    /// Opens the page again in another profile: `profile/<profile id>`.
    SetProfile(ProfileId),
}

/// What Peek's bar asks of its page (Command::PeekPage).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PeekAction {
    Back,
    Forward,
    Reload,
    Stop,
}

/// The UI view a button shows in (Command::ShowSiteControls).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonView {
    Docked,
    Revealed,
    Toolbar,
}

/// Where a button is, in CSS pixels from its view's top left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Button {
    pub view: ButtonView,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Button {
    /// `<docked, revealed or toolbar>/<x>/<y>/<width>/<height>`.
    fn parse(path: &str) -> Option<Self> {
        let args: Vec<_> = path.split('/').collect();
        let [view, x, y, width, height] = args[..] else {
            return None;
        };
        let view = match view {
            "docked" => ButtonView::Docked,
            "revealed" => ButtonView::Revealed,
            "toolbar" => ButtonView::Toolbar,
            _ => return None,
        };
        Some(Self {
            view,
            x: x.parse().ok()?,
            y: y.parse().ok()?,
            width: width.parse().ok()?,
            height: height.parse().ok()?,
        })
    }
}

/// The app's settings the settings window turns on or off (app/src/settings.rs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Setting {
    WarnBeforeQuit,
    RestoreSession,
    SuspendTabs,
}

#[derive(Clone, Debug, PartialEq)]
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
    /// Opens the page's source in a new tab, from the menu: `page/source`.
    ViewSource,
    /// Shows Chromium's developer tools for the page in a window of their own, or closes them,
    /// from the menu, `page/devtools`; from the page's menu, on the element at a point of the
    /// page.
    DevTools(Option<(i32, i32)>),
    /// Shows the page's developer tools on a panel, from the developer bar:
    /// `page/devtools/<console, network or elements>`.
    DevToolsPanel(&'static str),
    /// Captures the shown page, like Arc's: framed on a color, from the developer bar,
    /// `page/capture`; or to pick a part of, `page/capture/area`, which the interface then copies,
    /// saves or edits.
    Capture,
    CaptureArea,
    /// Copies the whole page, from the site controls: `page/capture/full`.
    CaptureFull,
    /// Shows the selected tab's site's settings, Chromium's page, like Arc's: `site/settings`.
    ShowSiteSettings,
    /// Scrolls the page under a capture's veil, by `dx` and `dy` at `x` and `y`, in its points:
    /// `page/capture/scroll/<dx>/<dy>/<x>/<y>`.
    ScrollCapture(f64, f64, f64, f64),
    /// Shows the find bar above the page, from the menu.
    ShowFind,
    /// Finds text in the page, forward or backward: `find/forward/<percent-encoded text>`,
    /// `find/backward/<text>`. The same text again goes to the next match. From the menu, the last
    /// text, `None`.
    Find {
        text: Option<String>,
        forward: bool,
    },
    /// Hides the find bar and stops finding: `find/hide`.
    HideFind,
    /// Shows the selected tab's site controls in a panel under their button, like Arc's:
    /// `site/show/<docked, revealed or toolbar>/<x>/<y>/<width>/<height>`.
    ShowSiteControls(Button),
    /// `site/hide`.
    HideSiteControls,
    /// Shows a Space's theme picker, beside the sidebar: `theme/show/<space id>`.
    ShowThemePicker(SpaceId),
    /// Hides the theme picker: `theme/hide`.
    HideThemePicker,
    /// The site controls' panel takes this height: `site/height/<height>`.
    SiteControlsHeight(i32),
    /// Opens an extension's popup from the site controls, under their button:
    /// `site/extension/<id>`.
    OpenExtensionPopup(String),
    CloseExtensionPopup,
    /// Opens the Chrome Web Store, to install extensions: `site/store`.
    ShowStore,
    /// Shows the shown page's certificate, like Arc's: `site/certificate`.
    ShowCertificate,
    /// Opens Chromium's extensions page, like Arc's Manage Extensions: `extension/manage`.
    ManageExtensions,
    /// Where the site controls' panel shows its extensions, where a right-click opens their menu:
    /// `site/targets/<percent-encoded JSON array>`, like
    /// `[{"id":"abc","x":12,"y":80,"width":64,"height":40}]`.
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
    /// Opens a pinned extension's popup under its button:
    /// `extension/popup/<id>/<docked, revealed or toolbar>/<x>/<y>/<width>/<height>`.
    OpenPinnedExtension(String, Button),
    /// Shows an extension's menu at the pointer, from a right click on its button or its tile:
    /// `extension/context/<id>`.
    ExtensionMenu(String),
    /// Shows the developer bar's extensions menu, whose popups open under its button:
    /// `extension/menu/<docked, revealed or toolbar>/<x>/<y>/<width>/<height>`.
    ExtensionsMenu(Button),
    /// Acts on a download from the sidebar: `download/<cancel, open, show or remove>/<id>`.
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
    /// Shows a tab in the content view: `select/<tab id>`.
    Select(NodeId),
    /// Shows the next tab in the sidebar, `next`, or the previous one, `previous`.
    NextTab,
    PreviousTab,
    /// Shows a favorite by its number, from 1 to 9: `favorite/<n>`.
    Favorite(usize),
    ShowOverlay,
    HideOverlay,
    /// Shows the Command Bar holding the selected tab's address, to edit it: `overlay/edit`.
    EditUrl,
    /// Asks for the Command Bar's results for a text: `overlay/search/<percent-encoded text>`.
    SearchOverlay(String),
    /// Fits the Command Bar to its results: `overlay/height/<points>`.
    OverlayHeight(u32),
    /// Loads an address in the selected tab, or a new one without: `navigate/<percent-encoded url>`.
    Navigate(String),
    /// Hides the sidebar, or shows it again: `sidebar/toggle`.
    ToggleSidebar,
    /// Shows the bar above the page, or hides it: `toolbar/toggle`.
    ToggleToolbar,
    /// Opens a Little Arbalete window on a link another app asks to open, or the Command Bar
    /// that opens one, from the menu: `little/new/<percent-encoded url>` or `little/new`.
    NewLittle(Option<String>),
    /// Asks for the results of the Command Bar opening a Little Arbalete window, for a text:
    /// `little/search/<percent-encoded text>`.
    SearchLittle(String),
    /// Opens another window, on the same Space, without a tab: `window/new`.
    NewWindow,
    /// Opens an incognito window, whose browsing leaves nothing behind: `window/incognito`.
    NewIncognitoWindow,
    /// Acts in a Little Arbalete window, from its bar: `little/<window id>/ready`,
    /// `little/<window id>/navigate/<percent-encoded url>` or `little/<window id>/open`.
    Little(u32, LittleAction),
    /// Moves the page of the Little Arbalete window that has the keyboard into the displayed
    /// Space, from the menu.
    OpenLittleInSpace,
    /// Shows the hidden sidebar above the page, `sidebar/reveal`, or hides it again,
    /// `sidebar/conceal`.
    RevealSidebar,
    ConcealSidebar,
    /// Moves the page's card right of where it rests beside the sidebar, by a number of points,
    /// as the sidebar pages towards the library: `card/offset/<points>`.
    CardOffset(u32),
    /// Widens the sidebar for the library, the card resting beside it: `library/show`. Any card
    /// offset gives the sidebar its own width back.
    ShowLibrary,
    /// The library fills the window, its Spaces shown, `library/wide`, or rests beside the card,
    /// `library/narrow`.
    WideLibrary(bool),
    /// Opens a tab: `open/<percent-encoded url>`.
    Open(String),
    /// Closes a tab, `close/<tab id>`, or the selected one, `close`.
    Close(Option<NodeId>),
    /// Closes a tab's page's developer tools, `inspector/close/<tab id>`, like their window's
    /// close button.
    CloseInspector(NodeId),
    /// Reopens a closed tab, by its rank from the last closed one, 0: `reopen/<rank>`, or the last
    /// closed one, `reopen`.
    Reopen(usize),
    /// Pins or unpins a tab, `pin/<tab id>`, or the selected one, `pin`.
    Pin(Option<NodeId>),
    /// Creates a folder for the UI to rename, at the end of the displayed Space's pinned tabs,
    /// `folder/new`, or of a folder, from its menu.
    NewFolder(Option<NodeId>),
    /// Closes every tab of a folder, from its menu.
    CloseFolderTabs(NodeId),
    /// Removes a tab from the favorites, from its menu.
    RemoveFavorite(NodeId),
    /// Hides the box inviting to drag tabs into the empty favorites of a Space's profile:
    /// `favorites/hide-hint/<space id>`.
    HideFavoritesHint(SpaceId),
    /// Closes every "Today" tab of the displayed Space: `today/clear`.
    ClearToday,
    /// Puts the url of the displayed Space's selected tab on the clipboard: `link/copy`.
    CopyLink,
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
    /// Opens the native menu of a sidebar row at the pointer: `menu/node/<tab or folder id>`, or
    /// of a Space's header, `menu/space/<space id>`.
    Menu(Target),
    /// Shows the menu of several tabs selected at once, by their ids: `menu/tabs/<json ids>`.
    TabsMenu(Vec<NodeId>),
    /// The rows, Space header and pinned extensions a view shows, where a right-click opens
    /// their menu: `menu/targets/<docked, revealed or toolbar>/<percent-encoded JSON array of
    /// targets>`, like `[{"target":{"node":12},"x":7,"y":180,"width":250,"height":41}]`.
    MenuTargets {
        view: ButtonView,
        targets: Vec<HitTarget>,
    },
    /// Runs an item of a menu without showing it, by the item's id (app/src/menu.rs), as the
    /// end-to-end checks cannot pick in native menus: `menu/choose/<node or space>/<id>/<item id>`,
    /// or `menu/choose/site/<any>/<item id>`.
    ChooseMenu(Target, String),
    /// Collapses or expands a folder: `toggle/<folder id>`.
    ToggleFolder(NodeId),
    /// Renames a folder or a tab, a blank name giving a tab its page's title back:
    /// `rename/<id>/<percent-encoded title>`.
    Rename(NodeId, String),
    /// Changes the url a pinned tab goes back to, from its menu.
    SetPinnedUrl(NodeId, String),
    /// Mutes a tab or gives it its sound back, from its menu.
    ToggleMute(NodeId),
    /// Moves a tab or a folder to an index among a container's or a folder's children, as the
    /// app counts them once the node has left: `move/<id>/<parent id>/<index>`.
    Move(NodeId, NodeId, usize),
    /// Puts a tab beside another in a split row, in the other's place, after it or before it:
    /// `split/<tab id>/<target id>`, `split/<tab id>/<target id>/before`.
    Split(NodeId, NodeId, bool),
    /// Adds a copy of a tab at an index among a container's or a folder's children:
    /// `duplicate/<tab id>/<parent id>/<index>`.
    Duplicate(NodeId, NodeId, usize),
    /// Puts a copy of a tab beside another in a split row, as `Split` puts the tab:
    /// `split-duplicate/<tab id>/<target id>`, `split-duplicate/<tab id>/<target id>/before`.
    SplitDuplicate(NodeId, NodeId, bool),
    /// Takes a tab out of its split row, as the row after it: `separate/<tab id>`.
    Separate(NodeId),
    /// Moves the handle after a split's page at an index to a point along the split, in the
    /// window's points from its left or top edge: `pane/resize/<index>/<point>`.
    ResizePane(usize, f64),
    /// Gives the sidebar the width its edge was dragged to, in the window's points from its left
    /// edge: `sidebar/width/<point>`.
    ResizeSidebar(f64),
    /// Keeps the sizes a drag of the sidebar's edge or of a split's handle gave, once it ends:
    /// `resized`.
    Resized,
    /// Gives the pages on either side of a split's handle at an index equal parts:
    /// `pane/even/<index>`.
    EvenPanes(usize),
    /// Opens the Command Bar for a page to show beside the selected tab, or a given one, in its
    /// split: `pane/add`, `pane/add/<tab id>`.
    AddPane(Option<NodeId>),
    /// Closes the selected tab of a split: `pane/close`.
    ClosePane,
    /// Selects the next page of the split shown, or the previous one, wrapping around, like Arc's:
    /// `pane/next`, `pane/previous`.
    SwitchPane(bool),
    /// Shows a link in Peek, over the window (from the app itself).
    Peek(String),
    /// Closes Peek: `peek/close`.
    ClosePeek,
    /// Moves Peek's page back or forward, reloads or stops it, from its bar: `peek/back`,
    /// `peek/forward`, `peek/reload`, `peek/stop`.
    PeekPage(PeekAction),
    /// Opens Peek's page in a tab of its own, or beside the selected tab in a split:
    /// `peek/tab`, `peek/split`.
    KeepPeek {
        split: bool,
    },
    /// Lays the pages of the split shown, or a given one, out one above the other, or side by
    /// side again: `pane/flip`, `pane/flip/<split id>`.
    FlipPanes(Option<NodeId>),
    /// While a tab is dragged over the page shown alone, makes room for it before or after the
    /// page, one above the other with `stacked`, like Arc's, or gives the page its room back:
    /// `page/preview/<before|after>/<side|stacked>`, `page/preview/none`.
    PreviewDrop(Option<(bool, bool)>),
    /// While a tab is dragged over the split shown, opens an empty page at `index` among its
    /// pages, which share the room evenly with it, like Arc's, or closes it again:
    /// `pane/preview/<index>`, `pane/preview/none`.
    PreviewPane(Option<usize>),
    /// Takes every tab out of a split, as rows in its place: `pane/separate-all/<split id>`.
    SeparateAll(NodeId),
    /// Closes a split's tabs, which may be reopened: `pane/close-split/<split id>`.
    CloseSplit(NodeId),
    /// Creates a profile, with a Space of its own, for the UI to rename: `profile/new`.
    NewProfile,
    /// Renames a profile: `profile/rename/<profile id>/<percent-encoded name>`.
    RenameProfile(ProfileId, String),
    /// Changes a profile's color: `profile/color/<profile id>/<color>`, like `blue`.
    ColorProfile(ProfileId, Color),
    /// Changes how long a profile's "Today" tabs stay unshown before they are archived:
    /// `profile/archive/<profile id>/<delay>`, like `twelve-hours` or `never`.
    SetArchiveAfter(ProfileId, ArchiveAfter),
    /// Sends the library the archived tabs: `archive/list`.
    SendArchive,
    /// Opens an archived tab again, by its rank in the archive, and shows it:
    /// `archive/restore/<rank>`.
    RestoreArchived(usize),
    /// Changes what a profile's Command Bar searches with:
    /// `profile/search/<profile id>/<engine>`, like `duckduckgo`.
    SetSearchEngine(ProfileId, SearchEngine),
    /// Lets a profile's Command Bar suggest searches, or not:
    /// `profile/suggestions/<profile id>/<on or off>`.
    SetSearchSuggestions(ProfileId, bool),
    /// Shows the settings window, from the menu: `settings/show`.
    ShowSettings,
    /// Closes the settings window: `settings/hide`.
    HideSettings,
    /// The settings window is listening: send it the settings, `settings/ready`.
    SettingsReady,
    /// Keeps a site's pages from being suspended, or no longer: `awake/<on or off>/<host>`.
    KeepAwake(String, bool),
    /// Turns a site's developer mode on or off: `developer/<on or off>/<host>`.
    DeveloperMode(String, bool),
    /// Turns a setting on or off: `settings/<warn-before-quit, restore-session or
    /// suspend-tabs>/<true or false>`.
    SetSetting(Setting, bool),
    /// Shows a Space in the sidebar, and its selected tab in the content view: `space/<space id>`.
    ShowSpace(SpaceId),
    /// Shows a Space by its number in sidebar order, from 1 to 9: `space/number/<n>`.
    SpaceNumber(usize),
    /// Shows the Space after the displayed one, `space/next`, or before it, `space/previous`.
    NextSpace,
    PreviousSpace,
    /// Creates a Space in the displayed Space's profile and displays it:
    /// `space/create/<color>/<percent-encoded emoji>/<percent-encoded title>`, an empty emoji for
    /// none, and `/<tab id>` after it to move a tab into it.
    CreateSpace {
        title: String,
        emoji: String,
        color: Color,
        tab: Option<NodeId>,
    },
    /// Renames a Space: `space/rename/<space id>/<percent-encoded title>`.
    RenameSpace(SpaceId, String),
    /// Sets a Space's emoji, or removes it when empty: `space/emoji/<space id>/<percent-encoded
    /// emoji>`.
    SetSpaceEmoji(SpaceId, String),
    /// Changes a Space's theme: `space/theme/<space id>/<percent-encoded JSON theme>`.
    SetSpaceTheme(SpaceId, Theme),
    /// Replaces the URL Routing rules and default: `routing/<percent-encoded JSON>`.
    SetUrlRouting(UrlRouting),
    /// Hides a Space's pinned tabs in the sidebar, or shows them again: `space/toggle/<space id>`.
    ToggleSpace(SpaceId),
    /// Deletes a Space with its tabs: `space/delete/<space id>`.
    DeleteSpace(SpaceId),
    /// Moves a Space to an index in sidebar order, as the app counts them once the Space has
    /// left: `space/move/<space id>/<index>`.
    MoveSpace(SpaceId, usize),
    /// Attaches a Space to another profile: `space/profile/<space id>/<profile id>`.
    SetSpaceProfile(SpaceId, ProfileId),
    /// Attaches a Space to a new profile for the UI to rename, from its menu.
    NewSpaceProfile(SpaceId),
    /// Opens the import assistant: `import/show`.
    ShowImport,
    /// The import assistant is listening: send it the browsers found, `import/ready`.
    ImportReady,
    /// Closes the import assistant without importing: `import/close`.
    CloseImport,
    /// Imports browser profiles: `import/run/<percent-encoded JSON array of choices>`.
    Import(Vec<Choice>),
    /// Opens the System Settings pane where the user gives Arbalete Full Disk Access:
    /// `import/full-disk-access`.
    OpenFullDiskAccess,
    /// Paints the window around the sidebar and the card like the sidebar, and an empty Space's
    /// card, with colors as opaque ARGB: `window/colors/<card rrggbb>/<grain>/<rrggbb>...`, the
    /// gradient's one to three colors, and its grain from 0 to 1.
    WindowColors {
        card: u32,
        grain: f64,
        colors: Vec<u32>,
    },
}

/// What a menu is about.
#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    In,
    Out,
    /// Back to the page's actual size.
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// A tab, a folder or a favorite.
    Node(NodeId),
    /// A Space, from its header.
    Space(SpaceId),
    /// The profiles a Space may be attached to, from its card in the library.
    #[serde(rename = "space-profile")]
    SpaceProfile(SpaceId),
    /// The site of the selected tab, from the url bar's site controls: `menu/site`.
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

impl Target {
    fn parse(path: &str) -> Option<Self> {
        if path == "site" {
            return Some(Self::Site);
        }
        if let Some(id) = path.strip_prefix("space/") {
            return serde_json::from_str(id).ok().map(Self::Space);
        }
        if let Some(id) = path.strip_prefix("space-profile/") {
            return serde_json::from_str(id).ok().map(Self::SpaceProfile);
        }
        let id = path.strip_prefix("node/")?;
        serde_json::from_str(id).ok().map(Self::Node)
    }
}

impl Command {
    pub fn parse(path: &str) -> Option<Self> {
        match path {
            "ready" => Some(Self::Ready),
            "back" => Some(Self::Back),
            "forward" => Some(Self::Forward),
            "reload" => Some(Self::Reload),
            "reload/force" => Some(Self::ForceReload),
            "stop" => Some(Self::Stop),
            "overlay/show" => Some(Self::ShowOverlay),
            "overlay/hide" => Some(Self::HideOverlay),
            "overlay/edit" => Some(Self::EditUrl),
            "sidebar/toggle" => Some(Self::ToggleSidebar),
            "toolbar/toggle" => Some(Self::ToggleToolbar),
            "little/new" => Some(Self::NewLittle(None)),
            "window/new" => Some(Self::NewWindow),
            "window/incognito" => Some(Self::NewIncognitoWindow),
            "sidebar/reveal" => Some(Self::RevealSidebar),
            "sidebar/conceal" => Some(Self::ConcealSidebar),
            "library/show" => Some(Self::ShowLibrary),
            "library/wide" => Some(Self::WideLibrary(true)),
            "library/narrow" => Some(Self::WideLibrary(false)),
            "close" => Some(Self::Close(None)),
            "reopen" => Some(Self::Reopen(0)),
            "pin" => Some(Self::Pin(None)),
            "folder/new" => Some(Self::NewFolder(None)),
            "link/copy" => Some(Self::CopyLink),
            "today/clear" => Some(Self::ClearToday),
            "next" => Some(Self::NextTab),
            "previous" => Some(Self::PreviousTab),
            "profile/new" => Some(Self::NewProfile),
            "space/next" => Some(Self::NextSpace),
            "space/previous" => Some(Self::PreviousSpace),
            "import/show" => Some(Self::ShowImport),
            "import/ready" => Some(Self::ImportReady),
            "settings/show" => Some(Self::ShowSettings),
            "page/source" => Some(Self::ViewSource),
            "page/devtools" => Some(Self::DevTools(None)),
            "page/devtools/console" => Some(Self::DevToolsPanel("console")),
            "page/devtools/network" => Some(Self::DevToolsPanel("network")),
            "page/devtools/elements" => Some(Self::DevToolsPanel("elements")),
            "page/capture" => Some(Self::Capture),
            "page/capture/area" => Some(Self::CaptureArea),
            "page/capture/full" => Some(Self::CaptureFull),
            "settings/ready" => Some(Self::SettingsReady),
            "settings/hide" => Some(Self::HideSettings),
            "import/close" => Some(Self::CloseImport),
            "import/full-disk-access" => Some(Self::OpenFullDiskAccess),
            "find/hide" => Some(Self::HideFind),
            "extension/close" => Some(Self::CloseExtensionPopup),
            "site/hide" => Some(Self::HideSiteControls),
            "site/settings" => Some(Self::ShowSiteSettings),
            "theme/hide" => Some(Self::HideThemePicker),
            "site/store" => Some(Self::ShowStore),
            "site/certificate" => Some(Self::ShowCertificate),
            "extension/manage" => Some(Self::ManageExtensions),
            "history/show" => Some(Self::ShowHistory),
            "downloads/show" => Some(Self::ShowDownloads),
            "archive/list" => Some(Self::SendArchive),
            _ => {
                if let Some((action, id)) = path
                    .strip_prefix("extension/")
                    .and_then(|r| r.split_once('/'))
                    .filter(|(action, _)| !["popup", "menu"].contains(action))
                {
                    let id = id.to_string();
                    return match action {
                        "on" => Some(Self::SetExtensionEnabled(id, true)),
                        "off" => Some(Self::SetExtensionEnabled(id, false)),
                        "pin" => Some(Self::PinExtension(id, true)),
                        "unpin" => Some(Self::PinExtension(id, false)),
                        "options" => Some(Self::ExtensionOptions(id)),
                        "context" => Some(Self::ExtensionMenu(id)),
                        "remove" => Some(Self::RemoveExtension(id)),
                        _ => None,
                    };
                }
                if let Some(button) = path.strip_prefix("site/show/") {
                    return Button::parse(button).map(Self::ShowSiteControls);
                }
                if let Some(button) = path.strip_prefix("extension/menu/") {
                    return Button::parse(button).map(Self::ExtensionsMenu);
                }
                if let Some((id, button)) = path
                    .strip_prefix("extension/popup/")
                    .and_then(|r| r.split_once('/'))
                {
                    let button = Button::parse(button)?;
                    return Some(Self::OpenPinnedExtension(id.to_string(), button));
                }
                if let Some(height) = path.strip_prefix("site/height/") {
                    return height.parse().ok().map(Self::SiteControlsHeight);
                }
                if let Some(targets) = path.strip_prefix("site/targets/") {
                    let targets = serde_json::from_str(&percent_decode(targets)?).ok()?;
                    return Some(Self::SiteTargets(targets));
                }
                if let Some(id) = path.strip_prefix("site/extension/") {
                    return Some(Self::OpenExtensionPopup(id.to_string()));
                }
                if let Some((action, id)) = path
                    .strip_prefix("download/")
                    .and_then(|r| r.split_once('/'))
                {
                    let action = match action {
                        "cancel" => DownloadAction::Cancel,
                        "open" => DownloadAction::Open,
                        "show" => DownloadAction::Show,
                        "remove" => DownloadAction::Remove,
                        _ => return None,
                    };
                    return id.parse().ok().map(|id| Self::Download(action, id));
                }
                if let Some((direction, text)) =
                    path.strip_prefix("find/").and_then(|r| r.split_once('/'))
                {
                    let forward = match direction {
                        "forward" => true,
                        "backward" => false,
                        _ => return None,
                    };
                    let text = Some(percent_decode(text)?);
                    return Some(Self::Find { text, forward });
                }
                if let Some(args) = path.strip_prefix("window/colors/") {
                    let mut args = args.split('/');
                    let card = parse_rgb(args.next()?)?;
                    let grain: f64 = args.next()?.parse().ok()?;
                    let colors = args.map(parse_rgb).collect::<Option<Vec<_>>>()?;
                    if !(0.0..=1.0).contains(&grain) || !(1..=3).contains(&colors.len()) {
                        return None;
                    }
                    return Some(Self::WindowColors {
                        card,
                        grain,
                        colors,
                    });
                }
                if let Some(offset) = path.strip_prefix("card/offset/") {
                    return offset.parse().ok().map(Self::CardOffset);
                }
                if let Some(choices) = path.strip_prefix("import/run/") {
                    let choices = serde_json::from_str(&percent_decode(choices)?).ok();
                    return choices.map(Self::Import);
                }
                if let Some((id, name)) = path
                    .strip_prefix("profile/rename/")
                    .and_then(|r| r.split_once('/'))
                {
                    let id = serde_json::from_str(id).ok()?;
                    return percent_decode(name).map(|name| Self::RenameProfile(id, name));
                }
                if let Some((id, on)) = path
                    .strip_prefix("profile/suggestions/")
                    .and_then(|r| r.split_once('/'))
                {
                    let id = serde_json::from_str(id).ok()?;
                    let on = match on {
                        "on" => true,
                        "off" => false,
                        _ => return None,
                    };
                    return Some(Self::SetSearchSuggestions(id, on));
                }
                if let Some((id, after)) = path
                    .strip_prefix("profile/archive/")
                    .and_then(|r| r.split_once('/'))
                {
                    let id = serde_json::from_str(id).ok()?;
                    let after = serde_json::from_value(after.into()).ok()?;
                    return Some(Self::SetArchiveAfter(id, after));
                }
                if let Some(rank) = path.strip_prefix("archive/restore/") {
                    return rank.parse().ok().map(Self::RestoreArchived);
                }
                if let Some((id, engine)) = path
                    .strip_prefix("profile/search/")
                    .and_then(|r| r.split_once('/'))
                {
                    let id = serde_json::from_str(id).ok()?;
                    let engine = serde_json::from_value(engine.into()).ok()?;
                    return Some(Self::SetSearchEngine(id, engine));
                }
                if let Some((on, site)) =
                    path.strip_prefix("awake/").and_then(|r| r.split_once('/'))
                {
                    let on = match on {
                        "on" => true,
                        "off" => false,
                        _ => return None,
                    };
                    return percent_decode(site).map(|site| Self::KeepAwake(site, on));
                }
                if let Some((on, site)) = path
                    .strip_prefix("developer/")
                    .and_then(|r| r.split_once('/'))
                {
                    let on = match on {
                        "on" => true,
                        "off" => false,
                        _ => return None,
                    };
                    return percent_decode(site).map(|site| Self::DeveloperMode(site, on));
                }
                if let Some((setting, on)) = path
                    .strip_prefix("settings/")
                    .and_then(|r| r.split_once('/'))
                {
                    let setting = match setting {
                        "warn-before-quit" => Setting::WarnBeforeQuit,
                        "restore-session" => Setting::RestoreSession,
                        "suspend-tabs" => Setting::SuspendTabs,
                        _ => return None,
                    };
                    return on.parse().ok().map(|on| Self::SetSetting(setting, on));
                }
                if let Some((id, color)) = path
                    .strip_prefix("profile/color/")
                    .and_then(|r| r.split_once('/'))
                {
                    let id = serde_json::from_str(id).ok()?;
                    return parse_color(color).map(|color| Self::ColorProfile(id, color));
                }
                if let Some(args) = path.strip_prefix("space/create/") {
                    let mut args = args.split('/');
                    let (color, emoji, title) = (args.next()?, args.next()?, args.next()?);
                    let tab = match args.next() {
                        Some(tab) => Some(serde_json::from_str(tab).ok()?),
                        None => None,
                    };
                    if args.next().is_some() {
                        return None;
                    }
                    return Some(Self::CreateSpace {
                        title: percent_decode(title)?,
                        emoji: percent_decode(emoji)?,
                        color: parse_color(color)?,
                        tab,
                    });
                }
                if let Some((view, targets)) = path
                    .strip_prefix("menu/targets/")
                    .and_then(|r| r.split_once('/'))
                {
                    let view = match view {
                        "docked" => ButtonView::Docked,
                        "revealed" => ButtonView::Revealed,
                        "toolbar" => ButtonView::Toolbar,
                        _ => return None,
                    };
                    let targets = serde_json::from_str(&percent_decode(targets)?).ok()?;
                    return Some(Self::MenuTargets { view, targets });
                }
                if let Some(args) = path.strip_prefix("menu/choose/") {
                    // The target's kind and id, then the item's id, which may hold slashes.
                    let mut args = args.splitn(3, '/');
                    let (kind, id, item) = (args.next()?, args.next()?, args.next()?);
                    // The site has no id: any stands for it.
                    let target = match kind {
                        "site" => Target::Site,
                        _ => Target::parse(&format!("{kind}/{id}"))?,
                    };
                    return Some(Self::ChooseMenu(target, item.to_string()));
                }
                if let Some(ids) = path.strip_prefix("menu/tabs/") {
                    return serde_json::from_str(&percent_decode(ids)?)
                        .ok()
                        .map(Self::TabsMenu);
                }
                if let Some(target) = path.strip_prefix("menu/") {
                    return Target::parse(target).map(Self::Menu);
                }
                if let Some((id, title)) = path
                    .strip_prefix("space/rename/")
                    .and_then(|r| r.split_once('/'))
                {
                    let id = serde_json::from_str(id).ok()?;
                    return percent_decode(title).map(|title| Self::RenameSpace(id, title));
                }
                if let Some((id, emoji)) = path
                    .strip_prefix("space/emoji/")
                    .and_then(|r| r.split_once('/'))
                {
                    let id = serde_json::from_str(id).ok()?;
                    return percent_decode(emoji).map(|emoji| Self::SetSpaceEmoji(id, emoji));
                }
                if let Some((id, theme)) = path
                    .strip_prefix("space/theme/")
                    .and_then(|r| r.split_once('/'))
                {
                    let id = serde_json::from_str(id).ok()?;
                    let theme = serde_json::from_str(&percent_decode(theme)?).ok()?;
                    return Some(Self::SetSpaceTheme(id, theme));
                }
                if let Some(routing) = path.strip_prefix("routing/") {
                    let routing = serde_json::from_str(&percent_decode(routing)?).ok()?;
                    return Some(Self::SetUrlRouting(routing));
                }
                if let Some(id) = path.strip_prefix("favorites/hide-hint/") {
                    return serde_json::from_str(id).ok().map(Self::HideFavoritesHint);
                }
                if let Some(id) = path.strip_prefix("theme/show/") {
                    return serde_json::from_str(id).ok().map(Self::ShowThemePicker);
                }
                if let Some(id) = path.strip_prefix("space/toggle/") {
                    return serde_json::from_str(id).ok().map(Self::ToggleSpace);
                }
                if let Some(id) = path.strip_prefix("space/delete/") {
                    return serde_json::from_str(id).ok().map(Self::DeleteSpace);
                }
                if let Some((id, profile)) = path
                    .strip_prefix("space/profile/")
                    .and_then(|r| r.split_once('/'))
                {
                    let id = serde_json::from_str(id).ok()?;
                    return serde_json::from_str(profile)
                        .ok()
                        .map(|profile| Self::SetSpaceProfile(id, profile));
                }
                if let Some((id, index)) = path
                    .strip_prefix("space/move/")
                    .and_then(|r| r.split_once('/'))
                {
                    let id = serde_json::from_str(id).ok()?;
                    return index.parse().ok().map(|index| Self::MoveSpace(id, index));
                }
                if let Some(number) = path.strip_prefix("space/number/") {
                    let number = number.parse().ok().filter(|n| (1..=9).contains(n));
                    return number.map(Self::SpaceNumber);
                }
                if let Some(number) = path.strip_prefix("favorite/") {
                    let number = number.parse().ok().filter(|n| (1..=9).contains(n));
                    return number.map(Self::Favorite);
                }
                if let Some(id) = path.strip_prefix("space/") {
                    return serde_json::from_str(id).ok().map(Self::ShowSpace);
                }
                if let Some(rank) = path.strip_prefix("reopen/") {
                    return rank.parse().ok().map(Self::Reopen);
                }
                if let Some(id) = path.strip_prefix("select/") {
                    return serde_json::from_str(id).ok().map(Self::Select);
                }
                if let Some(id) = path.strip_prefix("inspector/close/") {
                    return serde_json::from_str(id).ok().map(Self::CloseInspector);
                }
                if let Some(id) = path.strip_prefix("close/") {
                    return serde_json::from_str(id)
                        .ok()
                        .map(|id| Self::Close(Some(id)));
                }
                if let Some(id) = path.strip_prefix("pin/") {
                    return serde_json::from_str(id).ok().map(|id| Self::Pin(Some(id)));
                }
                if let Some(id) = path.strip_prefix("toggle/") {
                    return serde_json::from_str(id).ok().map(Self::ToggleFolder);
                }
                if let Some((id, title)) =
                    path.strip_prefix("rename/").and_then(|r| r.split_once('/'))
                {
                    let id = serde_json::from_str(id).ok()?;
                    return percent_decode(title).map(|title| Self::Rename(id, title));
                }
                if let Some(args) = path.strip_prefix("split/") {
                    let (tab, target, before) = parse_split(args)?;
                    return Some(Self::Split(tab, target, before));
                }
                if let Some(args) = path.strip_prefix("split-duplicate/") {
                    let (tab, target, before) = parse_split(args)?;
                    return Some(Self::SplitDuplicate(tab, target, before));
                }
                if let Some(id) = path.strip_prefix("separate/") {
                    return serde_json::from_str(id).ok().map(Self::Separate);
                }
                if let Some(args) = path.strip_prefix("page/capture/scroll/") {
                    let numbers: Vec<f64> = args
                        .split('/')
                        .filter_map(|n| n.parse().ok().filter(|n: &f64| n.is_finite()))
                        .collect();
                    return match numbers[..] {
                        [dx, dy, x, y] => Some(Self::ScrollCapture(dx, dy, x, y)),
                        _ => None,
                    };
                }
                if let Some(args) = path.strip_prefix("pane/resize/") {
                    let (index, point) = args.split_once('/')?;
                    let point: f64 = point.parse().ok().filter(|p: &f64| p.is_finite())?;
                    return Some(Self::ResizePane(index.parse().ok()?, point));
                }
                match path {
                    "resized" => return Some(Self::Resized),
                    "pane/add" => return Some(Self::AddPane(None)),
                    "pane/close" => return Some(Self::ClosePane),
                    "pane/next" => return Some(Self::SwitchPane(true)),
                    "peek/close" => return Some(Self::ClosePeek),
                    "peek/back" => return Some(Self::PeekPage(PeekAction::Back)),
                    "peek/forward" => return Some(Self::PeekPage(PeekAction::Forward)),
                    "peek/reload" => return Some(Self::PeekPage(PeekAction::Reload)),
                    "peek/stop" => return Some(Self::PeekPage(PeekAction::Stop)),
                    "peek/tab" => return Some(Self::KeepPeek { split: false }),
                    "peek/split" => return Some(Self::KeepPeek { split: true }),
                    "pane/previous" => return Some(Self::SwitchPane(false)),
                    "pane/flip" => return Some(Self::FlipPanes(None)),
                    "page/preview/none" => return Some(Self::PreviewDrop(None)),
                    "pane/preview/none" => return Some(Self::PreviewPane(None)),
                    _ => {}
                }
                let id = |prefix| serde_json::from_str(path.strip_prefix(prefix)?).ok();
                if path.starts_with("pane/add/") {
                    return id("pane/add/").map(|tab| Self::AddPane(Some(tab)));
                }
                if path.starts_with("pane/flip/") {
                    return id("pane/flip/").map(|split| Self::FlipPanes(Some(split)));
                }
                if path.starts_with("pane/separate-all/") {
                    return id("pane/separate-all/").map(Self::SeparateAll);
                }
                if let Some(index) = path.strip_prefix("pane/preview/") {
                    return index
                        .parse()
                        .ok()
                        .map(|index| Self::PreviewPane(Some(index)));
                }
                if let Some(args) = path.strip_prefix("page/preview/") {
                    let (place, axis) = args.split_once('/')?;
                    let before = match place {
                        "before" => true,
                        "after" => false,
                        _ => return None,
                    };
                    let stacked = match axis {
                        "side" => false,
                        "stacked" => true,
                        _ => return None,
                    };
                    return Some(Self::PreviewDrop(Some((before, stacked))));
                }
                if path.starts_with("pane/close-split/") {
                    return id("pane/close-split/").map(Self::CloseSplit);
                }
                if let Some(point) = path.strip_prefix("sidebar/width/") {
                    let point: f64 = point.parse().ok().filter(|p: &f64| p.is_finite())?;
                    return Some(Self::ResizeSidebar(point));
                }
                if let Some(index) = path.strip_prefix("pane/even/") {
                    return index.parse().ok().map(Self::EvenPanes);
                }
                if let Some(args) = path.strip_prefix("move/") {
                    let (id, parent, index) = parse_place(args)?;
                    return Some(Self::Move(id, parent, index));
                }
                if let Some(args) = path.strip_prefix("duplicate/") {
                    let (tab, parent, index) = parse_place(args)?;
                    return Some(Self::Duplicate(tab, parent, index));
                }
                if let Some(url) = path.strip_prefix("little/new/") {
                    return percent_decode(url).map(|url| Self::NewLittle(Some(url)));
                }
                if let Some(text) = path.strip_prefix("little/search/") {
                    return percent_decode(text).map(Self::SearchLittle);
                }
                if let Some((id, action)) =
                    path.strip_prefix("little/").and_then(|r| r.split_once('/'))
                {
                    let id = id.parse().ok()?;
                    let action = match action {
                        "ready" => LittleAction::Ready,
                        "focus" => LittleAction::Focused,
                        "open" => LittleAction::OpenInSpace(None),
                        _ if action.starts_with("open/") => LittleAction::OpenInSpace(Some(
                            serde_json::from_str(action.strip_prefix("open/")?).ok()?,
                        )),
                        _ if action.starts_with("profile/") => LittleAction::SetProfile(
                            serde_json::from_str(action.strip_prefix("profile/")?).ok()?,
                        ),
                        _ => LittleAction::Navigate(percent_decode(
                            action.strip_prefix("navigate/")?,
                        )?),
                    };
                    return Some(Self::Little(id, action));
                }
                if let Some(text) = path.strip_prefix("overlay/search/") {
                    return percent_decode(text).map(Self::SearchOverlay);
                }
                if let Some(height) = path.strip_prefix("overlay/height/") {
                    return height.parse().ok().map(Self::OverlayHeight);
                }
                if let Some(url) = path.strip_prefix("navigate/") {
                    return percent_decode(url).map(Self::Navigate);
                }
                percent_decode(path.strip_prefix("open/")?).map(Self::Open)
            }
        }
    }
}

/// A node, a parent and an index among its children: `<id>/<parent id>/<index>`.
/// A tab, the tab to put it beside, and whether before it: `<tab id>/<target id>[/before]`.
fn parse_split(args: &str) -> Option<(NodeId, NodeId, bool)> {
    let mut args = args.split('/');
    let tab = serde_json::from_str(args.next()?).ok()?;
    let target = serde_json::from_str(args.next()?).ok()?;
    let before = match args.next() {
        None => false,
        Some("before") => true,
        Some(_) => return None,
    };
    args.next().is_none().then_some((tab, target, before))
}

fn parse_place(args: &str) -> Option<(NodeId, NodeId, usize)> {
    let mut args = args.split('/');
    let (id, parent, index) = (args.next()?, args.next()?, args.next()?);
    if args.next().is_some() {
        return None;
    }
    Some((
        serde_json::from_str(id).ok()?,
        serde_json::from_str(parent).ok()?,
        index.parse().ok()?,
    ))
}

/// A color by its name, like `blue`.
fn parse_color(name: &str) -> Option<Color> {
    serde_json::from_value(serde_json::Value::String(name.to_string())).ok()
}

/// An opaque ARGB color from its hexadecimal RGB, like `1b0e1f`.
fn parse_rgb(hex: &str) -> Option<u32> {
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16)
        .ok()
        .map(|rgb| 0xff00_0000 | rgb)
}

/// Decodes what `encodeURIComponent` encodes.
fn percent_decode(text: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut rest = text.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == b'%' {
            let hex = std::str::from_utf8(tail.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(byte);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        Button, ButtonView, Command, ExtensionTarget, LittleAction, PeekAction, Setting, Target,
    };
    use arbalete_core::search::SearchEngine;
    use arbalete_core::sidebar::{ArchiveAfter, Color};

    #[test]
    fn parses_known_commands() {
        assert_eq!(Command::parse("ready"), Some(Command::Ready));
        assert_eq!(Command::parse("back"), Some(Command::Back));
        assert_eq!(Command::parse("forward"), Some(Command::Forward));
        assert_eq!(Command::parse("reload"), Some(Command::Reload));
        assert_eq!(Command::parse("reload/force"), Some(Command::ForceReload));
        assert_eq!(Command::parse("stop"), Some(Command::Stop));
        assert_eq!(Command::parse("find/hide"), Some(Command::HideFind));
        assert_eq!(Command::parse("history/show"), Some(Command::ShowHistory));
        assert_eq!(
            Command::parse("downloads/show"),
            Some(Command::ShowDownloads)
        );
        assert_eq!(
            Command::parse("site/show/toolbar/10/4/28/28"),
            Some(Command::ShowSiteControls(Button {
                view: ButtonView::Toolbar,
                x: 10,
                y: 4,
                width: 28,
                height: 28,
            }))
        );
        assert_eq!(Command::parse("site/show/menu/10/4/28/28"), None);
        assert_eq!(
            Command::parse("extension/context/abc"),
            Some(Command::ExtensionMenu("abc".into()))
        );
        assert_eq!(
            Command::parse("extension/menu/toolbar/1/2/3/4"),
            Some(Command::ExtensionsMenu(Button {
                view: ButtonView::Toolbar,
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            }))
        );
        assert_eq!(
            Command::parse("extension/popup/abc/docked/1/2/3/4"),
            Some(Command::OpenPinnedExtension(
                "abc".into(),
                Button {
                    view: ButtonView::Docked,
                    x: 1,
                    y: 2,
                    width: 3,
                    height: 4,
                }
            ))
        );
        assert_eq!(Command::parse("site/hide"), Some(Command::HideSiteControls));
        assert_eq!(Command::parse("site/store"), Some(Command::ShowStore));
        assert_eq!(
            Command::parse("site/certificate"),
            Some(Command::ShowCertificate)
        );
        assert_eq!(
            Command::parse("extension/manage"),
            Some(Command::ManageExtensions)
        );
        assert_eq!(
            Command::parse("extension/off/abc"),
            Some(Command::SetExtensionEnabled("abc".into(), false))
        );
        assert_eq!(
            Command::parse("extension/pin/abc"),
            Some(Command::PinExtension("abc".into(), true))
        );
        assert_eq!(
            Command::parse("extension/remove/abc"),
            Some(Command::RemoveExtension("abc".into()))
        );
        assert_eq!(Command::parse("extension/fly/abc"), None);
        assert_eq!(
            Command::parse(
                "site/targets/%5B%7B%22id%22%3A%22abc%22%2C%22x%22%3A1%2C%22y%22%3A2%2C%22width%22%3A3%2C%22height%22%3A4%7D%5D"
            ),
            Some(Command::SiteTargets(vec![ExtensionTarget {
                id: "abc".into(),
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0
            }]))
        );
        assert_eq!(
            Command::parse("site/height/240"),
            Some(Command::SiteControlsHeight(240))
        );
        assert_eq!(
            Command::parse("site/extension/abc"),
            Some(Command::OpenExtensionPopup("abc".into()))
        );
        assert_eq!(Command::parse("overlay/show"), Some(Command::ShowOverlay));
        assert_eq!(Command::parse("overlay/hide"), Some(Command::HideOverlay));
        assert_eq!(Command::parse("overlay/edit"), Some(Command::EditUrl));
        assert_eq!(
            Command::parse("overlay/search/caf%C3%A9%20au"),
            Some(Command::SearchOverlay("café au".to_string()))
        );
        assert_eq!(
            Command::parse("overlay/height/326"),
            Some(Command::OverlayHeight(326))
        );
        assert_eq!(Command::parse("overlay/height/-1"), None);
        assert_eq!(
            Command::parse("navigate/https%3A%2F%2Fexample.com%2F"),
            Some(Command::Navigate("https://example.com/".to_string()))
        );
        assert_eq!(Command::parse("close"), Some(Command::Close(None)));
        assert_eq!(Command::parse("reopen"), Some(Command::Reopen(0)));
        assert_eq!(Command::parse("reopen/2"), Some(Command::Reopen(2)));
        assert_eq!(Command::parse("reopen/x"), None);
        assert!(matches!(
            Command::parse("close/3"),
            Some(Command::Close(Some(_)))
        ));
        assert_eq!(Command::parse("pin"), Some(Command::Pin(None)));
        assert_eq!(Command::parse("link/copy"), Some(Command::CopyLink));
        assert_eq!(Command::parse("today/clear"), Some(Command::ClearToday));
        assert!(matches!(
            Command::parse("favorites/hide-hint/4"),
            Some(Command::HideFavoritesHint(_))
        ));
        assert_eq!(Command::parse("favorites/hide-hint/x"), None);
        assert!(matches!(
            Command::parse("pin/3"),
            Some(Command::Pin(Some(_)))
        ));
    }

    #[test]
    fn parses_window_colors() {
        assert_eq!(
            Command::parse("window/colors/F8F6FB/0.5/1b0e1f/6daee9"),
            Some(Command::WindowColors {
                card: 0xfff8_f6fb,
                grain: 0.5,
                colors: vec![0xff1b_0e1f, 0xff6d_aee9],
            })
        );
        assert_eq!(Command::parse("window/colors/f8f6fb/0"), None);
        assert_eq!(Command::parse("window/colors/f8f6fb/2/1b0e1f"), None);
        assert_eq!(Command::parse("window/colors/f8f6fb/0/1/2/3/4"), None);
        assert_eq!(Command::parse("window/colors/f8f6fb/x/1b0e1f"), None);
        assert_eq!(Command::parse("window/colors/f8f6fb/0/1b0e1"), None);
        assert_eq!(Command::parse("window/colors/+8f6fb/0/1b0e1f"), None);
        assert_eq!(Command::parse("window/colors/f8f6fb/0/1b0e1fff"), None);
    }

    #[test]
    fn parses_library_commands() {
        assert_eq!(Command::parse("library/show"), Some(Command::ShowLibrary));
        assert_eq!(
            Command::parse("library/wide"),
            Some(Command::WideLibrary(true))
        );
        assert_eq!(
            Command::parse("library/narrow"),
            Some(Command::WideLibrary(false))
        );
        assert_eq!(
            Command::parse("card/offset/180"),
            Some(Command::CardOffset(180))
        );
        assert_eq!(
            Command::parse("card/offset/0"),
            Some(Command::CardOffset(0))
        );
        assert_eq!(Command::parse("card/offset/-3"), None);
        assert_eq!(Command::parse("card/offset/1.5"), None);
    }

    #[test]
    fn parses_tab_shortcuts() {
        assert_eq!(Command::parse("next"), Some(Command::NextTab));
        assert_eq!(Command::parse("previous"), Some(Command::PreviousTab));
        assert_eq!(Command::parse("favorite/1"), Some(Command::Favorite(1)));
        assert_eq!(Command::parse("favorite/9"), Some(Command::Favorite(9)));
        assert_eq!(Command::parse("favorite/0"), None);
        assert_eq!(Command::parse("favorite/10"), None);
        assert_eq!(Command::parse("favorite/x"), None);
    }

    #[test]
    fn parses_sidebar_commands() {
        assert_eq!(
            Command::parse("sidebar/toggle"),
            Some(Command::ToggleSidebar)
        );
        assert_eq!(
            Command::parse("sidebar/reveal"),
            Some(Command::RevealSidebar)
        );
        assert_eq!(
            Command::parse("sidebar/conceal"),
            Some(Command::ConcealSidebar)
        );
        assert_eq!(Command::parse("sidebar"), None);
    }

    #[test]
    fn parses_an_encoded_url() {
        assert_eq!(
            Command::parse("open/https%3A%2F%2Fexample.com%2F%C3%A9t%C3%A9"),
            Some(Command::Open("https://example.com/été".to_string()))
        );
        assert_eq!(Command::parse("open/%zz"), None);
        assert_eq!(Command::parse("open/%C3"), None);
    }

    #[test]
    fn parses_download_commands() {
        use super::DownloadAction;
        assert_eq!(
            Command::parse("download/cancel/3"),
            Some(Command::Download(DownloadAction::Cancel, 3))
        );
        assert_eq!(
            Command::parse("download/show/12"),
            Some(Command::Download(DownloadAction::Show, 12))
        );
        assert_eq!(Command::parse("download/delete/3"), None);
        assert_eq!(Command::parse("download/open/x"), None);
    }

    #[test]
    fn parses_find_commands() {
        assert_eq!(
            Command::parse("find/forward/caf%C3%A9%2F"),
            Some(Command::Find {
                text: Some("café/".to_string()),
                forward: true,
            })
        );
        assert_eq!(
            Command::parse("find/backward/a"),
            Some(Command::Find {
                text: Some("a".to_string()),
                forward: false,
            })
        );
        assert_eq!(Command::parse("find/sideways/a"), None);
    }

    #[test]
    fn parses_folder_commands() {
        assert_eq!(Command::parse("folder/new"), Some(Command::NewFolder(None)));
        assert!(matches!(
            Command::parse("toggle/6"),
            Some(Command::ToggleFolder(_))
        ));
        let Some(Command::Rename(id, title)) = Command::parse("rename/6/Caf%C3%A9%2F2") else {
            panic!("not a rename")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "6");
        assert_eq!(title, "Café/2");
        assert_eq!(Command::parse("rename/6"), None);
        assert_eq!(Command::parse("rename/x/a"), None);
        assert_eq!(Command::parse("toggle/x"), None);
    }

    #[test]
    fn parses_a_split() {
        let Some(Command::Split(tab, target, false)) = Command::parse("split/4/7") else {
            panic!("not a split")
        };
        assert_eq!(serde_json::to_string(&(tab, target)).unwrap(), "[4,7]");
        assert!(matches!(
            Command::parse("split/4/7/before"),
            Some(Command::Split(_, _, true))
        ));
        assert_eq!(Command::parse("split/4/7/after"), None);
        assert_eq!(Command::parse("split/4"), None);
        assert_eq!(Command::parse("split/4/x"), None);
    }

    #[test]
    fn parses_a_duplicate_a_split_of_a_duplicate_and_a_separation() {
        let Some(Command::Duplicate(tab, parent, index)) = Command::parse("duplicate/12/6/3")
        else {
            panic!("not a duplicate")
        };
        assert_eq!(serde_json::to_string(&(tab, parent)).unwrap(), "[12,6]");
        assert_eq!(index, 3);
        assert_eq!(Command::parse("duplicate/12/6"), None);
        let Some(Command::SplitDuplicate(tab, target, false)) =
            Command::parse("split-duplicate/4/7")
        else {
            panic!("not a split of a duplicate")
        };
        assert_eq!(serde_json::to_string(&(tab, target)).unwrap(), "[4,7]");
        assert_eq!(Command::parse("split-duplicate/4"), None);
        let Some(Command::Separate(tab)) = Command::parse("separate/4") else {
            panic!("not a separation")
        };
        assert_eq!(serde_json::to_string(&tab).unwrap(), "4");
        assert_eq!(Command::parse("separate/x"), None);
        assert_eq!(
            Command::parse("pane/resize/1/412.5"),
            Some(Command::ResizePane(1, 412.5))
        );
        assert_eq!(Command::parse("pane/resize/1/NaN"), None);
        assert_eq!(Command::parse("pane/even/2"), Some(Command::EvenPanes(2)));
        assert_eq!(
            Command::parse("sidebar/width/312.5"),
            Some(Command::ResizeSidebar(312.5))
        );
        assert_eq!(Command::parse("sidebar/width/inf"), None);
        assert_eq!(Command::parse("pane/add"), Some(Command::AddPane(None)));
        assert_eq!(Command::parse("pane/flip"), Some(Command::FlipPanes(None)));
        assert_eq!(
            Command::parse("page/preview/before/stacked"),
            Some(Command::PreviewDrop(Some((true, true))))
        );
        assert_eq!(
            Command::parse("page/preview/none"),
            Some(Command::PreviewDrop(None))
        );
        assert_eq!(Command::parse("page/preview/over/side"), None);
        assert_eq!(
            Command::parse("pane/preview/2"),
            Some(Command::PreviewPane(Some(2)))
        );
        assert_eq!(
            Command::parse("pane/preview/none"),
            Some(Command::PreviewPane(None))
        );
        assert!(matches!(
            Command::parse("pane/add/4"),
            Some(Command::AddPane(Some(_)))
        ));
        assert!(matches!(
            Command::parse("pane/flip/4"),
            Some(Command::FlipPanes(Some(_)))
        ));
        assert!(matches!(
            Command::parse("pane/separate-all/4"),
            Some(Command::SeparateAll(_))
        ));
        assert!(matches!(
            Command::parse("pane/close-split/4"),
            Some(Command::CloseSplit(_))
        ));
        assert_eq!(Command::parse("pane/close-split/x"), None);
        assert!(matches!(
            Command::parse("peek/split"),
            Some(Command::KeepPeek { split: true })
        ));
        assert!(matches!(
            Command::parse("pane/next"),
            Some(Command::SwitchPane(true))
        ));
        assert!(matches!(
            Command::parse("pane/previous"),
            Some(Command::SwitchPane(false))
        ));
    }

    #[test]
    fn parses_profile_commands() {
        assert_eq!(Command::parse("profile/new"), Some(Command::NewProfile));
        let Some(Command::RenameProfile(id, name)) =
            Command::parse("profile/rename/1/Caf%C3%A9%2F2")
        else {
            panic!("not a rename")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "1");
        assert_eq!(name, "Café/2");
        let Some(Command::ColorProfile(id, color)) = Command::parse("profile/color/1/blue") else {
            panic!("not a color")
        };
        assert_eq!(
            serde_json::to_string(&(id, color)).unwrap(),
            r#"[1,"blue"]"#
        );
        assert_eq!(Command::parse("profile/color/1/teal"), None);
        assert_eq!(Command::parse("profile/color/x/blue"), None);
        assert_eq!(Command::parse("profile/rename/1"), None);
        assert_eq!(Command::parse("profile/rename/x/a"), None);
    }

    #[test]
    fn parses_a_move() {
        let Some(Command::Move(id, parent, index)) = Command::parse("move/12/6/3") else {
            panic!("not a move")
        };
        assert_eq!(serde_json::to_string(&(id, parent)).unwrap(), "[12,6]");
        assert_eq!(index, 3);
        assert_eq!(Command::parse("move/12/6"), None);
        assert_eq!(Command::parse("move/12/6/3/1"), None);
        assert_eq!(Command::parse("move/12/6/-1"), None);
        assert_eq!(Command::parse("move/x/6/3"), None);
    }

    #[test]
    fn parses_a_tab_selection() {
        let Some(Command::Select(id)) = Command::parse("select/12") else {
            panic!("not a selection")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "12");
    }

    #[test]
    fn parses_a_space_to_show() {
        let Some(Command::ShowSpace(id)) = Command::parse("space/4") else {
            panic!("not a Space")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "4");
        assert_eq!(Command::parse("space/x"), None);
        assert_eq!(Command::parse("space"), None);
    }

    #[test]
    fn parses_space_shortcuts() {
        assert_eq!(Command::parse("space/next"), Some(Command::NextSpace));
        assert_eq!(
            Command::parse("space/previous"),
            Some(Command::PreviousSpace)
        );
        assert_eq!(
            Command::parse("space/number/1"),
            Some(Command::SpaceNumber(1))
        );
        assert_eq!(
            Command::parse("space/number/9"),
            Some(Command::SpaceNumber(9))
        );
        assert_eq!(Command::parse("space/number/0"), None);
        assert_eq!(Command::parse("space/number/10"), None);
        assert_eq!(Command::parse("space/number/x"), None);
    }

    #[test]
    fn parses_space_commands() {
        assert_eq!(
            Command::parse("space/create/green/%F0%9F%8F%A1/Caf%C3%A9%2F2"),
            Some(Command::CreateSpace {
                title: "Café/2".into(),
                emoji: "🏡".into(),
                color: Color::Green,
                tab: None,
            })
        );
        let Some(Command::CreateSpace { tab: Some(tab), .. }) =
            Command::parse("space/create/blue//Home/12")
        else {
            panic!("not a Space with a tab")
        };
        assert_eq!(serde_json::to_string(&tab).unwrap(), "12");
        assert_eq!(Command::parse("space/create/blue//Home/x"), None);
        assert_eq!(Command::parse("space/create/blue//Home/12/3"), None);
        assert_eq!(
            Command::parse("space/create/blue//Home"),
            Some(Command::CreateSpace {
                title: "Home".into(),
                emoji: "".into(),
                color: Color::Blue,
                tab: None,
            })
        );
        assert_eq!(Command::parse("space/create/teal//Home"), None);
        assert_eq!(Command::parse("space/create/blue/Home"), None);
        let Some(Command::RenameSpace(id, title)) = Command::parse("space/rename/4/Caf%C3%A9%2F2")
        else {
            panic!("not a rename")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "4");
        assert_eq!(title, "Café/2");
        let Some(Command::SetSpaceEmoji(id, emoji)) = Command::parse("space/emoji/4/%F0%9F%8F%A1")
        else {
            panic!("not an emoji")
        };
        assert_eq!(
            (serde_json::to_string(&id).unwrap(), emoji.as_str()),
            ("4".into(), "🏡")
        );
        let Some(Command::SetSpaceEmoji(_, emoji)) = Command::parse("space/emoji/4/") else {
            panic!("not an emoji")
        };
        assert_eq!(emoji, "");
        let theme = r##"{"colors":["#6daee9"],"mode":"light","intensity":0.8,"noise":0}"##;
        let path = format!("space/theme/4/{}", theme.replace('#', "%23"));
        let Some(Command::SetSpaceTheme(id, theme)) = Command::parse(&path) else {
            panic!("not a theme")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "4");
        assert_eq!(
            (theme.colors, theme.intensity),
            (vec!["#6daee9".to_string()], 0.8)
        );
        let routing = r#"{"routes":[{"matching":"isEqualTo","text":"a.test","destination":{"space":4}}],"default":"little"}"#;
        let path = format!(
            "routing/{}",
            routing
                .replace('{', "%7B")
                .replace('}', "%7D")
                .replace('"', "%22")
        );
        let Some(Command::SetUrlRouting(routing)) = Command::parse(&path) else {
            panic!("not a routing")
        };
        assert_eq!(routing.routes[0].text, "a.test");
        assert_eq!(
            routing.default,
            arbalete_core::url_routing::Destination::Little
        );
        let Some(Command::ToggleSpace(id)) = Command::parse("space/toggle/4") else {
            panic!("not a toggle")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "4");
        let Some(Command::DeleteSpace(id)) = Command::parse("space/delete/4") else {
            panic!("not a deletion")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "4");
        let Some(Command::MoveSpace(id, 2)) = Command::parse("space/move/4/2") else {
            panic!("a Space to move");
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "4");
        assert_eq!(Command::parse("space/move/4/x"), None);
        assert_eq!(Command::parse("space/move/x/2"), None);
        assert_eq!(Command::parse("space/move/4"), None);
        let Some(Command::SetSpaceProfile(id, profile)) = Command::parse("space/profile/4/2")
        else {
            panic!("a Space's profile")
        };
        assert_eq!(
            (
                serde_json::to_string(&id).unwrap(),
                serde_json::to_string(&profile).unwrap()
            ),
            ("4".to_string(), "2".to_string())
        );
        assert_eq!(Command::parse("space/profile/4/x"), None);
        assert_eq!(Command::parse("space/toggle/x"), None);
        assert_eq!(Command::parse("space/delete/x"), None);
        let Some(Command::ShowThemePicker(id)) = Command::parse("theme/show/4") else {
            panic!("not a theme picker")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "4");
        assert_eq!(Command::parse("theme/hide"), Some(Command::HideThemePicker));
        assert_eq!(Command::parse("theme/show/x"), None);
        assert_eq!(Command::parse("space/theme/4/%7B"), None);
        assert_eq!(Command::parse("space/theme/x/%7B%7D"), None);
        assert_eq!(Command::parse("space/rename/4"), None);
        assert_eq!(Command::parse("space/emoji/x/a"), None);
    }

    #[test]
    fn parses_menu_commands() {
        let Some(Command::Menu(Target::Node(id))) = Command::parse("menu/node/12") else {
            panic!("not a menu")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "12");
        let Some(Command::ChooseMenu(Target::Node(id), item)) =
            Command::parse("menu/choose/node/12/move/4")
        else {
            panic!("not a menu choice")
        };
        assert_eq!(
            (serde_json::to_string(&id).unwrap(), item.as_str()),
            ("12".into(), "move/4")
        );
        let Some(Command::Menu(Target::Space(id))) = Command::parse("menu/space/4") else {
            panic!("not a Space's menu")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "4");
        let Some(Command::Menu(Target::SpaceProfile(id))) = Command::parse("menu/space-profile/4")
        else {
            panic!("not a Space's profiles")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "4");
        let Some(Command::ChooseMenu(Target::Space(id), item)) =
            Command::parse("menu/choose/space/4/profile/1")
        else {
            panic!("not a Space's menu choice")
        };
        assert_eq!(
            (serde_json::to_string(&id).unwrap(), item.as_str()),
            ("4".into(), "profile/1")
        );
        // encodeURIComponent('[{"target":{"space":4},"x":7,"y":180.5,"width":250,"height":32}]')
        let encoded = "%5B%7B%22target%22%3A%7B%22space%22%3A4%7D%2C%22x%22%3A7%2C%22y%22%3A180.5\
                       %2C%22width%22%3A250%2C%22height%22%3A32%7D%5D";
        let Some(Command::MenuTargets { view, targets }) =
            Command::parse(&format!("menu/targets/revealed/{encoded}"))
        else {
            panic!("not menu targets")
        };
        assert_eq!(view, ButtonView::Revealed);
        let Target::Space(id) = targets[0].target else {
            panic!("not a Space")
        };
        assert_eq!(serde_json::to_string(&id).unwrap(), "4");
        assert!(targets[0].contains(7.0, 180.5) && targets[0].contains(256.9, 212.4));
        assert!(!targets[0].contains(257.0, 200.0) && !targets[0].contains(10.0, 212.5));
        assert!(matches!(
            Command::parse("menu/targets/docked/%5B%5D"),
            Some(Command::MenuTargets {
                view: ButtonView::Docked,
                ..
            })
        ));
        // encodeURIComponent('[{"target":{"pinned":1},"x":0,"y":0,"width":26,"height":26}]')
        let pinned = "%5B%7B%22target%22%3A%7B%22pinned%22%3A1%7D%2C%22x%22%3A0%2C%22y%22%3A0\
                      %2C%22width%22%3A26%2C%22height%22%3A26%7D%5D";
        let Some(Command::MenuTargets { view, targets }) =
            Command::parse(&format!("menu/targets/toolbar/{pinned}"))
        else {
            panic!("not menu targets")
        };
        assert_eq!(
            (view, targets[0].target),
            (ButtonView::Toolbar, Target::Pinned(1))
        );
        assert_eq!(Command::parse("menu/targets/floating/%5B%5D"), None);
        assert_eq!(Command::parse("menu/targets/docked/%5B"), None);
        assert_eq!(Command::parse("menu/space/x"), None);
        assert_eq!(
            Command::parse("menu/site"),
            Some(Command::Menu(Target::Site))
        );
        assert_eq!(
            Command::parse("menu/choose/site/0/clear-cache"),
            Some(Command::ChooseMenu(Target::Site, "clear-cache".into()))
        );
        assert_eq!(Command::parse("menu/node/x"), None);
        assert!(matches!(
            Command::parse("menu/tabs/%5B1%2C2%5D"),
            Some(Command::TabsMenu(ids)) if ids.len() == 2
        ));
        assert_eq!(Command::parse("menu/tab/12"), None);
        assert_eq!(Command::parse("menu/choose/node/12"), None);
        assert_eq!(Command::parse("menu/choose/node/x/pin"), None);
        assert_eq!(Command::parse("menu"), None);
    }

    #[test]
    fn parses_import_commands() {
        assert_eq!(Command::parse("import/show"), Some(Command::ShowImport));
        assert_eq!(Command::parse("import/ready"), Some(Command::ImportReady));
    }

    #[test]
    fn parses_peeks_bar() {
        assert_eq!(
            Command::parse("peek/back"),
            Some(Command::PeekPage(PeekAction::Back))
        );
        assert_eq!(
            Command::parse("peek/stop"),
            Some(Command::PeekPage(PeekAction::Stop))
        );
    }

    #[test]
    fn parses_little_window_commands() {
        assert_eq!(Command::parse("window/new"), Some(Command::NewWindow));
        assert_eq!(
            Command::parse("window/incognito"),
            Some(Command::NewIncognitoWindow)
        );
        assert_eq!(Command::parse("little/new"), Some(Command::NewLittle(None)));
        assert_eq!(
            Command::parse("little/search/caf%C3%A9"),
            Some(Command::SearchLittle("café".into()))
        );
        assert_eq!(
            Command::parse("little/new/https%3A%2F%2Fexample.com%2F"),
            Some(Command::NewLittle(Some("https://example.com/".into())))
        );
        assert_eq!(
            Command::parse("little/3/ready"),
            Some(Command::Little(3, LittleAction::Ready))
        );
        assert_eq!(
            Command::parse("little/3/focus"),
            Some(Command::Little(3, LittleAction::Focused))
        );
        assert_eq!(
            Command::parse("little/3/open"),
            Some(Command::Little(3, LittleAction::OpenInSpace(None)))
        );
        assert!(matches!(
            Command::parse("little/3/open/7"),
            Some(Command::Little(3, LittleAction::OpenInSpace(Some(_))))
        ));
        assert!(matches!(
            Command::parse("little/3/profile/2"),
            Some(Command::Little(3, LittleAction::SetProfile(_)))
        ));
        assert_eq!(
            Command::parse("little/3/navigate/example.com%2Fa"),
            Some(Command::Little(
                3,
                LittleAction::Navigate("example.com/a".into())
            ))
        );
        assert_eq!(Command::parse("little/x/ready"), None);
        assert_eq!(Command::parse("little/3/close"), None);
    }

    #[test]
    fn parses_settings_commands() {
        assert_eq!(Command::parse("settings/show"), Some(Command::ShowSettings));
        assert_eq!(Command::parse("page/source"), Some(Command::ViewSource));
        assert_eq!(
            Command::parse("page/devtools"),
            Some(Command::DevTools(None))
        );
        assert_eq!(
            Command::parse("page/devtools/network"),
            Some(Command::DevToolsPanel("network"))
        );
        assert_eq!(Command::parse("page/capture"), Some(Command::Capture));
        assert_eq!(
            Command::parse("page/capture/area"),
            Some(Command::CaptureArea)
        );
        assert_eq!(
            Command::parse("page/capture/full"),
            Some(Command::CaptureFull)
        );
        assert_eq!(
            Command::parse("site/settings"),
            Some(Command::ShowSiteSettings)
        );
        assert_eq!(
            Command::parse("page/capture/scroll/0/-12.5/300/200"),
            Some(Command::ScrollCapture(0.0, -12.5, 300.0, 200.0))
        );
        assert_eq!(
            Command::parse("settings/ready"),
            Some(Command::SettingsReady)
        );
        assert_eq!(Command::parse("settings/hide"), Some(Command::HideSettings));
        assert_eq!(
            Command::parse("settings/warn-before-quit/true"),
            Some(Command::SetSetting(Setting::WarnBeforeQuit, true))
        );
        assert_eq!(
            Command::parse("settings/restore-session/false"),
            Some(Command::SetSetting(Setting::RestoreSession, false))
        );
        assert_eq!(
            Command::parse("settings/suspend-tabs/false"),
            Some(Command::SetSetting(Setting::SuspendTabs, false))
        );
        assert_eq!(
            Command::parse("developer/on/localhost%3A3000"),
            Some(Command::DeveloperMode("localhost:3000".into(), true))
        );
        assert_eq!(Command::parse("developer/maybe/localhost"), None);
        assert_eq!(
            Command::parse("awake/on/mail.google.com"),
            Some(Command::KeepAwake("mail.google.com".into(), true))
        );
        assert_eq!(
            Command::parse("awake/off/localhost%3A8080"),
            Some(Command::KeepAwake("localhost:8080".into(), false))
        );
        assert_eq!(Command::parse("settings/unknown/true"), None);
        assert_eq!(Command::parse("settings/restore-session/yes"), None);
        assert!(matches!(
            Command::parse("profile/suggestions/1/off"),
            Some(Command::SetSearchSuggestions(_, false))
        ));
        assert_eq!(Command::parse("profile/suggestions/1/maybe"), None);
        assert!(matches!(
            Command::parse("profile/archive/1/week"),
            Some(Command::SetArchiveAfter(_, ArchiveAfter::Week))
        ));
        assert_eq!(Command::parse("profile/archive/1/soon"), None);
        assert_eq!(Command::parse("archive/list"), Some(Command::SendArchive));
        assert_eq!(
            Command::parse("archive/restore/3"),
            Some(Command::RestoreArchived(3))
        );
        let Some(Command::SetSearchEngine(id, engine)) =
            Command::parse("profile/search/1/duckduckgo")
        else {
            panic!("a search engine");
        };
        assert_eq!(
            (serde_json::to_string(&id).unwrap(), engine),
            ("1".to_string(), SearchEngine::DuckDuckGo)
        );
        assert_eq!(Command::parse("profile/search/1/altavista"), None);
        assert_eq!(Command::parse("import/close"), Some(Command::CloseImport));
        assert_eq!(
            Command::parse("import/full-disk-access"),
            Some(Command::OpenFullDiskAccess)
        );
        // encodeURIComponent('[{"browser":1,"profile":0,"into":4,"bar":"pinnedFolder"}]')
        let encoded = "%5B%7B%22browser%22%3A1%2C%22profile%22%3A0%2C%22into%22%3A4%2C\
                       %22bar%22%3A%22pinnedFolder%22%7D%5D";
        let Some(Command::Import(choices)) = Command::parse(&format!("import/run/{encoded}"))
        else {
            panic!("not an import")
        };
        let expected = r#"[{"browser":1,"profile":0,"into":4,"bar":"pinnedFolder"}]"#;
        assert_eq!(choices, serde_json::from_str::<Vec<_>>(expected).unwrap());
        assert_eq!(choices[0].browser, 1);
        assert_eq!(
            Command::parse("import/run/%5B%5D"),
            Some(Command::Import(vec![]))
        );
        assert_eq!(Command::parse("import/run/%5B"), None);
        assert_eq!(Command::parse("import/run/%zz"), None);
        assert_eq!(Command::parse("import"), None);
    }

    #[test]
    fn ignores_unknown_commands() {
        assert_eq!(Command::parse("quit"), None);
        assert_eq!(Command::parse("select/tab"), None);
    }
}

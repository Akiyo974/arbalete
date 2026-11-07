//! The main window and the UI it shows.
//!
//! The `cef` crate's default callbacks return 0, which means "no" or "cancel": every callback the app
//! relies on is implemented explicitly.

use crate::command::{
    Button, ButtonView, Command, DownloadAction, ExtensionTarget, HitTarget, InspectorPanel,
    LittleAction, PeekAction, Setting, ShortcutEdit, SiteData, Target, Zoom,
};
use crate::downloads::{self, Download};
use crate::extensions;
use crate::menu::{self, Action};
use crate::permissions;
use crate::settings::{self, Settings};
use crate::strings::{chromium, chromium_with};
use arbalete_core::history::{self, History};
use arbalete_core::import::browsers::{self, Family};
use arbalete_core::import::{self, Choice, safari};
use arbalete_core::search::SearchEngine;
use arbalete_core::sidebar::{
    ArchiveAfter, Color, DownloadLocation, Error, NodeId, NodeKind, ProfileId, Sidebar, Space,
    SpaceId, SplitLayout, Tab, View as WindowView,
};
use arbalete_core::url_routing::Destination;
use arbalete_core::{favicon, store};
use cef::*;
use serde::Serialize;
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod archive;
mod auth_prompt;
mod capture;
mod chrome_pages;
mod content;
mod find;
mod import_ui;
mod inspector;
mod layout;
mod little;
mod overlay;
mod pages;
mod peek;
mod permission_prompt;
mod settings_ui;
mod site_controls;
mod split;
mod theme_picker;
mod windows;

use archive::*;
use auth_prompt::*;
use capture::*;
pub use chrome_pages::*;
use content::*;
use find::*;
use import_ui::*;
use inspector::*;
pub use layout::*;
pub use little::*;
use overlay::*;
pub use pages::*;
pub use peek::*;
use permission_prompt::*;
pub use settings_ui::*;
pub use site_controls::*;
pub use split::*;
use theme_picker::*;
pub use windows::*;

/// The UI is served by the app itself under this origin, of its own scheme (app/src/scheme.rs).
const UI_ORIGIN: &str = "arbalete://ui/";

/// CEF's root cache path, which holds every profile's web data. Read from the IO thread too,
/// which serves the favicons.
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

const SIDEBAR_WIDTH: i32 = 260;

/// The docked sidebar is wider while it shows the library.
const LIBRARY_WIDTH: i32 = SIDEBAR_WIDTH + 180;

/// The page is a card inset from the window's edges and from the sidebar, 10 points like Arc's,
/// as far as a split's pages stand from each other. The window paints its margins with the
/// colors the sidebar sends (Command::WindowColors).
const CARD_MARGIN: i32 = 10;

/// The strip along the window's left edge that reveals the hidden sidebar when the pointer
/// reaches it. Views cannot be transparent, so it shows the sidebar's background.
const EDGE_WIDTH: i32 = 4;

/// How far the floating sidebar sits from the window's edges.
const FLOAT_INSET: i32 = 5;

/// How long the sidebar's moves last, in milliseconds: hiding or showing it, the card following;
/// the revealed sidebar floating in, with a spring, and out; and docking, from above the page or
/// beside an empty Space's panel.
const HIDE_DURATION: u32 = 85;
const REVEAL_DURATION: u32 = 250;
const CONCEAL_DURATION: u32 = 90;
const DOCK_DURATION: u32 = 70;
const FLOATING_DOCK_DURATION: u32 = 50;

/// The time between two steps of a native slide: about a frame.
const FRAME_MS: i64 = 8;

/// The room between a split's pages, in which their dividers sit, like Arc's.
const SPLIT_GAP: i32 = 10;
/// The bar at the top of each page of a split, above the page, like Arc's.
const PANE_BAR_HEIGHT: i32 = 32;

/// The height of Peek's bar, over its page (app/macos/Peek.swift).
const PEEK_BAR: i32 = 32;

/// The height of the bar above the page, measured on Arc's in toolbar mode. The developer bar
/// takes it too, unlike Arc's, taller: switching between them, nothing moves, and the site
/// controls stay under their button.
const PAGE_TOOLBAR_HEIGHT: i32 = 33;

/// The first tab of a new session.
const START_URL: &str = "https://www.wikipedia.org/";

thread_local! {
    /// Each profile's web data, `None` while CEF initializes it (create_profile_context).
    static CONTEXTS: RefCell<HashMap<ProfileId, Option<RequestContext>>> = RefCell::default();
    /// The live page of each tab shown since it opened, hidden while another tab is shown:
    /// switching back to it does not load it again.
    static PAGES: RefCell<HashMap<NodeId, Live>> = RefCell::default();
    /// The import assistant, shown above the page; created the first time it is shown, as most
    /// sessions never import.
    static IMPORT_VIEW: RefCell<Option<Panel>> = const { RefCell::new(None) };
    static IMPORT: RefCell<Option<Spot>> = const { RefCell::new(None) };
    /// The native panels' places changed, which the interface learns soon (surfaces_changed).
    static SURFACES_DUE: Cell<bool> = const { Cell::new(false) };
    /// The Little Arbalete windows open: a page without the sidebar, which links from other apps
    /// open in.
    static LITTLE: RefCell<Vec<Little>> = const { RefCell::new(Vec::new()) };
    static NEXT_LITTLE: Cell<u32> = const { Cell::new(0) };
    /// The Little Arbalete windows' pages closing as another profile's take their place, by
    /// browser id.
    static REPLACED_LITTLE_PAGES: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    /// The Little Arbalete window in front last, until a main window comes there: its page is
    /// its profile's active tab for the extensions, while their popup has the keyboard too.
    static ACTIVE_LITTLE: Cell<Option<u32>> = const { Cell::new(None) };
    /// Links other apps asked to open before the main window existed.
    static PENDING_LINKS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The settings, in a window of their own while it is open.
    static SETTINGS_VIEW: RefCell<Option<BrowserView>> = const { RefCell::new(None) };
    /// The window Chromium asks the user in, for an extension's optional permissions or its
    /// removal (extensions::ChromeTask).
    static PERMISSION_WINDOW: RefCell<Option<Window>> = const { RefCell::new(None) };
    /// That window is invisible, but for its sheet.
    static PERMISSION_WINDOW_HIDDEN: Cell<bool> = const { Cell::new(false) };
    /// The browsers Chromium opened by itself (StrayClient), until an install closes them.
    static STRAYS: RefCell<Vec<Browser>> = const { RefCell::new(Vec::new()) };
    /// The app's settings, loaded when the context is initialized.
    static SETTINGS: RefCell<Settings> = RefCell::new(Settings::default());
    /// The tabs whose page is frozen or was unloaded, which the sidebar dims.
    static ASLEEP: RefCell<HashSet<NodeId>> = RefCell::default();
    /// The last picture of each tab's page, a JPEG in base64, which covers the page while it
    /// loads again once unloaded.
    static SNAPSHOTS: RefCell<HashMap<NodeId, String>> = RefCell::default();
    /// The link Peek shows over the window, if it shows one.
    static PEEK: RefCell<Option<Peek>> = RefCell::default();
    /// Peek's page closing, and the window it showed in.
    static PEEK_CLOSING: Cell<Option<(i32, u32)>> = const { Cell::new(None) };
    /// The color at the top of each tab's page, as #rrggbb, which tints its bar in a split.
    static TOP_COLORS: RefCell<HashMap<NodeId, String>> = RefCell::default();
    /// The tabs whose unloaded page opens again, covered by its picture once it is made.
    static REVIVED: RefCell<HashSet<NodeId>> = RefCell::default();
    /// The tab whose page is being made, as its view joins the card.
    static MAKING: Cell<Option<NodeId>> = const { Cell::new(None) };
    /// The downloads, kept across sessions, most recent last.
    static DOWNLOADS: RefCell<Vec<Download>> = const { RefCell::new(Vec::new()) };
    /// What cancels each download in progress, by its id in the list.
    static DOWNLOAD_CALLBACKS: RefCell<HashMap<u32, DownloadItemCallback>> = RefCell::default();
    /// The browsers the import assistant lists, which its choices refer to by index.
    static SOURCES: RefCell<Vec<browsers::Browser>> = const { RefCell::new(Vec::new()) };
    /// An import is reading the browsers' data, on a thread of its own.
    static IMPORTING: Cell<bool> = const { Cell::new(false) };
    /// The first launch shows the import assistant with the window, instead of the start page.
    static IMPORT_AT_LAUNCH: Cell<bool> = const { Cell::new(false) };
    static STATE: RefCell<UiState> = RefCell::default();
    /// Where the sidebar is saved, once the previous session is restored.
    static SESSION_FILE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    /// The session changed since it was last written (save_session).
    static SESSION_CHANGED: Cell<bool> = const { Cell::new(false) };
    /// The pages' titles the history has yet to keep, by profile and address.
    static NEW_TITLES: RefCell<HashMap<(ProfileId, String), String>> = RefCell::default();
    /// The changes above are to be written soon (save_later).
    static SAVE_POSTED: Cell<bool> = const { Cell::new(false) };
    /// Each profile's history, opened on its first visit.
    static HISTORIES: RefCell<HashMap<ProfileId, History>> = RefCell::default();
    /// The pages' developer tools open, by tab.
    static INSPECTORS: RefCell<HashMap<NodeId, Rc<Inspector>>> = RefCell::default();
    /// The page capture under way, until its picture is copied.
    static CAPTURING: RefCell<Option<Capturing>> = const { RefCell::new(None) };
    /// The sites' permission requests the user has not answered yet, oldest first.
    static PERMISSION_REQUESTS: RefCell<Vec<PermissionRequest>> = const { RefCell::new(Vec::new()) };
    static NEXT_PERMISSION_REQUEST: Cell<u64> = const { Cell::new(0) };
    /// The sites' and proxies' requests to sign in the user has not answered yet, oldest first.
    static AUTH_REQUESTS: RefCell<Vec<AuthRequest>> = const { RefCell::new(Vec::new()) };
    static NEXT_AUTH_REQUEST: Cell<u64> = const { Cell::new(0) };
    /// The shown page whose renderer last ended, by its browser's id, and when.
    static LAST_CRASH: Cell<Option<(i32, i64)>> = const { Cell::new(None) };
    /// The incognito windows' profiles, with the temporary folder that stands for their
    /// directory, removed with the window: their web data stays in memory.
    static INCOGNITO: RefCell<HashMap<ProfileId, PathBuf>> = RefCell::default();
    /// ⌥⌘N is registered to open Little Arbalete from any app (the little_hotkey setting).
    static LITTLE_HOTKEY: Cell<bool> = const { Cell::new(false) };
    /// Browsers not closed yet: the app quits when the last one closes.
    static OPEN_BROWSERS: Cell<usize> = const { Cell::new(0) };
    /// The window was asked to close, by quitting or its close button.
    static CLOSING: Cell<bool> = const { Cell::new(false) };
    /// The open windows, the first one made with the app.
    static WINDOWS: RefCell<Vec<Rc<Win>>> = RefCell::new(vec![Rc::new(Win::new(0))]);
    /// The window commands and events are about, by its id.
    static FOCUSED: Cell<u32> = const { Cell::new(0) };
    /// The sidebar's width as a drag of its edge began, while it goes on.
    static SIDEBAR_DRAG: Cell<Option<i32>> = const { Cell::new(None) };
    /// The extensions' bridge's browser (make_bridge).
    static BRIDGE: RefCell<Option<Browser>> = const { RefCell::new(None) };
    /// The other windows as quitting started, which the session keeps while they close.
    static QUIT_WINDOWS: RefCell<Option<Vec<WindowView>>> = const { RefCell::new(None) };
    /// Stands for the window once every window has closed, while the app quits.
    static CLOSED: Rc<Win> = Rc::new(Win::new(u32::MAX));
    /// The id the next window opened takes.
    static NEXT_WINDOW: Cell<u32> = const { Cell::new(1) };
    /// What the windows of the previous session but the focused one showed, which open again
    /// once the first window is made.
    static RESTORED_WINDOWS: RefCell<Vec<WindowView>> = const { RefCell::new(Vec::new()) };
}

/// Where a native panel shows over the window, and whether it does, which the app keeps as it
/// would a CEF overlay's: the interface shows the panel there (sync_native_surfaces), and any
/// change tells it soon.
#[derive(Clone, Default)]
struct Spot(Rc<SpotState>);

#[derive(Default)]
struct SpotState {
    visible: Cell<bool>,
    bounds: RefCell<Rect>,
}

impl Spot {
    fn is_visible(&self) -> i32 {
        self.0.visible.get().into()
    }

    fn set_visible(&self, visible: i32) {
        if self.0.visible.replace(visible == 1) != (visible == 1) {
            surfaces_changed();
        }
    }

    fn bounds(&self) -> Rect {
        self.0.bounds.borrow().clone()
    }

    fn set_bounds(&self, bounds: Option<&Rect>) {
        let bounds = bounds.cloned().unwrap_or_default();
        let same =
            |a: &Rect, b: &Rect| (a.x, a.y, a.width, a.height) == (b.x, b.y, b.width, b.height);
        if !same(&self.0.bounds.borrow(), &bounds) {
            self.0.bounds.replace(bounds);
            surfaces_changed();
        }
    }
}

/// Tells the interface where its panels show, once the current work is done.
fn surfaces_changed() {
    #[cfg(target_os = "macos")]
    if !SURFACES_DUE.replace(true) {
        post_task(ThreadId::UI, Some(&mut SyncSurfaces::new()));
    }
}

wrap_task! {
    struct SyncSurfaces {}

    impl Task {
        fn execute(&self) {
            SURFACES_DUE.set(false);
            #[cfg(target_os = "macos")]
            in_each_window(sync_native_surfaces);
        }
    }
}

/// What a window holds of its own: its views, the page it shows and its panels' state.
/// A main window: the native interface's on macOS, CEF's elsewhere.
#[cfg(target_os = "macos")]
type MainWindow = crate::native::Window;

struct Win {
    /// What the native interface knows the window by.
    id: u32,
    /// What the window shows of the sidebar while another window has the focus: the sidebar
    /// shows the focused window's.
    view: RefCell<Option<WindowView>>,
    /// The window it opened from, which it opens offset from.
    cascade: Cell<Option<Rect>>,
    /// An incognito window's Space, the only one it shows, gone with it.
    incognito: Cell<Option<SpaceId>>,
    /// Opened from another window, it shows the Command Bar once in front, like Arc's.
    fresh: Cell<bool>,
    /// The native card holding its pages, once the card has shown (send_panes), which a picture of
    /// the page covers while another window shows it.
    card_view: Cell<Option<*mut std::ffi::c_void>>,
    window: RefCell<Option<MainWindow>>,
    /// Beside the card, at the window's left edge. It must not reach under the card: a web view
    /// below the overlays receives no real mouse event on macOS, only synthesized ones.
    sidebar: RefCell<Option<Panel>>,
    /// The card while the sidebar slides, on its way to where the sidebar's mode puts it
    /// (card_rect).
    sliding_card: RefCell<Option<Rect>>,
    /// Where the pages show, as last sent to the native interface (send_panes).
    #[cfg(target_os = "macos")]
    panes_sent: RefCell<Option<String>>,
    /// The tab whose page the window shows.
    shown: Cell<Option<NodeId>>,
    /// Shown above the page, to open tabs.
    overlay_view: RefCell<Option<Panel>>,
    overlay: RefCell<Option<Spot>>,
    /// A second sidebar, shown above the page while the sidebar is hidden.
    reveal_view: RefCell<Option<Panel>>,
    reveal: RefCell<Option<Spot>>,
    /// The tabs of the split shown, in order, and how it lays them out: empty while none shows.
    split: RefCell<(Vec<NodeId>, Option<SplitLayout>)>,
    /// The find bar, at the top right corner of the card or Peek's; created the first time it is
    /// shown.
    find_view: RefCell<Option<Panel>>,
    find: RefCell<Option<Spot>>,
    /// The text last searched for, which ⌘G searches again.
    find_text: RefCell<String>,
    /// A Space's theme picker, beside the sidebar.
    theme_view: RefCell<Option<Panel>>,
    theme: RefCell<Option<Spot>>,
    /// How far down the window the pointer was as the theme picker opened, which it centers on.
    theme_at: Cell<Option<i32>>,
    /// The left and right edges of the Space's column in the Spaces view it opened from, which it
    /// shows beside.
    theme_beside: Cell<Option<(i32, i32)>>,
    /// The site controls' panel, under their button.
    site_view: RefCell<Option<Panel>>,
    site: RefCell<Option<Spot>>,
    /// Where the site controls' button was, in the window, when they were last shown: their
    /// panel, and the extension popups opened from it, show under it.
    site_button: RefCell<Option<Rect>>,
    site_height: Cell<i32>,
    /// How tall the permission prompt's content is (Command::PermissionPromptHeight).
    permission_height: Cell<i32>,
    /// How tall the sign-in prompt's content is (Command::AuthPromptHeight).
    auth_height: Cell<i32>,
    /// Where the site controls' panel shows its extensions (Command::SiteTargets).
    site_targets: RefCell<Vec<ExtensionTarget>>,
    /// When the panel last closed: a click on its button closes it, as the panel loses the focus,
    /// and must not show it again.
    site_closed: Cell<Option<Instant>>,
    extension_popup: RefCell<Option<ExtensionPopup>>,
    sidebar_mode: Cell<SidebarMode>,
    /// The Command Bar edits the address over the docked sidebar's url bar.
    over_url_bar: Cell<bool>,
    /// The window's least width while the Command Bar shows over the page (minimum_size).
    overlay_min_width: Cell<Option<i32>>,
    /// The Command Bar is open for a page to show beside the selected one, in its split.
    adding_pane: Cell<bool>,
    /// Where a tab dragged over the page shown alone would open: before it or after, one above
    /// the other or side by side. The page gives it the room meanwhile, like Arc's.
    drop_preview: Cell<Option<(bool, bool)>>,
    /// Where among the split's pages a tab dragged over it would open, and the empty page that
    /// stands there meanwhile, like Arc's.
    pane_preview: Cell<Option<usize>>,
    /// The tabs selected last, the most recent first, which the tab switcher lists.
    recent: RefCell<Vec<NodeId>>,
    /// The tab switcher's tabs and the picked one's index, while ⌃ is held.
    switch: RefCell<Option<(Vec<NodeId>, usize)>>,
    /// The sidebar sent the window's colors, which show the card.
    colored: Cell<bool>,
    /// The window's colors last sent (set_window_colors): the card's, the grain and the gradient.
    window_colors: RefCell<Option<(u32, f64, Vec<u32>)>>,
    /// The docked sidebar shows the library, wider (LIBRARY_WIDTH).
    library: Cell<bool>,
    /// The library shows the Spaces, filling the window while it is shown, the card hidden.
    wide_library: Cell<bool>,
    /// Under way while the sidebar moves to its mode.
    slide: RefCell<Option<Slide>>,
    /// The tab shown last, which showing another marks as shown until then (Sidebar::touch).
    last_shown: Cell<Option<NodeId>>,
    #[cfg(target_os = "macos")]
    swipe: RefCell<crate::swipe::Tracker>,
    /// Where the docked sidebar's rows, Space header and pinned extensions are, then the revealed
    /// one's, then the pinned extensions in the bar above the page.
    menu_targets: RefCell<[Vec<HitTarget>; 3]>,
}

impl Win {
    fn new(id: u32) -> Self {
        Self {
            id,
            view: RefCell::new(None),
            cascade: Cell::new(None),
            incognito: Cell::new(None),
            fresh: Cell::new(false),
            card_view: Cell::new(None),
            window: RefCell::new(None),
            sidebar: RefCell::new(None),
            sliding_card: RefCell::new(None),
            #[cfg(target_os = "macos")]
            panes_sent: RefCell::new(None),
            shown: Cell::new(None),
            overlay_view: RefCell::new(None),
            overlay: RefCell::new(None),
            reveal_view: RefCell::new(None),
            reveal: RefCell::new(None),
            split: RefCell::default(),
            find_view: RefCell::new(None),
            find: RefCell::new(None),
            find_text: RefCell::new(String::new()),
            theme_view: RefCell::new(None),
            theme: RefCell::new(None),
            theme_at: Cell::new(None),
            theme_beside: Cell::new(None),
            site_view: RefCell::new(None),
            site: RefCell::new(None),
            site_button: RefCell::new(None),
            site_height: Cell::new(1),
            permission_height: Cell::new(1),
            auth_height: Cell::new(1),
            site_targets: RefCell::new(Vec::new()),
            site_closed: Cell::new(None),
            extension_popup: RefCell::new(None),
            sidebar_mode: Cell::new(SidebarMode::Shown),
            over_url_bar: Cell::new(false),
            overlay_min_width: Cell::new(None),
            adding_pane: Cell::new(false),
            drop_preview: Cell::new(None),
            pane_preview: Cell::new(None),
            recent: RefCell::new(Vec::new()),
            switch: RefCell::new(None),
            colored: Cell::new(false),
            window_colors: RefCell::new(None),
            library: Cell::new(false),
            wide_library: Cell::new(false),
            slide: RefCell::new(None),
            last_shown: Cell::new(None),
            #[cfg(target_os = "macos")]
            swipe: RefCell::default(),
            menu_targets: RefCell::default(),
        }
    }
}

/// The window commands and events are about: the focused one, or another while it closes; once
/// every window has closed, as the app quits, one holding nothing.
fn win() -> Rc<Win> {
    let id = FOCUSED.get();
    WINDOWS
        .with_borrow(|windows| {
            windows
                .iter()
                .find(|win| win.id == id)
                .or(windows.first())
                .cloned()
        })
        .unwrap_or_else(|| CLOSED.with(Rc::clone))
}

/// The window `id`, if open.
fn window_by_id(id: u32) -> Option<Rc<Win>> {
    WINDOWS.with_borrow(|windows| windows.iter().find(|win| win.id == id).cloned())
}

/// Makes the window `id` the one commands and events are about: the sidebar shows what it shows,
/// and keeps what the window focused until then showed apart.
fn focus(id: u32) {
    let Some(target) = window_by_id(id).filter(|_| FOCUSED.get() != id) else {
        return;
    };
    if let Some(current) = window_by_id(FOCUSED.get()) {
        current
            .view
            .set(Some(STATE.with_borrow(|state| state.sidebar.view())));
    }
    FOCUSED.set(id);
    if let Some(view) = target.view.take() {
        STATE.with_borrow_mut(|state| state.sidebar.set_view(&view));
    }
}

/// Runs `act` in each window in turn, as the focused one, the window focused last: a page two
/// windows show stays in it.
fn in_each_window(act: impl Fn()) {
    let was = FOCUSED.get();
    let mut ids: Vec<u32> =
        WINDOWS.with_borrow(|windows| windows.iter().map(|win| win.id).collect());
    ids.sort_by_key(|&id| id == was);
    for id in ids {
        focus(id);
        act();
    }
    focus(was);
}

/// LocalKey's borrowing helpers, for a window's cells.
trait WithBorrow<T> {
    fn with_borrow<R>(&self, f: impl FnOnce(&T) -> R) -> R;
    fn with_borrow_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R;
    fn set(&self, value: T);
}

impl<T> WithBorrow<T> for RefCell<T> {
    fn with_borrow<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.borrow())
    }

    fn with_borrow_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut self.borrow_mut())
    }

    fn set(&self, value: T) {
        *self.borrow_mut() = value;
    }
}

/// The sidebar sits beside the page, or is hidden: then a strip at the window's left edge
/// reveals it above the page while the pointer is over it. Beside an empty Space's panel, it
/// floats instead of hiding. For this session only.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SidebarMode {
    Shown,
    Hidden,
    Revealed,
    Floating,
}

impl SidebarMode {
    /// Where the card and the revealed sidebar stand in this mode, in a window of `size`. While
    /// the sidebar is docked, the revealed one's place is the docked one's, which it docks into.
    fn layout(self, size: &Size) -> Layout {
        // Like Arc's, a page alone in full screen fills the room, without margins.
        let margin = if square_card() { 0 } else { CARD_MARGIN };
        let left = match self {
            // Like Arc's, the docked sidebar's own margin parts its content from the page.
            SidebarMode::Shown => docked_width(),
            SidebarMode::Floating => sidebar_width(),
            SidebarMode::Hidden | SidebarMode::Revealed => margin,
        };
        let card = Rect {
            x: left,
            y: margin,
            width: size.width - left - margin,
            height: size.height - 2 * margin,
        };
        let reveal = match self {
            SidebarMode::Shown => Rect {
                x: 0,
                y: 0,
                width: sidebar_width(),
                height: size.height,
            },
            SidebarMode::Hidden => Rect {
                x: 0,
                y: 0,
                width: EDGE_WIDTH,
                height: size.height,
            },
            SidebarMode::Revealed | SidebarMode::Floating => Rect {
                x: FLOAT_INSET,
                y: FLOAT_INSET,
                width: sidebar_width(),
                height: size.height - 2 * FLOAT_INSET,
            },
        };
        Layout { card, reveal }
    }
}

/// Where the card and the revealed sidebar stand.
#[derive(Clone, Debug)]
struct Layout {
    card: Rect,
    reveal: Rect,
}

/// The card, and the revealed sidebar when it docks, on their way to the sidebar's mode.
struct Slide {
    from: Layout,
    start: Instant,
    duration: Duration,
    /// The revealed sidebar grows into the docked one's place, losing its floating look.
    docking: bool,
}

/// How many closed tabs the UI lists, the last closed first: the state weighs on every update.
const RECENTLY_CLOSED: usize = 10;

/// What the UI shows.
#[derive(Default)]
struct UiState {
    page: Page,
    sidebar: Sidebar,
    /// Closed tabs, the last one at the end, until the app quits.
    closed: Vec<Closed>,
}

/// A closed tab, which may be reopened.
struct Closed {
    /// What it was while open, for the tabs closed beside it; unknown for those closed together.
    id: Option<NodeId>,
    tab: Tab,
    profile: ProfileId,
    at: SystemTime,
    /// The tab of its split it goes back beside, and whether before it.
    beside: Option<(NodeId, bool)>,
}

impl UiState {
    /// Sent whole to the UI on every change: the displayed Space, and its page and its
    /// neighbors', which the UI lays out side by side to swipe between them. The other pages are
    /// left out, as they would weigh on every update.
    fn to_json(&self) -> serde_json::Value {
        let displayed = self.sidebar.displayed().id;
        // An incognito window shows only its Space.
        let incognito = self.sidebar.is_incognito(displayed);
        let spaces = if incognito {
            std::slice::from_ref(self.sidebar.displayed())
        } else {
            self.sidebar.spaces()
        };
        let index = spaces
            .iter()
            .position(|s| s.id == displayed)
            .expect("a known Space");
        json!({
            "page": self.page,
            "toolbar": app_settings().show_toolbar,
            "mediaSources": app_settings().media_sources,
            "incognito": incognito,
            "space": displayed,
            "spaces": spaces.iter().map(|s| json!({
                "id": s.id,
                "title": s.title,
                "emoji": s.emoji,
                "theme": s.theme,
                "profile": s.profile,
                "pinned": self.overview(s.pinned, s.profile),
            })).collect::<Vec<_>>(),
            "pages": spaces
                .iter()
                .enumerate()
                .filter(|(i, _)| i.abs_diff(index) <= 1)
                .map(|(_, s)| self.page_json(s))
                .collect::<Vec<_>>(),
            "profiles": self.sidebar.profiles().iter().map(|p| json!({
                "id": p.id,
                "name": p.name,
                "color": p.color,
            })).collect::<Vec<_>>(),
            // The displayed Space's profile's pinned extensions, installed and turned on, whose
            // buttons show by the address.
            "extensions": self.pinned_extensions(),
            // All of them, which the library lists.
            "closed": self.closed.iter().rev().take(RECENTLY_CLOSED).map(|closed| json!({
                "url": closed.tab.url,
                "title": closed.tab.label(),
                "favicon": favicon_url(closed.profile, &closed.tab),
                "closedAt": closed.at.duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as u64),
            })).collect::<Vec<_>>(),
        })
    }

    /// The displayed Space's profile's pinned extensions that are installed and turned on, in the
    /// order they were pinned.
    fn pinned_extensions(&self) -> Vec<serde_json::Value> {
        let profile = self.sidebar.displayed().profile;
        let installed = extensions::list(profile);
        let profiles = self.sidebar.profiles();
        let pinned = profiles
            .iter()
            .find(|p| p.id == profile)
            .map(|p| &p.pinned_extensions);
        pinned
            .into_iter()
            .flatten()
            .filter_map(|id| {
                installed
                    .iter()
                    .find(|extension| &extension.id == id && extension.enabled)
            })
            .map(|extension| extension_json(profile, extension))
            .collect()
    }

    /// A Space's tabs and its profile's favorites. `roots` names the sections' containers, which
    /// the UI moves nodes into.
    fn page_json(&self, space: &Space) -> serde_json::Value {
        let favorites = self
            .sidebar
            .favorites_of(space.id)
            .expect("a known profile");
        let profile = self
            .sidebar
            .profiles()
            .iter()
            .find(|p| p.id == space.profile);
        json!({
            "space": space.id,
            "selected": space.selected,
            "collapsed": space.collapsed,
            "roots": { "favorites": favorites, "pinned": space.pinned, "today": space.today },
            // Tabs and, like Arc's, splits.
            "favorites": self.items(favorites, space.profile),
            "favoritesHint": profile.is_some_and(|p| !p.favorites_hint_hidden),
            "pinned": self.items(space.pinned, space.profile),
            "today": self.items(space.today, space.profile),
        })
    }

    /// The tabs directly inside a container or a split.
    fn tabs(&self, parent: NodeId, profile: ProfileId) -> Vec<serde_json::Value> {
        let node = self.sidebar.node(parent).expect("a known container");
        node.children
            .iter()
            .filter_map(|&id| match &self.sidebar.node(id)?.kind {
                NodeKind::Tab(tab) => Some(tab_json(id, tab, profile)),
                _ => None,
            })
            .collect()
    }

    /// The library's glance at a Space's pinned tabs: its tabs and its folders, without their
    /// content, so that every Space may be sent on each update.
    fn overview(&self, parent: NodeId, profile: ProfileId) -> Vec<serde_json::Value> {
        let node = self.sidebar.node(parent).expect("a known container");
        node.children
            .iter()
            .flat_map(|&id| match &self.sidebar.node(id).map(|node| &node.kind) {
                Some(NodeKind::Tab(tab)) => vec![tab_json(id, tab, profile)],
                Some(NodeKind::Folder { title, .. }) => {
                    vec![json!({ "id": id, "title": title, "folder": true })]
                }
                Some(NodeKind::Split) => self.tabs(id, profile),
                _ => Vec::new(),
            })
            .collect()
    }

    /// The tabs and folders inside a container or a folder, folders with their own content.
    fn items(&self, parent: NodeId, profile: ProfileId) -> Vec<serde_json::Value> {
        let node = self.sidebar.node(parent).expect("a known container");
        node.children
            .iter()
            .filter_map(|&id| match &self.sidebar.node(id)?.kind {
                NodeKind::Tab(tab) => Some(tab_json(id, tab, profile)),
                NodeKind::Folder { title, expanded } => Some(json!({
                    "id": id,
                    "title": title,
                    "expanded": expanded,
                    "children": self.items(id, profile),
                })),
                NodeKind::Split => Some(json!({ "id": id, "split": self.tabs(id, profile) })),
                NodeKind::Container => None,
            })
            .collect()
    }
}

fn tab_json(id: NodeId, tab: &Tab, profile: ProfileId) -> serde_json::Value {
    json!({
        "id": id,
        "url": tab.url,
        "title": tab.label(),
        "favicon": favicon_url(profile, tab),
        "muted": tab.muted,
        "asleep": ASLEEP.with_borrow(|asleep| asleep.contains(&id)),
        "developer": developer_mode(&tab.url),
    })
}

/// Where the UI loads a tab's icon from: the app serves the profile's cached icons (favicon_file),
/// from the UI's own origin so that the UI may read their pixels.
fn favicon_url(profile: ProfileId, tab: &Tab) -> Option<String> {
    Some(icon_url(profile, tab.favicon.as_deref()?))
}

/// Where the UI loads the cached icon `source` of `profile` from (favicon_url).
fn icon_url(profile: ProfileId, source: &str) -> String {
    let profile = serde_json::to_string(&profile).expect("an id");
    format!(
        "{UI_ORIGIN}favicon/{profile}/{}",
        favicon::file_name(source)
    )
}

/// The loading state of the shown page; its title and url belong to the active tab.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    /// From 0 to 1.
    progress: f64,
    loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
}

/// Changes the page and sends the state to the UI.
fn update_page(change: impl FnOnce(&mut Page)) {
    STATE.with_borrow_mut(|state| change(&mut state.page));
    send_state();
}

/// Changes the sidebar, saves it and sends the state to the UI.
fn update_sidebar<R>(change: impl FnOnce(&mut Sidebar) -> R) -> R {
    let result = STATE.with_borrow_mut(|state| change(&mut state.sidebar));
    save_session();
    send_state();
    result
}

/// What the windows but the focused one show of the sidebar.
fn other_windows() -> Vec<WindowView> {
    let focused = FOCUSED.get();
    WINDOWS.with_borrow(|windows| {
        windows
            .iter()
            .filter(|win| win.id != focused && win.incognito.get().is_none())
            .filter_map(|win| win.view.with_borrow(Clone::clone))
            .collect()
    })
}

/// How long a change waits to be written, at most: a page changing its title many times a
/// second writes the session and the history once a second.
const SAVE_DELAY_MS: i64 = 1000;

/// Saves the sidebar, and what the other windows show of it, once the session is restored: soon,
/// or right away while the app quits.
fn save_session() {
    if SESSION_FILE.with_borrow(Option::is_none) {
        return;
    }
    SESSION_CHANGED.set(true);
    if CLOSING.get() {
        save_pending();
    } else {
        save_later();
    }
}

/// Writes the changes waiting to be in a moment, unless it is already planned.
fn save_later() {
    if !SAVE_POSTED.replace(true) {
        post_delayed_task(ThreadId::UI, Some(&mut SavePending::new()), SAVE_DELAY_MS);
    }
}

wrap_task! {
    struct SavePending {}

    impl Task {
        fn execute(&self) {
            save_pending();
        }
    }
}

/// Writes the changes waiting to be: the pages' new titles and the session.
fn save_pending() {
    SAVE_POSTED.set(false);
    for ((profile, url), title) in NEW_TITLES.take() {
        update_history(profile, |history| history.set_title(&url, &title));
    }
    if SESSION_CHANGED.replace(false) {
        write_session();
    }
}

fn write_session() {
    let Some(path) = SESSION_FILE.with_borrow(Clone::clone) else {
        return;
    };
    // Quitting closes the windows one by one: the session keeps them all.
    let windows = QUIT_WINDOWS
        .with_borrow(Clone::clone)
        .unwrap_or_else(other_windows);
    if let Err(error) = STATE.with_borrow(|state| store::save(&state.sidebar, &windows, &path)) {
        eprintln!("cannot save the session: {error}");
    }
}

fn send_state() {
    prune_pages();
    // The page shown, or its address, may have changed whether its site is in developer mode.
    in_each_window(sync_page_toolbar);
    // The site controls' panel shows the extensions as they are, while it is open too.
    let open = win()
        .site
        .with_borrow(|site| site.as_ref().is_some_and(|site| site.is_visible() == 1));
    if let Some(url) = selected_url().filter(|_| open) {
        send_site_controls(&url);
    }
    // Each window's sidebar shows its own Space and tab.
    in_each_window(|| {
        let json = STATE.with_borrow(UiState::to_json).to_string();
        dispatch(&win().sidebar, "arbalete:state", &json);
        dispatch(&win().reveal_view, "arbalete:state", &json);
    });
    // Extensions see the displayed Space's selected tab as their profile's active tab.
    let active = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let space = sidebar.displayed();
        let NodeKind::Tab(tab) = &sidebar.node(space.selected?)?.kind else {
            return None;
        };
        let page =
            PAGES.with_borrow(|pages| Some(pages.get(&space.selected?)?.browser.identifier()));
        let tab = extensions::ActiveTab {
            page,
            url: tab.url.clone(),
            title: tab.label().to_string(),
            favicon: tab.favicon.clone(),
        };
        Some((space.profile, tab))
    });
    let mut active: HashMap<_, _> = active.into_iter().collect();
    // The Little Arbalete window in front last shows its profile's active tab.
    let little = LITTLE.with_borrow(|littles| {
        let little = littles.iter().find(|l| Some(l.id) == ACTIVE_LITTLE.get())?;
        let tab = extensions::ActiveTab {
            page: little.browser().map(|browser| browser.identifier()),
            url: little.url.clone(),
            title: little.title.clone(),
            favicon: None,
        };
        Some((little.profile, tab))
    });
    active.extend(little);
    extensions::sync(active);
}

/// Saves the downloads list. A list that cannot be written is not worth stopping a download for.
fn save_downloads() {
    let path = DATA_DIR.get().expect("set").join(downloads::FILE_NAME);
    let list = DOWNLOADS.with_borrow(|list| {
        downloads::lasting(list, |profile| {
            INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile))
        })
    });
    if let Err(error) = downloads::save(&list, &path) {
        eprintln!("cannot save the downloads: {error}");
    }
}

/// Sends the downloads to both sidebars, which show the recent ones at their bottom and all of
/// them in the library.
fn send_downloads() {
    update_media_files();
    let json = DOWNLOADS.with_borrow(|downloads| serde_json::to_string(downloads).expect("JSON"));
    in_each_window(|| dispatch(&win().sidebar, "arbalete:downloads", &json));
    dispatch(&win().reveal_view, "arbalete:downloads", &json);
}

/// The images and videos downloaded and still there, which the library's Media section shows:
/// the UI loads them by their download's id (media_file).
static MEDIA_FILES: Mutex<Vec<(u32, PathBuf, &'static str)>> = Mutex::new(Vec::new());

fn update_media_files() {
    let files = DOWNLOADS.with_borrow(|downloads| {
        downloads
            .iter()
            .filter(|download| download.state == downloads::State::Complete && !download.missing)
            .filter_map(|download| {
                let mime = downloads::media_type(&download.name)?;
                Some((download.id, download.path.clone(), mime))
            })
            .collect()
    });
    *MEDIA_FILES.lock().expect("not poisoned") = files;
}

/// The downloaded image or video a UI path names, `media/<download id>`, with its media type.
fn media_file(path: &str) -> Option<(PathBuf, &'static str)> {
    let id: u32 = path.strip_prefix("media/")?.parse().ok()?;
    let files = MEDIA_FILES.lock().expect("not poisoned");
    let (_, file, mime) = files.iter().find(|(media, _, _)| *media == id)?;
    Some((file.clone(), mime))
}

/// Opens the library on its downloads, showing the sidebar first if it is hidden.
fn show_downloads() {
    if win().sidebar_mode.get() == SidebarMode::Hidden {
        set_sidebar(SidebarMode::Shown);
    }
    let current = win();
    let view = &current.sidebar;
    dispatch(view, "arbalete:show-downloads", "null");
}

/// Opens the library on the section it showed last, showing the sidebar first if it is hidden.
fn show_library_page() {
    if win().sidebar_mode.get() == SidebarMode::Hidden {
        set_sidebar(SidebarMode::Shown);
    }
    dispatch(&win().sidebar, "arbalete:open-library", "null");
}

fn download_command(action: DownloadAction, id: u32) {
    let path = DOWNLOADS.with_borrow(|downloads| {
        let download = downloads.iter().find(|download| download.id == id)?;
        Some(download.path.clone())
    });
    let Some(path) = path else { return };
    match action {
        DownloadAction::Cancel => {
            if let Some(callback) =
                DOWNLOAD_CALLBACKS.with_borrow(|callbacks| callbacks.get(&id).cloned())
            {
                callback.cancel();
            }
        }
        // A file deleted or moved since is shown missing.
        DownloadAction::Open | DownloadAction::Show if !path.exists() => {
            DOWNLOADS.with_borrow_mut(|list| {
                if let Some(download) = list.iter_mut().find(|download| download.id == id) {
                    download.missing = true;
                }
            });
            send_downloads();
        }
        #[cfg(target_os = "macos")]
        DownloadAction::Open => crate::mac::open_file(&path),
        #[cfg(target_os = "macos")]
        DownloadAction::Show => crate::mac::show_file(&path),
        DownloadAction::Remove => {
            DOWNLOADS.with_borrow_mut(|downloads| downloads.retain(|download| download.id != id));
            save_downloads();
            send_downloads();
        }
    }
}

/// Gives one of the native interface's surfaces in the window, by name, like the Command Bar's,
/// "overlay", an event: its name and its JSON detail.
fn send_surface(surface: &str, name: &str, detail: &str) {
    #[cfg(target_os = "macos")]
    crate::native::event(win().id, surface, name, detail);
}

/// Gives the native interface's surface standing for one of the window's places, like the
/// sidebar's, an event: its name and its JSON detail.
fn dispatch<T>(view: &RefCell<Option<T>>, name: &str, detail: &str) {
    #[cfg(target_os = "macos")]
    if let Some(surface) = native_surface(std::ptr::from_ref(view).cast()) {
        crate::native::event(win().id, surface, name, detail);
    }
}

/// The native interface's surface that stands for a web view, which gets its events. The
/// revealed sidebar's events also go to the docked one's (shown_sidebar), which the native
/// sidebar stands for in every mode.
#[cfg(target_os = "macos")]
fn native_surface(view: *const ()) -> Option<&'static str> {
    let current = win();
    let place = |cell: &RefCell<Option<Panel>>| std::ptr::from_ref(cell).cast::<()>();
    [
        (place(&current.sidebar), "sidebar"),
        (place(&current.overlay_view), "overlay"),
        (place(&current.find_view), "find"),
        (place(&current.site_view), "site"),
        (place(&current.theme_view), "theme"),
        (
            SETTINGS_VIEW.with(|cell| std::ptr::from_ref(cell).cast()),
            "settings",
        ),
        (IMPORT_VIEW.with(|cell| place(cell)), "import"),
    ]
    .into_iter()
    .find(|(native, _)| std::ptr::eq(view, *native))
    .map(|(_, surface)| surface)
}

/// Shows the native panels where the app places them (Spot), after every command and layout,
/// and the pages.
#[cfg(target_os = "macos")]
fn sync_native_surfaces() {
    let current = win();
    for (view, overlay) in [
        (&current.overlay_view, &current.overlay),
        (&current.find_view, &current.find),
        (&current.site_view, &current.site),
        (&current.theme_view, &current.theme),
    ] {
        if let Some(overlay) = overlay.with_borrow(Clone::clone) {
            mirror(view, &overlay);
        }
    }
    sync_permission_prompt();
    sync_auth_prompt();
    if let Some(overlay) = IMPORT.with_borrow(Clone::clone) {
        IMPORT_VIEW.with(|view| mirror(view, &overlay));
    }
    send_panes();
}

/// Shows a native panel where its spot is, if it shows.
#[cfg(target_os = "macos")]
fn mirror(view: &RefCell<Option<Panel>>, overlay: &Spot) {
    if let Some(surface) = native_surface(std::ptr::from_ref(view).cast()) {
        crate::native::surface(
            win().id,
            surface,
            overlay.is_visible() == 1,
            &overlay.bounds(),
        );
    }
}

/// Sets a data file that cannot be loaded aside, so starting over from defaults does not lose it.
pub fn keep_broken(path: &Path) {
    match store::set_aside(path) {
        Ok(aside) => eprintln!("kept {} as {}", path.display(), aside.display()),
        Err(error) => eprintln!("cannot set {} aside: {error}", path.display()),
    }
}

/// Loads the sidebar saved by the previous session; a file that cannot be read is set aside, so the
/// next save does not overwrite it. The first launch offers to import the browsers found, if any, before the start page.
fn restore(path: PathBuf) {
    let first_launch = match store::load(&path) {
        Ok(Some((sidebar, windows))) => {
            STATE.with_borrow_mut(|state| state.sidebar = sidebar);
            // The other windows open again with the first one.
            if app_settings().restore_session {
                RESTORED_WINDOWS.set(windows);
            }
            false
        }
        Ok(None) => true,
        Err(error) => {
            eprintln!("cannot restore the session: {error:?}");
            keep_broken(&path);
            false
        }
    };
    SESSION_FILE.set(Some(path));
    // Without restoring the session, every Space starts without "Today" tabs.
    if !app_settings().restore_session {
        STATE.with_borrow_mut(|state| {
            let spaces: Vec<_> = state.sidebar.spaces().iter().map(|s| s.id).collect();
            for space in spaces {
                let _ = state.sidebar.clear_today(space);
            }
        });
    }
    STATE.with_borrow_mut(|state| favicon::sweep(&mut state.sidebar, profile_dir));
    if first_launch && !browsers::detect(&crate::user_home()).is_empty() {
        IMPORT_AT_LAUNCH.set(true);
    } else if STATE.with_borrow_mut(|state| active_tab(&mut state.sidebar).is_none()) {
        open_tab(START_URL);
    }
}

/// A request context for a profile's web data, sharing its storage with the profile's content
/// view: contexts with the same cache path share it.
fn profile_context(profile: ProfileId) -> Option<RequestContext> {
    let dir = profile_dir(profile);
    let settings = RequestContextSettings {
        cache_path: CefString::from(dir.to_str().expect("a UTF-8 path")),
        ..Default::default()
    };
    let context = request_context_create_context(Some(&settings), None)?;
    // Save Link As… and Save Image As… ask where from the profile's downloads folder, like Arc's,
    // rather than the home folder.
    let (downloads, _) = download_folder(Some(profile));
    for name in ["savefile.default_directory", "download.default_directory"] {
        let Some(mut value) = value_create() else {
            break;
        };
        value.set_string(Some(&CefString::from(downloads.to_string_lossy().as_ref())));
        let mut error = CefString::default();
        context.set_preference(
            Some(&CefString::from(name)),
            Some(&mut value),
            Some(&mut error),
        );
    }
    Some(context)
}

pub fn displayed_profile() -> ProfileId {
    STATE.with_borrow(|state| state.sidebar.displayed().profile)
}

/// The selected tab's address.
fn selected_url() -> Option<String> {
    STATE.with_borrow_mut(|state| active_tab(&mut state.sidebar).map(|tab| tab.url.clone()))
}

fn active_tab(sidebar: &mut Sidebar) -> Option<&mut Tab> {
    let id = sidebar.displayed().selected?;
    sidebar.tab_mut(id)
}

/// Opens a tab in the displayed Space and shows it.
fn open_tab(url: &str) {
    if is_chrome_page(url) {
        return open_chrome_page(url);
    }
    update_sidebar(|sidebar| {
        let space = sidebar.displayed().id;
        let tab = sidebar.open_tab(space, url).expect("the displayed Space");
        sidebar.select(space, tab).expect("a new tab");
    });
    show_selected();
}

/// Shows a tab, with its Space; a favorite is shown in the displayed Space.
fn select_tab(id: NodeId) {
    let selected = update_sidebar(|sidebar| {
        let shown = sidebar.displayed().id;
        let space = sidebar.space_of(id).unwrap_or(shown);
        // An incognito window and the others never show each other's tabs.
        if !sidebar.reaches(shown, space) {
            return Err(arbalete_core::sidebar::Error::UnknownNode);
        }
        sidebar
            .select(space, id)
            .and_then(|()| sidebar.display(space))
    });
    if selected.is_ok() {
        show_selected();
    }
}

/// Shows the tab `find` picks, if any.
fn select_found(find: impl FnOnce(&Sidebar) -> Option<NodeId>) {
    if let Some(tab) = STATE.with_borrow(|state| find(&state.sidebar)) {
        select_tab(tab);
    }
}

/// Shows a Space in the sidebar and its selected tab; like a selection, an unknown Space from the
/// UI is ignored.
fn show_space(id: SpaceId) {
    let shown = STATE.with_borrow(|state| state.sidebar.displayed().id);
    if !STATE.with_borrow(|state| state.sidebar.reaches(shown, id)) {
        return;
    }
    if update_sidebar(|sidebar| sidebar.display(id)).is_ok() {
        show_selected();
    }
}

/// Shows the Space `find` picks, if any.
fn show_found_space(find: impl FnOnce(&Sidebar) -> Option<SpaceId>) {
    if let Some(space) = STATE.with_borrow(|state| find(&state.sidebar)) {
        show_space(space);
    }
}

/// Closes a tab; closing the selected one shows the next tab, or the overlay when none is left.
/// Favorites and open pinned tabs stay where they are: closing one deselects it and takes it back
/// to its saved url, and it cannot be reopened since it was never removed. A pinned tab already
/// closed, which its Space does not show, is removed, and may be reopened.
fn close_tab(tab: Option<NodeId>) {
    close_keeping(tab, false);
}

/// Closes a tab as close_tab does; with `keep_pinned`, a closed pinned tab stays too.
fn close_keeping(tab: Option<NodeId>, keep_pinned: bool) {
    let (selected, tab) = STATE.with_borrow_mut(|state| {
        let selected = state.sidebar.displayed().selected;
        let tab = tab
            .or(selected)
            .filter(|&id| state.sidebar.tab_mut(id).is_some());
        (selected, tab)
    });
    let Some(tab) = tab else { return };
    let stays = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        // A pinned tab is open only while it is its Space's selected one.
        let open = sidebar.spaces().iter().any(|s| s.selected == Some(tab));
        sidebar.is_favorite(tab) || (sidebar.is_pinned(tab) && (open || keep_pinned))
    });
    let next = if stays {
        // It opens again at its saved url.
        close_page(tab);
        update_sidebar(|sidebar| {
            let space = sidebar.displayed().id;
            sidebar
                .close_in_place(space, tab)
                .expect("the displayed Space")
        })
    } else {
        remove_tab(tab)
    };
    if selected == Some(tab) {
        show_next(next);
    }
}

/// Removes an open tab, which may be reopened, and returns the tab to show instead.
fn remove_tab(tab: NodeId) -> Option<NodeId> {
    let next = STATE.with_borrow_mut(|state| {
        let profile = state.sidebar.profile_of(tab).expect("an open tab");
        let closed = Closed {
            id: Some(tab),
            tab: state.sidebar.tab_mut(tab).expect("an open tab").clone(),
            profile,
            at: SystemTime::now(),
            beside: state.sidebar.split_neighbor(tab),
        };
        // An incognito window's tabs cannot be reopened, nor are they archived.
        if !INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile)) {
            // Like Arc's, a tab closed goes to the archive too.
            if let Some(space) = state.sidebar.space_of(tab) {
                state
                    .sidebar
                    .archive_closed(closed.tab.clone(), space, now() as u64);
            }
            state.closed.push(closed);
        }
        state.sidebar.next_after_close(tab)
    });
    update_sidebar(|sidebar| sidebar.remove(tab)).expect("an open tab");
    send_archive();
    next
}

/// Shows the tab that replaces the selected one, or, like Arc's, the empty Space's panel when
/// none is left.
fn show_next(next: Option<NodeId>) {
    match next {
        Some(next) => select_tab(next),
        None => hide_content(),
    }
}

/// Closes the tabs of a folder, the selected one last, so the tab shown instead is not one about
/// to close.
fn close_folder_tabs(folder: NodeId) {
    let tabs = STATE.with_borrow(|state| {
        let mut tabs = state.sidebar.folder_tabs(folder).ok()?;
        let selected = state.sidebar.displayed().selected;
        tabs.sort_by_key(|&tab| Some(tab) == selected);
        Some(tabs)
    });
    for tab in tabs.unwrap_or_default() {
        close_keeping(Some(tab), true);
    }
}

/// Closes every "Today" tab of the displayed Space, which may be reopened.
fn clear_today() {
    close_rows(|sidebar| {
        let space = sidebar.displayed();
        let (id, profile) = (space.id, space.profile);
        Ok((sidebar.clear_today(id)?, profile))
    });
}

/// Closes the rows after a tab's, as clear_today closes "Today" tabs.
fn close_tabs_below(tab: NodeId) {
    close_rows(|sidebar| {
        let profile = sidebar.profile_of(tab).ok_or(Error::UnknownNode)?;
        Ok((sidebar.remove_below(tab)?, profile))
    });
}

/// Removes a folder and closes its tabs, as clear_today closes "Today" tabs.
fn delete_folder(folder: NodeId) {
    close_rows(|sidebar| {
        let profile = sidebar.profile_of(folder).ok_or(Error::UnknownNode)?;
        Ok((sidebar.remove_folder(folder)?, profile))
    });
}

/// Removes rows with `remove`, which returns their tabs and their profile; the tabs may be
/// reopened. When the selected tab was one of them, shows the tab closing it would have shown,
/// if it is left, or the overlay. Like a selection, an unknown row from a menu is ignored.
fn close_rows(remove: impl FnOnce(&mut Sidebar) -> Result<(Vec<Tab>, ProfileId), Error>) {
    let space = STATE.with_borrow(|state| state.sidebar.displayed().id);
    let (selected, next) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let selected = sidebar.displayed().selected;
        (
            selected,
            selected.and_then(|tab| sidebar.next_after_close(tab)),
        )
    });
    let Ok((removed, profile)) = update_sidebar(remove) else {
        return;
    };
    let now = SystemTime::now();
    // Like Arc's, the tabs closed go to the archive too, but an incognito window's.
    if !INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile)) {
        STATE.with_borrow_mut(|state| {
            for tab in removed.iter().rev() {
                state
                    .sidebar
                    .archive_closed(tab.clone(), space, self::now() as u64);
            }
        });
        send_archive();
    }
    STATE.with_borrow_mut(|state| {
        state.closed.extend(removed.into_iter().map(|tab| Closed {
            id: None,
            tab,
            profile,
            at: now,
            beside: None,
        }))
    });
    send_state();
    let open = |id: NodeId| STATE.with_borrow_mut(|state| state.sidebar.tab_mut(id).is_some());
    if selected.is_some_and(|tab| !open(tab)) {
        show_next(next.filter(|&id| open(id)));
    }
}

/// Removes a favorite for good, unlike closing it; it may be reopened like a closed tab.
fn remove_favorite(tab: NodeId) {
    let (favorite, selected) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        (
            sidebar.is_favorite(tab),
            sidebar.displayed().selected == Some(tab),
        )
    });
    if !favorite {
        return;
    }
    let next = remove_tab(tab);
    if selected {
        show_next(next);
    }
}

/// Moves a tab to another Space; the selected tab takes the Space along, so it stays in view.
fn move_to_space(id: NodeId, space: SpaceId) {
    // The selected tab, if it moves: the tab itself, or one in the folder.
    let selected = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let selected = sidebar.displayed().selected?;
        let moves = sidebar.row_of(selected) == sidebar.row_of(id)
            || sidebar
                .folder_tabs(id)
                .is_ok_and(|tabs| tabs.contains(&selected));
        moves.then_some(selected)
    });
    // Like a selection, an unknown tab, folder or Space from a menu is ignored.
    if update_sidebar(|sidebar| sidebar.move_to_space(id, space)).is_ok()
        && let Some(selected) = selected
    {
        select_tab(selected);
    }
}

/// Shows the menu of a sidebar row at the pointer.
fn show_menu(target: Target) {
    if let Target::Pinned(rank) = target {
        show_extension_menu(pinned_extensions(displayed_profile()).get(rank));
        return;
    }
    let Some(entries) = STATE.with_borrow(|state| menu::build(&state.sidebar, target)) else {
        return;
    };
    #[cfg(target_os = "macos")]
    crate::mac::pop_up(entries);
}

/// Shows the developer bar's extensions menu at the pointer, like Arc's: the displayed Space's
/// profile's extensions turned on, then adding and managing them.
fn show_extensions_menu() {
    let extensions: Vec<_> = extensions::list(extension_profile())
        .into_iter()
        .filter(|extension| extension.enabled)
        .map(|extension| (extension.id, extension.name))
        .collect();
    let entries = menu::extensions_menu(&extensions);
    #[cfg(target_os = "macos")]
    crate::mac::pop_up(entries);
}

/// The profile whose extensions act: the Little Arbalete window's in front last, if any, else the
/// displayed Space's.
fn extension_profile() -> ProfileId {
    let little = LITTLE.with_borrow(|littles| {
        Some(
            littles
                .iter()
                .find(|l| Some(l.id) == ACTIVE_LITTLE.get())?
                .profile,
        )
    });
    little.unwrap_or_else(displayed_profile)
}

/// Shows the menu of an extension of the displayed Space's profile, by its id.
fn show_extension_menu(id: Option<&String>) {
    let profile = displayed_profile();
    let extension = id.and_then(|id| extensions::list(profile).into_iter().find(|e| &e.id == id));
    let Some(extension) = extension else { return };
    let pinned = pinned_extensions(profile).contains(&extension.id);
    let entries = menu::extension_menu(
        &extension.id,
        &extension.name,
        extension.options.is_some(),
        pinned,
    );
    #[cfg(target_os = "macos")]
    crate::mac::pop_up(entries);
}

/// Runs an item of a row's menu, as if it was picked.
fn choose_menu(target: Target, item: &str) {
    let entries = STATE.with_borrow(|state| menu::build(&state.sidebar, target));
    if let Some(action) = entries.and_then(|entries| menu::find(entries, item)) {
        perform(action);
    }
}

/// Runs the item picked in a menu.
pub fn perform(action: Action) {
    match action {
        Action::Run(command) => run(command),
        Action::RunEach(commands) => commands.into_iter().for_each(run),
        Action::Ask(event, detail) => ask(event, &detail),
        #[cfg(target_os = "macos")]
        Action::CopyLink(url) => crate::mac::copy(&url),
        #[cfg(target_os = "macos")]
        Action::Share(url) => crate::mac::share(&url),
        #[cfg(target_os = "macos")]
        Action::EditPinnedUrl { tab, url } => {
            let detail = "This is what opens first when you click this tab.";
            if let Some(url) = crate::mac::prompt("Edit Pinned URL", detail, &url, "Save") {
                run(Command::SetPinnedUrl(tab, url));
            }
        }
        #[cfg(target_os = "macos")]
        Action::Confirm {
            question,
            detail,
            button,
            command,
        } => {
            if crate::mac::confirm(&question, detail, button) {
                run(command);
            }
        }
    }
}

/// Dispatches an event in the sidebars, which takes the keyboard focus, even from the page.
fn ask(event: &str, detail: &serde_json::Value) {
    focus_sidebar();
    let detail = detail.to_string();
    dispatch(&win().sidebar, event, &detail);
    dispatch(&win().reveal_view, event, &detail);
}

/// Pins or unpins a tab, or the selected one.
fn toggle_pin(tab: Option<NodeId>) {
    update_sidebar(|sidebar| {
        let space = sidebar.displayed();
        let (space, tab) = (space.id, tab.or(space.selected));
        if let Some(tab) = tab {
            // Like a selection, an unknown tab from the UI is ignored.
            let _ = sidebar.toggle_pin(space, tab);
        }
    });
}

/// Creates a folder at the end of `parent`, or of the pinned tabs, and asks the sidebar to rename
/// it, even when the menu was used from the page. Like a selection, an unknown parent is ignored.
fn new_folder(parent: Option<NodeId>) {
    let folder = update_sidebar(|sidebar| {
        let parent = parent.unwrap_or(sidebar.displayed().pinned);
        // Like Arc's.
        sidebar.add_folder(parent, "Untitled")
    });
    if let Ok(folder) = folder {
        ask("arbalete:rename", &json!(folder));
    }
}

/// Gives the sidebar the keyboard focus, even from the page, whose card is a window of its own.
fn focus_sidebar() {
    if let Some(window) = win().window.with_borrow(Clone::clone) {
        window.activate();
    }
    if let Some(sidebar) = &win().sidebar.with_borrow(Clone::clone) {
        sidebar.request_focus();
    }
}

/// Where a profile's downloads go, and whether they ask where each time, like Arc's: the
/// Downloads folder unless the profile has another one.
fn download_folder(profile: Option<ProfileId>) -> (PathBuf, bool) {
    let location = STATE.with_borrow(|state| {
        let profiles = state.sidebar.profiles();
        let profile = profiles.iter().find(|p| Some(p.id) == profile);
        profile.map(|p| p.download_location.clone())
    });
    let downloads = crate::user_home().join("Downloads");
    match location.unwrap_or_default() {
        DownloadLocation::Downloads => (downloads, false),
        DownloadLocation::Folder(folder) => (PathBuf::from(folder), false),
        DownloadLocation::Ask => (downloads, true),
    }
}

/// Deletes a profile no Space uses, like Arc's, with its web data, history included: like a
/// rename, the Default profile or one in use from the UI is ignored.
fn delete_profile(profile: ProfileId) {
    if update_sidebar(|sidebar| sidebar.delete_profile(profile)).is_err() {
        return;
    }
    CONTEXTS.with_borrow_mut(|contexts| contexts.remove(&profile));
    HISTORIES.with_borrow_mut(|histories| histories.remove(&profile));
    NEW_TITLES.with_borrow_mut(|titles| titles.retain(|(of, _), _| *of != profile));
    let dir = profile_dir(profile);
    if let Err(error) = std::fs::remove_dir_all(&dir)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("cannot delete the profile's data: {error}");
    }
    send_settings();
}

/// Creates a profile used by `spaces`, from Arc's Create a new Profile dialog, and shows the
/// displayed Space's page again when it changes profile. Like a rename, a blank or taken name from
/// the UI is ignored.
fn create_profile(name: &str, spaces: &[SpaceId]) {
    let displayed = STATE.with_borrow(|state| state.sidebar.displayed().id);
    if update_sidebar(|sidebar| sidebar.create_profile(name, spaces)).is_ok() {
        send_settings();
        if spaces.contains(&displayed) {
            show_selected();
        }
    }
}

/// Creates a Space in the displayed Space's profile and shows it, with `tab` moved into it and
/// selected. Like a rename, a blank title from the UI is ignored, and like a selection, an unknown
/// tab.
fn create_space(title: &str, emoji: &str, color: Color, tab: Option<NodeId>) {
    let created = update_sidebar(|sidebar| {
        let displayed = sidebar.displayed();
        let (profile, current) = (displayed.profile, displayed.id);
        let space = sidebar.create_space(title, profile)?;
        // Like Arc's, right after the Space it was created from.
        if let Some(index) = sidebar.spaces().iter().position(|s| s.id == current) {
            sidebar.move_space(space, index + 1)?;
        }
        sidebar.set_space_emoji(space, emoji)?;
        sidebar.set_space_theme(space, color.into())?;
        // A tab moved there is selected; a folder moved there is not a tab to select.
        if let Some(tab) = tab
            && sidebar.move_to_space(tab, space).is_ok()
        {
            let _ = sidebar.select(space, tab);
        }
        Ok::<_, arbalete_core::sidebar::Error>(())
    });
    if created.is_ok() {
        show_selected();
    }
}

/// Changes the Spaces, then shows the displayed Space's selected tab if the change displays
/// another Space or `space` itself, whose page may change. Like a selection, an unknown Space or
/// profile from the UI or a menu is ignored.
fn update_spaces(
    space: SpaceId,
    change: impl FnOnce(&mut Sidebar) -> Result<(), arbalete_core::sidebar::Error>,
) {
    let displayed = STATE.with_borrow(|state| state.sidebar.displayed().id);
    let changed = update_sidebar(change).is_ok();
    let now = STATE.with_borrow(|state| state.sidebar.displayed().id);
    if changed && (now != displayed || now == space) {
        show_selected();
    }
}

/// Reopens a closed tab, by its rank from the last closed one: back in its split, beside the tab
/// it showed with while that one is open, like Arc's; else in the displayed Space.
fn reopen_tab(rank: usize) {
    let closed = STATE.with_borrow_mut(|state| {
        let index = state.closed.len().checked_sub(rank + 1)?;
        Some(state.closed.remove(index))
    });
    let Some(closed) = closed else {
        return;
    };
    let tab = update_sidebar(|sidebar| {
        let shown = sidebar.displayed().id;
        // An incognito window and the others never show each other's tabs.
        let beside = closed.beside.and_then(|(target, before)| {
            let space = sidebar.space_of(target)?;
            sidebar
                .reaches(shown, space)
                .then_some((space, target, before))
        });
        let space = beside.map_or(shown, |(space, ..)| space);
        let tab = sidebar
            .open_tab(space, &closed.tab.url)
            .expect("a known Space");
        if let Some((_, target, before)) = beside {
            // A split already full takes it as a row of its own.
            let _ = sidebar.split_beside(tab, target, before);
        }
        // Another profile has not cached its icon: its page sends it as it loads.
        let same_profile = sidebar.profile_of_space(space) == Some(closed.profile);
        let reopened = sidebar.tab_mut(tab).expect("just opened");
        reopened.title = closed.tab.title;
        if same_profile {
            reopened.favicon = closed.tab.favicon;
        }
        tab
    });
    // The tabs closed beside it go back beside it again.
    STATE.with_borrow_mut(|state| {
        for other in &mut state.closed {
            if let Some((target, _)) = &mut other.beside
                && closed.id == Some(*target)
            {
                *target = tab;
            }
        }
    });
    select_tab(tab);
}

/// A tab's live page, in its profile's web data.
struct Live {
    profile: ProfileId,
    /// Drawn in a native container of the interface, rather than in a CEF view (decision 0002).
    browser: Browser,
    /// Since when it is hidden, in milliseconds since the epoch.
    hidden_since: Option<i64>,
    /// Frozen by Chromium: its scripts and timers stop until it shows again.
    frozen: bool,
    /// It uses the camera, the microphone or the screen.
    capturing: bool,
}

/// Loads a url in the shown page.
fn load(url: &str) {
    if let Some(frame) = content().and_then(|browser| browser.main_frame()) {
        frame.load_url(Some(&CefString::from(url)));
    }
}

/// The page the window shows.
fn content() -> Option<Browser> {
    let tab = win().shown.get()?;
    PAGES.with_borrow(|pages| Some(pages.get(&tab)?.browser.clone()))
}

/// Whether a browser is the shown page's.
fn is_shown(browser: Option<&mut Browser>) -> bool {
    let (Some(browser), Some(mut shown)) = (browser, content()) else {
        return false;
    };
    browser.is_same(Some(&mut shown)) == 1
}

/// The tab whose page a browser is, if it is one's.
fn page_tab(browser: &Browser) -> Option<NodeId> {
    let id = browser.identifier();
    PAGES.with_borrow(|pages| {
        pages
            .iter()
            .find(|(_, live)| live.browser.identifier() == id)
            .map(|(tab, _)| *tab)
    })
}

/// The selected tab's profile and url.
fn selected() -> Option<(ProfileId, String)> {
    STATE.with_borrow_mut(|state| {
        let tab = state.sidebar.displayed().selected?;
        let profile = state.sidebar.profile_of(tab)?;
        Some((profile, state.sidebar.tab_mut(tab)?.url.clone()))
    })
}

/// Shows the selected tab's page, which opens the first time, once its profile's web data is
/// ready; or leaves the card empty when the displayed Space has no selected tab.
fn show_selected() {
    let tab = STATE.with_borrow(|state| state.sidebar.displayed().selected);
    if let Some(tab) = tab {
        win().recent.with_borrow_mut(|recent| remember(recent, tab));
    }
    // The tab left and the tab shown both showed until now, for the archive.
    let now = now().try_into().unwrap_or_default();
    STATE.with_borrow_mut(|state| {
        for shown in [win().last_shown.replace(tab), tab].into_iter().flatten() {
            state.sidebar.touch(shown, now);
        }
    });
    prune_pages();
    let Some(tab) = tab.filter(|_| selected().is_some()) else {
        hide_content();
        return;
    };
    // A split's tabs show together, the selected one focused.
    let (tabs, layout) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let row = sidebar.row_of(tab);
        if row == tab {
            (vec![tab], None)
        } else {
            let tabs = sidebar
                .node(row)
                .map_or_else(Vec::new, |node| node.children.clone());
            (tabs, Some(sidebar.split_layout(row)))
        }
    });
    let opened: Option<Vec<_>> = tabs.iter().map(|&tab| page(tab)).collect();
    if opened.is_none() {
        return;
    }
    match layout {
        Some(layout) => show_split(tab, &tabs, layout),
        None => show_content(tab),
    }
}

/// A tab's page, opened the first time, hidden, once its profile's web data is ready: until
/// then, none.
fn page(tab: NodeId) -> Option<()> {
    if PAGES.with_borrow(|pages| pages.contains_key(&tab)) {
        return Some(());
    }
    let (profile, url) = STATE.with_borrow_mut(|state| {
        let profile = state.sidebar.profile_of(tab)?;
        Some((profile, state.sidebar.tab_mut(tab)?.url.clone()))
    })?;
    match CONTEXTS.with_borrow(|contexts| contexts.get(&profile).cloned()) {
        Some(Some(mut context)) => {
            let start = if url.is_empty() { "about:blank" } else { &url };
            if SNAPSHOTS.with_borrow(|snapshots| snapshots.contains_key(&tab)) {
                REVIVED.with_borrow_mut(|revived| revived.insert(tab));
            }
            MAKING.set(Some(tab));
            let browser = make_page(tab, profile, start, &mut context);
            MAKING.set(None);
            let browser = browser?;
            if let Some(host) = browser.host() {
                host.was_hidden(1);
            }
            PAGES.with_borrow_mut(|pages| {
                pages.insert(
                    tab,
                    Live {
                        profile,
                        browser,
                        hidden_since: None,
                        frozen: false,
                        capturing: false,
                    },
                )
            });
            mute_page(tab);
            Some(())
        }
        // The tab shows once its profile's web data is ready.
        Some(None) => None,
        None => {
            create_profile_context(profile);
            None
        }
    }
}

/// Makes a tab's page's browser at `url`, drawn in the view the native interface holds for it.
fn make_page(
    tab: NodeId,
    profile: ProfileId,
    url: &str,
    context: &mut RequestContext,
) -> Option<Browser> {
    #[cfg(target_os = "macos")]
    let info = WindowInfo {
        parent_view: crate::native::page_view(win().id, tab_number(tab))?,
        // Any size: the container gives the page its own.
        bounds: Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        },
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    };
    browser_host_create_browser_sync(
        Some(&info),
        Some(&mut ContentClient::new(profile, false)),
        Some(&CefString::from(url)),
        Some(&BrowserSettings::default()),
        None,
        Some(context),
    )
}

/// The number the native interface knows a tab by.
fn tab_number(tab: NodeId) -> i64 {
    serde_json::to_value(tab)
        .ok()
        .and_then(|number| number.as_i64())
        .unwrap_or(-1)
}

/// Gives a tab's page the keyboard, and its window, if it shows.
fn focus_page(tab: NodeId) {
    let Some(host) = PAGES.with_borrow(|pages| pages.get(&tab)?.browser.host()) else {
        return;
    };
    #[cfg(target_os = "macos")]
    crate::native::focus_page(tab_number(tab));
    host.set_focus(1);
}

/// Shows or hides a tab's page where the card lays it out; Chromium learns it, which stops
/// drawing a hidden page.
fn show_page(tab: NodeId, shown: bool) {
    if let Some(host) = PAGES.with_borrow(|pages| pages.get(&tab)?.browser.host()) {
        host.was_hidden((!shown).into());
    }
}

/// Mutes a tab's page as the tab says, or gives it its sound back.
fn mute_page(tab: NodeId) {
    let muted =
        STATE.with_borrow_mut(|state| state.sidebar.tab_mut(tab).is_some_and(|tab| tab.muted));
    let host = PAGES.with_borrow(|pages| pages.get(&tab)?.browser.host());
    if let Some(host) = host {
        host.set_audio_muted(muted.into());
    }
}

/// Makes a page usable once its browser exists: the extensions learn it, and it takes its tab's
/// sound and, when shown, the card's corners.
fn content_created(profile: ProfileId, browser: Option<&mut Browser>) {
    let Some(browser) = browser else { return };
    extensions::page_created(profile, browser);
    #[cfg(target_os = "macos")]
    cover_revived(Some(browser), true);
    // Made in page(), it is not one of the pages yet, which mutes it once it is.
    let Some(tab) = page_tab(browser) else { return };
    mute_page(tab);
    if win().shown.get() == Some(tab) {
        send_page_state();
    }
}

/// Puts a tab's page in the window instead of the shown one, if any.
fn show_content(tab: NodeId) {
    if win().shown.get() == Some(tab) && win().split.borrow().0.is_empty() {
        return;
    }
    hide_content();
    take_page(tab);
    win().shown.set(Some(tab));
    wake_page(tab);
    // Square in full screen, now that a page shows alone.
    place_card();
    shown_on_card(tab);
}

/// A page shows in one window at a time, like Arc's: this one takes a tab's page from another,
/// which shows its picture instead, dimmed by its Space's color.
fn take_page(tab: NodeId) {
    let current = FOCUSED.get();
    let others: Vec<_> = WINDOWS.with_borrow(|windows| {
        windows
            .iter()
            .filter(|other| {
                other.id != current
                    && (other.shown.get() == Some(tab) || other.split.borrow().0.contains(&tab))
            })
            .cloned()
            .collect()
    });
    for other in others {
        let shown = other.shown.take();
        *other.split.borrow_mut() = Default::default();
        #[cfg(target_os = "macos")]
        if let (Some(card), Some(shown)) = (other.card_view.get(), shown)
            && let Some(jpeg) = SNAPSHOTS.with_borrow(|snapshots| snapshots.get(&shown).cloned())
        {
            crate::mac::cover_page(card, &jpeg, Some(space_tint(shown)));
        }
    }
}

/// Lets a page shown run again, if it was frozen or asleep while hidden.
fn wake_page(tab: NodeId) {
    let frozen = PAGES.with_borrow_mut(|pages| {
        let live = pages.get_mut(&tab)?;
        live.hidden_since = None;
        Some((std::mem::take(&mut live.frozen), live.browser.host()))
    });
    ASLEEP.with_borrow_mut(|asleep| asleep.remove(&tab));
    if let Some((true, host)) = frozen {
        set_frozen(host.as_ref(), false);
    }
    show_page(tab, true);
}

/// The shown page, `tab`'s, takes the keyboard, its container the card, and the toolbar its state.
fn shown_on_card(tab: NodeId) {
    send_empty_card();
    send_panes();
    // Not while the Command Bar shows over it, which keeps the keyboard.
    let overlay = win()
        .overlay
        .with_borrow(|overlay| overlay.as_ref().is_some_and(|o| o.is_visible() == 1));
    if !overlay {
        focus_page(tab);
    }
    send_page_state();
}

/// The color dimming a picture of a tab's page in a window that does not show it: its Space's
/// leading one, or gray.
#[cfg(target_os = "macos")]
fn space_tint(tab: NodeId) -> u32 {
    STATE
        .with_borrow(|state| {
            let sidebar = &state.sidebar;
            let space = sidebar.space_of(tab)?;
            let space = sidebar.spaces().iter().find(|s| s.id == space)?;
            let lead = space.theme.colors.first()?;
            u32::from_str_radix(lead.trim_start_matches('#'), 16).ok()
        })
        .unwrap_or(0x80_8080)
}

/// Tells the UI how the shown page stands: its progress and history are its own.
fn send_page_state() {
    let browser = content();
    let loading = browser
        .as_ref()
        .is_none_or(|browser| browser.is_loading() == 1);
    update_page(|page| {
        page.loading = loading;
        page.progress = if loading { 0.0 } else { 1.0 };
        page.can_go_back = browser
            .as_ref()
            .is_some_and(|browser| browser.can_go_back() == 1);
        page.can_go_forward = browser
            .as_ref()
            .is_some_and(|browser| browser.can_go_forward() == 1);
    });
}

/// Hides the shown page, which keeps running. Without one, the card shows its own color, an empty
/// Space's panel.
fn hide_content() {
    if let Some(shown) = win().shown.get() {
        show_page(shown, false);
    }
    let (split, _) = std::mem::take(&mut *win().split.borrow_mut());
    if !split.is_empty() {
        for &tab in &split {
            show_page(tab, false);
        }
        PAGES.with_borrow_mut(|pages| {
            for tab in &split {
                if let Some(live) = pages.get_mut(tab) {
                    live.hidden_since.get_or_insert_with(now);
                }
            }
        });
        place_card();
        send_panes();
    }
    // A picture of a page another window took no longer stands in for it.
    #[cfg(target_os = "macos")]
    if let Some(card) = win().card_view.get() {
        crate::mac::uncover_page(card);
    }
    if let Some(tab) = win().shown.take() {
        PAGES.with_borrow_mut(|pages| {
            if let Some(live) = pages.get_mut(&tab) {
                live.hidden_since = Some(now());
            }
        });
    }
    send_empty_card();
}

/// Closes the pages of the tabs closed, archived or moved to another profile's Space, whose web
/// data a page cannot change, and forgets the pictures of the tabs gone.
fn prune_pages() {
    let gone: Vec<NodeId> = STATE.with_borrow(|state| {
        PAGES.with_borrow(|pages| {
            pages
                .iter()
                .filter(|(tab, live)| state.sidebar.profile_of(**tab) != Some(live.profile))
                .map(|(tab, _)| *tab)
                .collect()
        })
    });
    for tab in gone {
        close_page(tab);
    }
    STATE.with_borrow(|state| {
        SNAPSHOTS.with_borrow_mut(|snapshots| {
            snapshots.retain(|tab, _| state.sidebar.node(*tab).is_some());
        });
    });
}

/// Closes a tab's page, if it has one: the tab opens again at its url. Its container then goes
/// (BrowserLifeSpan's do_close).
fn close_page(tab: NodeId) {
    close_inspector(tab);
    let Some(live) = PAGES.with_borrow_mut(|pages| pages.remove(&tab)) else {
        return;
    };
    if win().shown.get() == Some(tab) {
        win().shown.set(None);
    }
    if let Some(host) = live.browser.host() {
        host.close_browser(1);
    }
}

wrap_task! {
    struct RemoveContainer {
        view: usize,
    }

    impl Task {
        /// Takes a closing page's view out of the interface with its container, which destroys
        /// its browser.
        fn execute(&self) {
            #[cfg(target_os = "macos")]
            crate::native::page_closed(self.view as *mut std::ffi::c_void);
        }
    }
}

wrap_task! {
    struct RemovePeek {
        window: u32,
    }

    impl Task {
        /// Takes Peek's closing page's view out of its card, which destroys its browser.
        fn execute(&self) {
            #[cfg(target_os = "macos")]
            crate::native::peek_closed(self.window);
        }
    }
}

/// Opens a profile's web data in its own directory, `Profile <id>`: Chromium only accepts profile
/// directories directly in the root cache path. CEF needs it initialized before a browser can
/// use it (ProfileContextHandler).
fn create_profile_context(profile: ProfileId) {
    CONTEXTS.with_borrow_mut(|contexts| contexts.insert(profile, None));
    // Without a directory, an incognito window's web data stays in memory.
    let incognito = INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile));
    let dir = profile_dir(profile);
    let settings = RequestContextSettings {
        cache_path: CefString::from(if incognito {
            ""
        } else {
            dir.to_str().expect("a UTF-8 path")
        }),
        ..Default::default()
    };
    request_context_create_context(
        Some(&settings),
        Some(&mut ProfileContextHandler::new(profile)),
    );
}

fn profile_dir(profile: ProfileId) -> PathBuf {
    if let Some(dir) = INCOGNITO.with_borrow(|incognito| incognito.get(&profile).cloned()) {
        return dir;
    }
    let id = serde_json::to_string(&profile).expect("an id");
    let dir = DATA_DIR.get().expect("set when the context is initialized");
    dir.join(format!("Profile {id}"))
}

/// Changes a profile's history, kept in its directory beside its web data. A history that cannot
/// be written is not worth stopping the browsing for.
fn update_history<E: std::fmt::Display>(
    profile: ProfileId,
    change: impl FnOnce(&mut History) -> Result<(), E>,
) {
    if let Some(Err(error)) = with_history(profile, change) {
        eprintln!("cannot write the history: {error}");
    }
}

/// Runs `f` on a profile's history, opened the first time, unless it cannot be opened.
fn with_history<R>(profile: ProfileId, f: impl FnOnce(&mut History) -> R) -> Option<R> {
    // An incognito window keeps no history.
    if INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile)) {
        return None;
    }
    HISTORIES.with_borrow_mut(|histories| {
        let history = match histories.entry(profile) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                // An imported profile has no directory before its pages first load.
                let dir = profile_dir(profile);
                if let Err(error) = std::fs::create_dir_all(&dir) {
                    eprintln!("cannot create the profile's directory: {error}");
                    return None;
                }
                match History::open(&dir.join(history::FILE_NAME)) {
                    Ok(history) => entry.insert(history),
                    Err(error) => {
                        eprintln!("cannot open the history: {error}");
                        return None;
                    }
                }
            }
        };
        Some(f(history))
    })
}

/// Shows the history: Chromium's own page, as the library has none, like Arc's.
fn show_history() {
    open_chrome_page("chrome://history");
}

fn now() -> i64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    since_epoch.as_millis().try_into().unwrap_or(i64::MAX)
}

wrap_request_context_handler! {
    struct ProfileContextHandler {
        profile: ProfileId,
    }

    impl RequestContextHandler {
        /// Keeps the profile's web data for its pages, reads its extensions, and shows the
        /// selected tab if it waited for them.
        fn on_request_context_initialized(&self, context: Option<&mut RequestContext>) {
            let Some(context) = context else { return };
            // Like Arc's, an incognito window's pages show dark.
            if INCOGNITO.with_borrow(|incognito| incognito.contains_key(&self.profile)) {
                context.set_chrome_color_scheme(ColorVariant::DARK, 0);
            }
            CONTEXTS.with_borrow_mut(|contexts| contexts.insert(self.profile, Some(context.clone())));
            extensions::profile_ready(self.profile, &profile_dir(self.profile));
            in_each_window(show_selected);
        }
    }
}

/// Runs a command later on the UI thread, out of the callback that asks for it.
pub fn post(command: Command) {
    post_task(ThreadId::UI, Some(&mut RunCommand::new(command)));
}

/// Runs a command from the UI or a menu, on the UI thread.
/// Runs a command from the window `window`.
#[cfg(target_os = "macos")]
pub fn run_in(window: u32, command: Command) {
    focus(window);
    run(command);
}

pub fn run(command: Command) {
    run_command(command);
    #[cfg(target_os = "macos")]
    sync_native_surfaces();
}

fn run_command(command: Command) {
    let browser = content();
    match (command, browser) {
        (Command::Ready, _) => {
            send_state();
            send_downloads();
            send_window_width();
        }
        (Command::Download(action, id), _) => download_command(action, id),
        (Command::ShowDownloads, _) => show_downloads(),
        (Command::ShowLibraryPage, _) => show_library_page(),
        (Command::SwitchTab(forward), _) => switch_tab(forward),
        (Command::EndTabSwitch, _) => end_tab_switch(),
        (Command::ShowHistory, _) => show_history(),
        (Command::Select(id), _) => select_tab(id),
        (Command::NextTab, _) => select_found(|s| s.next_tab(s.displayed().id, true)),
        (Command::PreviousTab, _) => select_found(|s| s.next_tab(s.displayed().id, false)),
        (Command::Favorite(number), _) => select_found(|s| s.favorite(s.displayed().id, number)),
        (Command::ShowSpace(id), _) => show_space(id),
        (Command::SpaceNumber(number), _) => show_found_space(|s| s.space_at(number)),
        (Command::NextSpace, _) => show_found_space(|s| s.next_space(true)),
        (Command::PreviousSpace, _) => show_found_space(|s| s.next_space(false)),
        (Command::ShowOverlay, _) => show_overlay(true),
        (Command::HideOverlay, _) => show_overlay(false),
        (Command::EditUrl, _) => edit_url(),
        (Command::SearchOverlay(text), _) => search_overlay(&text, "overlay"),
        (Command::SearchLittle(text), _) => search_overlay(&text, "little"),
        (Command::OverlayHeight(height), _) => {
            place_overlay(i32::try_from(height).unwrap_or(OVERLAY_ROW));
            round_overlay();
        }
        (Command::Navigate(url), _) => navigate(&url),
        (Command::ToggleSidebar, _) => {
            let has_tab = STATE.with_borrow(|state| state.sidebar.displayed().selected.is_some());
            set_sidebar(toggled(win().sidebar_mode.get(), has_tab));
        }
        (Command::RevealSidebar, _) if win().sidebar_mode.get() == SidebarMode::Hidden => {
            set_sidebar(SidebarMode::Revealed);
        }
        (Command::ConcealSidebar, _) if win().sidebar_mode.get() == SidebarMode::Revealed => {
            set_sidebar(SidebarMode::Hidden);
        }
        (Command::RevealSidebar | Command::ConcealSidebar, _) => {}
        (Command::CardOffset(offset), _) => shift_card(offset),
        (Command::ShowLibrary, _) => show_library(),
        (Command::WideLibrary(wide), _) => widen_library(wide),
        (Command::Open(url), _) => {
            let pane = win().adding_pane.get();
            show_overlay(false);
            if pane {
                open_in_pane(&url);
            } else {
                open_tab(&url);
            }
        }
        (Command::AddPane(tab), _) => {
            if let Some(tab) = tab {
                select_tab(tab);
            }
            if selected().is_some() {
                show_overlay(true);
                win().adding_pane.set(true);
            }
        }
        (Command::ClosePane, _) if split_shown().is_some() => close_tab(None),
        (Command::ClosePane, _) => {}
        (Command::SwitchPane(next), _) => switch_pane(next),
        (Command::Peek(url), _) => peek(&url),
        (Command::ClosePeek, _) => close_peek(),
        (Command::PeekPage(action), _) => peek_page(action),
        (Command::KeepPeek { split }, _) => keep_peek(split),
        (Command::FlipPanes(split), _) => flip_panes(split),
        (Command::PreviewPane(index), _) => preview_pane(index),
        (Command::PreviewDrop(preview), _) => {
            if win().drop_preview.replace(preview) != preview {
                place_card();
            }
        }
        (Command::SeparateAll(split), _) => {
            if update_sidebar(|sidebar| sidebar.separate_all(split)).is_ok() {
                show_selected();
            }
        }
        (Command::CloseSplit(split), _) => close_rows(|sidebar| {
            let profile = sidebar.profile_of(split).ok_or(Error::UnknownNode)?;
            Ok((sidebar.remove_split(split)?, profile))
        }),
        (Command::Close(None), _) if settings_focused() => close_settings(),
        // ⌘W closes the window of Chromium's own pages when it is in front.
        (Command::Close(None), _) if chrome_page_focused() => {
            if let Some((window, _, _)) = CHROME_PAGE.with_borrow(Clone::clone) {
                window.close();
            }
        }
        // Like Arc's, ⌘W closes Peek and ⌘O keeps its page in a tab.
        (Command::Close(None), _) if peek_shown() => close_peek(),
        (Command::OpenLittleInSpace, _) if peek_shown() => keep_peek(false),
        (Command::Close(None), _) if focused_little().is_some() => {
            if let Some(id) = focused_little() {
                close_little(id);
            }
        }
        (Command::Close(tab), _) => close_tab(tab),
        (Command::CloseInspector(tab), _) => close_inspector(tab),
        (Command::ResizePane(index, point), _) => resize_panes(index, point),
        (Command::ResizeSidebar(x), _) => resize_sidebar(x),
        // Kept once the drag ends, not on every move, which would stutter.
        (Command::Resized, _) => {
            end_sidebar_resize();
            save_session();
            save_settings();
        }
        (Command::EvenPanes(index), _) => even_panes(index),
        (Command::Reopen(rank), _) => reopen_tab(rank),
        (Command::Pin(tab), _) => toggle_pin(tab),
        (Command::NewFolder(parent), _) => new_folder(parent),
        (Command::CloseFolderTabs(folder), _) => close_folder_tabs(folder),
        (Command::RemoveFavorite(tab), _) => remove_favorite(tab),
        // Like a selection, an unknown Space from the UI is ignored.
        (Command::HideFavoritesHint(space), _) => {
            let _ = update_sidebar(|sidebar| sidebar.hide_favorites_hint(space));
        }
        (Command::ClearToday, _) => clear_today(),
        (Command::CopyLink, _) => {
            let url = STATE
                .with_borrow_mut(|state| active_tab(&mut state.sidebar).map(|tab| tab.url.clone()));
            if let Some(url) = url {
                crate::mac::copy(&url);
            }
        }
        (Command::Shortcut(edit), _) => edit_shortcut(edit),
        (Command::MediaSource(source, on), _) => {
            let mut settings = app_settings();
            settings.media_sources.retain(|kept| *kept != source);
            if on {
                settings.media_sources.push(source);
            }
            SETTINGS.set(settings);
            save_settings();
            send_state();
        }
        (Command::SaveCaptureToLibrary(file), _) => save_capture_to_library(&file),
        (Command::CopyMarkdownLink, _) => {
            let link = STATE.with_borrow_mut(|state| {
                active_tab(&mut state.sidebar).map(|tab| format!("[{}]({})", tab.label(), tab.url))
            });
            if let Some(link) = link {
                crate::mac::copy(&link);
            }
        }
        (Command::MoveToSpace(id, space), _) => move_to_space(id, space),
        (Command::DuplicateRow(id), _) => {
            let _ = update_sidebar(|sidebar| sidebar.duplicate_below(id));
        }
        (Command::CloseTabsBelow(tab), _) => close_tabs_below(tab),
        (Command::DeleteFolder(folder), _) => delete_folder(folder),
        (Command::FolderToSpace(folder), _) => {
            if update_sidebar(|sidebar| sidebar.folder_to_space(folder)).is_ok() {
                show_selected();
            }
        }
        (Command::Menu(target), _) => show_menu(target),
        (Command::TabsMenu(ids), _) => {
            let entries = STATE.with_borrow(|state| menu::tabs_menu(&state.sidebar, &ids));
            #[cfg(target_os = "macos")]
            crate::mac::pop_up(entries);
        }
        (Command::MenuTargets { view, targets }, _) => {
            win()
                .menu_targets
                .with_borrow_mut(|all| all[view as usize] = targets);
        }
        (Command::ChooseMenu(target, item), _) => choose_menu(target, &item),
        // Like a selection, an unknown folder or a blank title from the UI is ignored.
        (Command::ToggleFolder(id), _) => {
            let _ = update_sidebar(|sidebar| sidebar.toggle_folder(id));
        }
        (Command::Rename(id, title), _) => {
            let _ = update_sidebar(|sidebar| sidebar.rename(id, &title));
        }
        (Command::SetPinnedUrl(tab, url), _) => {
            let _ = update_sidebar(|sidebar| sidebar.set_saved_url(tab, &url));
        }
        (Command::ToggleMute(tab), _) => {
            if update_sidebar(|sidebar| sidebar.toggle_mute(tab)).is_ok() {
                mute_page(tab);
            }
        }
        (Command::Move(id, parent, index), _) => {
            if update_sidebar(|sidebar| sidebar.move_node(id, parent, index)).is_ok() {
                show_selected();
            }
        }
        (Command::Split(tab, target, before), _) => {
            if update_sidebar(|sidebar| sidebar.split_beside(tab, target, before)).is_ok() {
                show_selected();
            }
        }
        (Command::Duplicate(tab, parent, index), _) => {
            let _ = update_sidebar(|sidebar| sidebar.duplicate(tab, parent, index));
        }
        (Command::SplitDuplicate(tab, target, before), _) => {
            if update_sidebar(|sidebar| sidebar.split_duplicate(tab, target, before)).is_ok() {
                show_selected();
            }
        }
        (Command::Separate(tab), _) => {
            if update_sidebar(|sidebar| sidebar.separate(tab)).is_ok() {
                show_selected();
            }
        }
        (Command::NewProfile, _) => ask("arbalete:new-profile", &json!(null)),
        (Command::CreateProfile { name, spaces }, _) => create_profile(&name, &spaces),
        (Command::DeleteProfile(id), _) => delete_profile(id),
        // Like Arc's, the page shows in front of the browser, the settings closed.
        (Command::ProfilePage(id, url), _) => {
            if is_chrome_page(&url) {
                close_settings();
                open_profile_page(&url, id);
            }
        }
        // Like a folder, an unknown profile or a blank name from the UI is ignored.
        (Command::RenameProfile(id, name), _) => {
            if update_sidebar(|sidebar| sidebar.rename_profile(id, &name)).is_ok() {
                send_settings();
            }
        }
        (Command::SetArchiveAfter(id, after), _) => {
            if update_sidebar(|sidebar| sidebar.set_archive_after(id, after)).is_ok() {
                send_settings();
            }
        }
        (Command::SetDownloadLocation(id, location), _) => {
            if update_sidebar(|sidebar| sidebar.set_download_location(id, location)).is_ok() {
                send_settings();
            }
        }
        (Command::SendArchive, _) => send_archive(),
        (Command::RestoreArchived(rank), _) => restore_archived(rank),
        (Command::SetSearchEngine(id, engine), _) => {
            if update_sidebar(|sidebar| sidebar.set_search_engine(id, engine)).is_ok() {
                send_settings();
            }
        }
        (Command::SetSearchSuggestions(id, on), _) => {
            if update_sidebar(|sidebar| sidebar.set_search_suggestions(id, on)).is_ok() {
                send_settings();
            }
        }
        (Command::ShowSettings, _) => show_settings(),
        (Command::ToggleToolbar, _) => toggle_toolbar(),
        // Like Arc's, the menu opens a Command Bar of its own, which opens the window.
        #[cfg(target_os = "macos")]
        (Command::NewLittle(None), _) => crate::native::little_bar(),
        (Command::NewLittle(url), _) => open_link(url),
        (Command::NewWindow, _) => new_window(),
        (Command::NewIncognitoWindow, _) => new_incognito_window(),
        (Command::Little(id, action), _) => little_action(id, action),
        (Command::OpenLittleInSpace, _) => {
            if let Some(id) = focused_little() {
                little_action(id, LittleAction::OpenInSpace(None));
            }
        }
        (Command::HideSettings, _) => close_settings(),
        (Command::SettingsReady, _) => send_settings(),
        (Command::SetSetting(setting, on), _) => set_setting(setting, on),
        (Command::KeepAwake(site, on), _) => keep_awake(site, on),
        (Command::DeveloperMode(site, on), _) => set_developer_mode(site, on),
        (
            Command::CreateSpace {
                title,
                emoji,
                color,
                tab,
            },
            _,
        ) => create_space(&title, &emoji, color, tab),
        // Like a profile, an unknown Space or a blank title from the UI is ignored.
        (Command::RenameSpace(id, title), _) => {
            let _ = update_sidebar(|sidebar| sidebar.rename_space(id, &title));
        }
        (Command::SetSpaceEmoji(id, emoji), _) => {
            let _ = update_sidebar(|sidebar| sidebar.set_space_emoji(id, &emoji));
        }
        (Command::SetUrlRouting(routing), _) => {
            update_sidebar(|sidebar| sidebar.set_url_routing(routing));
            send_settings();
        }
        (Command::SetSpaceTheme(id, theme), _) => {
            let _ = update_sidebar(|sidebar| sidebar.set_space_theme(id, theme));
        }
        (Command::ToggleSpace(id), _) => {
            let _ = update_sidebar(|sidebar| sidebar.toggle_space(id));
        }
        // The selected tab shows in its new profile's web data.
        (Command::SetSpaceProfile(id, profile), _) => {
            update_spaces(id, |sidebar| sidebar.set_space_profile(id, profile));
        }
        (Command::DeleteSpace(id), _) => update_spaces(id, |sidebar| sidebar.delete_space(id)),
        (Command::MoveSpace(id, index), _) => {
            let _ = update_sidebar(|sidebar| sidebar.move_space(id, index));
        }
        (Command::ShowImport, _) => show_import(),
        (Command::ImportReady, _) => send_sources(),
        (Command::CloseImport, _) => close_import(),
        (Command::Import(choices), _) => run_import(choices),
        (Command::OpenFullDiskAccess, _) => open_full_disk_access(),
        (
            Command::WindowColors {
                card,
                grain,
                colors,
            },
            _,
        ) => set_window_colors(card, grain, &colors),
        (Command::Back, Some(browser)) => browser.go_back(),
        (Command::Forward, Some(browser)) => browser.go_forward(),
        (Command::Reload, Some(browser)) => browser.reload(),
        (Command::ForceReload, Some(browser)) => browser.reload_ignore_cache(),
        (Command::ViewSource, Some(browser)) => {
            if let Some(frame) = browser.main_frame() {
                open_tab(&format!("view-source:{}", CefString::from(&frame.url())));
            }
        }
        (Command::DevTools(at), Some(browser)) => {
            if let Some(tab) = page_tab(&browser) {
                match at {
                    Some((x, y)) => {
                        show_inspector(tab, "elements", Some(InspectorAction::InspectAt(x, y)))
                    }
                    None => toggle_inspector(tab),
                }
            }
        }
        // The bar's picker picks an element in the page, the others show their panel.
        (Command::DevToolsPanel(panel), Some(browser)) => {
            let then = (panel == InspectorPanel::Elements)
                .then(|| InspectorAction::Script("DevToolsAPI.enterInspectElementMode()".into()));
            if let Some(tab) = page_tab(&browser) {
                show_inspector(tab, panel.name(), then);
            }
        }
        (Command::Capture, Some(_)) => capture(CaptureMode::Portrait),
        (Command::CaptureArea, Some(_)) => capture(CaptureMode::Area),
        (Command::CaptureFull, Some(_)) => capture(CaptureMode::Full),
        (Command::ShowSiteSettings, _) => show_site_settings(),
        (Command::ScrollCapture(dx, dy, x, y), Some(_)) => scroll_capture(dx, dy, x, y),
        (Command::Stop, Some(browser)) => browser.stop_load(),
        (Command::ClearSiteData(data), Some(browser)) => clear_site_data(&browser, data),
        (Command::Zoom(zoom), Some(browser)) => {
            if let Some(host) = browser.host() {
                host.zoom(match zoom {
                    Zoom::In => ZoomCommand::IN,
                    Zoom::Out => ZoomCommand::OUT,
                    Zoom::Reset => ZoomCommand::RESET,
                });
            }
        }
        (Command::ShowFind, _) => show_find(),
        (Command::Find { text, forward }, _) => {
            if let Some(browser) = find_page() {
                find(&browser, text, forward);
            }
        }
        (Command::HideFind, _) => hide_find(),
        (Command::ShowSiteControls(button), _) => show_site_controls(button),
        (Command::HideSiteControls, _) => hide_site_controls(),
        (Command::ShowThemePicker(id, beside), _) => show_theme_picker(id, beside),
        (Command::HideThemePicker, _) => {
            if let Some(controller) = win().theme.with_borrow(Clone::clone) {
                controller.set_visible(0);
            }
        }
        (Command::SiteControlsHeight(height), _) => {
            win().site_height.set(height);
            place_site_controls();
        }
        (Command::AnswerPermission(id, allow), _) => answer_permission(id, allow),
        (Command::SetSitePermission(setting, allow), _) => set_site_permissions(
            Some(setting),
            if allow {
                ContentSettingValues::ALLOW
            } else {
                ContentSettingValues::BLOCK
            },
        ),
        (Command::ResetSitePermissions, _) => {
            set_site_permissions(None, ContentSettingValues::DEFAULT);
        }
        (Command::PermissionPromptHeight(height), _) => {
            win().permission_height.set(height);
            surfaces_changed();
        }
        (Command::AnswerAuth(id, credentials), _) => answer_auth(id, credentials),
        (Command::AuthPromptHeight(height), _) => {
            win().auth_height.set(height);
            surfaces_changed();
        }
        (Command::OpenExtensionPopup(id), _) => open_extension_popup(&id),
        (Command::ShowStore, _) => show_store(),
        (Command::ShowCertificate, _) => show_certificate(),
        // Like Arc's, Chromium's extensions page.
        (Command::ManageExtensions, _) => open_chrome_page("chrome://extensions"),
        (Command::SiteTargets(targets), _) => win().site_targets.set(targets),
        (Command::RemoveExtension(id), _) => {
            let profile = displayed_profile();
            let extension = extensions::list(profile).into_iter().find(|e| e.id == id);
            let Some(extension) = extension else { return };
            // Chromium confirms it otherwise.
            #[cfg(target_os = "macos")]
            if extensions::removes_itself(profile, &id) {
                let question = format!("Remove “{}”?", extension.name);
                let detail = "Its data in this profile is deleted too.";
                if !crate::mac::confirm(&question, detail, "Remove") {
                    return;
                }
            }
            extensions::uninstall(profile, &extension.id);
        }
        (Command::ExtensionOptions(id), _) => open_extension_options(&id),
        (Command::SetExtensionEnabled(id, enabled), _) => {
            let profile = STATE.with_borrow(|state| state.sidebar.displayed().profile);
            extensions::set_enabled(profile, &id, enabled);
            send_state();
        }
        (Command::PinExtension(id, pinned), _) => {
            let profile = STATE.with_borrow(|state| state.sidebar.displayed().profile);
            let _ = update_sidebar(|sidebar| sidebar.pin_extension(profile, &id, pinned));
        }
        (Command::OpenPinnedExtension(id, button), _) => {
            if let Some(button) = button_in_window(button) {
                win().site_button.set(Some(button));
                open_extension_popup(&id);
            }
        }
        (Command::ExtensionMenu(id), _) => show_extension_menu(Some(&id)),
        (Command::ExtensionsMenu(button), _) => {
            if let Some(button) = button_in_window(button) {
                win().site_button.set(Some(button));
                show_extensions_menu();
            }
        }
        (Command::CloseExtensionPopup, _) => close_extension_popup(),
        (Command::Print, Some(browser)) => {
            if let Some(host) = browser.host() {
                host.print();
            }
        }
        (_, None) => {}
    }
}

fn no_width() -> i32 {
    0
}

/// A place in the window that a panel of the native interface shows over, `height` tall when a
/// layout stacks it: an empty view, without a page of its own.
fn place_view(width: fn() -> i32, height: i32) -> Panel {
    panel_create(Some(&mut PlaceDelegate::new(width, height))).expect("a place")
}

wrap_panel_delegate! {
    struct PlaceDelegate {
        width: fn() -> i32,
        height: i32,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: (self.width)(), height: self.height }
        }
    }

    impl PanelDelegate {}
}

/// The sidebar views a right-click may land on.
#[derive(Debug, PartialEq)]
enum SidebarView {
    Docked,
    /// Above the page, or the strip at the window's edge that reveals it.
    Revealed,
}

/// The sidebar view under a point of a window of `size`, in points from its top left corner,
/// with the point in the view's coordinates.
fn sidebar_point(
    mode: SidebarMode,
    size: &Size,
    x: f64,
    y: f64,
) -> Option<(SidebarView, f64, f64)> {
    let (view, rect) = match mode {
        SidebarMode::Shown => {
            let rect = Rect {
                x: 0,
                y: 0,
                width: docked_width(),
                height: size.height,
            };
            (SidebarView::Docked, rect)
        }
        _ => (SidebarView::Revealed, mode.layout(size).reveal),
    };
    let (left, top) = (f64::from(rect.x), f64::from(rect.y));
    let inside = (left..left + f64::from(rect.width)).contains(&x)
        && (top..top + f64::from(rect.height)).contains(&y);
    inside.then_some((view, x - left, y - top))
}

/// The target under a point of a sidebar view, if any.
fn target_at(targets: &[HitTarget], x: f64, y: f64) -> Option<Target> {
    let target = targets.iter().find(|target| target.contains(x, y));
    target.map(|target| target.target)
}

/// Shows the native menu of what is under a point of the window, in points from its top left
/// corner, at once like a native view, and returns whether a sidebar is there. Real right-clicks
/// reach neither the UI's `contextmenu` listener on macOS nor CEF's context menu handler, and
/// would show Chromium's page menu: the sidebars send where their rows are instead
/// (Command::MenuTargets).
#[cfg(target_os = "macos")]
pub fn menu_at(x: f64, y: f64) -> bool {
    let Some(size) = win()
        .window
        .with_borrow(|window| window.as_ref().map(|w| w.size()))
    else {
        return false;
    };
    // The site controls' panel, above the sidebar, shows its extensions' menu.
    let panel = win().site.with_borrow(|site| {
        let site = site.as_ref().filter(|site| site.is_visible() == 1)?;
        Some(site.bounds())
    });
    if let Some(panel) = panel {
        let (x, y) = (x - f64::from(panel.x), y - f64::from(panel.y));
        if (0.0..f64::from(panel.width)).contains(&x) && (0.0..f64::from(panel.height)).contains(&y)
        {
            let id = win().site_targets.with_borrow(|targets| {
                let target = targets.iter().find(|t| {
                    (t.x..t.x + t.width).contains(&x) && (t.y..t.y + t.height).contains(&y)
                })?;
                Some(target.id.clone())
            });
            show_extension_menu(id.as_ref());
            return true;
        }
    }
    // The bar above the page, in toolbar mode, shows its pinned extensions' menus.
    if shown_toolbar_height() > 0 {
        // At the card's top.
        let card = card_rect().unwrap_or_default();
        let bounds = Rect {
            x: 0,
            y: 0,
            width: card.width,
            height: shown_toolbar_height(),
        };
        let point = Point {
            x: x as i32 - card.x - bounds.x,
            y: y as i32 - card.y - bounds.y,
        };
        if (0..bounds.width).contains(&point.x) && (0..bounds.height).contains(&point.y) {
            let targets = |all: &[Vec<HitTarget>; 3]| {
                target_at(
                    &all[ButtonView::Toolbar as usize],
                    f64::from(point.x),
                    f64::from(point.y),
                )
            };
            if let Some(target) = win().menu_targets.with_borrow(targets) {
                show_menu(target);
            }
            return true;
        }
    }
    let Some((view, x, y)) = sidebar_point(win().sidebar_mode.get(), &size, x, y) else {
        return false;
    };
    let view = match view {
        SidebarView::Docked => ButtonView::Docked,
        SidebarView::Revealed => ButtonView::Revealed,
    };
    let target = win()
        .menu_targets
        .with_borrow(|targets| target_at(&targets[view as usize], x, y));
    if let Some(target) = target {
        show_menu(target);
    }
    true
}

/// The site of a url: its host and port, or the url itself when it has no host, like about:blank.
fn site(url: &str) -> &str {
    url.split_once("://")
        .and_then(|(_, rest)| rest.split(['/', '?', '#']).next())
        .unwrap_or(url)
}

/// Counts a browser closed, and quits once the last one is.
fn browser_closed() {
    OPEN_BROWSERS.set(OPEN_BROWSERS.get() - 1);
    if OPEN_BROWSERS.get() == 0 {
        // A delayed task never runs once the loop has quit.
        save_pending();
        quit_message_loop();
    } else if CLOSING.get() {
        // A browser ready to close while another is not yet cancels the window's close, and with
        // it the ready one's: ask again, or the last ones never close.
        close();
    }
}

// Runs a command posted from a callback on the UI thread (post).
wrap_task! {
    struct RunCommand {
        command: Command,
    }

    impl Task {
        fn execute(&self) {
            run(self.command.clone());
        }    }
}

/// The file behind a url of the UI: a tab's cached icon, a media file or an extension's icon.
pub(crate) fn ui_file_path(url: &str) -> Option<PathBuf> {
    let path = url.strip_prefix(UI_ORIGIN)?;
    favicon_file(path)
        .or_else(|| media_file(path).map(|(file, _)| file))
        .or_else(|| extensions::icon_file(path).map(|(file, _)| file))
        .filter(|file| file.is_file())
}

/// The cached icon a UI path names (favicon_url), which may be missing.
fn favicon_file(path: &str) -> Option<PathBuf> {
    let (profile, name) = path.strip_prefix("favicon/")?.split_once('/')?;
    let profile: ProfileId = serde_json::from_str(profile).ok()?;
    favicon::is_file_name(name).then(|| profile_dir(profile).join(favicon::DIR).join(name))
}

#[cfg(test)]
mod tests {
    use super::{
        CARD_MARGIN, EDGE_WIDTH, FLOAT_INSET, LIBRARY_WIDTH, Rect, SIDEBAR_WIDTH, SidebarMode,
        Size, UiState, favicon_file, lerp_rect, site, win,
    };

    #[test]
    fn tells_the_site_of_a_url() {
        assert_eq!(
            site("https://www.wikipedia.org/wiki/Arrow"),
            "www.wikipedia.org"
        );
        assert_eq!(site("http://localhost:3000?q=1"), "localhost:3000");
        assert_eq!(site("https://example.com#top"), "example.com");
        assert_eq!(site("about:blank"), "about:blank");
    }

    fn rect(rect: &Rect) -> (i32, i32, i32, i32) {
        (rect.x, rect.y, rect.width, rect.height)
    }

    #[test]
    fn the_card_keeps_its_margin_on_all_sides_without_the_sidebar() {
        let m = CARD_MARGIN;
        let card = (m, m, 1000 - 2 * m, 600 - 2 * m);
        assert_eq!(rect(&SidebarMode::Hidden.layout(&SIZE).card), card);
        // The revealed sidebar floats above the page without resizing it.
        assert_eq!(rect(&SidebarMode::Revealed.layout(&SIZE).card), card);
        let docked = (SIDEBAR_WIDTH, m, 1000 - SIDEBAR_WIDTH - m, 600 - 2 * m);
        assert_eq!(rect(&SidebarMode::Shown.layout(&SIZE).card), docked);
        // An empty Space's panel stays where it is while the sidebar floats beside it.
        assert_eq!(rect(&SidebarMode::Floating.layout(&SIZE).card), docked);
    }

    #[test]
    fn the_card_rests_beside_the_wider_library_while_the_sidebar_is_docked() {
        let m = CARD_MARGIN;
        win().library.set(true);
        let docked = rect(&SidebarMode::Shown.layout(&SIZE).card);
        let hidden = rect(&SidebarMode::Hidden.layout(&SIZE).card);
        win().library.set(false);
        assert_eq!(
            docked,
            (LIBRARY_WIDTH, m, 1000 - LIBRARY_WIDTH - m, 600 - 2 * m)
        );
        // Hidden, the sidebar leaves the card its margins whatever it shows.
        assert_eq!(hidden, (m, m, 1000 - 2 * m, 600 - 2 * m));
    }

    #[test]
    fn the_floating_sidebar_is_inset_from_the_window_edges() {
        let i = FLOAT_INSET;
        let floating = (i, i, SIDEBAR_WIDTH, 600 - 2 * i);
        assert_eq!(rect(&SidebarMode::Revealed.layout(&SIZE).reveal), floating);
        assert_eq!(rect(&SidebarMode::Floating.layout(&SIZE).reveal), floating);
        assert_eq!(
            rect(&SidebarMode::Hidden.layout(&SIZE).reveal),
            (0, 0, EDGE_WIDTH, 600)
        );
        // It docks into the docked sidebar's place.
        let docked = (0, 0, SIDEBAR_WIDTH, 600);
        assert_eq!(rect(&SidebarMode::Shown.layout(&SIZE).reveal), docked);
    }

    #[test]
    fn interpolates_rectangles() {
        let from = Rect {
            x: 269,
            y: 9,
            width: 722,
            height: 582,
        };
        let to = SidebarMode::Hidden.layout(&SIZE).card;
        assert_eq!(rect(&lerp_rect(&from, &to, 0.0)), rect(&from));
        assert_eq!(rect(&lerp_rect(&from, &to, 0.5)), (140, 10, 851, 581));
        assert_eq!(rect(&lerp_rect(&from, &to, 1.0)), rect(&to));
    }

    const SIZE: Size = Size {
        width: 1000,
        height: 600,
    };

    #[test]
    fn sends_the_page_and_the_tabs() {
        let mut state = UiState::default();
        let home = state.sidebar.spaces()[0].id;
        let tab = state
            .sidebar
            .open_tab(home, "https://example.com/")
            .unwrap();
        state.sidebar.select(home, tab).unwrap();
        let json = state.to_json();
        assert_eq!(json["page"]["canGoBack"], false);
        let page = &json["pages"][0];
        assert_eq!(page["space"], serde_json::json!(home));
        assert_eq!(page["today"][0]["url"], "https://example.com/");
        assert_eq!(page["today"][0]["id"], page["selected"]);
        let roots = serde_json::json!({
            "favorites": state.sidebar.profiles()[0].favorites,
            "pinned": state.sidebar.spaces()[0].pinned,
            "today": state.sidebar.spaces()[0].today,
        });
        assert_eq!(page["roots"], roots);
    }

    #[test]
    fn sends_pinned_folders_with_their_tabs() {
        let mut state = UiState::default();
        let home = state.sidebar.spaces()[0].clone();
        let folder = state.sidebar.add_folder(home.pinned, "Docs").unwrap();
        let tab = state
            .sidebar
            .open_tab(home.id, "https://example.com/")
            .unwrap();
        state.sidebar.move_node(tab, folder, 0).unwrap();
        let page = &state.to_json()["pages"][0];
        assert_eq!(page["pinned"][0]["title"], "Docs");
        assert_eq!(page["pinned"][0]["expanded"], true);
        assert_eq!(
            page["pinned"][0]["children"][0]["url"],
            "https://example.com/"
        );
        assert_eq!(page["today"], serde_json::json!([]));
    }

    #[test]
    fn sends_a_split_with_its_two_tabs() {
        let mut state = UiState::default();
        let home = state.sidebar.spaces()[0].id;
        let [a, b] = ["https://a.test/", "https://b.test/"]
            .map(|url| state.sidebar.open_tab(home, url).unwrap());
        let split = state.sidebar.split(b, a).unwrap();
        let page = &state.to_json()["pages"][0];
        assert_eq!(page["today"][0]["id"], serde_json::json!(split));
        assert_eq!(page["today"][0]["split"][0]["id"], serde_json::json!(a));
        assert_eq!(page["today"][0]["split"][1]["url"], "https://b.test/");
    }

    #[test]
    fn sends_each_space_with_the_favorites_of_its_profile() {
        let mut state = UiState::default();
        let home = state.sidebar.spaces()[0].id;
        let favorites = state.sidebar.profiles()[0].favorites;
        let tab = state
            .sidebar
            .open_tab(home, "https://example.com/")
            .unwrap();
        state.sidebar.move_node(tab, favorites, 0).unwrap();
        let work = state.sidebar.create_profile("Work", &[]).unwrap();
        state.sidebar.add_space("Work", work).unwrap();
        let json = state.to_json();
        let page = &json["pages"][0];
        assert_eq!(page["favorites"][0]["url"], "https://example.com/");
        assert_eq!(page["favorites"][0]["id"], serde_json::json!(tab));
        assert_eq!(page["today"], serde_json::json!([]));
        let office = &json["pages"][1];
        assert_eq!(office["favorites"], serde_json::json!([]));
        let work_favorites = state.sidebar.profiles()[1].favorites;
        assert_eq!(
            office["roots"]["favorites"],
            serde_json::json!(work_favorites)
        );
        assert_eq!(office["favoritesHint"], true);
        let work = state.sidebar.spaces()[1].id;
        state.sidebar.hide_favorites_hint(work).unwrap();
        assert_eq!(state.to_json()["pages"][1]["favoritesHint"], false);
    }

    #[test]
    fn serves_only_the_files_of_cached_icons() {
        for path in [
            "favicon/1/../../session.json",
            "favicon/x/d41d8cd98f00b204e9800998ecf8427e.png",
            "favicon/1/session.json",
            "favicon/1",
            "index.html",
        ] {
            assert_eq!(favicon_file(path), None, "{path}");
        }
    }

    #[test]
    fn sends_the_closed_tabs_with_when_they_closed_and_their_cached_icon() {
        let mut state = UiState::default();
        let profile = state.sidebar.profiles()[0].id;
        let tab = arbalete_core::sidebar::Tab {
            url: "https://example.com/".into(),
            title: "Example".into(),
            favicon: Some(String::new()),
            opener: None,
            saved_url: None,
            name: None,
            muted: false,
            last_shown: None,
        };
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(60);
        state.closed.push(super::Closed {
            id: None,
            tab,
            profile,
            at,
            beside: None,
        });
        let closed = &state.to_json()["closed"][0];
        assert_eq!(closed["url"], "https://example.com/");
        assert_eq!(closed["closedAt"], 60_000);
        let favicon = format!(
            "arbalete://ui/favicon/{}/d41d8cd98f00b204e9800998ecf8427e.png",
            serde_json::json!(profile)
        );
        assert_eq!(closed["favicon"], favicon);
    }

    #[test]
    fn sends_the_displayed_space_and_every_space() {
        let mut state = UiState::default();
        let home = state.sidebar.spaces()[0].id;
        let profile = state.sidebar.profiles()[0].id;
        let work = state.sidebar.add_space("Work", profile).unwrap();
        state
            .sidebar
            .open_tab(home, "https://example.com/")
            .unwrap();
        let tab = state
            .sidebar
            .open_tab(work, "https://example.org/")
            .unwrap();
        state.sidebar.select(work, tab).unwrap();
        state.sidebar.display(work).unwrap();
        state.sidebar.set_space_emoji(work, "💼").unwrap();
        state
            .sidebar
            .set_space_theme(work, arbalete_core::sidebar::Color::Blue.into())
            .unwrap();
        // The library glances at every Space's pinned tabs, folders closed.
        let pinned = state.sidebar.spaces()[1].pinned;
        let folder = state.sidebar.add_folder(pinned, "Docs").unwrap();
        state
            .sidebar
            .add_tab(folder, "https://a.test/", "A")
            .unwrap();
        let docs = state
            .sidebar
            .add_tab(pinned, "https://b.test/", "B")
            .unwrap();
        let json = state.to_json();
        assert_eq!(json["space"], serde_json::json!(work));
        let theme = |color: &str| serde_json::json!({ "colors": [color], "mode": "auto", "intensity": 0.25, "noise": 0.0 });
        let spaces = serde_json::json!([
            { "id": home, "title": "Home", "emoji": null, "theme": theme("#6e56cf"), "profile": profile, "pinned": [] },
            { "id": work, "title": "Work", "emoji": "💼", "theme": theme("#3e7bfa"), "profile": profile, "pinned": [
                { "id": folder, "title": "Docs", "folder": true },
                { "id": docs, "url": "https://b.test/", "title": "B", "favicon": null, "muted": false, "asleep": false, "developer": false },
            ] },
        ]);
        assert_eq!(json["spaces"], spaces);
        let page = &json["pages"][1];
        assert_eq!(page["space"], serde_json::json!(work));
        assert_eq!(page["today"][0]["url"], "https://example.org/");
        assert_eq!(page["today"].as_array().unwrap().len(), 1);
        assert_eq!(page["selected"], serde_json::json!(tab));
        assert_eq!(json["pages"][0]["today"][0]["url"], "https://example.com/");
    }

    #[test]
    fn sends_only_the_pages_of_the_displayed_space_and_its_neighbors() {
        let mut state = UiState::default();
        let profile = state.sidebar.profiles()[0].id;
        let ids: Vec<_> = (1..5)
            .map(|n| {
                state
                    .sidebar
                    .add_space(&format!("Space {n}"), profile)
                    .unwrap()
            })
            .collect();
        let pages = |state: &UiState| {
            let json = state.to_json();
            let pages = json["pages"].as_array().unwrap();
            pages
                .iter()
                .map(|page| page["space"].clone())
                .collect::<serde_json::Value>()
        };
        let home = state.sidebar.spaces()[0].id;
        assert_eq!(pages(&state), serde_json::json!([home, ids[0]]));
        state.sidebar.display(ids[1]).unwrap();
        assert_eq!(pages(&state), serde_json::json!(ids[..3]));
        state.sidebar.display(ids[3]).unwrap();
        assert_eq!(pages(&state), serde_json::json!(ids[2..]));
    }

    #[test]
    fn sends_every_profile_with_its_name_and_color() {
        let mut state = UiState::default();
        let work = state.sidebar.create_profile("Work", &[]).unwrap();
        state
            .sidebar
            .set_profile_color(work, arbalete_core::sidebar::Color::Blue)
            .unwrap();
        let json = state.to_json();
        let default = state.sidebar.profiles()[0].id;
        let profiles = serde_json::json!([
            { "id": default, "name": "Default", "color": "purple" },
            { "id": work, "name": "Work", "color": "blue" },
        ]);
        assert_eq!(json["profiles"], profiles);
    }

    #[test]
    fn finds_the_sidebar_under_a_point_in_its_own_coordinates() {
        use super::{SidebarView, sidebar_point};
        let at = |mode, x, y| sidebar_point(mode, &SIZE, x, y);
        assert_eq!(
            at(SidebarMode::Shown, 10.0, 300.0),
            Some((SidebarView::Docked, 10.0, 300.0))
        );
        assert_eq!(at(SidebarMode::Shown, 270.0, 300.0), None);
        let inset = f64::from(FLOAT_INSET);
        for mode in [SidebarMode::Revealed, SidebarMode::Floating] {
            assert_eq!(
                at(mode, 10.0, 300.0),
                Some((SidebarView::Revealed, 10.0 - inset, 300.0 - inset))
            );
            assert_eq!(at(mode, 2.0, 300.0), None);
            assert_eq!(at(mode, 270.0, 300.0), None);
        }
        assert_eq!(
            at(SidebarMode::Hidden, 2.0, 300.0),
            Some((SidebarView::Revealed, 2.0, 300.0))
        );
        assert_eq!(at(SidebarMode::Hidden, 10.0, 300.0), None);
        win().library.set(true);
        assert_eq!(
            at(SidebarMode::Shown, 400.0, 300.0),
            Some((SidebarView::Docked, 400.0, 300.0))
        );
        win().library.set(false);
    }

    #[test]
    fn finds_the_target_under_a_point() {
        let targets: Vec<super::HitTarget> = serde_json::from_str(
            r#"[{"target":{"space":4},"x":7,"y":100,"width":250,"height":32},
                {"target":{"node":12},"x":7,"y":140,"width":250,"height":41}]"#,
        )
        .unwrap();
        let at = |y| super::target_at(&targets, 20.0, y).map(|t| format!("{t:?}"));
        assert_eq!(at(110.0), Some(format!("{:?}", targets[0].target)));
        assert_eq!(at(180.0), Some(format!("{:?}", targets[1].target)));
        assert_eq!(at(135.0), None);
        assert_eq!(at(181.0), None);
    }
}

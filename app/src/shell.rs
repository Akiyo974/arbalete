//! The main window and the UI it shows.
//!
//! The `cef` crate's default callbacks return 0, which means "no" or "cancel": every callback the app
//! relies on is implemented explicitly.

use crate::command::{
    Button, ButtonView, Command, DownloadAction, ExtensionTarget, HitTarget, LittleAction,
    PeekAction, Setting, SiteData, Target, Zoom,
};
use crate::downloads::{self, Download};
use crate::extensions;
use crate::menu::{self, Action};
use crate::settings::{self, Settings};
use arbalete_core::history::{self, History};
use arbalete_core::import::browsers::{self, Family};
use arbalete_core::import::{self, Choice, safari};
use arbalete_core::search::SearchEngine;
use arbalete_core::sidebar::{
    ArchiveAfter, Color, Error, NodeId, NodeKind, ProfileId, Sidebar, Space, SpaceId, SplitLayout,
    Tab, View as WindowView,
};
use arbalete_core::url_routing::Destination;
use arbalete_core::{favicon, store};
use cef::wrapper::stream_resource_handler::StreamResourceHandler;
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

/// The UI is served by the app itself under this origin, of its own scheme (app/src/scheme.rs).
const UI_ORIGIN: &str = "arbalete://ui/";

/// CEF's root cache path, which holds every profile's web data. Read from the IO thread too,
/// which serves the favicons.
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

const SIDEBAR_WIDTH: i32 = 260;

/// The docked sidebar is wider while it shows the library (LIBRARY_SHIFT in ui/src/pager.ts).
const LIBRARY_WIDTH: i32 = SIDEBAR_WIDTH + 180;

/// The page is a card inset from the window's edges and from the sidebar, 10 points like Arc's,
/// as far as a split's pages stand from each other. The window paints its margins with the
/// colors the sidebar sends (Command::WindowColors).
const CARD_MARGIN: i32 = 10;

/// The window's color until the sidebar sends the displayed Space's: the dark one, its most
/// common appearance (--window-end in ui/src/index.css).
#[cfg(not(target_os = "macos"))]
const WINDOW_COLOR: u32 = 0xff1b_0e1f;

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

/// The Command Bar's width, at most, its search row's height, which its results add to, and
/// the height its box keeps at most, centered in the window, like Arc's.
const OVERLAY_WIDTH: i32 = 766;
const OVERLAY_ROW: i32 = 50;
const OVERLAY_BOX: i32 = 338;

/// Editing the address, the Command Bar sits over the docked sidebar's url bar instead, at
/// least as wide as this and at most as `URL_EDIT_MAX`, like Arc's: its left edge and top are
/// the url bar's.
const URL_EDIT_WIDTH: i32 = 350;
const URL_EDIT_MAX: i32 = 720;
const URL_BAR_LEFT: i32 = 8;
const URL_BAR_TOP: i32 = 44;

/// How many tabs selected last the tab switcher lists, at most.
const RECENT_TABS: usize = 10;

/// The open tabs and history pages the Command Bar shows for a text, at most.
const OVERLAY_TABS: usize = 3;
const OVERLAY_PAGES: usize = 5;

/// The find bar's size, and its margin from the card's top right corner (ui/src/Find.tsx).
const FIND_SIZE: Size = Size {
    width: 380,
    height: 32,
};
const FIND_MARGIN: i32 = 10;

/// The height of Peek's bar, over its page (app/macos/Peek.swift).
const PEEK_BAR: i32 = 32;

/// The import assistant's size, at most (ui/src/Import.tsx).
const IMPORT_SIZE: Size = Size {
    width: 560,
    height: 480,
};

/// A Little Arbalete window's size, and its bar's height, which holds the window buttons, like
/// Little Arc's: a 31-point row with 8 points above and below.
const LITTLE_SIZE: Size = Size {
    width: 768,
    height: 600,
};
const LITTLE_BAR_HEIGHT: i32 = 47;

/// The height of the bar above the page, measured on Arc's in toolbar mode. The developer bar
/// takes it too, unlike Arc's, taller: switching between them, nothing moves, and the site
/// controls stay under their button.
const PAGE_TOOLBAR_HEIGHT: i32 = 33;

/// The height of the settings window's title row, which holds the window buttons
/// (ui/src/index.css). Not on macOS, whose settings window is native.
#[cfg_attr(target_os = "macos", allow(dead_code))]
const SETTINGS_TITLE_HEIGHT: f32 = 32.0;

/// The settings window's size, like Arc's (ui/src/Settings.tsx).
#[cfg_attr(target_os = "macos", allow(dead_code))]
const SETTINGS_SIZE: Size = Size {
    width: 600,
    height: 520,
};

/// The Privacy & Security pane of System Settings that lists the apps with Full Disk Access.
const FULL_DISK_ACCESS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

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
    /// Links other apps asked to open before the main window existed.
    static PENDING_LINKS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The settings, in a window of their own while it is open.
    static SETTINGS_VIEW: RefCell<Option<BrowserView>> = const { RefCell::new(None) };
    static SETTINGS_WINDOW: RefCell<Option<Window>> = const { RefCell::new(None) };
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
    /// Each profile's history, opened on its first visit.
    static HISTORIES: RefCell<HashMap<ProfileId, History>> = RefCell::default();
    /// The pages' developer tools open, by tab.
    static INSPECTORS: RefCell<HashMap<NodeId, Rc<Inspector>>> = RefCell::default();
    /// The page capture under way, until its picture is copied.
    static CAPTURING: RefCell<Option<Capturing>> = const { RefCell::new(None) };
    /// The incognito windows' profiles, with the temporary folder that stands for their
    /// directory, removed with the window: their web data stays in memory.
    static INCOGNITO: RefCell<HashMap<ProfileId, PathBuf>> = RefCell::default();
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
#[cfg(not(target_os = "macos"))]
type MainWindow = Window;

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
    /// The site controls' panel, under their button.
    site_view: RefCell<Option<Panel>>,
    site: RefCell<Option<Spot>>,
    /// Where the site controls' button was, in the window, when they were last shown: their
    /// panel, and the extension popups opened from it, show under it.
    site_button: RefCell<Option<Rect>>,
    site_height: Cell<i32>,
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
            site_view: RefCell::new(None),
            site: RefCell::new(None),
            site_button: RefCell::new(None),
            site_height: Cell::new(1),
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

/// Opens another window, offset from the focused one, on its Space without a tab, on the Command
/// Bar, like Arc's.
fn new_window() {
    open_window(
        STATE.with_borrow(|state| state.sidebar.new_window_view()),
        true,
    );
}

/// Opens an incognito window, like Arc's: on a Space of its own, in a profile of its own whose
/// web data stays in memory, on the Command Bar.
fn new_incognito_window() {
    let space = update_sidebar(Sidebar::open_incognito);
    let (profile, view) = STATE.with_borrow(|state| {
        let profile = state.sidebar.profile_of_space(space).expect("its Space");
        (profile, state.sidebar.view_showing(space))
    });
    let dir = std::env::temp_dir().join(format!(
        "arbalete-incognito-{}-{}",
        std::process::id(),
        serde_json::to_string(&profile).expect("an id")
    ));
    INCOGNITO.with_borrow_mut(|incognito| incognito.insert(profile, dir));
    open_window_in(view, true, Some(space));
}

/// Closes an incognito window's pages and forgets its Space, its profile and their web data, as
/// the window `id` closes; any other window keeps its own.
fn close_incognito(id: u32) {
    let Some(space) = window_by_id(id).and_then(|win| win.incognito.get()) else {
        return;
    };
    let (profile, tabs) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let profile = sidebar.profile_of_space(space).expect("its Space");
        let tabs: Vec<_> = sidebar
            .tabs()
            .filter(|(tab, _)| sidebar.space_of(*tab) == Some(space))
            .map(|(tab, _)| tab)
            .collect();
        (profile, tabs)
    });
    for tab in tabs {
        close_page(tab);
    }
    let _ = update_sidebar(|sidebar| sidebar.close_incognito(space));
    CONTEXTS.with_borrow_mut(|contexts| contexts.remove(&profile));
    if let Some(dir) = INCOGNITO.with_borrow_mut(|incognito| incognito.remove(&profile)) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Opens another window, offset from the focused one, showing `view` of the sidebar; `fresh`, on
/// the Command Bar.
fn open_window(view: WindowView, fresh: bool) {
    open_window_in(view, fresh, None);
}

/// Opens a window as `open_window` does, an incognito one on its Space `incognito`.
fn open_window_in(view: WindowView, fresh: bool, incognito: Option<SpaceId>) {
    let id = NEXT_WINDOW.get();
    NEXT_WINDOW.set(id + 1);
    let window = Win::new(id);
    window.incognito.set(incognito);
    window.view.set(Some(view));
    window.cascade.set(
        win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.bounds())),
    );
    window.fresh.set(fresh);
    WINDOWS.with_borrow_mut(|windows| windows.push(Rc::new(window)));
    save_session();
    #[cfg(target_os = "macos")]
    open_main_window(id);
    #[cfg(not(target_os = "macos"))]
    window_create_top_level(Some(&mut MainWindowDelegate::new(id)));
}

wrap_task! {
    struct OpenCommandBar {
        window: u32,
    }

    impl Task {
        /// Shows the Command Bar in a window just opened from another, like Arc's.
        fn execute(&self) {
            focus(self.window);
            show_overlay(true);
        }
    }
}

/// How far a new window opens from the one it opens from, down and right, like macOS cascades.
const CASCADE: i32 = 22;

/// Moves the pages of the window `id`, which closes, into another window, hidden.
fn hand_pages_over(id: u32) {
    // Their containers, out of the window closing with them.
    #[cfg(target_os = "macos")]
    if let Some(to) =
        WINDOWS.with_borrow(|windows| windows.iter().find(|win| win.id != id).map(|win| win.id))
    {
        crate::native::hand_pages_over(id, to);
    }
    if let Some(win) = window_by_id(id)
        && let Some(tab) = win.shown.take()
    {
        PAGES.with_borrow_mut(|pages| {
            if let Some(live) = pages.get_mut(&tab) {
                live.hidden_since = Some(now());
            }
        });
    }
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

    /// Where the margins that move the window start: past the docked sidebar, which has its own
    /// drag areas, or at the window's edge.
    // Where the window's margins move it, which the native window does itself on macOS.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    fn margins_start(self) -> i32 {
        if self == SidebarMode::Shown {
            docked_width()
        } else {
            0
        }
    }
}

/// Whether the card is square, without margins: a page alone in full screen, like Arc's.
fn square_card() -> bool {
    #[cfg(target_os = "macos")]
    return win().shown.get().is_some()
        && win().split.borrow().0.is_empty()
        && win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.window_handle()))
            .is_some_and(crate::mac::full_screen);
    #[cfg(not(target_os = "macos"))]
    false
}

/// The docked sidebar's width: wider for the library, the window's for its Spaces.
fn docked_width() -> i32 {
    if fills_window() {
        win()
            .window
            .with_borrow(|window| window.as_ref().map(|window| window.size().width))
            .unwrap_or_else(library_width)
    } else if win().library.get() {
        library_width()
    } else {
        sidebar_width()
    }
}

/// The least and the most the sidebar's edge may make it wide.
// Measured on Arc's.
const MIN_SIDEBAR: i32 = 164;
const MAX_SIDEBAR: i32 = 500;
/// The width the sidebar's edge snaps to, like Arc's.
const SNAP_SIDEBAR: i32 = 228;
/// The window's least size, measured on Arc's, whatever the sidebar's width.
const MIN_WINDOW_WIDTH: i32 = 470;
const MIN_WINDOW_HEIGHT: i32 = 385;

/// The sidebar's width, as dragging its edge set it.
fn sidebar_width() -> i32 {
    app_settings().sidebar_width.clamp(MIN_SIDEBAR, MAX_SIDEBAR)
}

/// The docked sidebar's width while it shows the library, wider than its own by as much as
/// `LIBRARY_WIDTH` is wider than `SIDEBAR_WIDTH`.
fn library_width() -> i32 {
    sidebar_width() + LIBRARY_WIDTH - SIDEBAR_WIDTH
}

/// Gives the sidebar the width its edge was dragged to, `x` points from the window's left edge.
/// The cards follow at once; the rest of the window is laid out once the drag ends
/// (end_sidebar_resize).
fn resize_sidebar(x: f64) {
    // Like Arc's, dragged well past its least width, the sidebar hides; dragged back within the
    // same drag, it shows again.
    let has_tab = STATE.with_borrow(|state| state.sidebar.displayed().selected.is_some());
    let mode = win().sidebar_mode.get();
    // Like Arc's: under half its least width.
    if x < f64::from(MIN_SIDEBAR / 2) {
        if mode == SidebarMode::Shown && has_tab {
            SIDEBAR_DRAG.set(SIDEBAR_DRAG.get().or(Some(sidebar_width())));
            set_sidebar(SidebarMode::Hidden);
        }
        return;
    }
    if mode == SidebarMode::Hidden && SIDEBAR_DRAG.get().is_some() {
        set_sidebar(SidebarMode::Shown);
    }
    let mut width = (x.round() as i32).clamp(MIN_SIDEBAR, MAX_SIDEBAR);
    // Like Arc's, it snaps to its usual width near it.
    if (width - SNAP_SIDEBAR).abs() < 14 {
        width = SNAP_SIDEBAR;
    }
    if width == sidebar_width() {
        return;
    }
    if SIDEBAR_DRAG.get().is_none() {
        SIDEBAR_DRAG.set(Some(sidebar_width()));
    }
    SETTINGS.with_borrow_mut(|settings| settings.sidebar_width = width);
    in_each_window(|| {
        #[cfg(target_os = "macos")]
        {
            place_native_sidebar();
            place_card();
        }
        #[cfg(not(target_os = "macos"))]
        relayout_library();
    });
}

/// Lays the windows out for the sidebar's width, once a drag of its edge ends.
fn end_sidebar_resize() {
    if SIDEBAR_DRAG.take().is_some() {
        in_each_window(relayout_library);
    }
}

/// The docked library shows the Spaces, over the whole window: the card is hidden.
fn fills_window() -> bool {
    win().library.get()
        && win().wide_library.get()
        && win().sidebar_mode.get() == SidebarMode::Shown
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
            "installed": self.installed_extensions(),
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

    /// The displayed Space's profile's extensions, by name, with what the library shows of them.
    fn installed_extensions(&self) -> Vec<serde_json::Value> {
        let profile = self.sidebar.displayed().profile;
        let profiles = self.sidebar.profiles();
        let pinned = profiles
            .iter()
            .find(|p| p.id == profile)
            .map(|p| &p.pinned_extensions);
        extensions::list(profile)
            .iter()
            .map(|extension| {
                let mut json = extension_json(profile, extension);
                json["enabled"] = extension.enabled.into();
                json["pinned"] = pinned
                    .is_some_and(|pinned| pinned.contains(&extension.id))
                    .into();
                json["options"] = extension.options.is_some().into();
                json["optionsInTab"] = extension.options_in_tab.into();
                json["description"] = extension.description.clone().into();
                json["version"] = extension.version.clone().into();
                let details = &extension.details;
                json["details"] = json!({
                    "permissions": details.permissions,
                    "sites": details.sites,
                    "homepage": details.homepage,
                    "manifestVersion": details.manifest_version,
                    "fromStore": details.from_store,
                    "unpacked": details.unpacked,
                    "installedAt": details.installed_at,
                    "updatedAt": details.updated_at,
                });
                json
            })
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

/// Saves the sidebar, and what the other windows show of it, once the session is restored.
fn save_session() {
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
        let json = STATE.with_borrow(UiState::to_json);
        let event = format!(r#"new CustomEvent("arbalete:state", {{ detail: {json} }})"#);
        dispatch(&win().sidebar, &event);
        dispatch(&win().reveal_view, &event);
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
    extensions::sync(active.into_iter().collect());
}

/// Shows the bar above the page, with the navigation buttons and the address, which the sidebar
/// then leaves out; or hides it. Kept across sessions.
fn toggle_toolbar() {
    let shown = SETTINGS.with_borrow_mut(|settings| {
        settings.show_toolbar = !settings.show_toolbar;
        settings.show_toolbar
    });
    save_settings();
    sync_page_toolbar();
    #[cfg(target_os = "macos")]
    crate::mac::toolbar_shown(shown);
    send_state();
}

/// Whether the bar above the page shows: in toolbar mode, or over a site in developer mode, as
/// Arc's developer bar.
fn page_toolbar() -> bool {
    app_settings().show_toolbar || selected_url().is_some_and(|url| developer_mode(&url))
}

/// Shows or hides the bar above the page as `page_toolbar` says, but over a split, which shows
/// its pages' own bars.
fn sync_page_toolbar() {
    send_panes();
}

/// Whether a page's site is in developer mode, like Arc's: as it was turned, or else for
/// `localhost` only, not for any other local host.
fn developer_mode(url: &str) -> bool {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return false;
    }
    let site = site(url);
    SETTINGS
        .with_borrow(|settings| settings.developer_sites.get(site).copied())
        .unwrap_or_else(|| localhost(site))
}

/// Whether a site, a host and maybe a port, is the machine itself by name or address.
fn localhost(site: &str) -> bool {
    let host = match site.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(rest),
        None => site.split(':').next().unwrap_or(site),
    };
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host.ends_with(".localhost")
}

/// Turns a site's developer mode on or off, kept only where it differs from its default.
fn set_developer_mode(site: String, on: bool) {
    SETTINGS.with_borrow_mut(|settings| {
        if on == localhost(&site) {
            settings.developer_sites.remove(&site);
        } else {
            settings.developer_sites.insert(site, on);
        }
    });
    save_settings();
    send_state();
    if let Some(url) = selected_url() {
        send_site_controls(&url);
    }
}

/// Saves the downloads list. A list that cannot be written is not worth stopping a download for.
fn save_downloads() {
    let path = DATA_DIR.get().expect("set").join(downloads::FILE_NAME);
    if let Err(error) = DOWNLOADS.with_borrow(|list| downloads::save(list, &path)) {
        eprintln!("cannot save the downloads: {error}");
    }
}

/// Sends the downloads to both sidebars, which show the recent ones at their bottom and all of
/// them in the library.
fn send_downloads() {
    update_media_files();
    let json = DOWNLOADS.with_borrow(|downloads| serde_json::to_string(downloads).expect("JSON"));
    let event = format!(r#"new CustomEvent("arbalete:downloads", {{ detail: {json} }})"#);
    in_each_window(|| dispatch(&win().sidebar, &event));
    dispatch(&win().reveal_view, &event);
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
    dispatch(view, r#"new Event("arbalete:show-downloads")"#);
}

/// Opens the library on the section it showed last, showing the sidebar first if it is hidden.
fn show_library_page() {
    if win().sidebar_mode.get() == SidebarMode::Hidden {
        set_sidebar(SidebarMode::Shown);
    }
    dispatch(&win().sidebar, r#"new Event("arbalete:open-library")"#);
}

/// Puts a tab first among the tabs selected last, which the tab switcher lists.
fn remember(recent: &mut Vec<NodeId>, tab: NodeId) {
    recent.retain(|&id| id != tab);
    recent.insert(0, tab);
    recent.truncate(RECENT_TABS);
}

/// The picked tab's index after a step forward or back among `count` tabs, round the list.
fn step(index: usize, count: usize, forward: bool) -> usize {
    if forward {
        (index + 1) % count
    } else {
        (index + count - 1) % count
    }
}

/// ⌃⇥ shows the tabs selected last, still open, the most recent first, in the Command Bar's panel,
/// and picks the one before the shown tab; each ⌃⇥ more picks the next one, ⌃⇧⇥ the one before.
/// Releasing ⌃ shows the picked tab (end_tab_switch).
fn switch_tab(forward: bool) {
    let switch = win().switch.take().or_else(|| {
        let tabs: Vec<_> = STATE.with_borrow_mut(|state| {
            win()
                .recent
                .with_borrow(Clone::clone)
                .into_iter()
                .filter(|&id| state.sidebar.tab_mut(id).is_some())
                .collect()
        });
        (tabs.len() > 1).then_some((tabs, 0))
    });
    let Some((tabs, index)) = switch else { return };
    let index = step(index, tabs.len(), forward);
    let rows: Vec<_> = STATE.with_borrow(|state| {
        let profile = |id| state.sidebar.profile_of(id);
        tabs.iter()
            .filter_map(|&id| {
                let NodeKind::Tab(tab) = &state.sidebar.node(id)?.kind else {
                    return None;
                };
                let favicon = favicon_url(profile(id)?, tab);
                Some(json!({ "id": id, "url": tab.url, "title": tab.label(), "favicon": favicon }))
            })
            .collect()
    });
    let detail = json!({ "tabs": rows, "picked": index });
    dispatch(
        &win().overlay_view,
        &format!(r#"new CustomEvent("arbalete:switch", {{ detail: {detail} }})"#),
    );
    if let Some(overlay) = win().overlay.with_borrow(Clone::clone)
        && overlay.is_visible() == 0
    {
        win().over_url_bar.set(false);
        place_overlay(OVERLAY_ROW);
        overlay.set_visible(1);
        round_overlay();
    }
    win().switch.set(Some((tabs, index)));
}

/// Shows the tab picked in the switcher, if it is shown.
fn end_tab_switch() {
    if let Some((tabs, index)) = win().switch.take() {
        show_overlay(false);
        select_tab(tabs[index]);
    }
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
        #[cfg(not(target_os = "macos"))]
        DownloadAction::Open | DownloadAction::Show => {}
        DownloadAction::Remove => {
            DOWNLOADS.with_borrow_mut(|downloads| downloads.retain(|download| download.id != id));
            save_downloads();
            send_downloads();
        }
    }
}

/// Gives the native interface's surface standing for one of the window's places, like the
/// sidebar's, an event, given as the JavaScript a web page would dispatch.
fn dispatch<T>(view: &RefCell<Option<T>>, event: &str) {
    #[cfg(target_os = "macos")]
    if let Some(surface) = native_surface(std::ptr::from_ref(view).cast()) {
        crate::native::event(win().id, surface, event);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (view, event);
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

/// Shows the Command Bar above the page, empty to open a tab, or hides it.
fn show_overlay(visible: bool) {
    open_overlay(visible, None);
}

/// Shows the Command Bar holding the selected tab's address, to edit it; without one, empty.
fn edit_url() {
    let tab = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let id = sidebar.displayed().selected?;
        let NodeKind::Tab(tab) = &sidebar.node(id)?.kind else {
            return None;
        };
        Some((tab.url.clone(), favicon_url(sidebar.profile_of(id)?, tab)))
    });
    open_overlay(true, tab);
}

/// Shows the Command Bar, or hides it. Holding a tab's address and icon, it edits the address,
/// over the docked sidebar's url bar; empty, it opens a tab, centered on the window.
fn open_overlay(visible: bool, tab: Option<(String, Option<String>)>) {
    let Some(overlay) = win().overlay.with_borrow(Clone::clone) else {
        return;
    };
    if visible {
        // The sidebar shows its url bar while docked, unless the bar above the page holds it.
        let over_url_bar = tab.is_some()
            && win().sidebar_mode.get() == SidebarMode::Shown
            && !app_settings().show_toolbar;
        win().over_url_bar.set(over_url_bar);
        place_overlay(OVERLAY_ROW);
        let (url, favicon) = tab.unzip();
        let engine = STATE.with_borrow(|state| {
            let profile = state.sidebar.displayed().profile;
            let profiles = state.sidebar.profiles();
            profiles
                .iter()
                .find(|p| p.id == profile)
                .map(|p| p.search_engine)
        });
        let engine = engine.unwrap_or_default();
        let search = json!({ "name": engine.name(), "url": engine.url() });
        let empty = url.is_none();
        let detail = json!({ "url": url, "favicon": favicon.flatten(), "search": search });
        dispatch(
            &win().overlay_view,
            &format!(r#"new CustomEvent("arbalete:show", {{ detail: {detail} }})"#),
        );
        // Like Arc's, an empty bar offers the sites visited most.
        if empty {
            search_overlay("");
        }
    }
    if !visible {
        win().adding_pane.set(false);
        // The page takes the keyboard back from the Command Bar.
        if overlay.is_visible() == 1
            && let Some(tab) = win().shown.get()
        {
            focus_page(tab);
        }
    }
    // Like Arc's, the window keeps the Command Bar's room while it shows, as wide as it was.
    let window = win().window.with_borrow(Clone::clone);
    let min_width = window
        .as_ref()
        .filter(|_| visible && !win().over_url_bar.get())
        .map(|window| {
            window
                .size()
                .width
                .clamp(MIN_WINDOW_WIDTH, OVERLAY_WIDTH + 160)
        });
    if win().overlay_min_width.replace(min_width) != min_width
        && let Some(window) = window
    {
        #[cfg(target_os = "macos")]
        crate::mac::set_min_size(
            window.window_handle(),
            min_width.unwrap_or(MIN_WINDOW_WIDTH),
            MIN_WINDOW_HEIGHT,
        );
        #[cfg(not(target_os = "macos"))]
        window.invalidate_layout();
    }
    overlay.set_visible(visible.into());
    #[cfg(target_os = "macos")]
    round_overlay();
    #[cfg(not(target_os = "macos"))]
    if visible && let Some(view) = overlay.contents_view() {
        view.request_focus();
    }
}

/// Shows the Command Bar where the app placed it, again after it changes size: the native one,
/// on macOS, over the web one, which stays out of sight.
fn round_overlay() {
    #[cfg(target_os = "macos")]
    sync_native_surfaces();
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

/// Centers the Command Bar on the window, or places it over the url bar, `height` tall.
fn place_overlay(height: i32) {
    let (Some(overlay), Some(size)) = (
        win().overlay.with_borrow(Clone::clone),
        win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.size())),
    ) else {
        return;
    };
    let bounds = if win().over_url_bar.get() {
        let width = URL_EDIT_WIDTH.max(sidebar_width() - 2 * URL_BAR_LEFT);
        Rect {
            x: URL_BAR_LEFT,
            y: URL_BAR_TOP,
            width: width.min(URL_EDIT_MAX).min(size.width - URL_BAR_LEFT - 80),
            height,
        }
    } else {
        // Inset 160 points from the window's sides, or 20 in a narrow window.
        let margin = if size.width >= OVERLAY_WIDTH { 160 } else { 20 };
        let width = OVERLAY_WIDTH.min(size.width - margin);
        Rect {
            x: (size.width - width) / 2,
            y: ((size.height - OVERLAY_BOX) / 2).max(24),
            width,
            height,
        }
    };
    overlay.set_bounds(Some(&bounds));
}

/// Sends the Command Bar the open tabs, in every Space and the favorites, then the displayed
/// Space's profile history pages, best first, whose address or title holds `text`.
fn search_overlay(text: &str) {
    let wanted = text.trim().to_lowercase();
    let (tabs, profile) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let tabs: Vec<_> = if wanted.is_empty() {
            Vec::new()
        } else {
            let shown = sidebar.displayed().id;
            sidebar
                .tabs()
                // A favorite belongs to no Space: an incognito window has none.
                .filter(|(id, _)| {
                    sidebar
                        .space_of(*id)
                        .map_or(!sidebar.is_incognito(shown), |space| {
                            sidebar.reaches(shown, space)
                        })
                })
                .filter(|(_, tab)| {
                    tab.label().to_lowercase().contains(&wanted)
                        || tab.url.to_lowercase().contains(&wanted)
                })
                .take(OVERLAY_TABS)
                .filter_map(|(id, tab)| Some((id, tab.clone(), sidebar.profile_of(id)?)))
                .collect()
        };
        (tabs, sidebar.displayed().profile)
    });
    // Nothing typed, like Arc's: the sites visited most, one page each.
    let pages = with_history(profile, |history| {
        let limit = if wanted.is_empty() { 8 } else { 1 } * (OVERLAY_PAGES + OVERLAY_TABS);
        history.suggest(text.trim(), limit, now())
    })
    .and_then(Result::ok)
    .unwrap_or_default();
    let pages: Vec<_> = if wanted.is_empty() {
        let mut sites = std::collections::HashSet::new();
        pages
            .into_iter()
            .filter(|page| sites.insert(site(&page.url).trim_start_matches("www.").to_string()))
            .collect()
    } else {
        pages
    };
    // Like Arc's, each page shows its site's icon: the one Chromium found, if any.
    let chromium = favicon::chromium(&profile_dir(profile));
    let pages: Vec<_> = pages
        .iter()
        .filter(|page| tabs.iter().all(|(_, tab, _)| tab.url != page.url))
        .take(OVERLAY_PAGES)
        .map(|page| {
            let icon = chromium
                .as_ref()
                .and_then(|db| favicon::of_page(&profile_dir(profile), db, &page.url));
            let favicon = icon.map(|source| icon_url(profile, &source));
            json!({ "url": page.url, "title": page.title, "favicon": favicon })
        })
        .collect();
    let tabs: Vec<_> = tabs
        .iter()
        .map(|(id, tab, profile)| {
            let favicon = favicon_url(*profile, tab);
            json!({ "id": id, "url": tab.url, "title": tab.label(), "favicon": favicon })
        })
        .collect();
    let detail = json!({ "text": text, "tabs": tabs, "pages": pages });
    dispatch(
        &win().overlay_view,
        &format!(r#"new CustomEvent("arbalete:results", {{ detail: {detail} }})"#),
    );
    suggest(profile, text);
}

/// Asks the displayed Space's profile's search engine for searches completing `text`, unless the
/// profile asks for none: the Command Bar shows them once they come (SuggestionsClient). An
/// address typed is not sent, nor any cookie.
fn suggest(profile: ProfileId, text: &str) {
    let typed = text.trim();
    let engine = STATE.with_borrow(|state| {
        let profiles = state.sidebar.profiles();
        let profile = profiles.iter().find(|p| p.id == profile)?;
        profile.search_suggestions.then_some(profile.search_engine)
    });
    let Some(base) = engine.and_then(SearchEngine::suggest_url) else {
        return;
    };
    if typed.is_empty() || typed.contains("://") {
        return;
    }
    let Some(request) = request_create() else {
        return;
    };
    let url = format!("{base}{}", arbalete_core::search::encode(typed));
    request.set_url(Some(&CefString::from(url.as_str())));
    request.set_method(Some(&CefString::from("GET")));
    let mut client = SuggestionsClient::new(text.to_string(), std::rc::Rc::default());
    let _ = urlrequest_create(Some(&mut request.clone()), Some(&mut client), None);
}

wrap_urlrequest_client! {
    struct SuggestionsClient {
        text: String,
        body: std::rc::Rc<RefCell<Vec<u8>>>,
    }

    impl UrlrequestClient {
        fn on_download_data(&self, _request: Option<&mut Urlrequest>, data: *const u8, data_length: usize) {
            if !data.is_null() {
                // SAFETY: CEF hands the chunk received, `data_length` bytes long.
                let chunk = unsafe { std::slice::from_raw_parts(data, data_length) };
                self.body.borrow_mut().extend_from_slice(chunk);
            }
        }

        fn on_request_complete(&self, _request: Option<&mut Urlrequest>) {
            let body = String::from_utf8_lossy(&self.body.borrow()).into_owned();
            let detail = json!({ "text": self.text, "suggestions": arbalete_core::search::suggestions(&body) });
            dispatch(
                &win().overlay_view,
                &format!(r#"new CustomEvent("arbalete:suggestions", {{ detail: {detail} }})"#),
            );
        }
    }
}

/// Loads an address in the selected tab, or in a new one when none is selected.
fn navigate(url: &str) {
    show_overlay(false);
    let selected = STATE.with_borrow_mut(|state| active_tab(&mut state.sidebar).is_some());
    if content().is_some() {
        load(url);
    } else if selected {
        // Its page is not created yet: it opens there.
        update_sidebar(|sidebar| {
            if let Some(tab) = active_tab(sidebar) {
                tab.url = url.to_string();
            }
        });
    } else {
        open_tab(url);
    }
}

/// Moves the sidebar to `mode`. The views it shows on the way are shown first, the others hidden
/// once it gets there (settle). The card and the revealed sidebar slide natively, frame by frame;
/// the revealed sidebar floats in and out in the compositor, which clips it by its own window.
fn set_sidebar(mode: SidebarMode) {
    let before = win().sidebar_mode.replace(mode);
    let (Some(size), Some(card), Some(reveal)) = (
        win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.size())),
        // Where it stands now: on its way, or where the mode it leaves put it.
        win().sliding_card.borrow().clone().or_else(|| {
            win()
                .window
                .with_borrow(|window| Some(before.layout(&window.as_ref()?.size()).card))
        }),
        win().reveal.with_borrow(Clone::clone),
    ) else {
        return;
    };
    // Like Arc's, a drag of the sidebar's edge hides and shows it at once, under the pointer.
    #[cfg(target_os = "macos")]
    let duration = if crate::mac::reduce_motion() || SIDEBAR_DRAG.get().is_some() {
        0
    } else {
        slide_duration(before, mode)
    };
    #[cfg(not(target_os = "macos"))]
    let duration = 0;
    let docked = mode == SidebarMode::Shown;
    if docked
        && !cfg!(target_os = "macos")
        && let Some(sidebar) = win().sidebar.with_borrow(Clone::clone)
    {
        sidebar.set_visible(1);
    }
    if docked != (before == SidebarMode::Shown) {
        let slide = duration > 0 && matches!(before, SidebarMode::Shown | SidebarMode::Hidden);
        let detail = json!({ "docked": docked, "slide": slide });
        dispatch(
            &win().sidebar,
            &format!(r#"new CustomEvent("arbalete:docked", {{ detail: {detail} }})"#),
        );
    }
    let floating = |mode| matches!(mode, SidebarMode::Revealed | SidebarMode::Floating);
    if floating(mode) && !floating(before) {
        reveal.set_bounds(Some(&mode.layout(&size).reveal));
        reveal.set_visible((!cfg!(target_os = "macos")).into());
    }
    let from = Layout {
        card,
        reveal: reveal.bounds(),
    };
    let slide = Slide {
        from,
        start: Instant::now(),
        duration: Duration::from_millis(duration.into()),
        docking: docked && floating(before),
    };
    if win().slide.replace(Some(slide)).is_none() {
        step_slide();
    }
}

/// How long the sidebar takes from one mode to another, in milliseconds; 0 for at once.
fn slide_duration(from: SidebarMode, to: SidebarMode) -> u32 {
    match (from, to) {
        (SidebarMode::Shown, SidebarMode::Hidden) | (SidebarMode::Hidden, SidebarMode::Shown) => {
            HIDE_DURATION
        }
        (SidebarMode::Hidden, SidebarMode::Revealed) => REVEAL_DURATION,
        (SidebarMode::Revealed, SidebarMode::Hidden) => CONCEAL_DURATION,
        (SidebarMode::Revealed, SidebarMode::Shown) => DOCK_DURATION,
        (SidebarMode::Floating, SidebarMode::Shown) => FLOATING_DOCK_DURATION,
        _ => 0,
    }
}

/// The mode the sidebar toggle leads to. A Space without a tab shows an empty panel, which has
/// nothing to grow into: the sidebar floats beside it instead of hiding.
fn toggled(mode: SidebarMode, has_tab: bool) -> SidebarMode {
    match mode {
        SidebarMode::Shown if has_tab => SidebarMode::Hidden,
        SidebarMode::Shown => SidebarMode::Floating,
        _ => SidebarMode::Shown,
    }
}

/// Moves the card, and the revealed sidebar while it docks, one frame further, until the slide
/// ends.
fn step_slide() {
    let Some((from, start, duration, docking)) = win().slide.with_borrow(|slide| {
        let slide = slide.as_ref()?;
        Some((
            slide.from.clone(),
            slide.start,
            slide.duration,
            slide.docking,
        ))
    }) else {
        return;
    };
    let elapsed = start.elapsed().as_secs_f64();
    let progress = if duration.is_zero() {
        1.0
    } else {
        elapsed / duration.as_secs_f64()
    };
    let size = win()
        .window
        .with_borrow(|window| window.as_ref().map(|w| w.size()));
    let (Some(size), true) = (size, progress < 1.0) else {
        win().slide.set(None);
        win().sliding_card.replace(None);
        settle_sidebar();
        // The bar over the page, which the card hid while it moved.
        send_panes();
        return;
    };
    let eased = crate::motion::ease_out(progress);
    let to = win().sidebar_mode.get().layout(&size);
    win()
        .sliding_card
        .replace(Some(lerp_rect(&from.card, &to.card, eased)));
    place_card();
    if docking && let Some(reveal) = win().reveal.with_borrow(Clone::clone) {
        reveal.set_bounds(Some(&lerp_rect(&from.reveal, &to.reveal, eased)));
        #[cfg(target_os = "macos")]
        place_native_sidebar();
    }
    post_delayed_task(ThreadId::UI, Some(&mut StepSlide::new()), FRAME_MS);
}

wrap_task! {
    struct StepSlide {}

    impl Task {
        fn execute(&self) {
            step_slide();
        }
    }
}

/// `t` of the way from one rectangle to another.
fn lerp_rect(from: &Rect, to: &Rect, t: f64) -> Rect {
    use crate::motion::lerp;
    Rect {
        x: lerp(from.x, to.x, t),
        y: lerp(from.y, to.y, t),
        width: lerp(from.width, to.width, t),
        height: lerp(from.height, to.height, t),
    }
}

/// Shows the views of the sidebar's mode where it wants them, and hides the others.
fn settle_sidebar() {
    let mode = win().sidebar_mode.get();
    let docked = mode == SidebarMode::Shown;
    if let Some(sidebar) = win().sidebar.with_borrow(Clone::clone) {
        sidebar.set_visible((docked && !cfg!(target_os = "macos")).into());
    }
    place_card();
    place_reveal();
    update_drag_regions();
    #[cfg(target_os = "macos")]
    {
        if let Some(window) = win().window.with_borrow(Clone::clone) {
            crate::mac::sidebar_shown(window.window_handle(), docked);
        }
    }
}

/// The window moves by its margins around the card, and by the sidebar's drag areas while it is
/// docked. CEF applies the regions in order, a region that is not draggable cutting out of the
/// ones before: the sidebar's come last, so its controls stay clickable.
fn update_drag_regions() {
    // On macOS, the native window's own background moves it.
    #[cfg(not(target_os = "macos"))]
    {
        let Some(window) = win().window.with_borrow(Clone::clone) else {
            return;
        };
        let mode = win().sidebar_mode.get();
        let size = window.size();
        let regions = if fills_window() {
            Vec::new()
        } else {
            margin_regions(&size, &mode.layout(&size).card, mode.margins_start())
        };
        window.set_draggable_regions(Some(&regions));
    }
}

/// The margins around the `card` of a window of `size`, from `start`: the docked sidebar's edge,
/// or the window's. Beside the docked sidebar, the margin resizes it instead.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn margin_regions(size: &Size, card: &Rect, start: i32) -> Vec<DraggableRegion> {
    let (width, height) = (size.width, size.height);
    let (right, bottom) = (card.x + card.width, card.y + card.height);
    [
        (start, 0, width - start, card.y),
        (start, bottom, width - start, height - bottom),
        (start, 0, if start == 0 { card.x } else { 0 }, height),
        (right, 0, width - right, height),
    ]
    .into_iter()
    .filter(|&(_, _, width, _)| width > 0)
    .map(|(x, y, width, height)| DraggableRegion {
        bounds: Rect {
            x,
            y,
            width,
            height,
        },
        draggable: 1,
    })
    .collect()
}

/// Insets the card from the window's edges, and from the sidebar while it is docked or floating.
fn place_card() {
    place_split_handles();
    send_empty_card();
    send_panes();
    float_beside_empty_card();
}

/// Like Arc's, beside an empty Space's panel, the hidden sidebar floats, which has nothing to
/// give the room to; once a tab shows, it hides again.
fn float_beside_empty_card() {
    let empty = STATE.with_borrow(|state| state.sidebar.displayed().selected.is_none());
    match win().sidebar_mode.get() {
        SidebarMode::Hidden | SidebarMode::Revealed if empty => set_sidebar(SidebarMode::Floating),
        SidebarMode::Floating if !empty => set_sidebar(SidebarMode::Hidden),
        _ => {}
    }
}

/// Moves the card `offset` points right of where it rests beside the docked sidebar, which pages
/// towards the library; the sidebar gets its own width back first. The compositor moves the
/// card's stage, clipped by the card: the page keeps its size.
fn shift_card(offset: u32) {
    if win().sidebar_mode.get() != SidebarMode::Shown {
        return;
    }
    widen_for_library(false);
    #[cfg(target_os = "macos")]
    if let Some(view) = card_window() {
        crate::mac::shift(view, f64::from(offset));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = offset;
}

/// Widens the docked sidebar for the library, the card resting beside it.
fn show_library() {
    if win().sidebar_mode.get() != SidebarMode::Shown {
        return;
    }
    widen_for_library(true);
    #[cfg(target_os = "macos")]
    if let Some(view) = card_window() {
        crate::mac::shift(view, 0.0);
    }
}

/// Fills the window with the library while it shows the Spaces, or rests it beside the card.
fn widen_library(wide: bool) {
    if win().wide_library.replace(wide) != wide && win().library.get() {
        relayout_library();
    }
}

/// Gives the docked sidebar the library's width, or its own, and places the card beside it.
fn widen_for_library(library: bool) {
    if win().library.replace(library) != library {
        relayout_library();
    }
}

/// Sizes the docked sidebar and places the card after the library's width changed.
fn relayout_library() {
    if let Some(sidebar) = win().sidebar.with_borrow(Clone::clone) {
        sidebar.invalidate_layout();
    }
    #[cfg(target_os = "macos")]
    place_native_sidebar();
    #[cfg(not(target_os = "macos"))]
    if let Some(window) = win().window.with_borrow(Clone::clone) {
        window.layout();
    }
    place_card();
    update_drag_regions();
}

/// The native card holding the pages, once the card has shown (send_panes).
#[cfg(target_os = "macos")]
fn card_window() -> Option<*mut std::ffi::c_void> {
    win().card_view.get()
}

/// Places the revealed sidebar above the page, or the strip that reveals it along the window's
/// left edge.
fn place_reveal() {
    let Some(reveal) = win().reveal.with_borrow(Clone::clone) else {
        return;
    };
    let mode = win().sidebar_mode.get();
    let size = win()
        .window
        .with_borrow(|window| window.as_ref().map(|w| w.size()));
    let Some(size) = size else { return };
    reveal.set_bounds(Some(&mode.layout(&size).reveal));
    reveal.set_visible((mode != SidebarMode::Shown && !cfg!(target_os = "macos")).into());
    #[cfg(target_os = "macos")]
    place_native_sidebar();
}

/// Puts the native sidebar where the web one would be: docked at the window's left edge, or
/// floating above the page; in neither while hidden.
#[cfg(target_os = "macos")]
fn place_native_sidebar() {
    let (Some(size), Some(reveal)) = (
        win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.size())),
        win().reveal.with_borrow(Clone::clone),
    ) else {
        return;
    };
    let mode = win().sidebar_mode.get();
    let frame = match mode {
        SidebarMode::Shown => Some(Rect {
            x: 0,
            y: 0,
            width: docked_width(),
            height: size.height,
        }),
        SidebarMode::Hidden => None,
        SidebarMode::Revealed | SidebarMode::Floating => Some(reveal.bounds()),
    };
    crate::native::place(
        win().id,
        frame.as_ref(),
        mode != SidebarMode::Shown,
        mode == SidebarMode::Revealed,
    );
}

/// Loads the sidebar saved by the previous session; a file that cannot be read is replaced on the
/// next save. The first launch offers to import the browsers found, if any, before the start page.
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

/// The first launch's sidebar, which nothing has changed yet: an import replaces it.
fn is_untouched(sidebar: &Sidebar) -> bool {
    *sidebar == Sidebar::default()
}

/// Shows the import assistant above the page, with the browsers found now.
fn show_import() {
    let Some(window) = win().window.with_borrow(Clone::clone) else {
        return;
    };
    // It shows the import running.
    if IMPORTING.get() {
        return;
    }
    SOURCES.set(browsers::detect(&crate::user_home()));
    make_import();
    let Some(controller) = IMPORT.with_borrow(Clone::clone) else {
        return;
    };
    let size = window.size();
    let (width, height) = (
        IMPORT_SIZE.width.min(size.width),
        IMPORT_SIZE.height.min(size.height),
    );
    controller.set_bounds(Some(&Rect {
        x: (size.width - width) / 2,
        y: (size.height - height) / 2,
        width,
        height,
    }));
    controller.set_visible(1);
    // A page still loading asks for them once it listens (Command::ImportReady).
    send_sources();
    // At launch too, out of any command.
    #[cfg(target_os = "macos")]
    sync_native_surfaces();
    // On macOS, the native surface takes the keyboard instead.
    #[cfg(not(target_os = "macos"))]
    if let Some(view) = controller.contents_view() {
        view.request_focus();
    }
}

/// Clears what the shown page's site keeps, through Chromium's DevTools protocol, then reloads the
/// page: its cache storage and service workers, loading it again from the network, or its
/// cookies.
fn clear_site_data(browser: &Browser, data: SiteData) {
    let (Some(host), Some(frame)) = (browser.host(), browser.main_frame()) else {
        return;
    };
    let url = CefString::from(&frame.url()).to_string();
    let Some(origin) = origin(&url) else { return };
    let types = match data {
        SiteData::Cache => "cache_storage,service_workers",
        SiteData::Cookies => "cookies",
    };
    let Some(mut params) = dictionary_value_create() else {
        return;
    };
    params.set_string(
        Some(&CefString::from("origin")),
        Some(&CefString::from(origin)),
    );
    params.set_string(
        Some(&CefString::from("storageTypes")),
        Some(&CefString::from(types)),
    );
    let method = CefString::from("Storage.clearDataForOrigin");
    host.execute_dev_tools_method(0, Some(&method), Some(&mut params));
    match data {
        SiteData::Cache => browser.reload_ignore_cache(),
        SiteData::Cookies => browser.reload(),
    }
}

/// A web page's origin: its scheme, host and port.
fn origin(url: &str) -> Option<&str> {
    let (scheme, _) = url.split_once("://")?;
    Some(&url[..scheme.len() + 3 + site(url).len()])
}

/// A page's developer tools, like Chrome's and Arc's: Chromium's own DevTools front end in a
/// view of the card, under the page, which it talks to through CEF's DevTools protocol. Docked,
/// the front end fills the card and the page shows in the room it leaves, at its right, left or
/// bottom; undocked, it moves to a window of its own.
struct Inspector {
    /// Drawn in its page's container, under the page, or in a window of its own while undocked.
    frontend: Browser,
    docked: Cell<bool>,
    /// The front end said it is ready; until then, what to do once it is.
    ready: Cell<bool>,
    pending: RefCell<Vec<InspectorAction>>,
    /// Keeps the page's protocol messages coming to the front end.
    _observer: Option<Registration>,
}

/// What a page's developer tools do once their front end is ready.
enum InspectorAction {
    /// Runs a script in the front end, like showing a panel.
    Script(String),
    /// Shows the element at a point of the page in the Elements panel, like Inspect.
    InspectAt(i32, i32),
}

/// The ids of the app's own protocol messages to a page, from this one on, whose answers its
/// developer tools' front end does not see: asking for the element at a point, its scroll
/// offset for a capture, and the capture.
const APP_MESSAGES: u64 = 1_000_000_000;
const INSPECT_AT_ID: u64 = APP_MESSAGES + 1;
const LAYOUT_ID: u64 = APP_MESSAGES + 2;
const CAPTURE_ID: u64 = APP_MESSAGES + 3;

/// The front end's page, told it may dock.
const INSPECTOR_URL: &str = "devtools://devtools/bundled/devtools_app.html?can_dock=true";

/// The front end's own host, which Chromium gives it, stands in for Chrome's: what it would ask
/// Chrome comes to the app as marked console messages, and the app sends it the page's protocol
/// messages through `DevToolsAPI`.
const INSPECTOR_HOST: &str = r#"(() => {
  const tell = (kind, data) => console.debug("\u0001arbalete-inspector:" + kind + ":" + data);
  const take = (host) => {
    if (!host || host.arbalete) return host;
    host.arbalete = true;
    host.sendMessageToBackend = (message) => tell("send", message);
    host.setIsDocked = (docked, done) => { tell("dock", docked ? "1" : "0"); if (done) setTimeout(done, 0); };
    host.setInspectedPageBounds = (bounds) => tell("room", JSON.stringify(bounds));
    host.closeWindow = () => tell("close", "");
    host.bringToFront = () => {};
    const loaded = host.loadCompleted && host.loadCompleted.bind(host);
    host.loadCompleted = () => { tell("ready", ""); if (loaded) loaded(); };
    return host;
  };
  let host = take(window.InspectorFrontendHost);
  try {
    Object.defineProperty(window, "InspectorFrontendHost", {
      configurable: true, get: () => host, set: (value) => { host = take(value); },
    });
  } catch (_) {}
})();"#;

/// Opens a tab's page's developer tools, docked, or closes them.
fn toggle_inspector(tab: NodeId) {
    if INSPECTORS.with_borrow(|inspectors| inspectors.contains_key(&tab)) {
        close_inspector(tab);
    } else {
        open_inspector(tab, None, None);
    }
}

/// Shows a tab's page's developer tools on `panel`, opening them if need be, then does `then`,
/// like Chrome's developer tools buttons and Inspect.
fn show_inspector(tab: NodeId, panel: &'static str, then: Option<InspectorAction>) {
    match INSPECTORS.with_borrow(|inspectors| inspectors.get(&tab).cloned()) {
        Some(inspector) => {
            act_in_inspector(
                tab,
                &inspector,
                InspectorAction::Script(format!("DevToolsAPI.showPanel({panel:?})")),
            );
            if let Some(then) = then {
                act_in_inspector(tab, &inspector, then);
            }
        }
        None => open_inspector(tab, Some(panel), then),
    }
}

/// Opens a page's developer tools, docked, on `panel` if given, doing `then` once ready.
fn open_inspector(tab: NodeId, panel: Option<&str>, then: Option<InspectorAction>) {
    let Some(host) = PAGES.with_borrow(|pages| pages.get(&tab)?.browser.host()) else {
        return;
    };
    let url = match panel {
        Some(panel) => format!("{INSPECTOR_URL}&panel={panel}"),
        None => INSPECTOR_URL.to_string(),
    };
    #[cfg(target_os = "macos")]
    let Some(parent) = crate::native::tools_view(tab_number(tab)) else {
        return;
    };
    let info = WindowInfo {
        #[cfg(target_os = "macos")]
        parent_view: parent,
        // Any size: the container gives the front end its own.
        bounds: Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        },
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    };
    let Some(frontend) = browser_host_create_browser_sync(
        Some(&info),
        Some(&mut InspectorClient::new(tab)),
        Some(&CefString::from(url.as_str())),
        Some(&BrowserSettings::default()),
        None,
        None,
    ) else {
        return;
    };
    let observer = host.add_dev_tools_message_observer(Some(&mut InspectorObserver::new(tab)));
    INSPECTORS.with_borrow_mut(|inspectors| {
        inspectors.insert(
            tab,
            Rc::new(Inspector {
                frontend,
                docked: Cell::new(true),
                ready: Cell::new(false),
                pending: RefCell::new(then.into_iter().collect()),
                _observer: observer,
            }),
        )
    });
}

/// Does `action` in a page's developer tools, at once if their front end is ready, or once it
/// is.
fn act_in_inspector(tab: NodeId, inspector: &Inspector, action: InspectorAction) {
    if !inspector.ready.get() {
        inspector.pending.borrow_mut().push(action);
        return;
    }
    match action {
        InspectorAction::Script(script) => run_in_inspector(inspector, &script),
        InspectorAction::InspectAt(x, y) => {
            let ask = json!({
                "id": INSPECT_AT_ID,
                "method": "DOM.getNodeForLocation",
                "params": { "x": x, "y": y, "includeUserAgentShadowDOM": false },
            });
            let host = PAGES.with_borrow(|pages| pages.get(&tab)?.browser.host());
            if let Some(host) = host {
                host.send_dev_tools_message(Some(ask.to_string().as_bytes()));
            }
        }
    }
}

/// Closes a page's developer tools, the page taking the card back.
fn close_inspector(tab: NodeId) {
    let Some(inspector) = INSPECTORS.with_borrow_mut(|inspectors| inspectors.remove(&tab)) else {
        return;
    };
    // Its view leaves its container (InspectorLifeSpan's do_close).
    if let Some(host) = inspector.frontend.host() {
        host.close_browser(1);
    }
}

/// Runs a script in a page's developer tools' front end.
fn run_in_inspector(inspector: &Inspector, script: &str) {
    if let Some(frame) = inspector.frontend.main_frame() {
        frame.execute_java_script(Some(&CefString::from(script)), None, 0);
    }
}

wrap_task! {
    struct InspectorTask {
        tab: NodeId,
        kind: String,
        data: String,
    }

    impl Task {
        fn execute(&self) {
            inspector_asked(self.tab, &self.kind, &self.data);
        }
    }
}

/// Does what a page's developer tools' front end asked its host: send the page a protocol
/// message, leave it some room, dock or undock, or close.
fn inspector_asked(tab: NodeId, kind: &str, data: &str) {
    let Some(inspector) = INSPECTORS.with_borrow(|inspectors| inspectors.get(&tab).cloned()) else {
        return;
    };
    match kind {
        "send" => {
            let host = PAGES.with_borrow(|pages| pages.get(&tab)?.browser.host());
            if let Some(host) = host {
                host.send_dev_tools_message(Some(data.as_bytes()));
            }
        }
        "room" => {
            #[derive(serde::Deserialize)]
            struct Room {
                x: f64,
                y: f64,
                width: f64,
                height: f64,
            }
            #[cfg(target_os = "macos")]
            if let Ok(room) = serde_json::from_str::<Room>(data) {
                crate::native::tools_room(tab_number(tab), room.x, room.y, room.width, room.height);
            }
        }
        "dock" => {
            let docked = data == "1";
            if docked == inspector.docked.get() {
                return;
            }
            inspector.docked.set(docked);
            #[cfg(target_os = "macos")]
            crate::native::tools_docked(tab_number(tab), docked);
        }
        "ready" => {
            inspector.ready.set(true);
            let pending = std::mem::take(&mut *inspector.pending.borrow_mut());
            for action in pending {
                act_in_inspector(tab, &inspector, action);
            }
        }
        "close" => close_inspector(tab),
        _ => {}
    }
}

// The front end of a page's developer tools.
wrap_client! {
    struct InspectorClient {
        tab: NodeId,
    }

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(InspectorDisplay::new(self.tab))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(InspectorLoad::new())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(InspectorLifeSpan::new())
        }
    }
}

wrap_life_span_handler! {
    struct InspectorLifeSpan {}

    impl LifeSpanHandler {
        /// The front end's own windows, like its device mode's, which Chrome hosts apart, open
        /// nowhere: never as tabs.
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            _target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            1
        }

        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            OPEN_BROWSERS.set(OPEN_BROWSERS.get() + 1);
        }

        /// The front end closing leaves its page's container, which CEF would otherwise close the
        /// window of.
        fn do_close(&self, browser: Option<&mut Browser>) -> i32 {
            let Some(host) = browser.and_then(|browser| browser.host()) else {
                return 0;
            };
            post_task(
                ThreadId::UI,
                Some(&mut RemoveTools::new(host.window_handle() as usize)),
            );
            1
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            browser_closed();
        }
    }
}

wrap_task! {
    struct RemoveTools {
        view: usize,
    }

    impl Task {
        /// Takes a closing front end's view out of the interface, which destroys its browser.
        fn execute(&self) {
            #[cfg(target_os = "macos")]
            crate::native::tools_closed(self.view as *mut std::ffi::c_void);
        }
    }
}

wrap_display_handler! {
    struct InspectorDisplay {
        tab: NodeId,
    }

    impl DisplayHandler {
        /// What the front end asks its host comes as marked console messages.
        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            _line: i32,
        ) -> i32 {
            let message = message.map(CefString::to_string).unwrap_or_default();
            let Some(asked) = message.strip_prefix("\u{1}arbalete-inspector:") else {
                return 0;
            };
            let (kind, data) = asked.split_once(':').unwrap_or((asked, ""));
            post_task(
                ThreadId::UI,
                Some(&mut InspectorTask::new(self.tab, kind.to_string(), data.to_string())),
            );
            1
        }
    }
}

wrap_load_handler! {
    struct InspectorLoad {}

    impl LoadHandler {
        /// The host stands in before the front end's scripts run.
        fn on_load_start(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _transition_type: TransitionType) {
            if let Some(frame) = frame.filter(|frame| frame.is_main() == 1) {
                frame.execute_java_script(Some(&CefString::from(INSPECTOR_HOST)), None, 0);
            }
        }
    }
}

// Brings a page's protocol messages to its developer tools' front end.
wrap_dev_tools_message_observer! {
    struct InspectorObserver {
        tab: NodeId,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> i32 {
            let Some(message) = message.and_then(|message| std::str::from_utf8(message).ok()) else {
                return 0;
            };
            let inspector = INSPECTORS.with_borrow(|inspectors| inspectors.get(&self.tab).cloned());
            // The element at the point Inspect asked about, which the front end reveals as if
            // picked.
            if message.contains(&INSPECT_AT_ID.to_string()) {
                let answer: serde_json::Value = serde_json::from_str(message).unwrap_or_default();
                if answer["id"] == INSPECT_AT_ID {
                    let node = &answer["result"]["backendNodeId"];
                    if let (Some(inspector), false) = (inspector, node.is_null()) {
                        let event = json!({
                            "method": "Overlay.inspectNodeRequested",
                            "params": { "backendNodeId": node },
                        });
                        let event = serde_json::to_string(&event.to_string()).expect("a string");
                        run_in_inspector(&inspector, &format!("DevToolsAPI.dispatchMessage({event})"));
                    }
                    return 1;
                }
            }
            if is_app_message(message) {
                return 0;
            }
            if let Some(inspector) = inspector {
                let message = serde_json::to_string(message).expect("a string");
                run_in_inspector(&inspector, &format!("DevToolsAPI.dispatchMessage({message})"));
            }
            1
        }
    }
}

/// Whether a page's protocol message answers one of the app's own.
fn is_app_message(message: &str) -> bool {
    message.starts_with(r#"{"id":1"#)
        && serde_json::from_str::<serde_json::Value>(message)
            .ok()
            .and_then(|message| message["id"].as_u64())
            .is_some_and(|id| id >= APP_MESSAGES)
}

/// Where the shown page lies in the window, in points from its top left corner.
fn page_frame() -> Option<Rect> {
    let shown = win().shown.get()?;
    let tabs = win().split.borrow().0.clone();
    if tabs.is_empty() {
        let card = card_rect()?;
        let bar = shown_toolbar_height();
        return Some(Rect {
            y: card.y + bar,
            height: card.height - bar,
            ..card
        });
    }
    // A split's page, under its bar.
    let rect = split_rects()
        .get(tabs.iter().position(|&tab| tab == shown)?)?
        .clone();
    Some(Rect {
        y: rect.y + PANE_BAR_HEIGHT,
        height: rect.height - PANE_BAR_HEIGHT,
        ..rect
    })
}

/// Shows the veil a drag picks the part of the shown page to capture on, like Arc's.
fn show_capture_veil() {
    #[cfg(target_os = "macos")]
    crate::native::capture(win().id, page_frame());
}

/// The capture under way: the page's tab, the part of it to capture, in the page's points, and
/// what brings its protocol messages, until the capture is copied.
struct Capturing {
    tab: NodeId,
    area: Option<(i32, i32, i32, i32)>,
    /// How wide the page shows, in CSS pixels, once it said: the picture's part is cut by it.
    viewport: f64,
    _observer: Option<Registration>,
}

/// Copies a picture of the shown page, or of a part of it, like Arc's captures: for a part, how
/// wide the page shows first, then a picture of what shows, which the part is cut from. Asked for
/// the part alone, Chromium would resize the page while it shoots it, which shows.
fn capture(area: Option<(i32, i32, i32, i32)>) {
    let Some(tab) = win().shown.get() else { return };
    let Some(host) = content().and_then(|browser| browser.host()) else {
        return;
    };
    let observer = host.add_dev_tools_message_observer(Some(&mut CaptureObserver::new()));
    CAPTURING.set(Some(Capturing {
        tab,
        area,
        viewport: 0.0,
        _observer: observer,
    }));
    let ask = match area {
        Some(_) => json!({ "id": LAYOUT_ID, "method": "Page.getLayoutMetrics" }),
        None => {
            json!({ "id": CAPTURE_ID, "method": "Page.captureScreenshot", "params": { "format": "png" } })
        }
    };
    host.send_dev_tools_message(Some(ask.to_string().as_bytes()));
}

wrap_task! {
    struct CaptureAnswer {
        message: String,
    }

    impl Task {
        fn execute(&self) {
            capture_answered(&self.message);
        }
    }
}

/// Carries a capture on with the page's answer: shoots the part asked for, or copies the picture.
fn capture_answered(message: &str) {
    let answer: serde_json::Value = serde_json::from_str(message).unwrap_or_default();
    let Some((tab, area)) = CAPTURING.with_borrow(|capturing| {
        capturing
            .as_ref()
            .map(|capturing| (capturing.tab, capturing.area))
    }) else {
        return;
    };
    if answer["id"] == LAYOUT_ID {
        let Some(host) = PAGES.with_borrow(|pages| pages.get(&tab)?.browser.host()) else {
            return;
        };
        let viewport = answer["result"]["cssVisualViewport"]["clientWidth"]
            .as_f64()
            .unwrap_or(0.0);
        CAPTURING.with_borrow_mut(|capturing| {
            if let Some(capturing) = capturing {
                capturing.viewport = viewport;
            }
        });
        let ask = json!({ "id": CAPTURE_ID, "method": "Page.captureScreenshot", "params": { "format": "png" } });
        host.send_dev_tools_message(Some(ask.to_string().as_bytes()));
        return;
    }
    let viewport = CAPTURING.take().map_or(0.0, |capturing| capturing.viewport);
    let Some(picture) = answer["result"]["data"].as_str() else {
        return;
    };
    #[cfg(target_os = "macos")]
    {
        crate::native::copy_png(picture, area.filter(|_| viewport > 0.0), viewport);
        if let Some(frame) = page_frame() {
            crate::native::note(
                win().id,
                "Copied Screenshot",
                frame.x + frame.width,
                frame.y + 8,
            );
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (picture, area, viewport);
}

// Brings a page's answers to a capture under way.
wrap_dev_tools_message_observer! {
    struct CaptureObserver {}

    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> i32 {
            let Some(message) = message.and_then(|message| std::str::from_utf8(message).ok()) else {
                return 0;
            };
            if !is_app_message(message) {
                return 0;
            }
            post_task(ThreadId::UI, Some(&mut CaptureAnswer::new(message.to_string())));
            1
        }
    }
}

/// A Little Arbalete window: its page, in the profile it was opened in, and what its bar shows of
/// the page.
struct Little {
    id: u32,
    profile: ProfileId,
    window: Window,
    page: BrowserView,
    url: String,
    title: String,
}

/// Opens a link another app asked to open where URL Routing sends it, or an empty page from the
/// menu in a Little Arbalete window; links arriving before the main window exists wait for it.
pub fn open_link(url: Option<String>) {
    if win().window.with_borrow(Option::is_none) {
        PENDING_LINKS.with_borrow_mut(|links| links.extend(url));
        return;
    }
    let Some(url) = url else {
        return open_little(None);
    };
    // Another app's links never open in an incognito window: in another window, or else alone.
    if win().incognito.get().is_some() {
        let other = WINDOWS.with_borrow(|windows| {
            windows
                .iter()
                .find(|win| win.incognito.get().is_none())
                .map(|win| win.id)
        });
        match other {
            Some(other) => focus(other),
            None => return open_little(Some(url)),
        }
    }
    match STATE.with_borrow(|state| state.sidebar.url_routing().destination(&url)) {
        Destination::Little => open_little(Some(url)),
        Destination::MostRecentSpace => open_in_window(None, &url),
        Destination::Space(space) => open_in_window(Some(space), &url),
    }
}

/// Opens a link from another app in a new tab of the focused window, in `space` or the Space it
/// shows, and brings the window to the front.
fn open_in_window(space: Option<SpaceId>, url: &str) {
    if let Some(space) = space {
        show_space(space);
    }
    open_tab(url);
    if let Some(window) = win().window.with_borrow(Clone::clone) {
        window.activate();
    }
}

/// Opens a Little Arbalete window on `url`, in the displayed Space's profile, like Arc's Little
/// Arc: a page without the sidebar, which its bar moves into the Space.
fn open_little(url: Option<String>) {
    let id = NEXT_LITTLE.get();
    NEXT_LITTLE.set(id + 1);
    let profile = STATE.with_borrow(|state| state.sidebar.displayed().profile);
    window_create_top_level(Some(&mut LittleWindowDelegate::new(
        id,
        profile,
        url.unwrap_or_default(),
    )));
}

/// Runs what a Little Arbalete window's bar asks for.
fn little_action(id: u32, action: LittleAction) {
    match action {
        LittleAction::Ready => send_little(id),
        LittleAction::Navigate(url) => {
            let frame = LITTLE.with_borrow(|littles| {
                littles
                    .iter()
                    .find(|l| l.id == id)?
                    .page
                    .browser()?
                    .main_frame()
            });
            if let Some(frame) = frame {
                frame.load_url(Some(&CefString::from(url.as_str())));
            }
        }
        LittleAction::OpenInSpace => {
            let url = LITTLE
                .with_borrow(|littles| Some(littles.iter().find(|l| l.id == id)?.url.clone()));
            let Some(url) = url.filter(|url| !url.is_empty()) else {
                return;
            };
            open_tab(&url);
            if let Some(window) = little_window(id) {
                window.close();
            }
            if let Some(window) = win().window.with_borrow(Clone::clone) {
                window.activate();
            }
        }
    }
}

/// Changes what a Little Arbalete window's bar shows of its page, if `browser` is one's page.
fn little_changed(browser: &mut Browser, change: impl FnOnce(&mut Little)) {
    let id = LITTLE.with_borrow_mut(|littles| {
        let little = littles.iter_mut().find(|little| {
            little
                .page
                .browser()
                .is_some_and(|mut page| browser.is_same(Some(&mut page)) == 1)
        })?;
        change(little);
        Some(little.id)
    });
    if let Some(id) = id {
        send_little(id);
    }
}

/// Sends a Little Arbalete window's bar its page's address and title, its profile's search
/// engine, and the Space its page would move into.
fn send_little(id: u32) {
    let found = LITTLE.with_borrow(|littles| {
        let little = littles.iter().find(|l| l.id == id)?;
        let (space, engine) = STATE.with_borrow(|state| {
            let sidebar = &state.sidebar;
            let space = sidebar.displayed();
            let profile = sidebar.profiles().iter().find(|p| p.id == little.profile);
            let engine = profile.map(|p| p.search_engine).unwrap_or_default();
            (
                json!({ "title": space.title, "emoji": space.emoji }),
                engine,
            )
        });
        let search = json!({ "name": engine.name(), "url": engine.url() });
        let detail =
            json!({ "url": little.url, "title": little.title, "space": space, "search": search });
        Some(detail)
    });
    if let Some(detail) = found {
        let event = format!(r#"new CustomEvent("arbalete:little", {{ detail: {detail} }})"#);
        #[cfg(target_os = "macos")]
        crate::native::event(win().id, &format!("little/{id}"), &event);
        #[cfg(not(target_os = "macos"))]
        let _ = event;
    }
}

fn little_window(id: u32) -> Option<Window> {
    LITTLE.with_borrow(|littles| Some(littles.iter().find(|l| l.id == id)?.window.clone()))
}

/// The Little Arbalete window that has the keyboard, if any: ⌘W closes it, ⌘O moves its page.
fn focused_little() -> Option<u32> {
    LITTLE.with_borrow(|littles| {
        littles
            .iter()
            .find(|l| l.window.is_active() == 1)
            .map(|l| l.id)
    })
}

/// A request context for a profile's web data, sharing its storage with the profile's content
/// view: contexts with the same cache path share it.
fn profile_context(profile: ProfileId) -> Option<RequestContext> {
    let dir = profile_dir(profile);
    let settings = RequestContextSettings {
        cache_path: CefString::from(dir.to_str().expect("a UTF-8 path")),
        ..Default::default()
    };
    request_context_create_context(Some(&settings), None)
}

/// Shows the settings in a window of their own, like Arc's, brought to the front when it is
/// already open. Its page asks for the settings once it listens (Command::SettingsReady).
fn show_settings() {
    #[cfg(target_os = "macos")]
    {
        crate::native::settings(true);
        send_settings();
    }
    #[cfg(not(target_os = "macos"))]
    match SETTINGS_WINDOW.with_borrow(Clone::clone) {
        Some(window) => window.activate(),
        None => {
            window_create_top_level(Some(&mut SettingsWindowDelegate::new()));
        }
    }
}

/// Runs `expression` in a page, `url`, of a profile, loaded hidden above the page for it, then
/// closes the page a moment later: an extension removing itself never answers.
pub fn run_in_hidden_page(profile: ProfileId, url: &str, expression: &str) {
    // Without a window, like the bridge's.
    let info = WindowInfo {
        windowless_rendering_enabled: 1,
        ..Default::default()
    };
    let Some(browser) = browser_host_create_browser_sync(
        Some(&info),
        Some(&mut HiddenPageClient::new(expression.to_string())),
        Some(&CefString::from(url)),
        Some(&BrowserSettings::default()),
        None,
        profile_context(profile).as_mut(),
    ) else {
        return;
    };
    post_delayed_task(ThreadId::UI, Some(&mut CloseHiddenPage::new(browser)), 3000);
}

wrap_task! {
    struct CloseHiddenPage {
        browser: Browser,
    }

    impl Task {
        fn execute(&self) {
            if let Some(host) = self.browser.host() {
                host.close_browser(1);
            }
        }
    }
}

wrap_client! {
    struct HiddenPageClient {
        expression: String,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(BrowserLifeSpan::new(None))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(HiddenPageLoadHandler::new(self.expression.clone()))
        }
    }
}

wrap_load_handler! {
    struct HiddenPageLoadHandler {
        expression: String,
    }

    impl LoadHandler {
        fn on_load_end(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _http_status_code: i32) {
            if let Some(frame) = frame.filter(|frame| frame.is_main() == 1) {
                frame.execute_java_script(Some(&CefString::from(self.expression.as_str())), None, 0);
            }
        }
    }
}

/// How often the "Today" tabs left unshown are archived.
const ARCHIVE_CHECK_MS: i64 = 60_000;

/// Archives the "Today" tabs left unshown longer than their profile allows, suspends the pages
/// hidden for long and takes a picture of the shown one, then again every minute.
fn check_archive() {
    let now = now().try_into().unwrap_or_default();
    let archived = STATE.with_borrow_mut(|state| state.sidebar.archive_due(now));
    if archived > 0 {
        update_sidebar(|_| ());
        send_archive();
    }
    suspend_hidden_pages();
    snapshot_shown();
    post_delayed_task(
        ThreadId::UI,
        Some(&mut CheckArchive::new()),
        ARCHIVE_CHECK_MS,
    );
}

wrap_task! {
    struct CheckArchive {}

    impl Task {
        fn execute(&self) {
            check_archive();
        }
    }
}

/// Sends the library the archived tabs, the most recent first, with their Space's name.
fn send_archive() {
    let json = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let spaces = sidebar.spaces();
        let items: Vec<_> = sidebar
            .archive()
            .iter()
            .map(|archived| {
                let space = spaces.iter().find(|space| space.id == archived.space);
                let profile = space.map_or(sidebar.displayed().profile, |space| space.profile);
                json!({
                    "url": archived.tab.url,
                    "title": archived.tab.label(),
                    "favicon": favicon_url(profile, &archived.tab),
                    "space": space.map(|space| space.title.clone()),
                    "archivedAt": archived.archived_at,
                })
            })
            .collect();
        serde_json::Value::from(items)
    });
    let event = format!(r#"new CustomEvent("arbalete:archive", {{ detail: {json} }})"#);
    in_each_window(|| dispatch(&win().sidebar, &event));
    dispatch(&win().reveal_view, &event);
}

/// Opens an archived tab again, by its rank, in its Space, and shows it.
fn restore_archived(rank: usize) {
    let now = now().try_into().unwrap_or_default();
    let restored = update_sidebar(|sidebar| {
        let (space, tab) = sidebar.restore(rank, now)?;
        sidebar.select(space, tab)?;
        Ok::<_, Error>(space)
    });
    if let Ok(space) = restored {
        show_space(space);
    }
    send_archive();
}

/// The displayed Space's profile.
/// The extensions' bridge's browser, which outlives every page and window: their DevTools
/// session runs in it.
pub fn bridge_host() -> Option<BrowserHost> {
    BRIDGE.with_borrow(|bridge| bridge.as_ref()?.host())
}

/// Makes the extensions' bridge's browser: a blank page without a window, the only one the app
/// keeps for itself, as DevTools sessions run in a browser.
fn make_bridge() {
    let info = WindowInfo {
        windowless_rendering_enabled: 1,
        ..Default::default()
    };
    let browser = browser_host_create_browser_sync(
        Some(&info),
        Some(&mut BridgeClient::new()),
        Some(&CefString::from("about:blank")),
        Some(&BrowserSettings::default()),
        None,
        None,
    );
    BRIDGE.set(browser);
}

pub fn displayed_profile() -> ProfileId {
    STATE.with_borrow(|state| state.sidebar.displayed().profile)
}

/// The invisible window's size, which its sheet, like Chromium's folder picker, fits.
const HIDDEN_SIZE: Rect = Rect {
    x: 0,
    y: 0,
    width: 720,
    height: 460,
};

/// The permission window's size, which holds Chromium's prompt.
const PERMISSION_SIZE: Size = Size {
    width: 480,
    height: 280,
};

/// Opens a small Chrome-style window, `title`, on a page of an extension, `url`, in a profile, for
/// Chromium to ask the user there: it prompts only from a Chrome-style page
/// (extensions::ChromeTask).
/// `hidden`, the window is invisible: only a sheet it shows, like Chromium's folder picker, is.
pub fn open_chrome_task_window(profile: ProfileId, url: &str, title: &str, hidden: bool) {
    window_create_top_level(Some(&mut PermissionWindowDelegate::new(
        profile,
        url.to_string(),
        title.to_string(),
        hidden,
    )));
}

/// An extension was removed: it is unpinned, and the UI shows the extensions left.
pub fn extension_removed(profile: ProfileId, id: &str) {
    let _ = update_sidebar(|sidebar| sidebar.pin_extension(profile, id, false));
}

/// Closes the window Chromium asked in, later, out of the callback that learns the answer.
pub fn close_chrome_task_window() {
    post_task(ThreadId::UI, Some(&mut ClosePermissionWindow::new()));
}

wrap_task! {
    struct ClosePermissionWindow {}

    impl Task {
        fn execute(&self) {
            if let Some(window) = PERMISSION_WINDOW.with_borrow(Clone::clone) {
                window.close();
            }
        }
    }
}

wrap_window_delegate! {
    struct PermissionWindowDelegate {
        profile: ProfileId,
        url: String,
        title: String,
        hidden: bool,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            PERMISSION_SIZE
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let window = window.expect("window").clone();
            window.set_title(Some(&CefString::from(self.title.as_str())));
            window.set_to_fill_layout();
            let view = browser_view_create(
                Some(&mut PermissionClient::new()),
                Some(&CefString::from(self.url.as_str())),
                Some(&BrowserSettings::default()),
                None,
                profile_context(self.profile).as_mut(),
                Some(&mut ChromeViewDelegate::new()),
            )
            .expect("permission view");
            window.add_child_view(Some(&mut View::from(&view)));
            PERMISSION_WINDOW_HIDDEN.set(self.hidden);
            let main = win().window.with_borrow(|w| w.as_ref().map(MainWindow::bounds));
            match main.filter(|_| self.hidden) {
                // Its sheet shows at its top, over the main window's middle.
                Some(main) => window.set_bounds(Some(&Rect {
                    x: main.x + (main.width - HIDDEN_SIZE.width) / 2,
                    y: main.y + main.height / 6,
                    ..HIDDEN_SIZE
                })),
                None => window.center_window(Some(&PERMISSION_SIZE)),
            }
            // Made invisible before it shows, then again once shown, in case showing undid it.
            #[cfg(target_os = "macos")]
            if self.hidden {
                crate::mac::hide_window(window.window_handle());
            }
            window.show();
            #[cfg(target_os = "macos")]
            if self.hidden {
                crate::mac::hide_window(window.window_handle());
            }
            PERMISSION_WINDOW.set(Some(window));
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            PERMISSION_WINDOW.set(None);
            extensions::chrome_task_window_closed();
        }

        fn can_close(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn can_minimize(&self, _window: Option<&mut Window>) -> i32 { 0 }
        fn can_maximize(&self, _window: Option<&mut Window>) -> i32 { 0 }
        // Hidden, it has no title bar to show.
        fn is_frameless(&self, _window: Option<&mut Window>) -> i32 { self.hidden.into() }
        fn can_resize(&self, _window: Option<&mut Window>) -> i32 { 0 }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::CHROME
        }
    }
}

// The permission window's page: Chromium is asked once it has loaded.
wrap_client! {
    struct PermissionClient {}

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(BrowserLifeSpan::new(None))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(PermissionLoadHandler::new())
        }
    }
}

wrap_load_handler! {
    struct PermissionLoadHandler {}

    impl LoadHandler {
        fn on_load_end(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, _http_status_code: i32) {
            if let (Some(browser), true) = (browser, frame.is_some_and(|frame| frame.is_main() == 1)) {
                // The window exists for sure by now: hidden, it is made invisible again.
                #[cfg(target_os = "macos")]
                if let Some(host) = browser.host().filter(|_| PERMISSION_WINDOW_HIDDEN.get()) {
                    crate::mac::hide_window(host.window_handle());
                }
                extensions::chrome_task_page_loaded(browser);
            }
        }
    }
}

/// Opens the Chrome Web Store in a tab, to install extensions, like Arc.
fn show_store() {
    hide_site_controls();
    open_tab("https://chromewebstore.google.com/category/extensions");
}

/// Closes the windows Chromium opened by itself, once an extension is installed: to say it was
/// added. Later, out of the callback that learns it.
pub fn extension_installed() {
    post_task(ThreadId::UI, Some(&mut CloseStrays::new()));
}

wrap_task! {
    struct CloseStrays {}

    impl Task {
        fn execute(&self) {
            // An extension may have opened its welcome page there: it opens in a tab instead.
            for stray in STRAYS.take() {
                let url = stray.main_frame().map(|frame| CefString::from(&frame.url()).to_string());
                if let Some(url) = url.filter(|url| url.starts_with("https://")) {
                    open_tab(&url);
                }
                if let Some(host) = stray.host() {
                    host.close_browser(1);
                }
            }
        }
    }
}

/// Closes the settings window, if it is open.
fn close_settings() {
    #[cfg(target_os = "macos")]
    crate::native::settings(false);
    if let Some(window) = SETTINGS_WINDOW.with_borrow(Clone::clone) {
        window.close();
    }
}

/// The settings window has the keyboard: ⌘W closes it rather than a tab.
fn settings_focused() -> bool {
    #[cfg(target_os = "macos")]
    if crate::native::settings_focused() {
        return true;
    }
    SETTINGS_WINDOW.with_borrow(|window| window.as_ref().is_some_and(|w| w.is_active() == 1))
}

/// Sends the settings window the app's settings, the search engines and each profile's.
fn send_settings() {
    let settings = app_settings();
    let engines: Vec<_> = SearchEngine::ALL
        .iter()
        .map(|engine| json!({ "id": engine, "name": engine.name() }))
        .collect();
    let profiles: Vec<_> = STATE.with_borrow(|state| {
        let profiles = state.sidebar.profiles().iter();
        profiles
            .map(|p| json!({ "id": p.id, "name": p.name, "color": p.color, "searchEngine": p.search_engine, "searchSuggestions": p.search_suggestions, "archiveAfter": p.archive_after }))
            .collect()
    });
    let delays: Vec<_> = ArchiveAfter::ALL
        .iter()
        .map(|after| json!({ "id": after, "name": after.name() }))
        .collect();
    // URL Routing's destinations: every Space, named with its profile.
    let (spaces, routing) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let spaces: Vec<_> = sidebar
            .spaces()
            .iter()
            .map(|space| {
                let profile = sidebar.profiles().iter().find(|p| p.id == space.profile);
                json!({ "id": space.id, "title": space.title, "emoji": space.emoji, "profile": profile.map(|p| &p.name) })
            })
            .collect();
        (spaces, json!(sidebar.url_routing()))
    });
    let detail = json!({ "settings": settings, "engines": engines, "delays": delays, "profiles": profiles, "spaces": spaces, "urlRouting": routing });
    let event = format!(r#"new CustomEvent("arbalete:settings", {{ detail: {detail} }})"#);
    SETTINGS_VIEW.with(|view| dispatch(view, &event));
}

/// Turns an app setting on or off and saves it.
fn set_setting(setting: Setting, on: bool) {
    let mut settings = app_settings();
    match setting {
        Setting::WarnBeforeQuit => settings.warn_before_quit = on,
        Setting::RestoreSession => settings.restore_session = on,
        Setting::SuspendTabs => settings.suspend_tabs = on,
    }
    SETTINGS.set(settings);
    save_settings();
    send_settings();
    if !on && setting == Setting::SuspendTabs {
        wake_frozen(|_| true);
    }
}

/// Keeps a site's pages from being suspended, waking those frozen, or lets them be suspended again.
fn keep_awake(site: String, on: bool) {
    SETTINGS.with_borrow_mut(|settings| {
        settings.awake_sites.retain(|awake| *awake != site);
        if on {
            settings.awake_sites.push(site.clone());
            settings.awake_sites.sort();
        }
    });
    save_settings();
    send_settings();
    if on {
        wake_frozen(|url| self::site(url) == site);
    }
    if let Some(url) = selected_url() {
        send_site_controls(&url);
    }
}

/// The app's settings, as they stand.
fn app_settings() -> Settings {
    SETTINGS.with_borrow(Clone::clone)
}

fn save_settings() {
    let path = DATA_DIR.get().expect("set").join(settings::FILE_NAME);
    if let Err(error) = settings::save(&app_settings(), &path) {
        eprintln!("cannot save the settings: {error}");
    }
}

/// Shows the find bar, with the text last searched for selected.
fn show_find() {
    make_find();
    place_find();
    let Some(controller) = win().find.with_borrow(Clone::clone) else {
        return;
    };
    controller.set_visible(1);
    let text = win()
        .find_text
        .with_borrow(|text| serde_json::to_string(text).expect("a string"));
    dispatch(
        &win().find_view,
        &format!(r#"new CustomEvent("arbalete:show", {{ detail: {text} }})"#),
    );
    // On macOS, the native surface takes the keyboard instead.
    #[cfg(not(target_os = "macos"))]
    if let Some(view) = controller.contents_view() {
        view.request_focus();
    }
}

/// Makes the find bar's place, once.
fn make_find() {
    if win().find.with_borrow(Option::is_none) {
        let view = place_view(no_width, 1);
        let controller = Spot::default();
        controller.set_visible(0);
        win().find_view.set(Some(view));
        win().find.set(Some(controller));
    }
}

/// Keeps the find bar at the top right corner of the page it searches: Peek's, under its bar, or
/// the card's.
fn place_find() {
    let Some(controller) = win().find.with_borrow(Clone::clone) else {
        return;
    };
    let Some(size) = win()
        .window
        .with_borrow(|window| window.as_ref().map(|w| w.size()))
    else {
        return;
    };
    let card = if peek_shown() {
        let frame = peek_frame(&size);
        Rect {
            y: frame.y + PEEK_BAR,
            height: frame.height - PEEK_BAR,
            ..frame
        }
    } else {
        win().sidebar_mode.get().layout(&size).card
    };
    let width = FIND_SIZE.width.min(card.width - 2 * FIND_MARGIN);
    controller.set_bounds(Some(&Rect {
        x: card.x + card.width - width - FIND_MARGIN,
        y: card.y + FIND_MARGIN,
        width,
        height: FIND_SIZE.height,
    }));
}

/// The page find in page acts on: Peek's while it shows, or the shown one.
fn find_page() -> Option<Browser> {
    PEEK.with_borrow(|peek| peek.as_ref().map(|peek| peek.browser.clone()))
        .or_else(content)
}

/// Finds text in `browser`, find_page's; the same text again goes to the next match. `None`
/// searches the last text again, or shows the find bar when there is none.
fn find(browser: &Browser, text: Option<String>, forward: bool) {
    let text = text.unwrap_or_else(|| win().find_text.with_borrow(Clone::clone));
    if text.is_empty() {
        if let Some(host) = browser.host() {
            host.stop_finding(1);
        }
        win().find_text.with_borrow_mut(String::clear);
        return show_find_if_hidden();
    }
    let next = win().find_text.replace(text.clone()) == text;
    if let Some(host) = browser.host() {
        host.find(
            Some(&CefString::from(text.as_str())),
            forward.into(),
            0,
            next.into(),
        );
    }
}

/// From ⌘G with nothing to search again.
fn show_find_if_hidden() {
    let hidden = win()
        .find
        .with_borrow(|find| find.as_ref().is_none_or(|find| find.is_visible() == 0));
    if hidden {
        show_find();
    }
}

/// Hides the find bar and the page's highlights, keeping the active match selected and the text,
/// which ⌘G searches again; the page gets the keyboard back.
fn hide_find() {
    close_find();
    if let Some(host) = PEEK.with_borrow(|peek| peek.as_ref()?.browser.host()) {
        host.set_focus(1);
    } else if let Some(tab) = win().shown.get() {
        focus_page(tab);
    }
}

/// Hides the find bar and the highlights of the page it searches, as Peek opens or closes.
fn close_find() {
    if let Some(controller) = win().find.with_borrow(Clone::clone) {
        controller.set_visible(0);
    }
    if let Some(host) = find_page().and_then(|browser| browser.host()) {
        host.stop_finding(0);
    }
}

/// An extension's popup, in a window of its own.
struct ExtensionPopup {
    /// Its extension's id.
    id: String,
    browser: Browser,
    /// Where the site controls' button is in the main window, which the popup opens under;
    /// without one, as for an options page, it opens over the main window's middle.
    button: Option<Rect>,
}

/// The gap between the site controls' button and the panel under it.
const PANEL_GAP: i32 = 6;
/// The site controls' panel's width, like Arc's (SiteView in app/macos/Panels.swift).
const SITE_WIDTH: i32 = 236;
/// The gap between the site controls' button and the tip of their panel's beak, like Arc's.
const BEAK_GAP: i32 = 2;

/// Where a panel `size` large stands under `button`, centered on it, within a window of `window`.
fn under(button: &Rect, size: &Size, window: &Size) -> Rect {
    let right = window.width - size.width - CARD_MARGIN;
    Rect {
        x: (button.x + button.width / 2 - size.width / 2)
            .min(right)
            .max(CARD_MARGIN),
        y: button.y + button.height + PANEL_GAP,
        width: size.width,
        height: size.height,
    }
}

/// Shows the selected tab's site controls in a panel under their button, like Arc's: actions on
/// its link and what the site keeps, and the profile's extensions. A click on the button while
/// the panel shows closes it, as the panel losing the focus does.
fn show_site_controls(button: Button) {
    let shown = win()
        .site
        .with_borrow(|site| site.as_ref().is_some_and(|site| site.is_visible() == 1));
    if shown
        || win()
            .site_closed
            .get()
            .is_some_and(|at| at.elapsed() < Duration::from_millis(300))
    {
        hide_site_controls();
        return;
    }
    let (Some(button), Some(url)) = (button_in_window(button), selected_url()) else {
        return;
    };
    win().site_button.set(Some(button));
    extensions::reload(displayed_profile());
    send_site_controls(&url);
    place_site_controls();
    let Some(controller) = win().site.with_borrow(Clone::clone) else {
        return;
    };
    controller.set_visible(1);
    // On macOS, the native surface takes the keyboard instead.
    #[cfg(not(target_os = "macos"))]
    if let Some(view) = controller.contents_view() {
        view.request_focus();
    }
}

/// The theme picker's size, like Arc's.
const THEME_SIZE: Size = Size {
    width: 365,
    height: 520,
};

/// Shows a Space's theme picker beside the sidebar, like Arc's, centered on the pointer, with the
/// Space's theme to edit.
fn show_theme_picker(id: SpaceId) {
    let theme = STATE.with_borrow(|state| {
        let space = state.sidebar.spaces().iter().find(|space| space.id == id)?;
        Some(space.theme.clone())
    });
    let (Some(theme), Some(controller)) = (theme, win().theme.with_borrow(Clone::clone)) else {
        return;
    };
    let detail = json!({ "space": id, "theme": theme });
    dispatch(
        &win().theme_view,
        &format!(r#"new CustomEvent("arbalete:theme", {{ detail: {detail} }})"#),
    );
    #[cfg(target_os = "macos")]
    win().theme_at.set(crate::mac::pointer_y());
    controller.set_visible(1);
    place_theme_picker();
    // On macOS, the native surface takes the keyboard instead.
    #[cfg(not(target_os = "macos"))]
    if let Some(view) = controller.contents_view() {
        view.request_focus();
    }
}

/// Puts the theme picker, if it shows, just right of the sidebar, centered on where the pointer
/// was, or else above the window's bottom edge; within the window.
fn place_theme_picker() {
    let (Some(controller), Some(size)) = (
        win().theme.with_borrow(Clone::clone),
        win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.size())),
    ) else {
        return;
    };
    if controller.is_visible() == 0 {
        return;
    }
    let card = win().sidebar_mode.get().layout(&size).card;
    let height = THEME_SIZE.height.min(size.height - 2 * CARD_MARGIN);
    let lowest = size.height - height - 2 * CARD_MARGIN;
    let y = win()
        .theme_at
        .get()
        .map_or(lowest, |at| (at - height / 2).clamp(CARD_MARGIN, lowest));
    controller.set_bounds(Some(&Rect {
        x: card.x + 6,
        y,
        width: THEME_SIZE.width,
        height,
    }));
}

/// Where a button of the interface is, in the window: the native interface gives it so.
fn button_in_window(button: Button) -> Option<Rect> {
    Some(Rect {
        x: button.x,
        y: button.y,
        width: button.width,
        height: button.height,
    })
}

/// What the UI shows of an extension: its name, its icon and whether its button opens a popup.
fn extension_json(profile: ProfileId, extension: &extensions::Installed) -> serde_json::Value {
    let icon = extensions::icon_path(profile, extension).map(|path| format!("{UI_ORIGIN}{path}"));
    json!({ "id": extension.id, "name": extension.name, "icon": icon, "popup": extension.popup.is_some() })
}

/// Sends the UI the pinned extensions again, after an extension was installed or removed.
pub fn extensions_changed() {
    send_state();
}

/// The ids of a profile's pinned extensions.
fn pinned_extensions(profile: ProfileId) -> Vec<String> {
    STATE.with_borrow(|state| {
        let profiles = state.sidebar.profiles();
        let profile = profiles.iter().find(|p| p.id == profile);
        profile
            .map(|p| p.pinned_extensions.clone())
            .unwrap_or_default()
    })
}

/// Sends the site controls' panel the selected tab's address, `url`, and the displayed Space's
/// profile's extensions.
fn send_site_controls(url: &str) {
    let profile = displayed_profile();
    let pinned = pinned_extensions(profile);
    let extensions: Vec<_> = extensions::list(profile)
        .iter()
        .map(|extension| {
            let mut json = extension_json(profile, extension);
            json["enabled"] = extension.enabled.into();
            json["pinned"] = pinned.contains(&extension.id).into();
            json
        })
        .collect();
    let settings = app_settings();
    let awake = settings.awake_sites.iter().any(|awake| awake == site(url));
    let detail = json!({ "url": url, "site": site(url), "awake": awake, "developer": developer_mode(url), "suspends": settings.suspend_tabs, "extensions": extensions });
    dispatch(
        &win().site_view,
        &format!(r#"new CustomEvent("arbalete:show", {{ detail: {detail} }})"#),
    );
}

/// The selected tab's address.
fn selected_url() -> Option<String> {
    STATE.with_borrow_mut(|state| active_tab(&mut state.sidebar).map(|tab| tab.url.clone()))
}

/// Puts the site controls' panel under their button, as tall as its page asks, its beak pointing
/// at the button.
fn place_site_controls() {
    let (Some(controller), Some(button), Some(window)) = (
        win().site.with_borrow(Clone::clone),
        win().site_button.with_borrow(Clone::clone),
        win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.size())),
    ) else {
        return;
    };
    let size = Size {
        width: SITE_WIDTH,
        height: win().site_height.get(),
    };
    let mut bounds = under(&button, &size, &window);
    bounds.y += BEAK_GAP - PANEL_GAP;
    let beak = button.x + button.width / 2 - bounds.x;
    dispatch(
        &win().site_view,
        &format!(r#"new CustomEvent("arbalete:beak", {{ detail: {beak} }})"#),
    );
    controller.set_bounds(Some(&bounds));
}

/// Shows the shown page's certificate and the ones that issued it, like Arc's, in place of the
/// site controls: macOS's certificate sheet on the window.
fn show_certificate() {
    hide_site_controls();
    let certificate = content()
        .and_then(|browser| browser.host())
        .and_then(|host| host.visible_navigation_entry())
        .and_then(|entry| entry.sslstatus())
        .and_then(|status| status.x509_certificate());
    let Some(certificate) = certificate else {
        return;
    };
    let mut issuers: Vec<Option<BinaryValue>> =
        (0..certificate.issuer_chain_size()).map(|_| None).collect();
    certificate.derencoded_issuer_chain(Some(&mut issuers));
    let chain: Vec<Vec<u8>> = std::iter::once(certificate.derencoded())
        .chain(issuers)
        .flatten()
        .map(|der| {
            let mut bytes = vec![0; der.size()];
            der.data(Some(&mut bytes), 0);
            bytes
        })
        .collect();
    #[cfg(target_os = "macos")]
    crate::native::certificate(win().id, &chain);
    #[cfg(not(target_os = "macos"))]
    let _ = chain;
}

/// Hides the site controls' panel, if it shows.
fn hide_site_controls() {
    if let Some(controller) = win().site.with_borrow(Clone::clone)
        && controller.is_visible() == 1
    {
        controller.set_visible(0);
        win().site_closed.set(Some(Instant::now()));
    }
}

/// Opens an extension's popup in a small window of its own under the site controls' button, like
/// Arc's, with a title bar to move or close it, in the displayed Space's profile, instead of the
/// site controls; or closes it, when it is the one open. The window takes the size its page asks
/// for. Unlike Chrome's popups, it stays open while the user is elsewhere, like in another app
/// for a sign-in code: its close button, Escape, the extension's button, or the extension close it.
fn open_extension_popup(id: &str) {
    hide_site_controls();
    let open = win()
        .extension_popup
        .with_borrow(|popup| popup.as_ref().map(|popup| popup.id.clone()));
    close_extension_popup();
    if open.as_deref() == Some(id) {
        return;
    }
    let profile = STATE.with_borrow(|state| state.sidebar.displayed().profile);
    let (Some(url), Some(button)) = (
        extensions::popup_url(profile, id),
        win().site_button.with_borrow(Clone::clone),
    ) else {
        return;
    };
    open_extension_window(profile, id.to_string(), url, Some(button));
}

/// Opens an extension's page in a small window of its own, like its popup's (PopupClient): under
/// `button`, or without one over the main window's middle, once its page has a size
/// (show_extension_popup).
fn open_extension_window(profile: ProfileId, id: String, url: String, button: Option<Rect>) {
    #[cfg(target_os = "macos")]
    let info = WindowInfo {
        parent_view: match crate::native::extension_popup_view(win().id) {
            Some(view) => view,
            None => return,
        },
        // The page's own size, once it has one.
        bounds: Rect {
            x: 0,
            y: 0,
            width: 25,
            height: 25,
        },
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    };
    #[cfg(not(target_os = "macos"))]
    let info = WindowInfo {
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    };
    let Some(browser) = browser_host_create_browser_sync(
        Some(&info),
        Some(&mut PopupClient::new(profile, url)),
        Some(&CefString::from("about:blank")),
        Some(&BrowserSettings::default()),
        None,
        profile_context(profile).as_mut(),
    ) else {
        return;
    };
    win().extension_popup.set(Some(ExtensionPopup {
        id,
        browser,
        button,
    }));
}

/// Opens an extension's options page: in a tab when it asks for one, like Chrome; otherwise, as
/// Chrome shows it in a dialog, in a window like its popup's, over the main window's middle.
fn open_extension_options(id: &str) {
    let profile = displayed_profile();
    let extension = extensions::list(profile).into_iter().find(|e| e.id == id);
    let (Some(extension), Some(url)) = (extension, extensions::options_url(profile, id)) else {
        return;
    };
    if extension.options_in_tab {
        open_tab(&url);
        return;
    }
    let key = format!("{id}/options");
    let open = win()
        .extension_popup
        .with_borrow(|popup| popup.as_ref().map(|popup| popup.id.clone()));
    close_extension_popup();
    if open != Some(key.clone()) {
        open_extension_window(profile, key, url, None);
    }
}

/// Shows the extension popup's window once its page has a size, under the button, with the focus.
fn show_extension_popup(size: &Size) {
    let Some((browser, button)) = win().extension_popup.with_borrow(|popup| {
        let popup = popup.as_ref()?;
        Some((popup.browser.clone(), popup.button.clone()))
    }) else {
        return;
    };
    #[cfg(target_os = "macos")]
    crate::native::extension_popup_show(win().id, button.as_ref(), size);
    #[cfg(not(target_os = "macos"))]
    let _ = button;
    if let Some(host) = browser.host() {
        host.set_focus(1);
    }
}

/// Closes the extension popup, if one is open: its window goes as its page closes
/// (PopupLifeSpan's do_close).
pub fn close_extension_popup() {
    let Some(popup) = win().extension_popup.take() else {
        return;
    };
    extensions::popup_closed();
    if let Some(host) = popup.browser.host() {
        host.close_browser(1);
    }
}

/// Sends the import assistant the browsers to list, and the profiles to merge them into: none
/// on the first launch, whose sidebar the import replaces.
fn send_sources() {
    let profiles = STATE.with_borrow(|state| {
        if is_untouched(&state.sidebar) {
            return Vec::new();
        }
        let profiles = state.sidebar.profiles().iter();
        profiles
            .map(|p| json!({ "id": p.id, "name": p.name }))
            .collect()
    });
    let browsers = SOURCES.with_borrow(|browsers| {
        let browsers = browsers.iter().map(|browser| {
            let profiles = browser.profiles.iter().map(|profile| {
                let denied = browser.family == Family::Safari
                    && safari::needs_full_disk_access(&profile.path);
                json!({ "name": profile.name, "needsFullDiskAccess": denied })
            });
            json!({
                "name": browser.name,
                "arc": browser.family == Family::Arc,
                "profiles": profiles.collect::<Vec<_>>(),
            })
        });
        browsers.collect::<Vec<_>>()
    });
    let detail = json!({ "browsers": browsers, "profiles": profiles });
    let event = format!(r#"new CustomEvent("arbalete:sources", {{ detail: {detail} }})"#);
    IMPORT_VIEW.with(|view| dispatch(view, &event));
}

/// Makes the import assistant's place, once.
fn make_import() {
    if IMPORT.with_borrow(Option::is_none) {
        let view = place_view(no_width, 1);
        let controller = Spot::default();
        controller.set_visible(0);
        IMPORT_VIEW.set(Some(view));
        IMPORT.set(Some(controller));
    }
}

fn hide_import() {
    if let Some(controller) = IMPORT.with_borrow(Clone::clone) {
        controller.set_visible(0);
    }
    // Once the import is done too, out of any command.
    #[cfg(target_os = "macos")]
    sync_native_surfaces();
}

/// Closes the import assistant. Closed on the first launch, it leaves the session as it would
/// have been without it: with the start page. It stays while an import runs.
fn close_import() {
    if IMPORTING.get() {
        return;
    }
    hide_import();
    if STATE.with_borrow(|state| is_untouched(&state.sidebar)) {
        open_tab(START_URL);
    }
}

/// Imports browser profiles. Their data is read on a thread of its own, as a large history takes
/// long to read: the window keeps answering meanwhile, and the assistant shows the import
/// running. A second import while one runs is ignored.
fn run_import(choices: Vec<Choice>) {
    if IMPORTING.replace(true) {
        return;
    }
    let browsers = SOURCES.take();
    thread::spawn(move || {
        let sources = import::read(&browsers, &choices);
        let sources = Arc::new(Mutex::new(Some(sources)));
        post_task(ThreadId::UI, Some(&mut ApplyImport::new(sources)));
    });
}

wrap_task! {
    struct ApplyImport {
        sources: Arc<Mutex<Option<Vec<import::Source>>>>,
    }

    impl Task {
        fn execute(&self) {
            let sources = self.sources.lock().ok().and_then(|mut sources| sources.take());
            apply_import(sources.unwrap_or_default());
        }
    }
}

/// Adds the browser profiles read to the sidebar as it is now, then shows the first Space
/// imported with its first tab, or a blank page when it has none. Their histories are merged on
/// a thread of their own.
fn apply_import(sources: Vec<import::Source>) {
    IMPORTING.set(false);
    let (imported, shown) = update_sidebar(|sidebar| {
        let mut new = if is_untouched(sidebar) {
            Sidebar::empty()
        } else {
            sidebar.clone()
        };
        let before = new.spaces().len();
        let imported = import::apply(&mut new, sources);
        // Before the sidebar shows them; a few kilobytes each.
        for (profile, favicons) in &imported.favicons {
            for (source, png) in favicons {
                if let Err(error) = favicon::write(&profile_dir(*profile), source, png) {
                    eprintln!("cannot cache the favicon: {error}");
                }
            }
        }
        let shown = new.spaces().get(before).map(|space| space.id);
        if let Some(space) = shown {
            new.display(space).expect("an imported Space");
            if let Some(tab) = new.next_tab(space, true) {
                new.select(space, tab).expect("a tab of the Space");
            }
        }
        // Nothing was imported into an empty sidebar: it keeps the default one.
        if !new.spaces().is_empty() {
            *sidebar = new;
        }
        (imported, shown)
    });
    for error in &imported.errors {
        eprintln!("cannot import {error}");
    }
    let histories = imported.histories.into_iter();
    let histories: Vec<_> = histories
        .map(|(profile, entries)| (profile_dir(profile), entries))
        .collect();
    thread::spawn(move || {
        for (dir, entries) in histories {
            if let Err(error) = merge_history(&dir, &entries) {
                eprintln!("cannot merge the history: {error}");
            }
        }
    });
    close_import();
    if shown.is_some() {
        show_selected();
    }
}

/// Merges entries into the history of the profile in `dir`, with a connection of its own: the
/// browsing's may write to it meanwhile.
fn merge_history(dir: &Path, entries: &[history::Entry]) -> Result<(), String> {
    // An imported profile has no directory before its pages first load.
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let history = History::open(&dir.join(history::FILE_NAME));
    let mut history = history.map_err(|error| error.to_string())?;
    history.merge(entries).map_err(|error| error.to_string())
}

fn open_full_disk_access() {
    if cfg!(target_os = "macos")
        && let Err(error) = std::process::Command::new("open")
            .arg(FULL_DISK_ACCESS)
            .status()
    {
        eprintln!("cannot open System Settings: {error}");
    }
}

fn active_tab(sidebar: &mut Sidebar) -> Option<&mut Tab> {
    let id = sidebar.displayed().selected?;
    sidebar.tab_mut(id)
}

/// Opens a tab in the displayed Space and shows it.
fn open_tab(url: &str) {
    update_sidebar(|sidebar| {
        let space = sidebar.displayed().id;
        let tab = sidebar.open_tab(space, url).expect("the displayed Space");
        sidebar.select(space, tab).expect("a new tab");
    });
    show_selected();
}

/// Opens a tab beside the selected one, in its split, like Arc's Add Split View; or alone when
/// they cannot share one, like a favorite's or a split already full.
fn open_in_pane(url: &str) {
    update_sidebar(|sidebar| {
        let space = sidebar.displayed().id;
        let beside = sidebar.displayed().selected;
        let tab = sidebar.open_tab(space, url).expect("the displayed Space");
        if let Some(beside) = beside {
            let _ = sidebar.split(tab, beside);
        }
        sidebar.select(space, tab).expect("a new tab");
    });
    show_selected();
}

/// Whether the window shows a split, and then whether one page above the other.
pub fn split_shown() -> Option<bool> {
    win().split.borrow().1.as_ref().map(|layout| layout.stacked)
}

/// Lays the pages of `split`, or of the split shown, out one above the other, or side by side
/// again.
fn flip_panes(split: Option<NodeId>) {
    let shown = win().split.borrow().0.first().copied();
    let flipped = STATE.with_borrow_mut(|state| {
        let sidebar = &mut state.sidebar;
        let split = split
            .or_else(|| Some(sidebar.row_of(shown?)))
            .ok_or(Error::UnknownNode)?;
        let stacked = sidebar.split_layout(split).stacked;
        sidebar.set_split_stacked(split, !stacked)
    });
    if flipped.is_ok() {
        save_session();
        show_selected();
    }
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
        // An incognito window's tabs cannot be reopened.
        if !INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile)) {
            state.closed.push(closed);
        }
        state.sidebar.next_after_close(tab)
    });
    update_sidebar(|sidebar| sidebar.remove(tab)).expect("an open tab");
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
    #[cfg(not(target_os = "macos"))]
    let _ = entries;
}

/// Shows the developer bar's extensions menu at the pointer, like Arc's: the displayed Space's
/// profile's extensions turned on, then adding and managing them.
fn show_extensions_menu() {
    let extensions: Vec<_> = extensions::list(displayed_profile())
        .into_iter()
        .filter(|extension| extension.enabled)
        .map(|extension| (extension.id, extension.name))
        .collect();
    let entries = menu::extensions_menu(&extensions);
    #[cfg(target_os = "macos")]
    crate::mac::pop_up(entries);
    #[cfg(not(target_os = "macos"))]
    let _ = entries;
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
    #[cfg(not(target_os = "macos"))]
    let _ = entries;
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
        #[cfg(not(target_os = "macos"))]
        Action::CopyLink(_)
        | Action::Share(_)
        | Action::Confirm { .. }
        | Action::EditPinnedUrl { .. } => {}
    }
}

/// Dispatches an event in the sidebars, which takes the keyboard focus, even from the page.
fn ask(event: &str, detail: &serde_json::Value) {
    focus_sidebar();
    let event = format!(r#"new CustomEvent("{event}", {{ detail: {detail} }})"#);
    dispatch(&win().sidebar, &event);
    dispatch(&win().reveal_view, &event);
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

/// Creates a profile and asks the sidebar to rename it, like a new folder.
fn new_profile() {
    let profile = update_sidebar(|sidebar| {
        sidebar
            .create_profile("New Profile")
            .expect("a name that is not blank")
    });
    ask("arbalete:rename-profile", &json!(profile));
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

/// Attaches a Space to a new profile and asks the sidebar to rename it.
fn new_space_profile(space: SpaceId) {
    let profile = update_sidebar(|sidebar| {
        let profile = sidebar.add_profile("New Profile");
        sidebar.set_space_profile(space, profile).map(|()| profile)
    });
    if let Ok(profile) = profile {
        if STATE.with_borrow(|state| state.sidebar.displayed().id == space) {
            show_selected();
        }
        ask("arbalete:rename-profile", &json!(profile));
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

/// How hard the system presses for memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryPressure {
    Normal,
    Warning,
    Critical,
}

/// How long a page stays hidden before it is frozen, then unloaded, in milliseconds: its tab opens
/// it again at its url when shown. Sooner when the system runs low on memory, every hidden page
/// unloading at once when it is critical; and frozen sooner on battery saving, which stops their
/// work.
fn suspend_delays(pressure: MemoryPressure, low_power: bool) -> (i64, i64) {
    const MINUTE: i64 = 60_000;
    match pressure {
        MemoryPressure::Critical => (0, 0),
        MemoryPressure::Warning => (MINUTE, 5 * MINUTE),
        MemoryPressure::Normal if low_power => (MINUTE, 60 * MINUTE),
        MemoryPressure::Normal => (5 * MINUTE, 60 * MINUTE),
    }
}

/// The suspension delays as the system stands now.
fn current_suspend_delays() -> (i64, i64) {
    #[cfg(target_os = "macos")]
    return suspend_delays(crate::mac::memory_pressure(), crate::mac::low_power());
    #[cfg(not(target_os = "macos"))]
    suspend_delays(MemoryPressure::Normal, false)
}

/// Asks each page hidden long enough, but not frozen yet or due to unload, whether it plays media
/// (page_playing): a page playing goes on. A frozen page, which plays nothing and answers nothing,
/// unloads once due. A page capturing, or of a site kept awake, is never suspended, nor is any
/// when the settings say so.
fn suspend_hidden_pages() {
    let settings = app_settings();
    if !settings.suspend_tabs {
        return;
    }
    let now = now();
    let (freeze_after, unload_after) = current_suspend_delays();
    let (ask, unload): (Vec<_>, Vec<_>) = PAGES.with_borrow(|pages| {
        pages
            .iter()
            .filter(|(tab, live)| {
                !live.capturing
                    && tab_url(**tab)
                        .is_none_or(|url| !settings.awake_sites.iter().any(|s| s == site(&url)))
            })
            .filter_map(|(tab, live)| {
                let hidden = now - live.hidden_since?;
                let due = hidden >= unload_after;
                (due || (hidden >= freeze_after && !live.frozen))
                    .then(|| (*tab, live.browser.clone(), live.frozen && due))
            })
            .partition(|(_, _, unload)| !unload)
    });
    for (tab, _, _) in unload {
        close_page(tab);
        fall_asleep(tab);
    }
    // A player tells the system what it plays through the Media Session API, even without an
    // element in the page, like a Web Audio player.
    let expression = r#"navigator.mediaSession?.playbackState === "playing"
        || [...document.querySelectorAll("video, audio")].some((media) => !media.paused && !media.ended)"#;
    let params = serde_json::json!({ "expression": expression, "returnByValue": true });
    let message = serde_json::json!({ "id": extensions::PLAYING, "method": "Runtime.evaluate", "params": params });
    for (_, browser, _) in ask {
        if let Some(host) = browser.host() {
            host.send_dev_tools_message(Some(message.to_string().as_bytes()));
        }
    }
}

/// Asks the shown page for a picture of itself (page_snapshot), while it draws: a hidden page
/// draws nothing, and would never answer.
fn snapshot_shown() {
    let params = serde_json::json!({ "format": "jpeg", "quality": 70 });
    let message = serde_json::json!({ "id": extensions::SNAPSHOT, "method": "Page.captureScreenshot", "params": params });
    if let Some(host) = content().and_then(|browser| browser.host()) {
        host.send_dev_tools_message(Some(message.to_string().as_bytes()));
    }
}

/// Covers a page opening again after it was unloaded with its picture as soon as it is made, or
/// takes the picture away once it has loaded.
#[cfg(target_os = "macos")]
fn cover_revived(browser: Option<&Browser>, loading: bool) {
    let Some(browser) = browser else { return };
    // A page being made, which its view does not hold yet, is MAKING's.
    let tab = page_tab(browser).or(MAKING.get());
    let (Some(tab), Some(host)) = (tab, browser.host()) else {
        return;
    };
    if !loading {
        crate::mac::uncover_page(host.window_handle());
    } else if REVIVED.with_borrow_mut(|revived| revived.remove(&tab))
        && let Some(jpeg) = SNAPSHOTS.with_borrow(|snapshots| snapshots.get(&tab).cloned())
    {
        crate::mac::cover_page(host.window_handle(), &jpeg, None);
    }
}

/// Asks a page for the color at its top, like Arc's bar samples it: the background of what
/// shows there, or of the page, unless it is transparent.
fn ask_top_color(browser: Option<&Browser>) {
    let expression = r#"(() => {
        const fill = (node) => {
            const color = node && getComputedStyle(node).backgroundColor;
            return color && !/^rgba\(.*, 0\)$|^transparent$/.test(color) ? color : null;
        };
        for (let node = document.elementFromPoint(innerWidth / 2, 1); node; node = node.parentElement) {
            const color = fill(node);
            if (color) return color;
        }
        return fill(document.body) ?? fill(document.documentElement);
    })()"#;
    let params = serde_json::json!({ "expression": expression, "returnByValue": true });
    let message = serde_json::json!({ "id": extensions::TOP_COLOR, "method": "Runtime.evaluate", "params": params });
    if let Some(host) = browser.and_then(Browser::host) {
        host.send_dev_tools_message(Some(message.to_string().as_bytes()));
    }
}

/// Keeps the color at the top of a page, a CSS rgb() or rgba() color, for its tab's bar.
pub fn page_top_color(page: i32, color: &str) {
    let tab = PAGES.with_borrow(|pages| {
        pages
            .iter()
            .find_map(|(tab, live)| (live.browser.identifier() == page).then_some(*tab))
    });
    let Some(hex) = css_hex(color) else { return };
    let Some(tab) = tab else {
        // Peek's page tints its bar.
        let changed = PEEK.with_borrow_mut(|peek| {
            let peek = peek
                .as_mut()
                .filter(|peek| peek.browser.identifier() == page)?;
            (peek.color.as_ref() != Some(&hex)).then(|| peek.color = Some(hex))
        });
        if changed.is_some() {
            send_peek();
        }
        return;
    };
    if TOP_COLORS.with_borrow_mut(|colors| colors.insert(tab, hex.clone())) != Some(hex) {
        pane_changed(tab);
    }
}

/// A CSS rgb() or rgba() color as #rrggbb.
fn css_hex(color: &str) -> Option<String> {
    let inside = color.split_once('(')?.1.strip_suffix(')')?;
    let channels: Vec<u8> = inside
        .split(',')
        .take(3)
        .map(|channel| channel.trim().parse::<f64>().ok().map(|c| c.round() as u8))
        .collect::<Option<_>>()?;
    let [r, g, b] = channels[..] else { return None };
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Keeps a page's picture, a JPEG in base64, for its tab.
pub fn page_snapshot(page: i32, jpeg: &str) {
    let tab = PAGES.with_borrow(|pages| {
        pages
            .iter()
            .find_map(|(tab, live)| (live.browser.identifier() == page).then_some(*tab))
    });
    if let Some(tab) = tab {
        SNAPSHOTS.with_borrow_mut(|snapshots| snapshots.insert(tab, jpeg.to_owned()));
    }
}

/// A tab's url, if it is still a tab.
fn tab_url(tab: NodeId) -> Option<String> {
    STATE.with_borrow(|state| match &state.sidebar.node(tab)?.kind {
        NodeKind::Tab(tab) => Some(tab.url.clone()),
        _ => None,
    })
}

/// Dims a tab whose page was frozen or unloaded in the sidebar.
fn fall_asleep(tab: NodeId) {
    if ASLEEP.with_borrow_mut(|asleep| asleep.insert(tab)) {
        send_state();
    }
}

/// Makes the frozen pages whose url `wake` picks run again.
fn wake_frozen(wake: impl Fn(&str) -> bool) {
    let woken: Vec<_> = PAGES.with_borrow_mut(|pages| {
        pages
            .iter_mut()
            .filter(|(tab, live)| live.frozen && tab_url(**tab).is_some_and(|url| wake(&url)))
            .map(|(tab, live)| {
                live.frozen = false;
                (*tab, live.browser.host())
            })
            .collect()
    });
    for (tab, host) in woken {
        set_frozen(host.as_ref(), false);
        ASLEEP.with_borrow_mut(|asleep| asleep.remove(&tab));
    }
    send_state();
}

/// Freezes or unloads a hidden page that plays no media, as long as it has been hidden; the page
/// shown again meanwhile stays.
pub fn page_playing(page: i32, playing: bool) {
    let found = PAGES.with_borrow(|pages| {
        pages.iter().find_map(|(tab, live)| {
            let browser = live.browser.clone();
            (browser.identifier() == page).then(|| (*tab, live.hidden_since, browser.host()))
        })
    });
    let Some((tab, Some(hidden_since), host)) = found.filter(|_| !playing) else {
        return;
    };
    if now() - hidden_since >= current_suspend_delays().1 {
        close_page(tab);
    } else {
        set_frozen(host.as_ref(), true);
        PAGES.with_borrow_mut(|pages| {
            if let Some(live) = pages.get_mut(&tab) {
                live.frozen = true;
            }
        });
    }
    fall_asleep(tab);
}

/// Freezes a page through Chromium's DevTools, or makes it run again.
fn set_frozen(host: Option<&BrowserHost>, frozen: bool) {
    let state = if frozen { "frozen" } else { "active" };
    let message = serde_json::json!({ "id": 0, "method": "Page.setWebLifecycleState", "params": { "state": state } });
    if let Some(host) = host {
        host.send_dev_tools_message(Some(message.to_string().as_bytes()));
    }
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
    #[cfg(not(target_os = "macos"))]
    let info = {
        let _ = tab;
        WindowInfo {
            runtime_style: RuntimeStyle::ALLOY,
            ..Default::default()
        }
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

/// Shows a split's `tabs` as `layout` lays them out, `focused`'s with the keyboard, like Arc's
/// Split View: each its own card, with room between them.
fn show_split(focused: NodeId, tabs: &[NodeId], layout: SplitLayout) {
    let same = *win().split.borrow() == (tabs.to_vec(), Some(layout.clone()));
    if !same {
        hide_content();
        win().pane_preview.set(None);
        for &tab in tabs {
            take_page(tab);
            wake_page(tab);
        }
        *win().split.borrow_mut() = (tabs.to_vec(), Some(layout));
        place_card();
    }
    win().shown.set(Some(focused));
    shown_on_card(focused);
}

/// Where the card is in the window: the page's room, or half of it while a tab dragged over a
/// page alone would take the other half, like Arc's; none while the library fills the window.
fn card_rect() -> Option<Rect> {
    if fills_window() {
        return None;
    }
    if let Some(sliding) = win().sliding_card.borrow().clone() {
        return Some(sliding);
    }
    let size = win()
        .window
        .with_borrow(|window| window.as_ref().map(|w| w.size()))?;
    let mut room = win().sidebar_mode.get().layout(&size).card;
    if split_shown().is_none()
        && let Some((before, stacked)) = win().drop_preview.get()
    {
        if stacked {
            let half = (room.height - SPLIT_GAP) / 2;
            if before {
                room.y += half + SPLIT_GAP;
            }
            room.height = half;
        } else {
            let half = (room.width - SPLIT_GAP) / 2;
            if before {
                room.x += half + SPLIT_GAP;
            }
            room.width = half;
        }
    }
    Some(room)
}

/// The height of the bar above a page alone, 0 while it hides: over a split, its pages show their
/// own bars.
fn shown_toolbar_height() -> i32 {
    if page_toolbar() && split_shown().is_none() {
        PAGE_TOOLBAR_HEIGHT
    } else {
        0
    }
}

/// The places of the split shown, in the window's points, in order: each of its pages' by its
/// index, and the empty page a tab dragged over it would open, without one.
fn split_slots() -> Vec<(Option<usize>, Rect)> {
    let (tabs, layout) = win().split.borrow().clone();
    let (Some(layout), Some(card)) = (layout, card_rect()) else {
        return Vec::new();
    };
    let room = if layout.stacked {
        card.height
    } else {
        card.width
    };
    // A tab dragged over the split: like Arc's, an empty page opens where it would go, all the
    // pages sharing the room evenly.
    let preview = win().pane_preview.get().map(|index| index.min(tabs.len()));
    let count = tabs.len() + usize::from(preview.is_some());
    let lengths = match preview {
        Some(_) => pane_lengths(room, &vec![1.0 / count as f64; count]),
        None => pane_lengths(room, &layout.factors),
    };
    let mut start = 0;
    let mut tab = 0;
    lengths
        .into_iter()
        .enumerate()
        .map(|(at, length)| {
            let rect = if layout.stacked {
                Rect {
                    x: card.x,
                    y: card.y + start,
                    width: card.width,
                    height: length,
                }
            } else {
                Rect {
                    x: card.x + start,
                    y: card.y,
                    width: length,
                    height: card.height,
                }
            };
            start += length + SPLIT_GAP;
            let index = (preview != Some(at)).then(|| {
                tab += 1;
                tab - 1
            });
            (index, rect)
        })
        .collect()
}

/// Where each of the split's pages shows, in the window's points, in order.
fn split_rects() -> Vec<Rect> {
    split_slots()
        .into_iter()
        .filter_map(|(index, rect)| index.map(|_| rect))
        .collect()
}

/// How long each of a split's pages is along `room`, by its part, SPLIT_GAP apart; the last
/// takes what is left.
fn pane_lengths(room: i32, factors: &[f64]) -> Vec<i32> {
    let count = factors.len() as i32;
    let shared = f64::from(room - SPLIT_GAP * (count - 1).max(0));
    let mut left = room - SPLIT_GAP * (count - 1).max(0);
    factors
        .iter()
        .enumerate()
        .map(|(index, factor)| {
            let length = if index as i32 == count - 1 {
                left
            } else {
                (shared * factor).round() as i32
            };
            left -= length;
            length.max(1)
        })
        .collect()
}

/// Opens the empty page standing where a tab dragged over the split would go, at `index` among
/// its pages, or closes it.
fn preview_pane(index: Option<usize>) {
    if win().pane_preview.replace(index) != index {
        place_card();
    }
}

/// Tells the native interface where the card is and where its pages show, a page alone or a
/// split's, and how each stands, for their containers, bars and the focused one's outline.
fn send_panes() {
    #[cfg(target_os = "macos")]
    {
        let tabs = win().split.borrow().0.clone();
        let focused = win().shown.get();
        let split = tabs
            .first()
            .map(|&tab| STATE.with_borrow(|state| state.sidebar.row_of(tab)));
        // In the card's view, which holds the pages.
        let card = card_rect().unwrap_or_default();
        let mut panes: Vec<_> = tabs
            .iter()
            .zip(split_rects())
            .map(|(&tab, rect)| {
                let browser = PAGES.with_borrow(|pages| Some(pages.get(&tab)?.browser.clone()));
                let url = STATE
                    .with_borrow_mut(|state| state.sidebar.tab_mut(tab).map(|tab| tab.url.clone()));
                let can = |back: bool| {
                    browser.as_ref().is_some_and(|browser| {
                        (if back {
                            browser.can_go_back()
                        } else {
                            browser.can_go_forward()
                        }) == 1
                    })
                };
                let developer = url.as_deref().is_some_and(developer_mode);
                json!({
                    "tab": tab, "split": split,
                    "x": rect.x - card.x, "y": rect.y - card.y, "width": rect.width, "height": rect.height,
                    "bar": PANE_BAR_HEIGHT,
                    "url": url.unwrap_or_default(),
                    "back": can(true), "forward": can(false),
                    "loading": browser.as_ref().is_some_and(|browser| browser.is_loading() == 1),
                    "focused": focused == Some(tab),
                    "color": TOP_COLORS.with_borrow(|colors| colors.get(&tab).cloned()),
                    "developer": developer,
                    "toolbar": false, "alone": false, "square": false,
                    "cardX": card.x, "cardY": card.y,
                })
            })
            .collect();
        // A page alone, where the card lays it out.
        if let Some(tab) = focused.filter(|_| tabs.is_empty())
            && card_rect().is_some()
        {
            let bar = shown_toolbar_height();
            panes.push(json!({
                "tab": tab, "split": null,
                "x": 0, "y": bar, "width": card.width, "height": card.height - bar,
                "bar": 0, "url": "", "back": false, "forward": false, "loading": false,
                "focused": true, "color": null, "developer": false, "toolbar": false,
                "alone": true, "square": square_card(), "cardX": card.x, "cardY": card.y,
            }));
        }
        // The empty page standing where a dragged tab would open, known by its tab, -1, which
        // the interface draws.
        if let Some((_, frame)) = split_slots().into_iter().find(|(index, _)| index.is_none()) {
            panes.push(json!({
                "tab": -1, "split": null,
                "x": frame.x - card.x, "y": frame.y - card.y,
                "width": frame.width, "height": frame.height,
                "bar": 0, "url": "", "back": false, "forward": false, "loading": false,
                "focused": false, "color": null, "developer": false, "toolbar": false,
                "alone": false, "square": false, "cardX": card.x, "cardY": card.y,
            }));
        }
        // A page alone has the bar above it in toolbar or developer mode, a view of the card like
        // a split's page's, which the sidebar passes over, like Arc's.
        if tabs.is_empty() && page_toolbar() {
            panes.push(json!({
                "tab": focused, "split": null,
                "x": 0, "y": 0, "width": card.width, "height": card.height,
                "bar": PAGE_TOOLBAR_HEIGHT, "url": selected_url().unwrap_or_default(),
                "back": false, "forward": false, "loading": false, "focused": true,
                // Like Arc's, the bar takes the color at the page's top.
                "color": focused.and_then(|tab| TOP_COLORS.with_borrow(|colors| colors.get(&tab).cloned())),
                "developer": false, "toolbar": true, "alone": false, "square": square_card(),
                "cardX": card.x, "cardY": card.y,
            }));
        }
        // Once the sidebar sent the window's colors, which first show it.
        let shows = win().colored.get() && card_rect().is_some();
        let frame = shows.then_some([card.x, card.y, card.width, card.height]);
        // Only once they change: placing the bars again would end a click on them.
        let sent = json!({ "card": frame, "panes": panes }).to_string();
        if win().panes_sent.borrow().as_ref() == Some(&sent) {
            return;
        }
        let view = crate::native::panes(win().id, &sent);
        if let Some(view) = view {
            win().card_view.set(Some(view));
            // A split's room between its pages shows the window; a page alone has the card's
            // corners.
            if tabs.is_empty() {
                crate::mac::round_card(view, !square_card());
            } else {
                crate::mac::round_split(view);
            }
        }
        // Until the window can show the card, as it opens.
        if view.is_some() || frame.is_none() {
            win().panes_sent.replace(Some(sent));
        }
    }
}

/// Peek, like Arc's: a link shown over the window in a card, which its controls close or keep in
/// a tab or a split.
struct Peek {
    browser: Browser,
    /// The color at the top of its page, as #rrggbb, which tints its bar, once known.
    color: Option<String>,
}

/// Peek's card in a window of `size`, like Arc's: 80% of the page's card's width, centered on it,
/// 12 points from the window's top and bottom.
fn peek_frame(size: &Size) -> Rect {
    let card = win().sidebar_mode.get().layout(size).card;
    let width = card.width * 4 / 5;
    Rect {
        x: card.x + (card.width - width) / 2,
        y: 12,
        width,
        height: size.height - 24,
    }
}

/// Shows `url` in Peek, in the displayed Space's profile, instead of what it showed.
fn peek(url: &str) {
    close_find();
    close_peek();
    let Some(window) = win().window.with_borrow(Clone::clone) else {
        return;
    };
    let frame = peek_frame(&window.size());
    // Its card first, which the page is drawn in, with its scrim and controls.
    #[cfg(target_os = "macos")]
    crate::native::peek(win().id, Some(frame));
    #[cfg(target_os = "macos")]
    let info = WindowInfo {
        parent_view: match crate::native::peek_view(win().id) {
            Some(view) => view,
            None => return,
        },
        // Any size: the card gives the page its own.
        bounds: Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        },
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    };
    #[cfg(not(target_os = "macos"))]
    let info = WindowInfo {
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    };
    let profile = displayed_profile();
    let mut context = profile_context(profile);
    let Some(browser) = browser_host_create_browser_sync(
        Some(&info),
        Some(&mut ContentClient::new(profile, true)),
        Some(&CefString::from(url)),
        Some(&BrowserSettings::default()),
        None,
        context.as_mut(),
    ) else {
        return;
    };
    if let Some(host) = browser.host() {
        host.set_focus(1);
    }
    // The extensions learn it, and it tells the color at its top.
    extensions::page_created(profile, &browser);
    PEEK.set(Some(Peek {
        browser,
        color: None,
    }));
    send_peek();
}

/// Whether `browser` is Peek's page.
fn is_peek(browser: &Browser) -> bool {
    PEEK.with_borrow(|peek| {
        peek.as_ref()
            .is_some_and(|peek| browser.is_same(Some(&mut peek.browser.clone())) == 1)
    })
}

/// Gives Peek's bar its page's address, and whether it can go back or forward and loads.
fn send_peek() {
    let Some((browser, color)) = PEEK.with_borrow(|peek| {
        peek.as_ref()
            .map(|peek| (peek.browser.clone(), peek.color.clone()))
    }) else {
        return;
    };
    let url = browser
        .main_frame()
        .map(|frame| CefString::from(&frame.url()).to_string())
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    crate::native::peek_bar(
        win().id,
        &url,
        color.as_deref().unwrap_or_default(),
        browser.can_go_back() == 1,
        browser.can_go_forward() == 1,
        browser.is_loading() == 1,
    );
    #[cfg(not(target_os = "macos"))]
    let _ = (url, color);
}

/// Does what Peek's bar asks of its page.
fn peek_page(action: PeekAction) {
    let Some(browser) = PEEK.with_borrow(|peek| peek.as_ref().map(|peek| peek.browser.clone()))
    else {
        return;
    };
    match action {
        PeekAction::Back => browser.go_back(),
        PeekAction::Forward => browser.go_forward(),
        PeekAction::Reload => browser.reload(),
        PeekAction::Stop => browser.stop_load(),
    }
}

/// Peek follows the window's size.
fn place_peek() {
    let size = win()
        .window
        .with_borrow(|window| window.as_ref().map(MainWindow::size));
    let Some(frame) = size.as_ref().map(peek_frame) else {
        return;
    };
    #[cfg(target_os = "macos")]
    if peek_shown() {
        crate::native::peek(win().id, Some(frame));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = frame;
}

/// Whether Peek shows a link.
pub fn peek_shown() -> bool {
    PEEK.with_borrow(Option::is_some)
}

/// Closes Peek, its page with it.
fn close_peek() {
    if !peek_shown() {
        return;
    }
    close_find();
    let Some(peek) = PEEK.take() else { return };
    #[cfg(target_os = "macos")]
    crate::native::peek(win().id, None);
    extensions::page_closed(peek.browser.identifier());
    // Its card's view goes as it closes (BrowserLifeSpan's do_close), which CEF would otherwise
    // close the window of.
    PEEK_CLOSING.set(Some((peek.browser.identifier(), win().id)));
    if let Some(host) = peek.browser.host() {
        host.close_browser(1);
    }
}

/// Opens Peek's page in a tab of its own, or beside the selected tab in a split, and closes it.
fn keep_peek(split: bool) {
    let url = PEEK.with_borrow(|peek| {
        let frame = peek.as_ref()?.browser.main_frame()?;
        Some(CefString::from(&frame.url()).to_string())
    });
    close_peek();
    match url.filter(|url| !url.is_empty()) {
        Some(url) if split => open_in_pane(&url),
        Some(url) => open_tab(&url),
        None => {}
    }
}

/// Selects the page after the one focused in the split shown, or before it, wrapping around.
fn switch_pane(next: bool) {
    let tabs = win().split.borrow().0.clone();
    let Some(index) = tabs.iter().position(|&tab| Some(tab) == win().shown.get()) else {
        return;
    };
    let step = if next { 1 } else { tabs.len() - 1 };
    select_tab(tabs[(index + step) % tabs.len()]);
}

/// Tells each window showing `tab` in a split how its page now stands.
fn pane_changed(tab: NodeId) {
    in_each_window(|| {
        if win().split.borrow().0.contains(&tab) {
            send_panes();
        }
    });
}

/// The split's pages, in the window's points, along its axis: where each starts and its length.
fn split_panes() -> Vec<(f64, f64)> {
    let stacked = split_shown() == Some(true);
    split_rects()
        .iter()
        .map(|rect| {
            if stacked {
                (f64::from(rect.y), f64::from(rect.height))
            } else {
                (f64::from(rect.x), f64::from(rect.width))
            }
        })
        .collect()
}

/// Puts the handles that resize a split's pages in the room between them, or takes them away
/// without a split shown.
fn place_split_handles() {
    #[cfg(target_os = "macos")]
    {
        let stacked = split_shown() == Some(true);
        let rects = split_rects();
        let handles: Vec<Rect> = rects
            .iter()
            .take(rects.len().saturating_sub(1))
            .map(|rect| {
                if stacked {
                    Rect {
                        y: rect.y + rect.height,
                        height: SPLIT_GAP,
                        ..*rect
                    }
                } else {
                    Rect {
                        x: rect.x + rect.width,
                        width: SPLIT_GAP,
                        ..*rect
                    }
                }
            })
            .collect();
        crate::native::split_handles(win().id, stacked, &handles);
    }
}

/// The least a split's page keeps along its axis, as Arc's.
const MIN_PANE: f64 = 220.0;
/// How close to equal parts a handle moved snaps to them.
const PANE_SNAP: f64 = 14.0;

/// Moves the handle after the split's page at `index` to `point`, along the split, in the
/// window's points: the pages on either side share their room by it, each keeping at least
/// `MIN_PANE`, and snap to equal parts near them, like Arc's.
fn resize_panes(index: usize, point: f64) {
    let panes = split_panes();
    if let (Some(&lead), Some(&(_, trail))) = (panes.get(index), panes.get(index + 1)) {
        share_panes(index, lead_part(lead, trail, point));
    }
}

/// The part of the room the page at `lead` (where it starts, its length) keeps of what it shares
/// with the next one, `trail` long, once the handle between them moves to `point`.
fn lead_part((start, lead): (f64, f64), trail: f64, point: f64) -> f64 {
    let room = lead + trail;
    let mut lead = point - f64::from(SPLIT_GAP) / 2.0 - start;
    if room >= 2.0 * MIN_PANE {
        lead = lead.clamp(MIN_PANE, room - MIN_PANE);
    }
    if (lead - room / 2.0).abs() < PANE_SNAP {
        lead = room / 2.0;
    }
    lead.clamp(1.0, room - 1.0) / room
}

/// Gives the pages on either side of the handle after the page at `index` equal parts.
fn even_panes(index: usize) {
    share_panes(index, 0.5);
    save_session();
}

/// Gives the page at `index` `part` of the room it shares with the next one, which takes the
/// rest, and keeps it.
fn share_panes(index: usize, part: f64) {
    let (tabs, Some(mut layout)) = win().split.borrow().clone() else {
        return;
    };
    if index + 1 >= layout.factors.len() {
        return;
    }
    let pair = layout.factors[index] + layout.factors[index + 1];
    layout.factors[index] = pair * part;
    layout.factors[index + 1] = pair - layout.factors[index];
    let saved = STATE.with_borrow_mut(|state| {
        let split = state.sidebar.row_of(tabs[0]);
        state.sidebar.set_split_factors(split, &layout.factors)
    });
    if saved.is_ok() {
        win().split.borrow_mut().1 = Some(layout);
        place_card();
    }
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
        #[cfg(not(target_os = "macos"))]
        let _ = shown;
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
            #[cfg(not(target_os = "macos"))]
            let _ = self.window;
        }
    }
}

/// Paints the window around the sidebar and the card as the sidebar paints itself, with its
/// gradient's `colors` and `grain`, and the card behind its content views. The card shows from the
/// sidebar's first colors on, so that it never flashes another color.
fn set_window_colors(card_color: u32, grain: f64, colors: &[u32]) {
    win()
        .window_colors
        .set(Some((card_color, grain, colors.to_vec())));
    if let Some(window) = win().window.with_borrow(Clone::clone) {
        // The native interface tints the window itself, over the blurred desktop, which shows
        // through: transparent, but not 0, which CEF takes for its theme's color.
        #[cfg(target_os = "macos")]
        let _ = (window, grain, colors);
        #[cfg(not(target_os = "macos"))]
        window.set_background_color(colors[0]);
    }
    // The card first shows once the window has its colors.
    if !win().colored.replace(true) {
        place_card();
    }
}

/// Tells the native interface where the card is, which shades the window around it and washes it
/// while it shows no page; a split's pages are cards of their own.
fn send_empty_card() {
    #[cfg(target_os = "macos")]
    {
        let alone = !fills_window() && split_shown().is_none();
        crate::native::card(
            win().id,
            card_rect().filter(|_| alone),
            win().shown.get().is_none(),
        );
    }
}

/// Tells the sidebars the window's width, which their gradient spans like the window's.
fn send_window_width() {
    let Some(size) = win()
        .window
        .with_borrow(|window| window.as_ref().map(|w| w.size()))
    else {
        return;
    };
    let event = format!(
        r#"new CustomEvent("arbalete:window-width", {{ detail: {} }})"#,
        size.width
    );
    dispatch(&win().sidebar, &event);
    dispatch(&win().reveal_view, &event);
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

/// Sends both sidebars the `limit` latest pages of the displayed Space's profile whose address or
/// title holds `text`, for the library's history.
fn search_history(text: &str, limit: usize) {
    let profile = STATE.with_borrow(|state| state.sidebar.displayed().profile);
    let entries = with_history(profile, |history| history.search(text, limit))
        .and_then(|found| {
            found
                .map_err(|error| eprintln!("cannot read the history: {error}"))
                .ok()
        })
        .unwrap_or_default();
    let entries: Vec<_> = entries
        .iter()
        .map(|entry| json!({ "url": entry.url, "title": entry.title, "lastVisit": entry.last_visit }))
        .collect();
    let detail = json!({ "text": text, "entries": entries });
    let event = format!(r#"new CustomEvent("arbalete:history", {{ detail: {detail} }})"#);
    dispatch(&win().sidebar, &event);
    dispatch(&win().reveal_view, &event);
}

/// Opens the library on the history, showing the sidebar first if it is hidden.
fn show_history() {
    if win().sidebar_mode.get() == SidebarMode::Hidden {
        set_sidebar(SidebarMode::Shown);
    }
    dispatch(&win().sidebar, r#"new Event("arbalete:show-history")"#);
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
        (Command::SearchHistory { text, limit }, _) => search_history(&text, limit),
        (Command::RemoveHistory(url), _) => {
            let profile = STATE.with_borrow(|state| state.sidebar.displayed().profile);
            update_history(profile, |history| history.remove(&url));
        }
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
        (Command::SearchOverlay(text), _) => search_overlay(&text),
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
        // Like Arc's, ⌘W closes Peek and ⌘O keeps its page in a tab.
        (Command::Close(None), _) if peek_shown() => close_peek(),
        (Command::OpenLittleInSpace, _) if peek_shown() => keep_peek(false),
        (Command::Close(None), _) if focused_little().is_some() => {
            if let Some(window) = focused_little().and_then(little_window) {
                window.close();
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
            #[cfg(not(target_os = "macos"))]
            let _ = entries;
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
        (Command::NewProfile, _) => new_profile(),
        // Like a folder, an unknown profile or a blank name from the UI is ignored.
        (Command::RenameProfile(id, name), _) => {
            let _ = update_sidebar(|sidebar| sidebar.rename_profile(id, &name));
        }
        (Command::ColorProfile(id, color), _) => {
            let _ = update_sidebar(|sidebar| sidebar.set_profile_color(id, color));
        }
        (Command::SetArchiveAfter(id, after), _) => {
            if update_sidebar(|sidebar| sidebar.set_archive_after(id, after)).is_ok() {
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
        (Command::NewLittle(url), _) => open_link(url),
        (Command::NewWindow, _) => new_window(),
        (Command::NewIncognitoWindow, _) => new_incognito_window(),
        (Command::Little(id, action), _) => little_action(id, action),
        (Command::OpenLittleInSpace, _) => {
            if let Some(id) = focused_little() {
                little_action(id, LittleAction::OpenInSpace);
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
        (Command::NewSpaceProfile(id), _) => new_space_profile(id),
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
            let then = (panel == "elements")
                .then(|| InspectorAction::Script("DevToolsAPI.enterInspectElementMode()".into()));
            if let Some(tab) = page_tab(&browser) {
                show_inspector(tab, panel, then);
            }
        }
        (Command::Capture(area), Some(_)) => capture(area),
        (Command::CaptureArea, Some(_)) => show_capture_veil(),
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
        (Command::ShowThemePicker(id), _) => show_theme_picker(id),
        (Command::HideThemePicker, _) => {
            if let Some(controller) = win().theme.with_borrow(Clone::clone) {
                controller.set_visible(0);
            }
        }
        (Command::SiteControlsHeight(height), _) => {
            win().site_height.set(height);
            place_site_controls();
        }
        (Command::OpenExtensionPopup(id), _) => open_extension_popup(&id),
        (Command::ShowStore, _) => show_store(),
        (Command::ShowCertificate, _) => show_certificate(),
        (Command::ShowExtensionsPage, _) => extensions::load_unpacked(displayed_profile()),
        (Command::UpdateExtensions, _) => extensions::update(displayed_profile()),
        (Command::ManageExtensions, _) => {
            hide_site_controls();
            if win().sidebar_mode.get() == SidebarMode::Hidden {
                set_sidebar(SidebarMode::Shown);
            }
            dispatch(&win().sidebar, r#"new Event("arbalete:show-extensions")"#);
        }
        (Command::ReloadExtensions, _) => {
            extensions::reload(STATE.with_borrow(|state| state.sidebar.displayed().profile));
        }
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

/// Whether a point at `x` from the window's left edge is over the sidebar, shown or revealed,
/// where swipes page between Spaces. The library's Spaces over the window scroll instead: they
/// are left by their back arrow or a click on a Space.
#[cfg(target_os = "macos")]
pub fn is_over_sidebar(x: f64) -> bool {
    if fills_window() {
        return false;
    }
    let width = match win().sidebar_mode.get() {
        SidebarMode::Hidden => return false,
        SidebarMode::Shown => docked_width(),
        SidebarMode::Revealed | SidebarMode::Floating => sidebar_width(),
    };
    x < f64::from(width)
}

/// Sends the swipes among the window's scroll events to the sidebar the window shows.
#[cfg(target_os = "macos")]
pub fn scroll(scroll: crate::swipe::Scroll) {
    use crate::swipe::Swipe;
    match win().swipe.with_borrow_mut(|tracker| tracker.track(scroll)) {
        Some(Swipe::Gesture(gesture)) => {
            let current = win();
            let view = &current.sidebar;
            let detail = serde_json::to_string(&gesture).expect("a gesture");
            dispatch(
                view,
                &format!(r#"new CustomEvent("arbalete:swipe", {{ detail: {detail} }})"#),
            );
        }
        Some(Swipe::Step(next)) => show_found_space(|s| s.next_space(next)),
        None => {}
    }
}

/// Closes every window, which quits the app once its browsers are closed. The last window's own
/// close button does the same on every OS.
pub fn close() {
    close_settings();
    let littles: Vec<_> =
        LITTLE.with_borrow(|littles| littles.iter().map(|l| l.window.clone()).collect());
    for window in littles {
        window.close();
    }
    if !CLOSING.replace(true) {
        QUIT_WINDOWS.set(Some(other_windows()));
    }
    let windows: Vec<_> = WINDOWS.with_borrow(|windows| {
        windows
            .iter()
            .filter_map(|win| win.window.with_borrow(Clone::clone))
            .collect()
    });
    for window in windows {
        window.close();
    }
}

/// Quits from the macOS app menu, once the user confirms it if the settings ask to warn before
/// quitting.
#[cfg(target_os = "macos")]
pub fn quit() {
    if app_settings().warn_before_quit && !crate::mac::confirm("Quit Arbalete?", "", "Quit") {
        return;
    }
    close();
}

wrap_app! {
    pub struct ArbaleteApp {
        data_dir: PathBuf,
    }

    impl App {
        fn on_before_command_line_processing(&self, _process_type: Option<&CefString>, command_line: Option<&mut CommandLine>) {
            let Some(command_line) = command_line else { return };
            // Development builds are signed ad hoc, so each build would ask for keychain access.
            if cfg!(debug_assertions) {
                command_line.append_switch(Some(&CefString::from("use-mock-keychain")));
            }
            // The helper extension, in every profile, tells extensions which tab is which.
            if let Some(dir) = extensions::helper_dir() {
                command_line.append_switch_with_value(
                    Some(&CefString::from("load-extension")),
                    Some(&CefString::from(dir.to_string_lossy().as_ref())),
                );
            }
            // Lets the end-to-end check and the benchmark (e2e/devtools.mjs) drive the UI through DevTools.
            if let Ok(port) = std::env::var("ARBALETE_DEBUG_PORT") {
                command_line.append_switch_with_value(
                    Some(&CefString::from("remote-debugging-port")),
                    Some(&CefString::from(port.as_str())),
                );
            }
        }

        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            crate::scheme::register(registrar);
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(ArbaleteBrowserProcessHandler::new(self.data_dir.clone()))
        }
    }
}

wrap_browser_process_handler! {
    struct ArbaleteBrowserProcessHandler {
        data_dir: PathBuf,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            DATA_DIR.set(self.data_dir.clone()).expect("one context");
            SETTINGS.set(settings::load(&self.data_dir.join(settings::FILE_NAME)));
            restore(self.data_dir.join(store::FILE_NAME));
            check_archive();
            DOWNLOADS.set(downloads::load(&self.data_dir.join(downloads::FILE_NAME)));
            make_bridge();
            #[cfg(target_os = "macos")]
            open_main_window(0);
            #[cfg(not(target_os = "macos"))]
            window_create_top_level(Some(&mut MainWindowDelegate::new(0)));
        }

        fn default_client(&self) -> Option<Client> {
            Some(StrayClient::new())
        }
    }
}

/// Opens the main window `id` on macOS, the native interface making it, then makes it usable
/// (window_ready).
#[cfg(target_os = "macos")]
fn open_main_window(id: u32) {
    focus(id);
    let bounds = win().cascade.take().map(|from| Rect {
        x: from.x + CASCADE,
        y: from.y + CASCADE,
        ..from
    });
    let min = Size {
        width: MIN_WINDOW_WIDTH,
        height: MIN_WINDOW_HEIGHT,
    };
    let window = crate::native::Window::open(id, bounds.as_ref(), &min);
    window_ready(window);
}

/// Makes the window just made usable: its panels' places, its interface's state, its card and
/// sidebar, then its selected tab's page, the links and windows waiting for it.
fn window_ready(window: MainWindow) {
    #[cfg(target_os = "macos")]
    crate::mac::toolbar_shown(app_settings().show_toolbar);
    // Below the overlay, which is added after it.
    let reveal = place_view(no_width, 1);
    let controller = Spot::default();
    win().reveal_view.set(Some(reveal));
    win().reveal.set(Some(controller));
    let overlay = place_view(no_width, 1);
    win().overlay_view.set(Some(overlay));
    win().overlay.set(Some(Spot::default()));
    let site = place_view(no_width, 1);
    win().site_view.set(Some(site));
    win().site.set(Some(Spot::default()));
    let theme = place_view(no_width, 1);
    win().theme_view.set(Some(theme));
    win().theme.set(Some(Spot::default()));
    window.show();
    #[cfg(target_os = "macos")]
    {
        // What the web sidebar asks for once ready (Command::Ready).
        send_state();
        send_downloads();
    }
    win().window.set(Some(window));
    place_card();
    place_reveal();
    // Only the selected tab loads: the others wait until they are selected.
    show_selected();
    if IMPORT_AT_LAUNCH.take() {
        show_import();
    }
    for link in PENDING_LINKS.take() {
        open_link(Some(link));
    }
    for view in RESTORED_WINDOWS.take() {
        open_window(view, false);
    }
    // Once it has settled in front: until then, the window taking the keyboard back from the
    // Command Bar's field would close it.
    if win().fresh.take() {
        post_delayed_task(ThreadId::UI, Some(&mut OpenCommandBar::new(win().id)), 300);
    }
}

/// The card, the revealed sidebar and the margins follow the window `id`'s size. A slide places
/// them on its next step.
pub fn window_resized(id: u32) {
    focus(id);
    if win().slide.with_borrow(Option::is_none) {
        place_card();
        place_reveal();
        place_find();
    }
    // Like Arc's, the Command Bar stays centered as the window is resized.
    let overlay = win().overlay.with_borrow(Clone::clone);
    if let Some(overlay) = overlay.filter(|o| o.is_visible() == 1 && !win().over_url_bar.get()) {
        place_overlay(overlay.bounds().height);
        round_overlay();
    }
    update_drag_regions();
    send_window_width();
    place_theme_picker();
    place_peek();
    #[cfg(target_os = "macos")]
    sync_native_surfaces();
}

/// The window `id` in front shows the live page of its tab, taking it from another window. One
/// going to the background takes a picture of its page, which still draws, for the window that
/// may take it.
pub fn window_activated(id: u32, active: bool) {
    focus(id);
    // Like Arc's, the Command Bar stays as the window comes back, as when it is resized: the page
    // it shows over keeps it from the focus.
    let overlay = win()
        .overlay
        .with_borrow(|overlay| overlay.as_ref().is_some_and(|o| o.is_visible() == 1));
    if active && !overlay {
        show_selected();
    } else if !active {
        snapshot_shown();
    }
}

/// Whether the window `id` may close: every browser is asked to close, and it closes once they
/// all can. With other windows open, it closes alone, its pages going to another one, like Arc's.
pub fn window_can_close(id: u32) -> bool {
    focus(id);
    if !CLOSING.get() && WINDOWS.with_borrow(Vec::len) > 1 {
        close_incognito(id);
        hand_pages_over(id);
        return true;
    }
    CLOSING.set(true);
    let hosts: Vec<_> = PAGES.with_borrow(|pages| {
        pages
            .values()
            .filter_map(|live| live.browser.host())
            .collect()
    });
    let mut closed = true;
    for host in hosts {
        closed &= host.try_close_browser() == 1;
    }
    closed
}

/// Forgets the window `id`, which closed; the last one closing quits.
pub fn window_closed(id: u32) {
    WINDOWS.with_borrow_mut(|windows| windows.retain(|win| win.id != id));
    #[cfg(target_os = "macos")]
    crate::native::detach(id);
    if let Some(next) = WINDOWS.with_borrow(|windows| windows.first().map(|win| win.id)) {
        focus(next);
        show_selected();
    } else if let Some(host) = bridge_host() {
        // Quitting: the incognito windows' folders go too.
        for (_, dir) in INCOGNITO.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
        // The bridge closes last, which quits (browser_closed): closed with the pages, it could
        // quit while the windows still close their overlays, and the app crashed.
        host.close_browser(1);
    }
    // Once another window holds the sidebar: quitting keeps every window for the next session.
    if !CLOSING.get() {
        save_session();
    }
}

// A main window on Linux and Windows, which CEF makes; on macOS, the native interface does
// (open_main_window).
#[cfg(not(target_os = "macos"))]
wrap_window_delegate! {
    struct MainWindowDelegate {
        id: u32,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1280, height: 820 }
        }

        /// Like Arc's, as narrow as its card's least width beside the sidebar.
        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            // Like Arc's, no narrower than the Command Bar's room while it shows over the page,
            // unless the window already was.
            let width = window_by_id(self.id)
                .and_then(|win| win.overlay_min_width.get())
                .unwrap_or(MIN_WINDOW_WIDTH);
            Size { width, height: MIN_WINDOW_HEIGHT }
        }

        // CEF gives views its theme's colors again once it changes, as when the window shows.
        fn on_theme_changed(&self, _view: Option<&mut View>) {
            focus(self.id);
            if let Some(colors) = win().window_colors.with_borrow(Clone::clone) {
                set_window_colors(colors.0, colors.1, &colors.2);
            }
        }

        fn on_layout_changed(&self, _view: Option<&mut View>, _new_bounds: Option<&Rect>) {
            window_resized(self.id);
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            focus(self.id);
            let window = window.expect("window").clone();
            window.set_title(Some(&CefString::from("Arbalete")));
            if let Some(from) = win().cascade.take() {
                window.set_bounds(Some(&Rect { x: from.x + CASCADE, y: from.y + CASCADE, ..from }));
            }
            // The sidebar at the left edge, the rest of the window its own background.
            window.set_background_color(WINDOW_COLOR);
            let layout = window
                .set_to_box_layout(Some(&BoxLayoutSettings {
                    horizontal: 1,
                    cross_axis_alignment: AxisAlignment::STRETCH,
                    ..Default::default()
                }))
                .expect("box layout");
            let sidebar = place_view(docked_width, 1);
            let mut view = View::from(&sidebar);
            window.add_child_view(Some(&mut view));
            layout.set_flex_for_view(Some(&mut view), 0);
            win().sidebar.set(Some(sidebar));
            window_ready(window);
        }

        fn on_window_activation_changed(&self, _window: Option<&mut Window>, active: i32) {
            window_activated(self.id, active == 1);
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            window_closed(self.id);
        }

        fn can_close(&self, _window: Option<&mut Window>) -> i32 {
            window_can_close(self.id).into()
        }

        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn can_resize(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn can_maximize(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn can_minimize(&self, _window: Option<&mut Window>) -> i32 { 1 }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}
wrap_window_delegate! {
    struct SettingsWindowDelegate {}

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            SETTINGS_SIZE
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let window = window.expect("window").clone();
            window.set_title(Some(&CefString::from("Settings")));
            window.set_to_fill_layout();
            let view = browser_view(UiClient::new(), "about:blank", no_width, None);
            window.add_child_view(Some(&mut View::from(&view)));
            SETTINGS_VIEW.set(Some(view));
            window.center_window(Some(&SETTINGS_SIZE));
            window.show();
            SETTINGS_WINDOW.set(Some(window));
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            SETTINGS_WINDOW.set(None);
            SETTINGS_VIEW.set(None);
        }

        /// The window closes once its page can.
        fn can_close(&self, _window: Option<&mut Window>) -> i32 {
            let host = SETTINGS_VIEW.with_borrow(|view| view.as_ref()?.browser()?.host());
            host.map_or(1, |host| host.try_close_browser())
        }

        // Its page draws the title and the toolbar, like Arc's: only macOS keeps standard window
        // buttons in a frameless window.
        fn is_frameless(&self, _window: Option<&mut Window>) -> i32 {
            cfg!(target_os = "macos").into()
        }

        // The window buttons are centered vertically in this height, the title's row.
        fn titlebar_height(&self, _window: Option<&mut Window>, height: Option<&mut f32>) -> i32 {
            if let Some(height) = height {
                *height = SETTINGS_TITLE_HEIGHT;
            }
            1
        }

        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn can_resize(&self, _window: Option<&mut Window>) -> i32 { 0 }
        fn can_maximize(&self, _window: Option<&mut Window>) -> i32 { 0 }
        fn can_minimize(&self, _window: Option<&mut Window>) -> i32 { 1 }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

// The browsers Chromium opens by itself, outside the app's windows: after an install, a full
// Chromium window to say the extension was added. Extensions would take its tab for the active
// one: it closes once the extension is installed. Closed sooner, while Chromium
// shows its bubble in it, it takes the app down.
wrap_client! {
    struct StrayClient {}

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(StrayLifeSpan::new())
        }
    }
}

wrap_life_span_handler! {
    struct StrayLifeSpan {}

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            OPEN_BROWSERS.set(OPEN_BROWSERS.get() + 1);
            if let Some(browser) = browser {
                STRAYS.with_borrow_mut(|strays| strays.push(browser.clone()));
            }
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                STRAYS.with_borrow_mut(|strays| strays.retain(|stray| stray.is_same(Some(browser)) == 0));
            }
            browser_closed();
        }
    }
}

// A Chrome-style page, without Chromium's toolbar.
wrap_browser_view_delegate! {
    struct ChromeViewDelegate {}

    impl ViewDelegate {}

    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::CHROME
        }

        fn chrome_toolbar_type(&self, _browser_view: Option<&mut BrowserView>) -> ChromeToolbarType {
            ChromeToolbarType::NONE
        }
    }
}

wrap_window_delegate! {
    struct LittleWindowDelegate {
        id: u32,
        profile: ProfileId,
        url: String,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            LITTLE_SIZE
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        // Its bar on top, holding the window buttons, then its page.
        fn on_window_created(&self, window: Option<&mut Window>) {
            let window = window.expect("window").clone();
            window.set_title(Some(&CefString::from("Little Arbalete")));
            let layout = window
                .set_to_box_layout(Some(&BoxLayoutSettings {
                    horizontal: 0,
                    cross_axis_alignment: AxisAlignment::STRETCH,
                    ..Default::default()
                }))
                .expect("box layout");
            // Room for the native bar, at the top.
            let bar = place_view(|| 1, LITTLE_BAR_HEIGHT);
            window.add_child_view(Some(&mut View::from(&bar)));
            let mut context = profile_context(self.profile);
            let start = if self.url.is_empty() { "about:blank" } else { &self.url };
            let page = browser_view(ContentClient::new(self.profile, true), start, no_width, context.as_mut());
            let mut view = View::from(&page);
            window.add_child_view(Some(&mut view));
            layout.set_flex_for_view(Some(&mut view), 1);
            LITTLE.with_borrow_mut(|littles| littles.push(Little {
                id: self.id,
                profile: self.profile,
                window: window.clone(),
                page,
                url: self.url.clone(),
                title: String::new(),
            }));
            window.center_window(Some(&LITTLE_SIZE));
            window.show();
            // The native bar, which asks nothing once ready, unlike the web one.
            #[cfg(target_os = "macos")]
            {
                crate::native::little(self.id, Some(window.window_handle()));
                send_little(self.id);
            }
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            #[cfg(target_os = "macos")]
            crate::native::little(self.id, None);
            LITTLE.with_borrow_mut(|littles| littles.retain(|little| little.id != self.id));
        }

        /// The window closes once its page can.
        fn can_close(&self, _window: Option<&mut Window>) -> i32 {
            let host = LITTLE.with_borrow(|littles| {
                littles.iter().find(|l| l.id == self.id)?.page.browser()?.host()
            });
            host.map_or(1, |host| host.try_close_browser())
        }

        // Its bar draws its title, like the main window's sidebar: only macOS keeps standard
        // window buttons in a frameless window.
        fn is_frameless(&self, _window: Option<&mut Window>) -> i32 {
            cfg!(target_os = "macos").into()
        }

        fn titlebar_height(&self, _window: Option<&mut Window>, height: Option<&mut f32>) -> i32 {
            if let Some(height) = height {
                *height = LITTLE_BAR_HEIGHT as f32;
            }
            1
        }

        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn can_resize(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn can_maximize(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn can_minimize(&self, _window: Option<&mut Window>) -> i32 { 1 }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

/// An Alloy browser view, as wide as `width` says when the layout asks, or sized by the layout or
/// its overlay's bounds for 0 (no_width). Without a request context, it uses the global one,
/// which only the app's own UI uses.
fn browser_view(
    client: Client,
    url: &str,
    width: fn() -> i32,
    context: Option<&mut RequestContext>,
) -> BrowserView {
    // A zero height would make the size "empty" and CEF would ignore it.
    browser_view_sized(client, url, width, 1, context)
}

/// A browser view as `browser_view` makes, `height` tall when a layout stacks it.
fn browser_view_sized(
    mut client: Client,
    url: &str,
    width: fn() -> i32,
    height: i32,
    context: Option<&mut RequestContext>,
) -> BrowserView {
    browser_view_create(
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&BrowserSettings::default()),
        None,
        context,
        Some(&mut AlloyViewDelegate::new(width, height)),
    )
    .expect("browser view")
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

wrap_browser_view_delegate! {
    struct AlloyViewDelegate {
        width: fn() -> i32,
        height: i32,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: (self.width)(), height: self.height }
        }
    }

    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

// Shows the app's own UI.
wrap_client! {
    struct UiClient {}

    impl Client {
        fn request_handler(&self) -> Option<RequestHandler> {
            Some(UiRequestHandler::new())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(BrowserLifeSpan::new(None))
        }

        fn drag_handler(&self) -> Option<DragHandler> {
            Some(UiDragHandler::new())
        }
    }
}

// The extensions' bridge's page, drawn nowhere.
wrap_client! {
    struct BridgeClient {}

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(BridgeRender::new())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(BrowserLifeSpan::new(None))
        }
    }
}

wrap_render_handler! {
    struct BridgeRender {}

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                *rect = Rect { x: 0, y: 0, width: 1, height: 1 };
            }
        }
    }
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

// Lets the UI's `-webkit-app-region: drag` areas move the window.
wrap_drag_handler! {
    struct UiDragHandler {}

    impl DragHandler {
        // The settings page fills its own window, which its title and toolbar move.
        fn on_draggable_regions_changed(&self, browser: Option<&mut Browser>, _frame: Option<&mut Frame>, regions: Option<&[DraggableRegion]>) {
            let settings = SETTINGS_VIEW.with_borrow(|view| view.as_ref()?.browser());
            if let (Some(browser), Some(mut settings)) = (browser.as_deref(), settings)
                && browser.is_same(Some(&mut settings)) == 1
                && let Some(window) = SETTINGS_WINDOW.with_borrow(Clone::clone)
            {
                window.set_draggable_regions(regions);
            }
        }
    }
}

// Shows a profile's web pages.
wrap_client! {
    struct ContentClient {
        profile: ProfileId,
        // A Little Arbalete window's page, rather than a tab's.
        little: bool,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(BrowserLifeSpan::new((!self.little).then_some(self.profile)))
        }

        fn focus_handler(&self) -> Option<FocusHandler> {
            (!self.little).then(PageFocus::new)
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(ContentDisplayHandler::new(self.profile))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(ContentLoadHandler::new(self.profile))
        }

        fn find_handler(&self) -> Option<FindHandler> {
            Some(ContentFindHandler::new())
        }

        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(ContentMenuHandler::new())
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(ContentDownloadHandler::new())
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            (!self.little).then(ContentRequestHandler::new)
        }
    }
}

wrap_request_handler! {
    struct ContentRequestHandler {}

    impl RequestHandler {
        /// Like Arc's, a link from a pinned tab or a favorite to another site shows in Peek,
        /// the tab staying on its site.
        fn on_before_browse(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, request: Option<&mut Request>, user_gesture: i32, is_redirect: i32) -> i32 {
            let (Some(browser), Some(frame), Some(request)) = (browser, frame, request) else { return 0 };
            if user_gesture == 0 || is_redirect == 1 || frame.is_main() == 0 {
                return 0;
            }
            let Some(tab) = page_tab(browser) else { return 0 };
            let anchored = STATE.with_borrow(|state| state.sidebar.is_pinned(tab) || state.sidebar.is_favorite(tab));
            let (url, current) = (CefString::from(&request.url()).to_string(), CefString::from(&frame.url()).to_string());
            let elsewhere = origin(&current).is_some() && origin(&url).is_some() && origin(&url) != origin(&current);
            if !(anchored && elsewhere) {
                return 0;
            }
            post(Command::Peek(url));
            1
        }

        /// A link clicked with ⇧ shows in Peek, and one clicked with ⌘ opens in a tab, like
        /// Arc's.
        fn on_open_urlfrom_tab(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, target_url: Option<&CefString>, target_disposition: WindowOpenDisposition, user_gesture: i32) -> i32 {
            let Some(url) = target_url.map(CefString::to_string).filter(|_| user_gesture == 1) else { return 0 };
            if target_disposition == WindowOpenDisposition::NEW_WINDOW {
                post(Command::Peek(url));
            } else if [WindowOpenDisposition::NEW_FOREGROUND_TAB, WindowOpenDisposition::NEW_BACKGROUND_TAB].contains(&target_disposition) {
                post(Command::Open(url));
            } else {
                return 0;
            }
            1
        }
    }
}

// Saves downloads in the Downloads folder without asking, under a name no file or other download
// has yet, and tells the sidebars how they go.
wrap_download_handler! {
    struct ContentDownloadHandler {}

    impl DownloadHandler {
        fn can_download(&self, _browser: Option<&mut Browser>, _url: Option<&CefString>, _request_method: Option<&CefString>) -> i32 {
            1
        }

        fn on_before_download(&self, _browser: Option<&mut Browser>, item: Option<&mut DownloadItem>, suggested_name: Option<&CefString>, callback: Option<&mut BeforeDownloadCallback>) -> i32 {
            let (Some(item), Some(callback)) = (item, callback) else { return 0 };
            let name = suggested_name.map(CefString::to_string).unwrap_or_default();
            // The Web Store's extensions, which Chromium installs once downloaded: kept out of the
            // downloads, in a temporary folder.
            let mime = CefString::from(&item.mime_type()).to_string();
            if mime == "application/x-chrome-extension" || name.ends_with(".crx") {
                let path = std::env::temp_dir().join(&name);
                callback.cont(Some(&CefString::from(path.to_string_lossy().as_ref())), 0);
                return 1;
            }
            let dir = crate::user_home().join("Downloads");
            if let Err(error) = std::fs::create_dir_all(&dir) {
                eprintln!("cannot create the Downloads folder: {error}");
                return 0;
            }
            let path = DOWNLOADS.with_borrow(|downloads| {
                downloads::unique_path(&dir, &name, |path| {
                    path.exists() || downloads.iter().any(|download| download.path == path)
                })
            });
            callback.cont(Some(&CefString::from(path.to_string_lossy().as_ref())), 0);
            let url = CefString::from(&item.url()).to_string();
            DOWNLOADS.with_borrow_mut(|list| {
                let id = downloads::next_id(list);
                list.push(Download::new(id, item.id(), path, url, now()));
            });
            save_downloads();
            send_downloads();
            1
        }

        fn on_download_updated(&self, _browser: Option<&mut Browser>, item: Option<&mut DownloadItem>, callback: Option<&mut DownloadItemCallback>) {
            let Some(item) = item else { return };
            let cef_id = item.id();
            let Some(id) = DOWNLOADS.with_borrow(|list| {
                list.iter().find(|download| download.cef_id == Some(cef_id)).map(|download| download.id)
            }) else {
                return;
            };
            let state = if item.is_complete() == 1 {
                downloads::State::Complete
            } else if item.is_canceled() == 1 {
                downloads::State::Canceled
            } else if item.is_interrupted() == 1 {
                downloads::State::Failed
            } else {
                downloads::State::InProgress
            };
            DOWNLOAD_CALLBACKS.with_borrow_mut(|callbacks| match (state, callback) {
                (downloads::State::InProgress, Some(callback)) => {
                    callbacks.insert(id, callback.clone());
                }
                _ => {
                    callbacks.remove(&id);
                }
            });
            let (changed, finished) = DOWNLOADS.with_borrow_mut(|list| {
                let Some(download) = list.iter_mut().find(|download| download.id == id) else {
                    return (false, false);
                };
                let before = download.clone();
                download.received = item.received_bytes();
                download.total = item.total_bytes().max(0);
                download.speed = item.current_speed();
                download.state = state;
                (*download != before, before.state != state)
            });
            // Only the end is saved: progress changes several times a second.
            if finished {
                save_downloads();
            }
            if changed {
                send_downloads();
            }
        }
    }
}

/// The items Arbalete adds to a page's menu, numbered from the first id CEF leaves to apps.
const OPEN_LINK: i32 = 26500;
const COPY_LINK: i32 = OPEN_LINK + 1;
const OPEN_IMAGE: i32 = OPEN_LINK + 2;
const COPY_IMAGE: i32 = OPEN_LINK + 3;
const INSPECT: i32 = OPEN_LINK + 4;

// Adds what a browser offers on a link or an image above Chromium's own items, and Inspect
// Element below them.
wrap_context_menu_handler! {
    struct ContentMenuHandler {}

    impl ContextMenuHandler {
        fn on_before_context_menu(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, params: Option<&mut ContextMenuParams>, model: Option<&mut MenuModel>) {
            let (Some(params), Some(model)) = (params, model) else { return };
            let mut items = Vec::new();
            if !CefString::from(&params.link_url()).to_string().is_empty() {
                items.extend([(OPEN_LINK, "Open Link in New Tab"), (COPY_LINK, "Copy Link Address")]);
            }
            if params.media_type() == ContextMenuMediaType::IMAGE {
                items.extend([(OPEN_IMAGE, "Open Image in New Tab"), (COPY_IMAGE, "Copy Image Address")]);
            }
            if !items.is_empty() {
                if model.count() > 0 {
                    model.insert_separator_at(0);
                }
                for (id, label) in items.into_iter().rev() {
                    model.insert_item_at(0, id, Some(&CefString::from(label)));
                }
            }
            if model.count() > 0 {
                model.add_separator();
            }
            model.add_item(INSPECT, Some(&CefString::from("Inspect Element")));
        }

        fn on_context_menu_command(&self, browser: Option<&mut Browser>, _frame: Option<&mut Frame>, params: Option<&mut ContextMenuParams>, command_id: i32, _event_flags: EventFlags) -> i32 {
            let Some(params) = params else { return 0 };
            // The page's own developer tools, docked in it, a split's other pages' too.
            if command_id == INSPECT {
                let at = InspectorAction::InspectAt(params.xcoord(), params.ycoord());
                if let Some(tab) = browser.and_then(|browser| page_tab(browser)) {
                    show_inspector(tab, "elements", Some(at));
                }
                return 1;
            }
            let link = CefString::from(&params.link_url()).to_string();
            let image = CefString::from(&params.source_url()).to_string();
            let action = match command_id {
                OPEN_LINK => Action::Run(Command::Open(link)),
                COPY_LINK => Action::CopyLink(link),
                OPEN_IMAGE => Action::Run(Command::Open(image)),
                COPY_IMAGE => Action::CopyLink(image),
                _ => return 0,
            };
            perform(action);
            1
        }
    }
}

// Tells the find bar how many matches the page it searches has, and which one is active, once the
// search ends: the updates before it count matches without an active one.
wrap_find_handler! {
    struct ContentFindHandler {}

    impl FindHandler {
        fn on_find_result(&self, browser: Option<&mut Browser>, _identifier: i32, count: i32, _selection_rect: Option<&Rect>, active_match_ordinal: i32, final_update: i32) {
            let searched = find_page().is_some_and(|mut page| {
                browser.is_some_and(|browser| browser.is_same(Some(&mut page)) == 1)
            });
            if final_update == 1 && searched {
                dispatch(
                    &win().find_view,
                    &format!(r#"new CustomEvent("arbalete:found", {{ detail: {{ "count": {count}, "active": {active_match_ordinal} }} }})"#),
                );
            }
        }
    }
}

wrap_display_handler! {
    struct ContentDisplayHandler {
        profile: ProfileId,
    }

    impl DisplayHandler {
        fn on_address_change(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            if frame.is_none_or(|frame| frame.is_main() == 0) {
                return;
            }
            let url = url.map(CefString::to_string).unwrap_or_default();
            let Some(browser) = browser else { return };
            if is_peek(browser) {
                return send_peek();
            }
            let Some(tab) = page_tab(browser) else {
                return little_changed(browser, |little| little.url = url);
            };
            update_sidebar(|sidebar| {
                if let Some(tab) = sidebar.tab_mut(tab) {
                    // Another site's page has its own icon, or none: the previous page's goes
                    // until it is known (on_favicon_urlchange).
                    if site(&tab.url) != site(&url) {
                        tab.favicon = None;
                    }
                    tab.url = url;
                }
            });
            pane_changed(tab);
        }

        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            let title = title.map(CefString::to_string).unwrap_or_default();
            if let Some(frame) = browser.as_ref().and_then(|browser| browser.main_frame()) {
                let url = CefString::from(&frame.url()).to_string();
                update_history(self.profile, |history| history.set_title(&url, &title));
            }
            let Some(browser) = browser else { return };
            let Some(tab) = page_tab(browser) else {
                return little_changed(browser, |little| little.title = title);
            };
            update_sidebar(|sidebar| {
                if let Some(tab) = sidebar.tab_mut(tab) {
                    tab.title = title;
                }
            });
        }

        fn on_favicon_urlchange(&self, browser: Option<&mut Browser>, icon_urls: Option<&mut CefStringList>) {
            let Some(browser) = browser else { return };
            let Some(tab) = page_tab(browser) else { return };
            // Not cloned: a clone of the list CEF lends lists no string.
            let source = icon_urls.and_then(|urls| std::mem::take(urls).into_iter().next());
            change_favicon(self.profile, tab, source, browser.host());
        }

        /// A page capturing is never suspended.
        fn on_media_access_change(&self, browser: Option<&mut Browser>, has_video_access: i32, has_audio_access: i32) {
            let Some(tab) = browser.and_then(|browser| page_tab(browser)) else { return };
            PAGES.with_borrow_mut(|pages| {
                if let Some(live) = pages.get_mut(&tab) {
                    live.capturing = has_video_access == 1 || has_audio_access == 1;
                }
            });
        }

        fn on_loading_progress_change(&self, browser: Option<&mut Browser>, progress: f64) {
            if is_shown(browser) {
                update_page(|page| page.progress = progress);
            }
        }
    }
}

/// The site of a url: its host and port, or the url itself when it has no host, like about:blank.
fn site(url: &str) -> &str {
    url.split_once("://")
        .and_then(|(_, rest)| rest.split(['/', '?', '#']).next())
        .unwrap_or(url)
}

/// Shows a tab's new icon once it is cached, downloading it first through its page's browser:
/// with the page's cookies and network settings, and no service of a third party.
fn change_favicon(
    profile: ProfileId,
    tab: NodeId,
    source: Option<String>,
    host: Option<BrowserHost>,
) {
    let Some((url, shown)) = STATE.with_borrow(|state| {
        let NodeKind::Tab(tab) = &state.sidebar.node(tab)?.kind else {
            return None;
        };
        Some((tab.url.clone(), tab.favicon.clone()))
    }) else {
        return;
    };
    let missing = source
        .as_ref()
        .filter(|source| !favicon::path(&profile_dir(profile), source).exists());
    let (Some(missing), Some(host)) = (missing, host) else {
        return set_favicon(tab, &url, source);
    };
    // Its file is missing, as for a tab moved from another profile: the same icon set again once
    // downloaded would not reload.
    if shown.as_ref() == Some(missing) {
        set_favicon(tab, &url, None);
    }
    let mut download = FaviconDownload::new(profile, tab, url, missing.clone());
    host.download_image(
        Some(&CefString::from(missing.as_str())),
        1,
        FAVICON_SIZE,
        0,
        Some(&mut download),
    );
}

/// The largest side of a cached icon, in pixels: twice the sidebar's 16 pt.
const FAVICON_SIZE: u32 = 64;

/// Changes a tab's icon unless the tab has gone or moved to another page since.
fn set_favicon(tab: NodeId, url: &str, source: Option<String>) {
    update_sidebar(|sidebar| {
        if let Some(tab) = sidebar.tab_mut(tab).filter(|tab| tab.url == url) {
            tab.favicon = source;
        }
    });
}

wrap_download_image_callback! {
    struct FaviconDownload {
        profile: ProfileId,
        tab: NodeId,
        url: String,
        source: String,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(&self, _image_url: Option<&CefString>, _http_status_code: i32, image: Option<&mut Image>) {
            let image = image.filter(|image| image.is_empty() == 0);
            // Its one representation holds every pixel downloaded, up to FAVICON_SIZE. CEF
            // refuses to encode without the size's out parameters.
            let (mut width, mut height) = (0, 0);
            let png = image.and_then(|image| image.as_png(1.0, 1, Some(&mut width), Some(&mut height)));
            let Some(png) = png else {
                return;
            };
            let mut bytes = vec![0; png.size()];
            png.data(Some(&mut bytes), 0);
            // A few kilobytes: quick enough for the UI thread, which then shows the icon.
            if let Err(error) = favicon::write(&profile_dir(self.profile), &self.source, &bytes) {
                return eprintln!("cannot cache the favicon: {error}");
            }
            set_favicon(self.tab, &self.url, Some(self.source.clone()));
        }
    }
}

wrap_load_handler! {
    struct ContentLoadHandler {
        profile: ProfileId,
    }

    impl LoadHandler {
        /// A page's navigation is committed: it is a visit, whether or not the page loads fully.
        fn on_load_start(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _transition_type: TransitionType) {
            let Some(frame) = frame.filter(|frame| frame.is_main() == 1) else { return };
            let url = CefString::from(&frame.url()).to_string();
            update_history(self.profile, |history| history.visit(&url, now()));
        }

        fn on_loading_state_change(&self, browser: Option<&mut Browser>, is_loading: i32, can_go_back: i32, can_go_forward: i32) {
            if browser.as_deref().is_some_and(is_peek) {
                if is_loading == 0 {
                    ask_top_color(browser.as_deref());
                }
                return send_peek();
            }
            #[cfg(target_os = "macos")]
            cover_revived(browser.as_deref(), is_loading == 1);
            if let Some(tab) = browser.as_deref().and_then(page_tab) {
                pane_changed(tab);
            }
            if is_loading == 0 {
                ask_top_color(browser.as_deref());
            }
            if !is_shown(browser) {
                return;
            }
            if is_loading == 0 {
                snapshot_shown();
            }
            update_page(|page| {
                page.loading = is_loading == 1;
                page.can_go_back = can_go_back == 1;
                page.can_go_forward = can_go_forward == 1;
            });
        }
    }
}

// Quits the app when its last browser closes, and opens popups as tabs. `profile` is the profile
// of a content view's browser, `None` for the app's own UI.
wrap_client! {
    struct PopupClient {
        profile: ProfileId,
        url: String,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(PopupLifeSpan::new())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(PopupDisplayHandler::new())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(PopupLoadHandler::new(self.profile, self.url.clone()))
        }
    }
}

// The popup's blank page has loaded: its own may be prepared, then loaded. Before, DevTools may
// not keep the shim for the next document.
wrap_load_handler! {
    struct PopupLoadHandler {
        profile: ProfileId,
        url: String,
    }

    impl LoadHandler {
        fn on_load_end(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, _http_status_code: i32) {
            let blank = frame.is_some_and(|frame| frame.is_main() == 1 && CefString::from(&frame.url()).to_string() == "about:blank");
            if let Some(browser) = browser.filter(|_| blank) {
                extensions::popup_created(self.profile, browser, &self.url);
            }
        }
    }
}

wrap_display_handler! {
    struct PopupDisplayHandler {}

    impl DisplayHandler {
        fn on_auto_resize(&self, _browser: Option<&mut Browser>, new_size: Option<&Size>) -> i32 {
            if let Some(size) = new_size {
                show_extension_popup(size);
            }
            1
        }
    }
}

// An extension's popup, whose links open tabs.
wrap_life_span_handler! {
    struct PopupLifeSpan {}

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            let Some(url) = target_url.map(CefString::to_string) else { return 1 };
            // A link clicked with ⇧, which would open a window, shows in Peek, like Arc's.
            if target_disposition == WindowOpenDisposition::NEW_WINDOW && user_gesture == 1 {
                post(Command::Peek(url));
            } else {
                open_tab(&url);
            }
            1
        }

        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            OPEN_BROWSERS.set(OPEN_BROWSERS.get() + 1);
        }

        /// Its window goes with it, which CEF would otherwise close through the window's own
        /// close button, asking the app to close the popup again.
        fn do_close(&self, browser: Option<&mut Browser>) -> i32 {
            let Some(browser) = browser else { return 0 };
            let view = browser.host().map(|host| host.window_handle() as usize).unwrap_or(0);
            post_task(
                ThreadId::UI,
                Some(&mut RemoveExtensionPopup::new(browser.identifier(), view)),
            );
            1
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            browser_closed();
        }
    }
}

wrap_task! {
    struct RemoveExtensionPopup {
        browser: i32,
        view: usize,
    }

    impl Task {
        /// Takes a closing extension popup's window away, which destroys its browser; the app
        /// forgets it, when the extension closed it itself.
        fn execute(&self) {
            let closing = win().extension_popup.with_borrow(|popup| {
                popup.as_ref().is_some_and(|popup| popup.browser.identifier() == self.browser)
            });
            if closing {
                win().extension_popup.set(None);
                extensions::popup_closed();
            }
            #[cfg(target_os = "macos")]
            crate::native::extension_popup_closed(self.view as *mut std::ffi::c_void);
        }
    }
}

// A click in a page of a split selects its tab, like Arc's Split View. The focus the app gives a
// page does not: it comes late, and the pages of a split would take turns being selected.
wrap_focus_handler! {
    struct PageFocus {}

    impl FocusHandler {
        fn on_got_focus(&self, browser: Option<&mut Browser>) {
            let Some(tab) = browser.and_then(|browser| page_tab(browser)) else {
                return;
            };
            #[cfg(target_os = "macos")]
            if !crate::mac::mouse_down() {
                return;
            }
            if win().shown.get() != Some(tab) && win().split.borrow().0.contains(&tab) {
                select_tab(tab);
            }
        }
    }
}

wrap_life_span_handler! {
    struct BrowserLifeSpan {
        profile: Option<ProfileId>,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            if let Some(url) = target_url {
                open_tab(&url.to_string());
            }
            1
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            OPEN_BROWSERS.set(OPEN_BROWSERS.get() + 1);
            if let Some(profile) = self.profile {
                content_created(profile, browser);
            }
        }

        /// A page closing leaves its container, which CEF would otherwise close the window of.
        fn do_close(&self, browser: Option<&mut Browser>) -> i32 {
            if self.profile.is_none() {
                // Peek's page leaves its card; a Little Arbalete window's closes it.
                let Some((id, window)) = PEEK_CLOSING.get() else { return 0 };
                if browser.is_none_or(|browser| browser.identifier() != id) {
                    return 0;
                }
                PEEK_CLOSING.set(None);
                post_task(ThreadId::UI, Some(&mut RemovePeek::new(window)));
                return 1;
            }
            let Some(host) = browser.and_then(|browser| browser.host()) else {
                return 0;
            };
            post_task(
                ThreadId::UI,
                Some(&mut RemoveContainer::new(host.window_handle() as usize)),
            );
            1
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let (Some(browser), Some(_)) = (browser, self.profile) {
                extensions::page_closed(browser.identifier());
            }
            browser_closed();
        }
    }
}

/// Counts a browser closed, and quits once the last one is.
fn browser_closed() {
    OPEN_BROWSERS.set(OPEN_BROWSERS.get() - 1);
    if OPEN_BROWSERS.get() == 0 {
        quit_message_loop();
    } else if CLOSING.get() {
        // A browser ready to close while another is not yet cancels the window's close, and with
        // it the ready one's: ask again, or the last ones never close.
        close();
    }
}

wrap_request_handler! {
    struct UiRequestHandler {}

    impl RequestHandler {
        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _is_navigation: i32,
            _is_download: i32,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut i32>,
        ) -> Option<ResourceRequestHandler> {
            let url = CefString::from(&request?.url()).to_string();
            url.starts_with(UI_ORIGIN).then(UiResourceRequestHandler::new)
        }
    }
}

wrap_resource_request_handler! {
    struct UiResourceRequestHandler {}

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            ReturnValue::CONTINUE
        }

        fn resource_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let url = CefString::from(&request?.url()).to_string();
            let path = ui_path(&url);
            if let Some(file) = favicon_file(path) {
                let file = CefString::from(file.to_str()?);
                let stream = stream_reader_create_for_file(Some(&file))?;
                return Some(StreamResourceHandler::new(200, "OK".to_string(), "image/png".to_string(), None, Some(stream)));
            }
            if let Some((file, mime)) = media_file(path).or_else(|| extensions::icon_file(path)) {
                let file = CefString::from(file.to_str()?);
                let stream = stream_reader_create_for_file(Some(&file))?;
                return Some(StreamResourceHandler::new(200, "OK".to_string(), mime.to_string(), None, Some(stream)));
            }
            let command = path.strip_prefix("command/").and_then(Command::parse)?;
            post_task(ThreadId::UI, Some(&mut RunCommand::new(command)));
            let body = b"{}";
            let stream = stream_reader_create_for_data(body.as_ptr() as *mut u8, body.len());
            Some(StreamResourceHandler::new(200, "OK".to_string(), "application/json".to_string(), None, stream))
        }
    }
}

// Requests arrive on the IO thread; browsers are driven from the UI thread.
wrap_task! {
    struct RunCommand {
        command: Command,
    }

    impl Task {
        fn execute(&self) {
            run(self.command.clone());
        }    }
}

/// The path of a UI url. Request urls keep their fragment, like `index.html#overlay`.
fn ui_path(url: &str) -> &str {
    let path = &url[UI_ORIGIN.len()..];
    path.split('#').next().unwrap_or(path)
}

/// The file behind a url of the UI: a tab's cached icon, a media file or an extension's icon.
pub(crate) fn ui_file_path(url: &str) -> Option<PathBuf> {
    let path = ui_path(url.strip_prefix(UI_ORIGIN).map(|_| url)?);
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
        CARD_MARGIN, DraggableRegion, EDGE_WIDTH, FLOAT_INSET, LIBRARY_WIDTH, MemoryPressure,
        RECENT_TABS, Rect, SIDEBAR_WIDTH, SidebarMode, Size, UiState, favicon_file, lead_part,
        lerp_rect, margin_regions, origin, pane_lengths, remember, site, slide_duration, step,
        suspend_delays, toggled, ui_path, win,
    };
    use arbalete_core::sidebar::NodeId;

    #[test]
    fn a_split_shares_its_room_by_the_parts_10_points_apart() {
        assert_eq!(pane_lengths(1010, &[0.5, 0.25, 0.25]), [495, 248, 247]);
        assert_eq!(pane_lengths(810, &[0.5, 0.5]), [400, 400]);
    }

    #[test]
    fn a_split_handle_keeps_the_pages_wide_enough_and_snaps_to_halves() {
        // Two pages of 500 points from 100, the handle 10 points wide between them.
        let part = |point| lead_part((100.0, 500.0), 500.0, point);
        assert!((part(405.0) - 0.3).abs() < 1e-9);
        assert_eq!(part(150.0), 0.22);
        assert_eq!(part(1000.0), 0.78);
        assert_eq!(part(612.0), 0.5);
        assert!((part(625.0) - 0.52).abs() < 1e-9);
    }

    #[test]
    fn tells_the_origin_of_a_web_page() {
        assert_eq!(
            origin("https://www.example.com/a?b#c"),
            Some("https://www.example.com")
        );
        assert_eq!(
            origin("http://localhost:3000/"),
            Some("http://localhost:3000")
        );
        assert_eq!(origin("https://example.com"), Some("https://example.com"));
        assert_eq!(origin("about:blank"), None);
    }

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

    #[test]
    fn remembers_the_tabs_selected_last_the_most_recent_first() {
        let id = |n: u64| -> NodeId { serde_json::from_str(&n.to_string()).unwrap() };
        let mut recent = Vec::new();
        for n in [1, 2, 3, 1] {
            remember(&mut recent, id(n));
        }
        assert_eq!(recent, [id(1), id(3), id(2)]);
        for n in 10..10 + RECENT_TABS as u64 {
            remember(&mut recent, id(n));
        }
        assert_eq!(recent.len(), RECENT_TABS);
    }

    #[test]
    fn the_tab_switcher_steps_round_the_list() {
        assert_eq!(step(0, 3, true), 1);
        assert_eq!(step(2, 3, true), 0);
        assert_eq!(step(0, 3, false), 2);
    }

    fn rect(rect: &Rect) -> (i32, i32, i32, i32) {
        (rect.x, rect.y, rect.width, rect.height)
    }

    fn regions(mode: SidebarMode) -> Vec<(i32, i32, i32, i32)> {
        let regions = margin_regions(&SIZE, &mode.layout(&SIZE).card, mode.margins_start());
        assert!(regions.iter().all(|region| region.draggable == 1));
        bounds(&regions)
    }

    #[test]
    fn the_margins_around_the_card_move_the_window_without_the_sidebar() {
        let m = CARD_MARGIN;
        let margins = [
            (0, 0, 1000, m),
            (0, 600 - m, 1000, m),
            (0, 0, m, 600),
            (1000 - m, 0, m, 600),
        ];
        assert_eq!(regions(SidebarMode::Hidden), margins);
        assert_eq!(regions(SidebarMode::Revealed), margins);
    }

    #[test]
    fn the_margins_start_past_the_sidebar_and_its_edge_while_it_is_docked() {
        let (m, s) = (CARD_MARGIN, SIDEBAR_WIDTH);
        assert_eq!(
            regions(SidebarMode::Shown),
            [
                (s, 0, 1000 - s, m),
                (s, 600 - m, 1000 - s, m),
                (1000 - m, 0, m, 600)
            ]
        );
    }

    #[test]
    fn the_window_moves_from_around_the_floating_sidebar() {
        let (m, s) = (CARD_MARGIN, SIDEBAR_WIDTH);
        assert_eq!(
            regions(SidebarMode::Floating),
            [
                (0, 0, 1000, m),
                (0, 600 - m, 1000, m),
                (0, 0, s, 600),
                (1000 - m, 0, m, 600)
            ]
        );
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
        let start = SidebarMode::Shown.margins_start();
        let hidden = rect(&SidebarMode::Hidden.layout(&SIZE).card);
        win().library.set(false);
        assert_eq!(
            docked,
            (LIBRARY_WIDTH, m, 1000 - LIBRARY_WIDTH - m, 600 - 2 * m)
        );
        assert_eq!(start, LIBRARY_WIDTH);
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
    fn the_toggle_hides_the_sidebar_beside_a_page_and_floats_it_beside_an_empty_panel() {
        assert_eq!(toggled(SidebarMode::Shown, true), SidebarMode::Hidden);
        assert_eq!(toggled(SidebarMode::Shown, false), SidebarMode::Floating);
        for mode in [
            SidebarMode::Hidden,
            SidebarMode::Revealed,
            SidebarMode::Floating,
        ] {
            assert_eq!(toggled(mode, true), SidebarMode::Shown);
            assert_eq!(toggled(mode, false), SidebarMode::Shown);
        }
    }

    #[test]
    fn the_sidebar_slides_quickly_and_floats_beside_an_empty_panel_at_once() {
        use SidebarMode::{Floating, Hidden, Revealed, Shown};
        assert_eq!(slide_duration(Shown, Hidden), slide_duration(Hidden, Shown));
        assert!((80..=90).contains(&slide_duration(Shown, Hidden)));
        assert!(slide_duration(Hidden, Revealed) > slide_duration(Revealed, Hidden));
        assert!(slide_duration(Revealed, Shown) < slide_duration(Shown, Hidden));
        assert!(slide_duration(Floating, Shown) > 0);
        assert_eq!(slide_duration(Shown, Floating), 0);
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

    fn bounds(regions: &[DraggableRegion]) -> Vec<(i32, i32, i32, i32)> {
        regions
            .iter()
            .map(|region| {
                let rect = &region.bounds;
                (rect.x, rect.y, rect.width, rect.height)
            })
            .collect()
    }

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
        state.sidebar.create_profile("Work").unwrap();
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
    fn puts_localhost_alone_in_developer_mode_by_default() {
        for url in [
            "http://localhost:3000/",
            "http://127.0.0.1/",
            "http://[::1]:8080/a",
            "https://app.localhost/",
        ] {
            assert!(super::developer_mode(url), "{url}");
        }
        for url in [
            "https://app.runora.local/",
            "https://example.com/",
            "about:blank",
        ] {
            assert!(!super::developer_mode(url), "{url}");
        }
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
        let work = state.sidebar.create_profile("Work").unwrap();
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

    #[test]
    fn ignores_the_fragment() {
        assert_eq!(ui_path("arbalete://ui/index.html#overlay"), "index.html");
        assert_eq!(
            ui_path("arbalete://ui/command/open/a%23b"),
            "command/open/a%23b"
        );
    }

    #[test]
    fn pages_suspend_sooner_on_low_memory_or_battery_saving() {
        let minute = 60_000;
        assert_eq!(
            suspend_delays(MemoryPressure::Normal, false),
            (5 * minute, 60 * minute)
        );
        assert_eq!(
            suspend_delays(MemoryPressure::Normal, true),
            (minute, 60 * minute)
        );
        assert_eq!(
            suspend_delays(MemoryPressure::Warning, true),
            (minute, 5 * minute)
        );
        assert_eq!(suspend_delays(MemoryPressure::Critical, false), (0, 0));
    }
}

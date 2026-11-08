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
mod commands;
mod content;
mod downloads_ui;
mod find;
mod history_ui;
mod import_ui;
mod inspector;
mod layout;
mod little;
mod menus;
mod onboarding;
mod overlay;
mod pages;
mod peek;
mod permission_prompt;
mod picture;
mod profiles;
mod session;
mod settings_ui;
mod site_controls;
mod split;
mod tabs;
mod theme_picker;
mod toasts;
mod views;
mod windows;

use archive::*;
use auth_prompt::*;
use capture::*;
pub use chrome_pages::*;
pub use commands::*;
pub use content::*;
use downloads_ui::*;
use find::*;
use history_ui::*;
use import_ui::*;
use inspector::*;
pub use layout::*;
pub use little::*;
pub use menus::*;
use onboarding::*;
use overlay::*;
pub use pages::*;
pub use peek::*;
use permission_prompt::*;
use picture::*;
use profiles::*;
pub use session::*;
pub use settings_ui::*;
pub use site_controls::*;
pub use split::*;
use tabs::*;
use theme_picker::*;
use toasts::*;
use views::*;
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
    /// The pages' popups CEF is making, by popup id, with the Little Arbalete window each opens in.
    static OPENING_POPUPS: RefCell<Vec<(i32, u32)>> = const { RefCell::new(Vec::new()) };
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
    /// The color at the top of the tabs' pages, as #rrggbb, which tints their bar in a split: the
    /// last ones sent only, the most recent last.
    static TOP_COLORS: RefCell<Vec<(NodeId, String)>> = RefCell::default();
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
    /// The first launch shows the welcome flow with the window, instead of the start page.
    static ONBOARD_AT_LAUNCH: Cell<bool> = const { Cell::new(false) };
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
    /// The menu item's title of the shortcut the page has first, pressed once, until the page
    /// leaves it (page_offered_shortcut).
    static OFFERED_SHORTCUT: RefCell<Option<String>> = const { RefCell::new(None) };
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
    /// The shown page fills the window, as it went full screen, like a video; and whether the
    /// window went full screen with it, to leave it with it (fill_with_page).
    page_full_screen: Cell<Option<bool>>,
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
            page_full_screen: Cell::new(None),
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
            // A page in full screen fills the window, over the sidebar.
            _ if page_full_screen() => 0,
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
/// How many closed tabs may be reopened, the oldest dropped beyond: Chromium's cap, which Arc's
/// keeps.
const MAX_CLOSED: usize = 25;

/// What the UI shows.
#[derive(Default)]
struct UiState {
    page: Page,
    sidebar: Sidebar,
    /// Closed tabs, the last one at the end, the [`MAX_CLOSED`] last ones until the app quits.
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
    /// Keeps tabs just closed, which may be reopened, dropping the oldest beyond [`MAX_CLOSED`].
    fn keep_closed(&mut self, tabs: impl IntoIterator<Item = Closed>) {
        self.closed.extend(tabs);
        let over = self.closed.len().saturating_sub(MAX_CLOSED);
        self.closed.drain(..over);
    }

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
                "emoji": space_icon(s),
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
                "url": shown_url(&closed.tab.url),
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
        "url": shown_url(&tab.url),
        "title": tab.label(),
        "favicon": favicon_url(profile, tab),
        "muted": tab.muted,
        "asleep": ASLEEP.with_borrow(|asleep| asleep.contains(&id)),
        "open": has_page(id),
        "developer": developer_mode(&tab.url),
    })
}

/// Whether a tab's page was shown since it opened, even when it was unloaded since to save
/// memory: closing an open pinned tab takes it back to its saved url instead of removing it.
fn has_page(tab: NodeId) -> bool {
    PAGES.with_borrow(|pages| pages.contains_key(&tab))
        || ASLEEP.with_borrow(|asleep| asleep.contains(&tab))
}

/// Where the UI loads a tab's icon from: the app serves the profile's cached icons (favicon_file),
/// from the UI's own origin so that the UI may read their pixels.
fn favicon_url(profile: ProfileId, tab: &Tab) -> Option<String> {
    Some(icon_url(profile, tab.favicon.as_deref()?))
}

/// A Space's icon for the UI: a site's as "site:" and where the UI loads it from (icon_url).
fn space_icon(space: &Space) -> Option<String> {
    match space.site_icon() {
        Some(source) => Some(format!("site:{}", icon_url(space.profile, source))),
        None => space.emoji.clone(),
    }
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
    // Extensions see the displayed Space's profile in front, with its selected tab as its active
    // tab.
    let active = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let space = sidebar.displayed();
        let tab = space.selected.and_then(|selected| {
            let NodeKind::Tab(tab) = &sidebar.node(selected)?.kind else {
                return None;
            };
            let browser = PAGES.with_borrow(|pages| Some(pages.get(&selected)?.browser.clone()));
            Some(extensions::ActiveTab {
                page: browser.as_ref().map(Browser::identifier),
                url: tab.url.clone(),
                title: tab.label().to_string(),
                favicon: tab.favicon.clone(),
                loading: browser.is_some_and(|browser| browser.is_loading() == 1),
            })
        });
        (space.profile, tab)
    });
    let mut active = HashMap::from([active]);
    // The Little Arbalete window in front last shows its profile's active tab.
    let little = LITTLE.with_borrow(|littles| {
        let little = littles.iter().find(|l| Some(l.id) == ACTIVE_LITTLE.get())?;
        let browser = little.browser();
        let tab = extensions::ActiveTab {
            page: browser.as_ref().map(Browser::identifier),
            url: little.url.clone(),
            title: little.title.clone(),
            favicon: None,
            loading: browser.is_some_and(|browser| browser.is_loading() == 1),
        };
        Some((little.profile, Some(tab)))
    });
    active.extend(little);
    extensions::sync(active);
}

/// Opens the library on the section it showed last, showing the sidebar first if it is hidden.
fn show_library_page() {
    if win().sidebar_mode.get() == SidebarMode::Hidden {
        set_sidebar(SidebarMode::Shown);
    }
    dispatch(&win().sidebar, "arbalete:open-library", "null");
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

pub fn displayed_profile() -> ProfileId {
    STATE.with_borrow(|state| state.sidebar.displayed().profile)
}

/// The selected tab's address.
fn selected_url() -> Option<String> {
    STATE.with_borrow_mut(|state| active_tab(&mut state.sidebar).map(|tab| tab.url.clone()))
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

fn now() -> i64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    since_epoch.as_millis().try_into().unwrap_or(i64::MAX)
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
        Size, UiState, favicon_file, site, space_icon, win,
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

    #[test]
    fn sends_a_space_site_icon_where_the_ui_loads_it_from() {
        let mut sidebar = arbalete_core::sidebar::Sidebar::default();
        let space = sidebar.spaces()[0].id;
        sidebar.set_space_emoji(space, "💼").unwrap();
        assert_eq!(space_icon(&sidebar.spaces()[0]).as_deref(), Some("💼"));
        let source = "https://a.example/favicon.ico";
        sidebar
            .set_space_emoji(space, &format!("site:{source}"))
            .unwrap();
        let profile = serde_json::to_string(&sidebar.spaces()[0].profile).unwrap();
        assert_eq!(
            space_icon(&sidebar.spaces()[0]),
            Some(format!(
                "site:arbalete://ui/favicon/{profile}/{}",
                arbalete_core::favicon::file_name(source)
            ))
        );
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
    fn keeps_the_last_closed_tabs_dropping_the_oldest() {
        let mut state = UiState::default();
        let profile = state.sidebar.profiles()[0].id;
        let closed = |i: usize| super::Closed {
            id: None,
            tab: arbalete_core::sidebar::Tab {
                url: format!("https://example.com/{i}"),
                title: String::new(),
                favicon: None,
                opener: None,
                saved_url: None,
                name: None,
                muted: false,
                last_shown: None,
            },
            profile,
            at: std::time::UNIX_EPOCH,
            beside: None,
        };
        state.keep_closed((0..20).map(closed));
        state.keep_closed((20..30).map(closed));
        assert_eq!(state.closed.len(), super::MAX_CLOSED);
        assert_eq!(state.closed[0].tab.url, "https://example.com/5");
        assert_eq!(
            state.closed.last().unwrap().tab.url,
            "https://example.com/29"
        );
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
            { "id": home, "title": "Home", "emoji": null, "theme": { "colors": ["#a352ff", "#5289ff", "#ff5252"], "mode": "auto", "intensity": 0.8, "noise": 0.4 }, "profile": profile, "pinned": [] },
            { "id": work, "title": "Work", "emoji": "💼", "theme": theme("#3e7bfa"), "profile": profile, "pinned": [
                { "id": folder, "title": "Docs", "folder": true },
                { "id": docs, "url": "https://b.test/", "title": "B", "favicon": null, "muted": false, "asleep": false, "open": false, "developer": false },
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
}

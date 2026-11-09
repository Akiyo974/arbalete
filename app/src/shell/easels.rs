//! Easels, boards in the sidebar like tabs (decision 0009): a tab at an Easel's address shows its
//! board in a native view instead of a page, and the board's view saves it through the app.

use super::*;
use arbalete_core::easel::{self, Easel};

thread_local! {
    /// The tabs whose board has a view.
    static EASEL_VIEWS: RefCell<HashSet<NodeId>> = RefCell::default();
    /// The live captures' pages, by their Easel's number and their own, with where they were on
    /// their page.
    static LIVE_CAPTURES: RefCell<HashMap<LiveCapture, (Browser, easel::PageView)>> = RefCell::default();
    /// The live captures' pages closing, whose views go once CEF asks.
    static LIVE_CLOSING: RefCell<HashSet<i32>> = RefCell::default();
    /// The Easel's tab Peek shows instead of a page, if any.
    static PEEK_EASEL: Cell<Option<NodeId>> = const { Cell::new(None) };
}

/// A live capture: its Easel's number and its own.
type LiveCapture = (u64, u64);

/// The folder holding every Easel's.
fn easels_dir() -> PathBuf {
    DATA_DIR.get().expect("set").join("Easels")
}

/// The Easel a tab shows, if its address is an Easel's.
pub(super) fn easel_of(tab: NodeId) -> Option<u64> {
    STATE.with_borrow_mut(|state| easel::number(&state.sidebar.tab_mut(tab)?.url))
}

/// A number for a new Easel: none of the open, closed or archived tabs shows it.
fn new_number() -> u64 {
    let used: Vec<u64> = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let open = sidebar.tabs().map(|(_, tab)| tab.url.as_str());
        let archived = sidebar.archive().iter().map(|a| a.tab.url.as_str());
        let closed = state.closed.iter().map(|c| c.tab.url.as_str());
        open.chain(archived)
            .chain(closed)
            .filter_map(easel::number)
            .collect()
    });
    easel::new_number(&easels_dir(), used)
}

/// Like Arc's, until the user names it.
const UNTITLED: &str = "Untitled Easel";

/// Opens a new, empty Easel in the displayed Space's "Today", shown, and asks the sidebar to
/// name it.
pub(super) fn new_easel() {
    open_tab(&easel::url(new_number()));
    let tab = update_sidebar(|sidebar| {
        let tab = sidebar.displayed().selected?;
        sidebar.tab_mut(tab)?.title = UNTITLED.into();
        Some(tab)
    });
    if let Some(tab) = tab {
        ask("arbalete:rename", &json!(tab));
    }
}

/// Adds the capture at `file`, `size` points, of the shown page, to the Easel `number`, or to a
/// new one in the displayed Space's "Today", behind the page; a toast tells, which a click goes
/// to the Easel. Its board, if shown, adds it itself (Easel.swift).
pub(super) fn add_capture_to_easel(
    file: &Path,
    size: (f64, f64),
    number: Option<u64>,
    page: Option<easel::PageView>,
) {
    let (title, url, favicon) = STATE.with_borrow_mut(|state| {
        let profile = state.sidebar.displayed().profile;
        active_tab(&mut state.sidebar)
            .map(|tab| {
                let icon = tab.favicon.as_deref().map(|source| {
                    profile_dir(profile)
                        .join(favicon::DIR)
                        .join(favicon::file_name(source))
                });
                (tab.label().to_string(), tab.url.clone(), icon)
            })
            .unwrap_or_default()
    });
    let number = number.unwrap_or_else(|| {
        let number = new_number();
        update_sidebar(|sidebar| {
            let space = sidebar.displayed().id;
            let tab = sidebar.open_tab(space, &easel::url(number)).ok()?;
            sidebar.tab_mut(tab)?.title = UNTITLED.into();
            Some(())
        });
        number
    });
    let dir = easel::dir(&easels_dir(), number);
    let capture = easel::NewCapture {
        file,
        size,
        url: &url,
        title: &title,
        favicon: favicon.as_deref().filter(|icon| icon.is_file()),
        page,
    };
    let id = match easel::add_capture(&dir, capture) {
        Ok(id) => id,
        Err(error) => return eprintln!("cannot add the capture to Easel {number}: {error:?}"),
    };
    // Like Arc's, the Easel shows in Peek, the capture in it.
    let tab = STATE.with_borrow(|state| {
        let url = easel::url(number);
        state
            .sidebar
            .tabs()
            .find(|(_, tab)| tab.url == url)
            .map(|(id, _)| id)
    });
    if let Some(tab) = tab {
        peek_easel(tab, Some(id));
    }
}

/// Gives a tab at an Easel's address the view of its board, loaded from its file, the first time
/// it shows: until the window can hold it, none.
pub(super) fn easel_view(tab: NodeId, number: u64) -> Option<()> {
    if EASEL_VIEWS.with_borrow(|views| views.contains(&tab)) {
        return Some(());
    }
    let detail = board_detail(number);
    #[cfg(target_os = "macos")]
    if !crate::native::easel(win().id, tab_number(tab), &detail.to_string()) {
        return None;
    }
    EASEL_VIEWS.with_borrow_mut(|views| views.insert(tab));
    Some(())
}

/// What the interface shows of the Easel `number`: its board, loaded from its file, the folder
/// of its pictures and the board's width.
fn board_detail(number: u64) -> serde_json::Value {
    let dir = easel::dir(&easels_dir(), number);
    let board = easel::load(&dir).unwrap_or_else(|error| {
        eprintln!("cannot load Easel {number}: {error:?}");
        Easel::default()
    });
    json!({
        "number": number,
        "images": easel::images(&dir),
        "width": easel::WIDTH,
        "easel": board,
    })
}

/// Shows the Easel of `tab` in Peek, like Arc's once a capture lands in it, scrolled to the
/// object `focus`.
pub(super) fn peek_easel(tab: NodeId, focus: Option<u64>) {
    let Some(number) = easel_of(tab) else { return };
    close_peek();
    let Some(size) = win()
        .window
        .with_borrow(|window| window.as_ref().map(MainWindow::size))
    else {
        return;
    };
    #[cfg(target_os = "macos")]
    {
        let title = STATE.with_borrow_mut(|state| {
            let tab = state.sidebar.tab_mut(tab)?;
            Some(tab.label().to_string())
        });
        crate::native::peek(win().id, Some(peek_frame(&size)));
        let mut detail = board_detail(number);
        detail["focus"] = json!(focus);
        let detail = detail.to_string();
        let title = title.unwrap_or_default();
        if !crate::native::peek_easel(win().id, tab_number(tab), &title, &detail) {
            return crate::native::peek(win().id, None);
        }
    }
    PEEK_EASEL.set(Some(tab));
}

/// Whether Peek shows an Easel.
pub(super) fn easel_peeked() -> bool {
    PEEK_EASEL.get().is_some()
}

/// Closes Peek's Easel, its board given back to its tab: the Easel's tab, if Peek showed one.
pub(super) fn close_peek_easel() -> Option<NodeId> {
    let tab = PEEK_EASEL.take()?;
    #[cfg(target_os = "macos")]
    {
        crate::native::peek(win().id, None);
        crate::native::peek_closed(win().id);
    }
    Some(tab)
}

/// Whether a tab's board has a view.
pub(super) fn has_easel_view(tab: NodeId) -> bool {
    EASEL_VIEWS.with_borrow(|views| views.contains(&tab))
}

/// Takes away the views of the boards whose tab closed, or left the Easel's address.
pub(super) fn prune_easel_views() {
    let gone: Vec<NodeId> = EASEL_VIEWS.with_borrow(|views| {
        views
            .iter()
            .copied()
            .filter(|&tab| easel_of(tab).is_none())
            .collect()
    });
    for tab in gone {
        EASEL_VIEWS.with_borrow_mut(|views| views.remove(&tab));
        #[cfg(target_os = "macos")]
        crate::native::easel_closed(tab_number(tab));
    }
}

pub(super) fn save_easel(number: u64, board: &Easel) {
    if let Err(error) = easel::save(board, &easel::dir(&easels_dir(), number)) {
        eprintln!("cannot save Easel {number}: {error:?}");
    }
}

pub(super) fn add_easel_image(number: u64, file: &Path, name: &str) {
    if let Err(error) = easel::add_image(&easel::dir(&easels_dir(), number), file, name) {
        eprintln!("cannot add a picture to Easel {number}: {error}");
    }
}

/// Deletes the Easels no tab or archived tab shows any more, at launch, before any tab closes.
pub(super) fn sweep_easels() {
    STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let open = sidebar.tabs().map(|(_, tab)| tab.url.as_str());
        let archived = sidebar.archive().iter().map(|a| a.tab.url.as_str());
        easel::sweep(&easels_dir(), open.chain(archived));
    });
}

/// Shows a live capture's page, at `url`, in the profile of its Easel's tab, in the view its board
/// holds for it: muted, laid out at the size of the page's view when captured, drawn `scale`
/// times its size, and scrolled there once loaded (live_capture_loaded).
pub(super) fn show_live_capture(
    capture: LiveCapture,
    url: &str,
    page: easel::PageView,
    scale: f64,
) {
    if LIVE_CAPTURES.with_borrow(|lives| lives.contains_key(&capture)) {
        return;
    }
    let (number, id) = capture;
    let profile = STATE.with_borrow(|state| {
        let url = easel::url(number);
        let sidebar = &state.sidebar;
        let tab = sidebar
            .tabs()
            .find(|(_, tab)| tab.url == url)
            .map(|(id, _)| id);
        tab.and_then(|tab| sidebar.profile_of(tab))
            .unwrap_or(sidebar.displayed().profile)
    });
    #[cfg(target_os = "macos")]
    let info = WindowInfo {
        parent_view: match crate::native::live_capture_view(number as i64, id as i64) {
            Some(view) => view,
            None => return,
        },
        // Any size: the board gives the page the size it was captured at.
        bounds: Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        },
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    };
    // Once the profile's web data is ready, like a tab's page.
    let mut context = match CONTEXTS.with_borrow(|contexts| contexts.get(&profile).cloned()) {
        Some(Some(context)) => context,
        ready => {
            if ready.is_none() {
                create_profile_context(profile);
            }
            let page = serde_json::to_string(&page).unwrap_or_default();
            let mut retry = RetryLiveCapture::new(number, id, url.to_string(), page, scale);
            post_delayed_task(ThreadId::UI, Some(&mut retry), 300);
            return;
        }
    };
    let Some(browser) = browser_host_create_browser_sync(
        Some(&info),
        Some(&mut ContentClient::new(profile, true, None)),
        Some(&CefString::from(url)),
        Some(&BrowserSettings::default()),
        None,
        Some(&mut context),
    ) else {
        return;
    };
    if let Some(host) = browser.host() {
        host.set_audio_muted(1);
    }
    emulate(&browser, &page, scale);
    LIVE_CAPTURES.with_borrow_mut(|lives| lives.insert(capture, (browser, page)));
}

/// Lays a live capture's page out at the size of the page's view when captured, and draws it
/// `scale` times its size, like the developer tools' device mode: for its page only, unlike a
/// zoom, which the site's other pages would take.
fn emulate(browser: &Browser, page: &easel::PageView, scale: f64) {
    let Some(host) = browser.host() else { return };
    let params = json!({
        "width": page.width.round(), "height": page.height.round(), "deviceScaleFactor": 0,
        "mobile": false, "scale": scale,
    });
    let message =
        json!({ "id": 1, "method": "Emulation.setDeviceMetricsOverride", "params": params });
    host.send_dev_tools_message(Some(message.to_string().as_bytes()));
}

pub(super) fn scale_live_capture(capture: LiveCapture, scale: f64) {
    let live = LIVE_CAPTURES.with_borrow(|lives| lives.get(&capture).cloned());
    if let Some((browser, page)) = live {
        emulate(&browser, &page, scale);
    }
}

/// Whether `browser` is a live capture's page.
pub(super) fn is_live_capture(browser: &Browser) -> bool {
    LIVE_CAPTURES.with_borrow(|lives| {
        lives
            .values()
            .any(|(live, _)| live.identifier() == browser.identifier())
    })
}

/// Scrolls a live capture's page, loaded, to where it was captured, and again a little later, as
/// pages often grow while their pictures come.
pub(super) fn live_capture_loaded(browser: &Browser) {
    let scroll = LIVE_CAPTURES.with_borrow(|lives| {
        lives
            .values()
            .find(|(live, _)| live.identifier() == browser.identifier())
            .map(|(_, page)| (page.scroll_x, page.scroll_y))
    });
    let (Some((x, y)), Some(frame)) = (scroll, browser.main_frame()) else {
        return;
    };
    let script = format!(
        "scrollTo({x}, {y}); for (const t of [500, 1500, 4000]) setTimeout(() => scrollTo({x}, {y}), t);"
    );
    frame.execute_java_script(Some(&CefString::from(script.as_str())), None, 0);
}

pub(super) fn close_live_capture(capture: LiveCapture) {
    let Some((browser, _)) = LIVE_CAPTURES.with_borrow_mut(|lives| lives.remove(&capture)) else {
        return;
    };
    LIVE_CLOSING.with_borrow_mut(|closing| closing.insert(browser.identifier()));
    if let Some(host) = browser.host() {
        host.close_browser(1);
    }
}

/// Whether CEF closes a live capture's page, whose view then goes (RemoveLiveCapture).
pub(super) fn live_capture_closing(browser: &Browser) -> bool {
    LIVE_CLOSING.with_borrow_mut(|closing| closing.remove(&browser.identifier()))
}

wrap_task! {
    struct RetryLiveCapture {
        number: u64,
        id: u64,
        url: String,
        page: String,
        scale: f64,
    }

    impl Task {
        /// Shows a live capture once its profile's web data is ready, while its board holds a
        /// view for it.
        fn execute(&self) {
            if let Ok(page) = serde_json::from_str(&self.page) {
                show_live_capture((self.number, self.id), &self.url, page, self.scale);
            }
        }
    }
}

wrap_task! {
    pub(super) struct RemoveLiveCapture {
        view: usize,
    }

    impl Task {
        /// Takes a closing live capture's page's view out of the board, which destroys its
        /// browser.
        fn execute(&self) {
            #[cfg(target_os = "macos")]
            crate::native::live_capture_closed(self.view as *mut std::ffi::c_void);
        }
    }
}

//! The app's windows: opening them, incognito ones, closing them, and quitting.

use super::*;

/// Opens another window, offset from the focused one, on its Space without a tab, on the Command
/// Bar, like Arc's.
pub(super) fn new_window() {
    open_window(
        STATE.with_borrow(|state| state.sidebar.new_window_view()),
        true,
    );
}

/// Opens an incognito window, like Arc's: on a Space of its own, in a profile of its own whose
/// web data stays in memory, on the Command Bar.
pub(super) fn new_incognito_window() {
    open_incognito_window(None);
}

/// Opens an incognito window as `new_incognito_window` does, on a tab showing `url` if any.
pub(super) fn open_incognito_window(url: Option<&str>) {
    let space = update_sidebar(Sidebar::open_incognito);
    let tab = url
        .map(|url| update_sidebar(|sidebar| sidebar.open_tab(space, url).expect("the new Space")));
    let (profile, view) = STATE.with_borrow(|state| {
        let profile = state.sidebar.profile_of_space(space).expect("its Space");
        let view = match tab {
            Some(tab) => state.sidebar.view_on(space, tab),
            None => state.sidebar.view_showing(space),
        };
        (profile, view)
    });
    let dir = std::env::temp_dir().join(format!(
        "arbalete-incognito-{}-{}",
        std::process::id(),
        serde_json::to_string(&profile).expect("an id")
    ));
    INCOGNITO.with_borrow_mut(|incognito| incognito.insert(profile, dir));
    open_window_in(view, tab.is_none(), Some(space));
}

/// Opens another window on a new tab of the displayed Space showing `url`, like Arc's Open Link
/// in New Window.
pub(super) fn open_link_in_new_window(url: &str) {
    let view = update_sidebar(|sidebar| {
        let space = sidebar.displayed().id;
        let tab = sidebar.open_tab(space, url).expect("the displayed Space");
        sidebar.view_on(space, tab)
    });
    open_window(view, false);
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
    // Its downloads leave the list, never saved; their files stay.
    DOWNLOADS.with_borrow_mut(|list| {
        list.retain(|download| download.profile != Some(profile));
        DOWNLOAD_CALLBACKS.with_borrow_mut(|callbacks| {
            callbacks.retain(|id, _| list.iter().any(|download| download.id == *id));
        });
    });
    send_downloads();
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

/// Closes every window, which quits the app once its browsers are closed. The last window's own
/// close button does the same on every OS.
pub fn close() {
    close_settings();
    let littles: Vec<_> = LITTLE.with_borrow(|littles| littles.iter().map(|l| l.id).collect());
    for id in littles {
        close_little(id);
    }
    // The developer tools in a window of their own, Chromium's own pages and the browsers
    // Chromium opened by itself, which would keep the app from quitting.
    let inspectors: Vec<_> =
        INSPECTORS.with_borrow(|inspectors| inspectors.keys().copied().collect());
    for tab in inspectors {
        close_inspector(tab);
    }
    if let Some((window, _, _)) = CHROME_PAGE.with_borrow(Clone::clone) {
        window.close();
    }
    for host in STRAYS
        .with_borrow(Clone::clone)
        .iter()
        .filter_map(|browser| browser.host())
    {
        host.close_browser(1);
    }
    if !CLOSING.get() {
        // Written while the sidebar still shows the focused window's view, which closing changes.
        save_pending();
        CLOSING.set(true);
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
            #[cfg(target_os = "macos")]
            if crate::mac::mock_keychain() {
                command_line.append_switch(Some(&CefString::from("use-mock-keychain")));
            }
            // Sites asking to sign in reach the sign-in prompt (auth_credentials): Chrome's own
            // prompt has no tab to show in.
            command_line.append_switch(Some(&CefString::from("disable-chrome-login-prompt")));
            // The helper extension, in every profile, tells extensions which tab is which.
            if let Some(dir) = extensions::helper_dir() {
                command_line.append_switch_with_value(
                    Some(&CefString::from("load-extension")),
                    Some(&CefString::from(dir.to_string_lossy().as_ref())),
                );
            }
            // Opens DevTools to the editor's debug launch (.vscode/launch.json). Development builds
            // only: the port gives full control of every profile to anything on this machine.
            if cfg!(debug_assertions)
                && let Ok(port) = std::env::var("ARBALETE_DEBUG_PORT")
            {
                command_line.append_switch_with_value(
                    Some(&CefString::from("remote-debugging-port")),
                    Some(&CefString::from(port.as_str())),
                );
            }
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
            apply_little_hotkey();
            apply_shortcuts();
            restore(self.data_dir.join(store::FILE_NAME));
            check_archive();
            DOWNLOADS.set(downloads::load(&self.data_dir.join(downloads::FILE_NAME)));
            make_bridge();
            #[cfg(target_os = "macos")]
            open_main_window(0);
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
    // Leaving the window's full screen, as by its green button, the page leaves its own.
    #[cfg(target_os = "macos")]
    if page_full_screen()
        && let Some(window) = win()
            .window
            .with_borrow(|w| w.as_ref().map(|w| w.window_handle()))
        && !crate::mac::full_screen(window)
    {
        leave_page_full_screen();
    }
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
    // Its tab is the active one for the extensions again.
    if active && ACTIVE_LITTLE.take().is_some() {
        send_state();
    }
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
    if !CLOSING.get() {
        save_pending();
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

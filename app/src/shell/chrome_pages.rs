//! Chromium's own pages and windows: its settings and the store in a window, the windows it
//! asks the user in, the browsers it opens by itself, and the pages drawn nowhere.

use super::*;

/// Clears what the shown page's site keeps, through Chromium's DevTools protocol, then reloads the
/// page: its cache storage and service workers, loading it again from the network, or its
/// cookies.
pub(super) fn clear_site_data(browser: &Browser, data: SiteData) {
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

thread_local! {
    /// The window showing Chromium's own pages, and its page, while it is open.
    pub(super) static CHROME_PAGE: RefCell<Option<(Window, BrowserView, ProfileId)>> = const { RefCell::new(None) };
}

/// Whether `url` is one of Chromium's own pages, like chrome://extensions, which Alloy pages do
/// not show.
pub(super) fn is_chrome_page(url: &str) -> bool {
    url.starts_with("chrome://")
}

/// Whether the window of Chromium's own pages is the one in front.
pub(super) fn chrome_page_focused() -> bool {
    CHROME_PAGE.with_borrow(|page| {
        page.as_ref()
            .is_some_and(|(window, _, _)| window.is_active() == 1)
    })
}

/// Shows one of Chromium's own pages, like a site's settings or the extensions, in a Chrome-style
/// window of its own, in the displayed Space's profile: Alloy pages have none, and macOS has no
/// Chrome style in a native view.
pub(super) fn open_chrome_page(url: &str) {
    // An incognito window's profile, whose web data stays in memory, has no Chrome-style page:
    // Chromium's own pages show the first profile's, as an incognito window keeps no history.
    let profile = Some(displayed_profile())
        .filter(|profile| INCOGNITO.with_borrow(|incognito| !incognito.contains_key(profile)))
        .or_else(|| STATE.with_borrow(|state| state.sidebar.profiles().first().map(|p| p.id)))
        .unwrap_or_else(displayed_profile);
    open_profile_page(url, profile);
}

/// Shows one of Chromium's own pages in `profile`: the window open loads it if it shows that
/// profile's pages, else it closes for one of that profile.
pub(super) fn open_profile_page(url: &str, profile: ProfileId) {
    hide_site_controls();
    show_overlay(false);
    if let Some((window, view, shown)) = CHROME_PAGE.with_borrow(Clone::clone) {
        if shown != profile {
            window.close();
        } else if let Some(frame) = view.browser().and_then(|browser| browser.main_frame()) {
            frame.load_url(Some(&CefString::from(url)));
            window.activate();
            return;
        }
    }
    window_create_top_level(Some(&mut ChromePageWindowDelegate::new(
        profile,
        url.to_string(),
    )));
}

/// Shows the selected tab's site's settings, like Arc's All Site Settings…: Chromium's own page
/// for the site.
pub(super) fn show_site_settings() {
    let Some(origin) = selected_url().and_then(|url| origin(&url).map(str::to_string)) else {
        return;
    };
    // The origin as a query's value: its ":" and "/" encoded, like Chrome's own links to it.
    let site = origin.replace(':', "%3A").replace('/', "%2F");
    open_chrome_page(&format!(
        "chrome://settings/content/siteDetails?site={site}"
    ));
}

/// A web page's origin: its scheme, host and port.
pub(super) fn origin(url: &str) -> Option<&str> {
    let (scheme, _) = url.split_once("://")?;
    Some(&url[..scheme.len() + 3 + site(url).len()])
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

/// The displayed Space's profile.
/// The extensions' bridge's browser, which outlives every page and window: their DevTools
/// session runs in it.
pub fn bridge_host() -> Option<BrowserHost> {
    BRIDGE.with_borrow(|bridge| bridge.as_ref()?.host())
}

/// Makes the extensions' bridge's browser: a blank page without a window, the only one the app
/// keeps for itself, as DevTools sessions run in a browser.
pub(super) fn make_bridge() {
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

/// The size of the window showing Chromium's own pages.
const CHROME_PAGE_SIZE: Size = Size {
    width: 820,
    height: 720,
};

wrap_window_delegate! {
    struct ChromePageWindowDelegate {
        profile: ProfileId,
        url: String,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            CHROME_PAGE_SIZE
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let window = window.expect("window").clone();
            window.set_title(Some(&CefString::from("Chromium")));
            window.set_to_fill_layout();
            let view = browser_view_create(
                Some(&mut ChromePageClient::new()),
                Some(&CefString::from(self.url.as_str())),
                Some(&BrowserSettings::default()),
                None,
                profile_context(self.profile).as_mut(),
                Some(&mut ChromeViewDelegate::new()),
            )
            .expect("Chromium page view");
            window.add_child_view(Some(&mut View::from(&view)));
            window.center_window(Some(&CHROME_PAGE_SIZE));
            window.show();
            CHROME_PAGE.set(Some((window, view, self.profile)));
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            // Unless one of another profile's took its place already.
            CHROME_PAGE.with_borrow_mut(|page| {
                if page.as_ref().is_some_and(|(_, _, profile)| *profile == self.profile) {
                    *page = None;
                }
            });
            // What the extensions page changed shows.
            extensions::reload(self.profile);
        }

        fn can_close(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn can_resize(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn can_maximize(&self, _window: Option<&mut Window>) -> i32 { 1 }
        fn can_minimize(&self, _window: Option<&mut Window>) -> i32 { 1 }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::CHROME
        }
    }
}

// The page of the window showing Chromium's own pages.
wrap_client! {
    struct ChromePageClient {}

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(BrowserLifeSpan::new(None))
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
pub(super) fn show_store() {
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

// The browsers Chromium opens by itself, outside the app's windows: after an install, a full
// Chromium window to say the extension was added. Extensions would take its tab for the active
// one: it closes once the extension is installed. Closed sooner, while Chromium
// shows its bubble in it, it takes the app down.
wrap_client! {
    pub(super) struct StrayClient {}

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

#[cfg(test)]
mod tests {
    use super::*;

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
}

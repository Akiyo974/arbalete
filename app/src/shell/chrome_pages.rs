//! Chromium's own pages and windows: its settings and others in tabs, shown as arbalete://, the
//! rest in a window, the windows it asks the user in, the browsers it opens by itself, and the
//! pages drawn nowhere.

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

/// Chromium's own pages that open in a tab, like any page, rather than in a window of their own:
/// those the CEF Arbalete takes lets Alloy pages show (cef-builds' alloy-webui patch, which keeps
/// what their buttons do from crashing out of a Chromium window), and the internals CEF lets them
/// show already. The others, like chrome://bookmarks, need a Chromium window and crash in a tab.
const TAB_PAGES: [&str; 26] = [
    "settings",
    "extensions",
    "history",
    "downloads",
    "password-manager",
    "flags",
    "management",
    "components",
    "inspect",
    "about",
    "chrome-urls",
    "version",
    "credits",
    "gpu",
    "net-internals",
    "net-export",
    "accessibility",
    "histograms",
    "media-internals",
    "process-internals",
    "serviceworker-internals",
    "webrtc-internals",
    "indexeddb-internals",
    "blob-internals",
    "tracing",
    "system",
];

/// The scheme the user sees Chromium's pages that open in a tab under, as Arbalete's own.
const SHOWN_SCHEME: &str = "arbalete://";

/// Whether `url` is one of Chromium's own pages that open in a tab.
pub(super) fn is_tab_page(url: &str) -> bool {
    page_host(url, "chrome://").is_some()
}

/// The host of one of Chromium's pages that open in a tab, at `url` under `scheme`.
fn page_host<'a>(url: &'a str, scheme: &str) -> Option<&'a str> {
    let rest = url
        .get(..scheme.len())
        .filter(|start| start.eq_ignore_ascii_case(scheme))
        .map(|_| &url[scheme.len()..])?;
    let host = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    TAB_PAGES
        .iter()
        .any(|page| page.eq_ignore_ascii_case(host))
        .then_some(host)
}

/// The address the user sees for `url`: arbalete://settings for chrome://settings, a page that
/// opens in a tab; any other as it is. Only what shows changes: the session and the history
/// keep Chromium's.
pub(super) fn shown_url(url: &str) -> String {
    match page_host(url, "chrome://") {
        Some(_) => format!("{SHOWN_SCHEME}{}", &url["chrome://".len()..]),
        None => url.to_string(),
    }
}

/// The address to load for `url`, typed or opened by the user: chrome://settings for
/// arbalete://settings (shown_url); any other as it is.
pub(super) fn url_to_load(url: &str) -> String {
    match page_host(url, SHOWN_SCHEME) {
        Some(_) => format!("chrome://{}", &url[SHOWN_SCHEME.len()..]),
        None => url.to_string(),
    }
}

/// Whether a page at `page` may send the app to `url`, in a tab, Peek or a window: never to the
/// app's pages, DevTools or a page's source, and to Chromium's own pages only from one of them
/// that opens in a tab, like chrome://settings to chrome://extensions. The app's loads reach
/// every chrome:// page in the CEF it takes, which refuses them to web pages only. Spaces and
/// controls inside a scheme are dropped first, like Chromium does.
pub(super) fn page_may_open(page: &str, url: &str) -> bool {
    let url: String = url
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && !c.is_ascii_control())
        .collect();
    let scheme = url
        .split_once(':')
        .map(|(scheme, _)| scheme.to_ascii_lowercase());
    match scheme.as_deref() {
        Some("chrome") => is_tab_page(page),
        Some("arbalete" | "devtools" | "view-source") => false,
        _ => true,
    }
}

/// Whether `url` is one of Chromium's own pages that opens in its window rather than in a tab:
/// one that tabs do not show, or any of them in an incognito window, whose profile has none.
pub(super) fn is_chrome_page(url: &str) -> bool {
    let incognito = INCOGNITO.with_borrow(|incognito| incognito.contains_key(&displayed_profile()));
    url.starts_with("chrome://") && (incognito || !is_tab_page(url))
}

/// A tab's page leaves its address, hidden or gone elsewhere: when it was the extensions page,
/// what it changed shows, as Chromium tells nothing of it.
pub(super) fn left_page(profile: ProfileId, tab: NodeId) {
    if tab_has(tab, |tab| tab.url.starts_with("chrome://extensions")) {
        extensions::reload(profile);
    }
}

/// Whether the window of Chromium's own pages is the one in front.
pub(super) fn chrome_page_focused() -> bool {
    CHROME_PAGE.with_borrow(|page| {
        page.as_ref()
            .is_some_and(|(window, _, _)| window.is_active() == 1)
    })
}

/// Shows one of Chromium's own pages that tabs do not show, like its bookmarks, in a Chrome-style
/// window of its own, in the displayed Space's profile: macOS has no Chrome style in a native
/// view.
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
    open_tab(&format!(
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
            Some(BrowserLifeSpan::new(None, None, None))
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
            Some(BrowserLifeSpan::new(None, None, None))
        }
    }
}

// The permission window's page: Chromium is asked once it has loaded.
wrap_client! {
    struct PermissionClient {}

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(BrowserLifeSpan::new(None, None, None))
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

/// Keeps the windows Chromium opens by itself invisible while extensions are added from the
/// Web Store, one after the other: each extension's welcome page, which opens in a tab instead.
/// Once the last is added, they close a moment later, after the extensions that open them late.
pub fn hide_strays(hide: bool) {
    if hide {
        HIDE_STRAYS.set(true);
        for host in STRAYS
            .with_borrow(Clone::clone)
            .iter()
            .filter_map(Browser::host)
        {
            #[cfg(target_os = "macos")]
            crate::mac::hide_window(host.window_handle());
        }
    } else {
        post_delayed_task(ThreadId::UI, Some(&mut CloseStrays::new()), 2000);
    }
}

wrap_task! {
    struct HideStray {
        browser: Browser,
    }

    impl Task {
        fn execute(&self) {
            #[cfg(target_os = "macos")]
            if let Some(host) = self.browser.host().filter(|_| HIDE_STRAYS.get()) {
                crate::mac::hide_window(host.window_handle());
            }
        }
    }
}

wrap_task! {
    struct CloseStrays {}

    impl Task {
        fn execute(&self) {
            HIDE_STRAYS.set(crate::extensions::installing_from_store());
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
                // Its window only comes a moment later: hidden then, and again after.
                if HIDE_STRAYS.get() {
                    for delay in [0, 50, 150, 400, 1000] {
                        post_delayed_task(ThreadId::UI, Some(&mut HideStray::new(browser.clone())), delay);
                    }
                }
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
            Some(BrowserLifeSpan::new(None, None, None))
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

    #[test]
    fn shows_chromium_pages_that_open_in_a_tab_as_arbalete_s() {
        assert_eq!(shown_url("chrome://settings"), "arbalete://settings");
        assert_eq!(
            shown_url("chrome://settings/content/siteDetails?site=https%3A%2F%2Fa.test"),
            "arbalete://settings/content/siteDetails?site=https%3A%2F%2Fa.test"
        );
        assert_eq!(
            shown_url("chrome://extensions/?id=a"),
            "arbalete://extensions/?id=a"
        );
        assert_eq!(shown_url("chrome://bookmarks/"), "chrome://bookmarks/");
        assert_eq!(shown_url("chrome://settingsx/"), "chrome://settingsx/");
        assert_eq!(
            shown_url("https://settings.test/"),
            "https://settings.test/"
        );
        assert_eq!(shown_url("arbalete://ui/"), "arbalete://ui/");
    }

    #[test]
    fn loads_arbalete_addresses_as_chromium_s() {
        assert_eq!(url_to_load("arbalete://settings"), "chrome://settings");
        assert_eq!(
            url_to_load("Arbalete://history/?q=a"),
            "chrome://history/?q=a"
        );
        assert_eq!(url_to_load("arbalete://bookmarks"), "arbalete://bookmarks");
        assert_eq!(url_to_load("arbalete://ui/"), "arbalete://ui/");
        assert_eq!(url_to_load("https://a.test/"), "https://a.test/");
        for url in [
            "chrome://settings/help",
            "chrome://password-manager/passwords",
            "chrome://flags/#a",
        ] {
            assert_eq!(url_to_load(&shown_url(url)), url);
        }
    }

    #[test]
    fn opens_only_some_chromium_pages_in_a_tab() {
        assert!(is_tab_page("chrome://settings/privacy"));
        assert!(is_tab_page("chrome://downloads"));
        assert!(!is_tab_page("chrome://bookmarks"));
        assert!(!is_tab_page("chrome://newtab/"));
        assert!(!is_tab_page("arbalete://settings"));
    }

    #[test]
    fn web_pages_never_open_internal_pages() {
        let web = "https://a.test/";
        assert!(page_may_open(web, "https://b.test/"));
        assert!(page_may_open(web, "chrome-extension://abc/welcome.html"));
        assert!(page_may_open(web, "about:blank"));
        assert!(page_may_open("chrome://settings/", "chrome://extensions/"));
        assert!(page_may_open("chrome://settings/", "https://a.test/"));
        assert!(!page_may_open("chrome://bookmarks/", "chrome://settings/"));
        assert!(!page_may_open("chrome://settings/", "arbalete://ui/"));
        for url in [
            "chrome://settings",
            "CHROME://settings",
            " chrome://settings",
            "chr\tome://settings",
            "chrome:settings",
            "arbalete://settings",
            "arbalete://ui/",
            "devtools://devtools/bundled/inspector.html",
            "view-source:chrome://settings",
        ] {
            assert!(!page_may_open(web, url), "{url}");
        }
    }
}

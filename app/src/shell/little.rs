//! Little Arbalete windows, like Arc's Little Arc: a page without the sidebar, which links
//! from other apps open in.

use super::*;

/// A Little Arbalete window: its page, in the profile it was opened in, and what its bar shows of
/// the page.
pub(super) struct Little {
    pub(super) id: u32,
    pub(super) profile: ProfileId,
    /// Its page, in a view of the native window.
    #[cfg(target_os = "macos")]
    browser: Browser,
    pub(super) url: String,
    pub(super) title: String,
}

impl Little {
    /// Its page's browser, once made.
    pub(super) fn browser(&self) -> Option<Browser> {
        #[cfg(target_os = "macos")]
        return Some(self.browser.clone());
    }
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
pub(super) fn open_little(url: Option<String>) {
    let id = NEXT_LITTLE.get();
    NEXT_LITTLE.set(id + 1);
    let profile = STATE.with_borrow(|state| state.sidebar.displayed().profile);
    let url = url.unwrap_or_default();
    #[cfg(target_os = "macos")]
    {
        let Some(view) = crate::native::open_little(id) else {
            return;
        };
        let Some(browser) = little_page(view, profile, &url) else {
            return crate::native::little_closed(id);
        };
        LITTLE.with_borrow_mut(|littles| {
            littles.push(Little {
                id,
                profile,
                browser,
                url,
                title: String::new(),
            })
        });
        send_little(id);
    }
}

/// Makes a Little Arbalete window's page, in `profile`, on `url`, in its window's `view`, with
/// the keyboard.
#[cfg(target_os = "macos")]
fn little_page(view: *mut std::ffi::c_void, profile: ProfileId, url: &str) -> Option<Browser> {
    let info = WindowInfo {
        parent_view: view,
        // Any size: the window gives the page its own.
        bounds: Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        },
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    };
    let start = if url.is_empty() { "about:blank" } else { url };
    let browser = browser_host_create_browser_sync(
        Some(&info),
        Some(&mut ContentClient::new(profile, true)),
        Some(&CefString::from(start)),
        Some(&BrowserSettings::default()),
        None,
        profile_context(profile).as_mut(),
    )?;
    if let Some(host) = browser.host() {
        host.set_focus(1);
    }
    // The extensions learn it, as they learn the tabs' pages.
    extensions::page_created(profile, &browser);
    Some(browser)
}

/// A Little Arbalete window's page's browser.
fn little_browser(id: u32) -> Option<Browser> {
    LITTLE.with_borrow(|littles| littles.iter().find(|l| l.id == id)?.browser())
}

/// Closes a Little Arbalete window, its page first, which may ask to stay: on macOS, the page
/// closing closes the window (RemoveLittle).
pub(super) fn close_little(id: u32) {
    #[cfg(target_os = "macos")]
    if let Some(host) = little_browser(id).and_then(|browser| browser.host()) {
        host.close_browser(0);
    }
}

/// Whether the Little Arbalete window `id` may close now: not while its page is open, which it
/// closes first.
#[cfg(target_os = "macos")]
pub fn little_can_close(id: u32) -> bool {
    if LITTLE.with_borrow(|littles| littles.iter().any(|l| l.id == id)) {
        close_little(id);
        return false;
    }
    true
}

/// The Little Arbalete window whose page is `browser`, if any.
#[cfg(target_os = "macos")]
pub(super) fn little_of(browser: &Browser) -> Option<u32> {
    LITTLE.with_borrow(|littles| {
        littles
            .iter()
            .find(|l| browser.is_same(Some(&mut l.browser.clone())) == 1)
            .map(|l| l.id)
    })
}

/// Runs what a Little Arbalete window's bar asks for.
pub(super) fn little_action(id: u32, action: LittleAction) {
    match action {
        LittleAction::Ready => send_little(id),
        LittleAction::Focused => {
            if ACTIVE_LITTLE.replace(Some(id)) != Some(id) {
                send_state();
            }
        }
        LittleAction::Navigate(url) => {
            if let Some(frame) = little_browser(id).and_then(|browser| browser.main_frame()) {
                frame.load_url(Some(&CefString::from(url_to_load(&url).as_str())));
            }
        }
        LittleAction::OpenInSpace(space) => {
            let url = LITTLE
                .with_borrow(|littles| Some(littles.iter().find(|l| l.id == id)?.url.clone()));
            let Some(url) = url.filter(|url| !url.is_empty()) else {
                return;
            };
            open_in_window(space, &url);
            close_little(id);
        }
        LittleAction::SetProfile(profile) => set_little_profile(id, profile),
    }
}

/// Opens a Little Arbalete window's page again in `profile`, at the same address, like Little
/// Arc's profile menu: the new page takes the old one's place, which closes.
fn set_little_profile(id: u32, profile: ProfileId) {
    #[cfg(target_os = "macos")]
    {
        let found = LITTLE.with_borrow(|littles| {
            let little = littles
                .iter()
                .find(|l| l.id == id && l.profile != profile)?;
            Some((little.browser.clone(), little.url.clone()))
        });
        let (Some((old, url)), Some(view)) = (found, crate::native::little_page(id)) else {
            return;
        };
        let Some(browser) = little_page(view, profile, &url) else {
            return;
        };
        LITTLE.with_borrow_mut(|littles| {
            if let Some(little) = littles.iter_mut().find(|l| l.id == id) {
                little.browser = browser;
                little.profile = profile;
            }
        });
        REPLACED_LITTLE_PAGES.with_borrow_mut(|pages| pages.push(old.identifier()));
        extensions::page_closed(old.identifier());
        if let Some(host) = old.host() {
            host.close_browser(1);
        }
        send_little(id);
    }
}

/// Changes what a Little Arbalete window's bar shows of its page, if `browser` is one's page.
pub(super) fn little_changed(browser: &mut Browser, change: impl FnOnce(&mut Little)) {
    let id = LITTLE.with_borrow_mut(|littles| {
        let little = littles.iter_mut().find(|little| {
            little
                .browser()
                .is_some_and(|mut page| browser.is_same(Some(&mut page)) == 1)
        })?;
        change(little);
        Some(little.id)
    });
    if let Some(id) = id {
        send_little(id);
        // The extensions see its page's address and title as the active tab's.
        if ACTIVE_LITTLE.get() == Some(id) {
            send_state();
        }
    }
}

/// Sends a Little Arbalete window's bar its page's address and title, its profile's name and
/// search engine, and the Space its page would move into.
fn send_little(id: u32) {
    let found = LITTLE.with_borrow(|littles| {
        let little = littles.iter().find(|l| l.id == id)?;
        let (space, engine, name, spaces, profiles) = STATE.with_borrow(|state| {
            let sidebar = &state.sidebar;
            let space = sidebar.displayed();
            let profile = sidebar.profiles().iter().find(|p| p.id == little.profile);
            let engine = profile.map(|p| p.search_engine).unwrap_or_default();
            let name = profile.map(|p| p.name.clone()).unwrap_or_default();
            // What its chevron moves its page into, and its profile's menu offers.
            let spaces: Vec<_> = sidebar
                .spaces()
                .iter()
                .map(|s| json!({ "id": s.id, "title": s.title, "emoji": s.emoji }))
                .collect();
            let profiles: Vec<_> = sidebar
                .profiles()
                .iter()
                .map(|p| json!({ "id": p.id, "name": p.name, "current": p.id == little.profile }))
                .collect();
            (
                json!({ "title": space.title, "emoji": space.emoji }),
                engine,
                name,
                spaces,
                profiles,
            )
        });
        let search = json!({ "name": engine.name(), "url": engine.url() });
        let detail = json!({
            "url": shown_url(&little.url),
            "title": little.title,
            "profile": name,
            "profiles": profiles,
            "space": space,
            "spaces": spaces,
            "search": search,
        });
        Some(detail)
    });
    if let Some(detail) = found {
        #[cfg(target_os = "macos")]
        crate::native::event(
            win().id,
            &format!("little/{id}"),
            "arbalete:little",
            &detail.to_string(),
        );
    }
}

/// The Little Arbalete window that has the keyboard, if any: ⌘W closes it, ⌘O moves its page.
pub(super) fn focused_little() -> Option<u32> {
    #[cfg(target_os = "macos")]
    return crate::native::focused_little();
}

#[cfg(target_os = "macos")]
wrap_task! {
    pub(super) struct RemoveLittlePage {
        view: usize,
    }

    impl Task {
        /// Takes a Little Arbalete window's page out of it, another profile's having taken its
        /// place, which destroys its browser.
        fn execute(&self) {
            crate::native::little_page_closed(self.view as *mut std::ffi::c_void);
        }
    }
}

#[cfg(target_os = "macos")]
wrap_task! {
    pub(super) struct RemoveLittle {
        id: u32,
    }

    impl Task {
        /// Closes a Little Arbalete window, its page closing, which destroys the page's browser.
        fn execute(&self) {
            let page = LITTLE.with_borrow_mut(|littles| {
                let at = littles.iter().position(|little| little.id == self.id)?;
                Some(littles.remove(at).browser.identifier())
            });
            if let Some(page) = page {
                extensions::page_closed(page);
            }
            crate::native::little_closed(self.id);
            if ACTIVE_LITTLE.get() == Some(self.id) {
                ACTIVE_LITTLE.set(None);
                send_state();
            }
        }
    }
}

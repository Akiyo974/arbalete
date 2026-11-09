//! The prompt asking the user before a site uses the camera, the microphone or the like.

use super::*;

/// A site's request for permissions, waiting for the user's answer in the permission prompt.
pub(super) struct PermissionRequest {
    /// What the prompt's answer names it by.
    id: u64,
    browser: Browser,
    /// The window in front as it came, which shows its prompt unless the page is a tab's.
    window: u32,
    /// The origin asking, like `https://meet.google.com/`, and its site, `meet.google.com`.
    origin: String,
    site: String,
    asks: Vec<&'static permissions::Capability>,
    callback: PermissionCallback,
}

/// What learns the user's answer to a permission request.
enum PermissionCallback {
    /// For the camera, the microphone or the screen: the media types asked for.
    Media(MediaAccessCallback, u32),
    /// For anything else, Chromium's prompt by its id, and the permissions it asks for.
    Prompt(u64, u32, PermissionPromptCallback),
}

thread_local! {
    /// The origin and the permissions of the prompt being answered, while Chromium learns the
    /// answer.
    static ANSWERING: RefCell<Option<(String, u32)>> = const { RefCell::new(None) };
}

// Asks the user before a site uses the camera, the microphone, the screen or anything else
// Chromium guards, like Arc: CEF denies them all otherwise.
wrap_permission_handler! {
    pub(super) struct ContentPermissions {}

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> i32 {
            let (Some(browser), Some(callback)) = (browser, callback) else { return 0 };
            let origin = requesting_origin.map(CefString::to_string).unwrap_or_default();
            let asks = permissions::media(requested_permissions);
            if asks.is_empty() {
                return 0;
            }
            // Chromium keeps no decision for media requests: the profile's content settings do,
            // where the page's permission queries read it too.
            if let Some(allow) = kept_decision(browser, &origin, &asks) {
                callback.cont(if allow { requested_permissions } else { 0 });
                return 1;
            }
            let callback = PermissionCallback::Media(callback.clone(), requested_permissions);
            ask_permission(browser, &origin, asks, callback);
            1
        }

        fn on_show_permission_prompt(
            &self,
            browser: Option<&mut Browser>,
            prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> i32 {
            let (Some(browser), Some(callback)) = (browser, callback) else { return 0 };
            let origin = requesting_origin.map(CefString::to_string).unwrap_or_default();
            let asks = permissions::prompted(requested_permissions);
            if asks.is_empty() {
                return 0;
            }
            // Once allowed, a page's permission button, like Meet's for the camera and the
            // microphone, shows Chromium's next screen for the same request, like the one waiting
            // for macOS's own prompt: the request is decided, and accepting it again would crash.
            let follows = ANSWERING.with_borrow(|answering| {
                answering.as_ref().is_some_and(|(answered, permissions)| {
                    *answered == origin && requested_permissions & !permissions == 0
                })
            });
            if follows {
                callback.cont(PermissionRequestResult::IGNORE);
                return 1;
            }
            let callback = PermissionCallback::Prompt(prompt_id, requested_permissions, callback.clone());
            ask_permission(browser, &origin, asks, callback);
            1
        }

        /// Chromium took the prompt back, as its page navigated or closed.
        fn on_dismiss_permission_prompt(&self, _browser: Option<&mut Browser>, prompt_id: u64, _result: PermissionRequestResult) {
            PERMISSION_REQUESTS.with_borrow_mut(|requests| {
                requests.retain(|request| {
                    !matches!(request.callback, PermissionCallback::Prompt(id, ..) if id == prompt_id)
                });
            });
            surfaces_changed();
        }
    }
}

/// Keeps a site's request until the user answers it in the permission prompt.
fn ask_permission(
    browser: &Browser,
    origin: &str,
    asks: Vec<&'static permissions::Capability>,
    callback: PermissionCallback,
) {
    let id = NEXT_PERMISSION_REQUEST.get();
    NEXT_PERMISSION_REQUEST.set(id + 1);
    let request = PermissionRequest {
        id,
        browser: browser.clone(),
        window: FOCUSED.get(),
        origin: origin.to_string(),
        site: site(origin).to_string(),
        asks,
        callback,
    };
    PERMISSION_REQUESTS.with_borrow_mut(|requests| requests.push(request));
    surfaces_changed();
}

/// Gives the site the user's answer to its request `id`, from the permission prompt.
pub(super) fn answer_permission(id: u64, allow: bool) {
    let request = PERMISSION_REQUESTS.with_borrow_mut(|requests| {
        let index = requests.iter().position(|request| request.id == id)?;
        Some(requests.remove(index))
    });
    let Some(request) = request else { return };
    match request.callback {
        PermissionCallback::Media(callback, requested) => {
            keep_decision(&request.browser, &request.origin, &request.asks, allow);
            callback.cont(if allow { requested } else { 0 });
        }
        PermissionCallback::Prompt(_, requested, callback) => {
            ANSWERING.set(Some((request.origin, requested)));
            callback.cont(if allow {
                PermissionRequestResult::ACCEPT
            } else {
                PermissionRequestResult::DENY
            });
            ANSWERING.set(None);
        }
    }
    surfaces_changed();
}

/// The decision the profile keeps on `origin` using all of `asks`: allowed once each is, blocked
/// once any is; none while one is undecided, or asked each time, like sharing the screen.
fn kept_decision(
    browser: &Browser,
    origin: &str,
    asks: &[&permissions::Capability],
) -> Option<bool> {
    let context = browser.host()?.request_context()?;
    let origin = CefString::from(origin);
    let mut allowed = true;
    for ask in asks {
        let value = context.content_setting(Some(&origin), Some(&origin), ask.setting?);
        if value == ContentSettingValues::BLOCK {
            return Some(false);
        }
        allowed &= value == ContentSettingValues::ALLOW;
    }
    allowed.then_some(true)
}

/// Keeps the user's decision on `origin` using `asks`, where the profile keeps one.
fn keep_decision(browser: &Browser, origin: &str, asks: &[&permissions::Capability], allow: bool) {
    let Some(context) = browser.host().and_then(|host| host.request_context()) else {
        return;
    };
    let origin = CefString::from(origin);
    let value = if allow {
        ContentSettingValues::ALLOW
    } else {
        ContentSettingValues::BLOCK
    };
    for setting in asks.iter().filter_map(|ask| ask.setting) {
        context.set_content_setting(Some(&origin), Some(&origin), setting, value);
    }
}

/// Whether the window shows a request's page: its tab's, shown alone or in a split; else, a page
/// that is no tab's, like Peek's, in the window in front as the request came.
fn shows_request(request: &PermissionRequest) -> bool {
    shows_page(&request.browser, request.window)
}

/// Whether the window shows `browser`'s page: its tab's, shown alone or in a split; else, a page
/// that is no tab's, like Peek's, in the window `window`.
pub(super) fn shows_page(browser: &Browser, window: u32) -> bool {
    match page_tab(browser) {
        Some(tab) => win().shown.get() == Some(tab) || win().split.borrow().0.contains(&tab),
        None => win().id == window,
    }
}

/// Shows the window the first request of a page it shows in the permission prompt, at the top
/// left of the card, over the page; or hides it. The requests of closed pages go.
#[cfg(target_os = "macos")]
pub(super) fn sync_permission_prompt() {
    PERMISSION_REQUESTS.with_borrow_mut(|requests| {
        requests.retain(|request| request.browser.is_valid() == 1);
    });
    let shown = PERMISSION_REQUESTS.with_borrow(|requests| {
        let request = requests.iter().find(|request| shows_request(request))?;
        let asks: Vec<_> = request
            .asks
            .iter()
            .map(|ask| json!({ "text": ask.asks, "symbol": ask.symbol }))
            .collect();
        Some(json!({ "id": request.id, "site": request.site, "asks": asks }))
    });
    let card = card_rect().unwrap_or_default();
    let frame = Rect {
        x: card.x + PERMISSION_INSET,
        y: card.y + PERMISSION_INSET,
        width: PERMISSION_WIDTH,
        height: win().permission_height.get(),
    };
    let detail = shown.clone().unwrap_or_default();
    crate::native::event(
        win().id,
        "permission",
        "arbalete:permission",
        &detail.to_string(),
    );
    crate::native::surface(win().id, "permission", shown.is_some(), &frame);
}

/// The permission prompt's width, and how far inside the card's top left corner it shows.
pub(super) const PERMISSION_WIDTH: i32 = 300;
pub(super) const PERMISSION_INSET: i32 = 10;

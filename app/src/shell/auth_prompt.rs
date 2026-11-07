//! The prompt asking the user to sign in when a site or a proxy wants a user name and password.

use super::*;

/// A site's or a proxy's request to sign in, waiting for the user's answer in the sign-in prompt.
/// What the user answers goes to Chromium alone, which keeps it for the session.
pub(super) struct AuthRequest {
    /// What the prompt's answer names it by.
    id: u64,
    browser: Browser,
    /// The window in front as it came, which shows its prompt unless the page is a tab's.
    window: u32,
    host: String,
    realm: String,
    proxy: bool,
    callback: AuthCallback,
}

wrap_task! {
    pub(super) struct AskAuth {
        browser: Browser,
        host: String,
        realm: String,
        proxy: bool,
        callback: AuthCallback,
    }

    impl Task {
        /// Keeps the request until the user answers it in the sign-in prompt.
        fn execute(&self) {
            let id = NEXT_AUTH_REQUEST.get();
            NEXT_AUTH_REQUEST.set(id + 1);
            let request = AuthRequest {
                id,
                browser: self.browser.clone(),
                window: FOCUSED.get(),
                host: self.host.clone(),
                realm: self.realm.clone(),
                proxy: self.proxy,
                callback: self.callback.clone(),
            };
            AUTH_REQUESTS.with_borrow_mut(|requests| requests.push(request));
            surfaces_changed();
        }
    }
}

/// Gives the request `id` the user name and password the user signed in with, or turns it down.
pub(super) fn answer_auth(id: u64, credentials: Option<(String, String)>) {
    let request = AUTH_REQUESTS.with_borrow_mut(|requests| {
        let index = requests.iter().position(|request| request.id == id)?;
        Some(requests.remove(index))
    });
    let Some(request) = request else { return };
    match credentials {
        Some((user, password)) => request.callback.cont(
            Some(&CefString::from(user.as_str())),
            Some(&CefString::from(password.as_str())),
        ),
        None => request.callback.cancel(),
    }
    surfaces_changed();
}

/// Turns down the requests to sign in of `browser`'s page.
pub(super) fn forget_auth(browser: &Browser) {
    let id = browser.identifier();
    let forgotten: Vec<_> = AUTH_REQUESTS.with_borrow_mut(|requests| {
        requests
            .extract_if(.., |request| request.browser.identifier() == id)
            .collect()
    });
    if !forgotten.is_empty() {
        forgotten
            .iter()
            .for_each(|request| request.callback.cancel());
        surfaces_changed();
    }
}

/// Shows the window the first request to sign in of a page it shows in the sign-in prompt, at the
/// top of the card, over the page; or hides it. The requests of closed pages go.
#[cfg(target_os = "macos")]
pub(super) fn sync_auth_prompt() {
    AUTH_REQUESTS.with_borrow_mut(|requests| {
        requests.retain(|request| request.browser.is_valid() == 1);
    });
    let shown = AUTH_REQUESTS.with_borrow(|requests| {
        let request = requests
            .iter()
            .find(|request| shows_page(&request.browser, request.window))?;
        Some(json!({
            "id": request.id,
            "host": request.host,
            "realm": request.realm,
            "proxy": request.proxy,
        }))
    });
    let card = card_rect().unwrap_or_default();
    let frame = Rect {
        x: card.x + (card.width - PERMISSION_WIDTH) / 2,
        y: card.y + PERMISSION_INSET,
        width: PERMISSION_WIDTH,
        height: win().auth_height.get(),
    };
    let detail = shown.clone().unwrap_or_default();
    crate::native::event(win().id, "auth", "arbalete:auth", &detail.to_string());
    crate::native::surface(win().id, "auth", shown.is_some(), &frame);
}

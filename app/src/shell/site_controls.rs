//! The site controls' panel, like Arc's, and the extensions' popups it and the pinned ones open.

use super::*;

/// An extension's popup, in a window of its own.
pub(super) struct ExtensionPopup {
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
pub(super) fn show_site_controls(button: Button) {
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
}

/// Where a button of the interface is, in the window: the native interface gives it so.
pub(super) fn button_in_window(button: Button) -> Option<Rect> {
    Some(Rect {
        x: button.x,
        y: button.y,
        width: button.width,
        height: button.height,
    })
}

/// What the UI shows of an extension: its name, its icon and whether its button opens a popup.
pub(super) fn extension_json(
    profile: ProfileId,
    extension: &extensions::Installed,
) -> serde_json::Value {
    let icon = extensions::icon_path(profile, extension).map(|path| format!("{UI_ORIGIN}{path}"));
    json!({ "id": extension.id, "name": extension.name, "icon": icon, "popup": extension.popup.is_some() })
}

/// Sends the UI the pinned extensions again, after an extension was installed or removed.
pub fn extensions_changed() {
    send_state();
}

/// The ids of a profile's pinned extensions.
pub(super) fn pinned_extensions(profile: ProfileId) -> Vec<String> {
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
pub(super) fn send_site_controls(url: &str) {
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
    let detail = json!({ "url": shown_url(url), "site": site(url), "awake": awake, "developer": developer_mode(url), "suspends": settings.suspend_tabs, "picture": !settings.no_picture_sites.iter().any(|blocked| blocked == site(url)), "pictures": settings.picture_in_picture, "extensions": extensions, "permissions": site_permissions(url) });
    dispatch(&win().site_view, "arbalete:show", &detail.to_string());
}

/// What the shown page's site, at `url`, was allowed or blocked, unlike Chromium's default, for
/// its site controls.
fn site_permissions(url: &str) -> Vec<serde_json::Value> {
    let Some(context) = shown_context() else {
        return Vec::new();
    };
    let url = CefString::from(url);
    permissions::remembered()
        .into_iter()
        .filter_map(|capability| {
            let setting = capability.setting?;
            let value = context.content_setting(Some(&url), Some(&url), setting);
            let decided = [ContentSettingValues::ALLOW, ContentSettingValues::BLOCK];
            let default = context.content_setting(None, None, setting);
            (decided.contains(&value) && value != default).then(|| {
                json!({
                    "setting": setting.get_raw(),
                    "text": capability.asks,
                    "symbol": capability.symbol,
                    "allowed": value == ContentSettingValues::ALLOW,
                })
            })
        })
        .collect()
}

/// The shown page's profile's web data, which keeps its sites' permissions.
fn shown_context() -> Option<RequestContext> {
    content()?.host()?.request_context()
}

/// Allows or blocks a permission, by the content setting that keeps it, for the shown page's
/// site; without a value, forgets every permission the site was allowed or blocked. Its site
/// controls show it.
pub(super) fn set_site_permissions(setting: Option<u32>, value: ContentSettingValues) {
    let (Some(context), Some(url)) = (shown_context(), selected_url()) else {
        return;
    };
    let address = CefString::from(url.as_str());
    let settings = permissions::remembered()
        .into_iter()
        .filter_map(|capability| capability.setting)
        .filter(|kept| setting.is_none_or(|setting| kept.get_raw() == setting));
    for kept in settings {
        context.set_content_setting(Some(&address), Some(&address), kept, value);
    }
    send_site_controls(&url);
}

/// Puts the site controls' panel under their button, as tall as its page asks, its beak pointing
/// at the button.
pub(super) fn place_site_controls() {
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
    dispatch(&win().site_view, "arbalete:beak", &beak.to_string());
    controller.set_bounds(Some(&bounds));
}

/// Shows the shown page's certificate and the ones that issued it, like Arc's, in place of the
/// site controls: macOS's certificate sheet on the window.
pub(super) fn show_certificate() {
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
}

/// Hides the site controls' panel, if it shows.
pub(super) fn hide_site_controls() {
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
pub(super) fn open_extension_popup(id: &str) {
    hide_site_controls();
    let open = win()
        .extension_popup
        .with_borrow(|popup| popup.as_ref().map(|popup| popup.id.clone()));
    close_extension_popup();
    if open.as_deref() == Some(id) {
        return;
    }
    let profile = extension_profile();
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
pub(super) fn open_extension_options(id: &str) {
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
            frame: Option<&mut Frame>,
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
            let page = frame.map(|frame| CefString::from(&frame.url()).to_string()).unwrap_or_default();
            let Some(url) = target_url.map(CefString::to_string).filter(|url| popup_allowed(user_gesture) && page_may_open(&page, url)) else { return 1 };
            // A link clicked with ⇧, which would open a window, shows in Peek, like Arc's.
            if target_disposition == WindowOpenDisposition::NEW_WINDOW {
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

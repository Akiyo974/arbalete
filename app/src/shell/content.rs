//! What tabs' pages ask of the app: their client and its handlers, from requests to closing.

use super::*;

// Shows a profile's web pages.
wrap_client! {
    pub(super) struct ContentClient {
        profile: ProfileId,
        // A Little Arbalete window's page, rather than a tab's.
        little: bool,
        // The Little Arbalete window of a page's popup.
        popup: Option<u32>,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            let tab = (!self.little).then_some(self.profile);
            Some(BrowserLifeSpan::new(tab, Some(self.profile), self.popup))
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

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(ContentKeyboard::new())
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(ContentDownloadHandler::new())
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            (!self.little).then(ContentRequestHandler::new)
        }

        fn permission_handler(&self) -> Option<PermissionHandler> {
            Some(ContentPermissions::new())
        }
    }
}

// Gives Arbalete the shortcuts it takes before the page, as their priority says (settings::Priority):
// the others reach the page first, and the menu only when the page leaves them.
wrap_keyboard_handler! {
    struct ContentKeyboard {}

    impl KeyboardHandler {
        fn on_pre_key_event(&self, browser: Option<&mut Browser>, event: Option<&KeyEvent>, os_event: *mut u8, _is_keyboard_shortcut: Option<&mut i32>) -> i32 {
            let Some(event) = event else { return 0 };
            if event.type_ != KeyEventType::RAWKEYDOWN {
                return 0;
            }
            // Like Chrome's, Escape takes a page out of full screen, which Alloy leaves to the app.
            if event.windows_key_code == ESCAPE
                && let Some(host) = browser.and_then(|browser| browser.host())
                && host.is_fullscreen() == 1
            {
                host.exit_fullscreen(1);
                return 1;
            }
            if os_event.is_null() {
                return 0;
            }
            #[cfg(target_os = "macos")]
            return crate::mac::take_shortcut(os_event.cast(), shortcut_priority).into();
        }

        /// The page left the key: a shortcut it was offered goes to the menu.
        fn on_key_event(&self, _browser: Option<&mut Browser>, event: Option<&KeyEvent>, _os_event: *mut u8) -> i32 {
            if event.is_some_and(|event| event.type_ == KeyEventType::RAWKEYDOWN) {
                OFFERED_SHORTCUT.take();
            }
            0
        }
    }
}

/// The Escape key's Windows key code, which CEF gives on every platform.
const ESCAPE: i32 = 0x1B;

/// How long the page has to leave a shortcut offered first before a toast says it kept it, in
/// milliseconds: it answers within a few frames.
const PAGE_KEY_ANSWER_MS: i64 = 150;

/// Like Arc's, a shortcut both the page and Arbalete take, which the page kept on a first press,
/// is told in a toast: "Press Again to Find…". `action` is its menu item's title, or none on the
/// second press, which Arbalete took.
pub fn page_offered_shortcut(action: Option<String>) {
    let offered = action.is_some();
    OFFERED_SHORTCUT.set(action);
    if offered {
        post_delayed_task(
            ThreadId::UI,
            Some(&mut PressAgain::new()),
            PAGE_KEY_ANSWER_MS,
        );
    }
}

fn press_again() {
    let Some(action) = OFFERED_SHORTCUT.take() else {
        return;
    };
    // As long as the toast shows (app/macos/Toast.swift).
    #[cfg(target_os = "macos")]
    crate::mac::wait_second_press(5.0);
    toast(json!({
        "key": "press-again",
        "text": format!("Press Again to {}", action.trim_end_matches('…')),
        "icon": "command",
    }));
}

wrap_task! {
    struct PressAgain {}

    impl Task {
        fn execute(&self) {
            press_again();
        }
    }
}

wrap_task! {
    struct PageEnded {
        browser: Browser,
    }

    impl Task {
        fn execute(&self) {
            page_ended(&self.browser);
        }
    }
}

/// Brings back a page whose renderer ended: a hidden tab's closes, to open again at its url when
/// shown, like an unloaded one; a page shown reloads, unless it ended a moment ago, as one that
/// crashes as it loads would, which the user's reload tries again.
fn page_ended(browser: &Browser) {
    let hidden = page_tab(browser).filter(|tab| {
        PAGES.with_borrow(|pages| {
            pages
                .get(tab)
                .is_some_and(|live| live.hidden_since.is_some())
        })
    });
    if let Some(tab) = hidden {
        close_page(tab);
        fall_asleep(tab);
        return;
    }
    let (id, now) = (browser.identifier(), now());
    let again = LAST_CRASH
        .get()
        .is_some_and(|(last, at)| last == id && now - at < 60_000);
    LAST_CRASH.set(Some((id, now)));
    if !again {
        browser.reload();
    }
}

/// The address of a frame, empty without one.
fn frame_url(frame: Option<&mut Frame>) -> String {
    frame
        .map(|frame| CefString::from(&frame.url()).to_string())
        .unwrap_or_default()
}

/// Who takes a shortcut first: the settings' choice, or its default.
fn shortcut_priority(id: &str) -> settings::Priority {
    let changed = app_settings()
        .shortcuts
        .get(id)
        .and_then(|changed| changed.priority);
    changed.unwrap_or_else(|| settings::default_priority(id))
}

wrap_request_handler! {
    struct ContentRequestHandler {}

    impl RequestHandler {
        /// Like Arc's, a link from a pinned tab or a favorite to another site shows in Peek,
        /// the tab staying on its site.
        fn on_before_browse(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, request: Option<&mut Request>, user_gesture: i32, is_redirect: i32) -> i32 {
            let (Some(browser), Some(frame), Some(request)) = (browser, frame, request) else { return 0 };
            let url = CefString::from(&request.url()).to_string();
            // A page going elsewhere no longer asks to sign in.
            if frame.is_main() == 1 {
                forget_auth(browser);
                // Nor reaches a Chromium page that crashes out of a Chromium window, from a link
                // of chrome://about or a tab saved on one.
                if url.starts_with("chrome://") && !is_tab_page(&url) {
                    return 1;
                }
            }
            if user_gesture == 0 || is_redirect == 1 || frame.is_main() == 0 {
                return 0;
            }
            let Some(tab) = page_tab(browser) else { return 0 };
            let anchored = STATE.with_borrow(|state| state.sidebar.is_pinned(tab) || state.sidebar.is_favorite(tab));
            let current = CefString::from(&frame.url()).to_string();
            let elsewhere = origin(&current).is_some() && origin(&url).is_some() && origin(&url) != origin(&current);
            if !(anchored && elsewhere && page_may_open(&current, &url)) {
                return 0;
            }
            post(Command::Peek(url));
            1
        }

        /// A page whose renderer crashed or was killed comes back, rather than staying blank:
        /// where Chrome shows "Aw, Snap!", page_ended.
        fn on_render_process_terminated(&self, browser: Option<&mut Browser>, _status: TerminationStatus, _error_code: i32, _error_string: Option<&CefString>) {
            // Once Chromium is done telling it, which a reload now would disturb.
            if let Some(browser) = browser {
                post_task(ThreadId::UI, Some(&mut PageEnded::new(browser.clone())));
            }
        }

        /// Asks the user to sign in to a site, or a proxy, using HTTP authentication, in the
        /// sign-in prompt; on the IO thread.
        fn auth_credentials(&self, browser: Option<&mut Browser>, _origin_url: Option<&CefString>, is_proxy: i32, host: Option<&CefString>, _port: i32, realm: Option<&CefString>, _scheme: Option<&CefString>, callback: Option<&mut AuthCallback>) -> i32 {
            let (Some(browser), Some(callback)) = (browser, callback) else { return 0 };
            let text = |text: Option<&CefString>| text.map(CefString::to_string).unwrap_or_default();
            let mut ask = AskAuth::new(browser.clone(), text(host), text(realm), is_proxy == 1, callback.clone());
            post_task(ThreadId::UI, Some(&mut ask));
            1
        }

        /// A link clicked with ⇧ shows in Peek, one clicked with ⌘ opens in a tab behind the one
        /// shown, and one clicked with ⌘⇧ in a tab shown, like Arc's.
        fn on_open_urlfrom_tab(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, target_url: Option<&CefString>, target_disposition: WindowOpenDisposition, user_gesture: i32) -> i32 {
            let Some(url) = target_url.map(CefString::to_string).filter(|_| user_gesture == 1) else { return 0 };
            if !page_may_open(&frame_url(frame), &url) {
                return 1;
            }
            if target_disposition == WindowOpenDisposition::NEW_WINDOW {
                post(Command::Peek(url));
            } else if target_disposition == WindowOpenDisposition::NEW_FOREGROUND_TAB {
                post(Command::Open(url));
            } else if target_disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB {
                post(Command::OpenInBackground(url));
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

        fn on_before_download(&self, browser: Option<&mut Browser>, item: Option<&mut DownloadItem>, suggested_name: Option<&CefString>, callback: Option<&mut BeforeDownloadCallback>) -> i32 {
            let (Some(item), Some(callback)) = (item, callback) else { return 0 };
            let name = suggested_name.map(CefString::to_string).unwrap_or_default();
            // The Web Store's extensions, which Chromium installs once downloaded: kept out of the
            // downloads, in a temporary folder. Another site's are ordinary downloads.
            let mime = CefString::from(&item.mime_type()).to_string();
            let extension = mime == "application/x-chrome-extension" || name.ends_with(".crx");
            if extension && downloads::from_web_store(&CefString::from(&item.original_url()).to_string()) {
                let path = std::env::temp_dir().join(&name);
                callback.cont(Some(&CefString::from(path.to_string_lossy().as_ref())), 0);
                return 1;
            }
            let profile = browser.as_deref().and_then(page_tab).and_then(|tab| {
                STATE.with_borrow(|state| state.sidebar.profile_of(tab))
            });
            // Save Link As… and Save Image As… ask where, from the profile's downloads folder,
            // like a profile whose downloads always ask.
            let url = CefString::from(&item.original_url()).to_string();
            let save_as = SAVE_AS.with_borrow_mut(|urls| urls.remove(&url));
            let (dir, always_ask) = download_folder(profile);
            let ask = save_as || always_ask;
            if let Err(error) = std::fs::create_dir_all(&dir) {
                eprintln!("cannot create the downloads folder: {error}");
                return 0;
            }
            let path = DOWNLOADS.with_borrow(|downloads| {
                downloads::unique_path(&dir, &name, |path| {
                    path.exists() || downloads.iter().any(|download| download.path == path)
                })
            });
            callback.cont(Some(&CefString::from(path.to_string_lossy().as_ref())), ask.into());
            let url = CefString::from(&item.url()).to_string();
            DOWNLOADS.with_borrow_mut(|list| {
                let id = downloads::next_id(list);
                let mut download = Download::new(id, item.id(), path, url, now());
                download.profile = profile;
                list.push(download);
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
                // Where the file is saved, which Save As… asks, elsewhere than planned maybe.
                let path = PathBuf::from(CefString::from(&item.full_path()).to_string());
                if let Some(name) = path.file_name().map(|name| name.to_string_lossy().into_owned()) {
                    download.name = name;
                    download.path = path;
                }
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

/// The items Arbalete adds to a page's menu, numbered from the first id CEF leaves to apps, like
/// Arc's: Chromium's on a link, an image or selected text, and Arc's own.
const OPEN_LINK: i32 = 26500;
const OPEN_LINK_WINDOW: i32 = OPEN_LINK + 1;
const OPEN_LINK_SPLIT: i32 = OPEN_LINK + 2;
const OPEN_LINK_LITTLE: i32 = OPEN_LINK + 3;
const OPEN_LINK_INCOGNITO: i32 = OPEN_LINK + 4;
const SAVE_LINK: i32 = OPEN_LINK + 5;
const COPY_LINK: i32 = OPEN_LINK + 6;
const OPEN_IMAGE: i32 = OPEN_LINK + 7;
const SAVE_IMAGE: i32 = OPEN_LINK + 8;
const COPY_IMAGE: i32 = OPEN_LINK + 9;
const COPY_IMAGE_ADDRESS: i32 = OPEN_LINK + 10;
const SEARCH_IMAGE: i32 = OPEN_LINK + 11;
const SEARCH_SELECTION: i32 = OPEN_LINK + 12;
const INSPECT: i32 = OPEN_LINK + 13;
const COPY_HIGHLIGHT: i32 = OPEN_LINK + 14;
/// Chromium's Copy, which the menu has on selected text.
const CHROMIUM_COPY: i32 = 113;

thread_local! {
    /// The addresses Save Link As… or Save Image As… asked to download, which ask where.
    static SAVE_AS: RefCell<HashSet<String>> = RefCell::default();
}

/// The search engine of the displayed Space's profile.
fn displayed_search_engine() -> SearchEngine {
    STATE.with_borrow(|state| {
        let profile = state.sidebar.displayed().profile;
        state
            .sidebar
            .profiles()
            .iter()
            .find(|p| p.id == profile)
            .map(|p| p.search_engine)
            .unwrap_or_default()
    })
}

/// A link to text in a page, a text fragment that browsers scroll to and highlight: the page's
/// address, without its own fragment, and the text, its spaces and the fragment's syntax encoded.
fn highlight_link(page: &str, text: &str) -> String {
    let base = page.split('#').next().unwrap_or(page);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let encoded = arbalete_core::search::encode(&text).replace('-', "%2D");
    format!("{base}#:~:text={encoded}")
}

/// Selected text as a menu shows it: on one line, cut after 32 characters.
fn quoted_selection(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(32) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}

// Adds what Arc offers on a link, an image or selected text above Chromium's own items, and
// Inspect below them.
wrap_context_menu_handler! {
    struct ContentMenuHandler {}

    impl ContextMenuHandler {
        fn on_before_context_menu(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, params: Option<&mut ContextMenuParams>, model: Option<&mut MenuModel>) {
            let (Some(params), Some(model)) = (params, model) else { return };
            // Groups of items, separated.
            let mut groups: Vec<Vec<(i32, String)>> = Vec::new();
            let item = |id: i32, english: &str| (id, chromium(english));
            if !CefString::from(&params.link_url()).to_string().is_empty() {
                groups.push(vec![
                    item(OPEN_LINK, "Open Link in New &Tab"),
                    item(OPEN_LINK_WINDOW, "Open Link in New &Window"),
                    item(OPEN_LINK_SPLIT, "Open Link in Split View"),
                    (OPEN_LINK_LITTLE, chromium_with("Open Link in $1", &["Little Arbalete"])),
                    item(OPEN_LINK_INCOGNITO, "Open Link in Inco&gnito Window"),
                ]);
                groups.push(vec![item(SAVE_LINK, "Save Lin&k As…"), item(COPY_LINK, "Copy Link Addr&ess")]);
            }
            if params.media_type() == ContextMenuMediaType::IMAGE {
                groups.push(vec![
                    item(OPEN_IMAGE, "Open &Image in New Tab"),
                    item(SAVE_IMAGE, "Sa&ve Image As…"),
                    item(COPY_IMAGE, "Cop&y Image"),
                    item(COPY_IMAGE_ADDRESS, "C&opy Image Address"),
                    (SEARCH_IMAGE, chromium_with("Search Image with $1", &["Google"])),
                ]);
            }
            let selection = CefString::from(&params.selection_text()).to_string();
            if !selection.trim().is_empty() {
                let label = chromium_with(
                    "&Search $1 for “$2”",
                    &[displayed_search_engine().name(), &quoted_selection(&selection)],
                );
                groups.push(vec![(SEARCH_SELECTION, label)]);
            }
            for group in groups.into_iter().rev() {
                if model.count() > 0 {
                    model.insert_separator_at(0);
                }
                for (id, label) in group.into_iter().rev() {
                    model.insert_item_at(0, id, Some(&CefString::from(label.as_str())));
                }
            }
            // Like Arc's, a link to the selected text after Chromium's Copy.
            if !selection.trim().is_empty() {
                let label = CefString::from(chromium("Copy Link to Highlight").as_str());
                match model.index_of(CHROMIUM_COPY) {
                    -1 => model.add_item(COPY_HIGHLIGHT, Some(&label)),
                    copy => model.insert_item_at((copy + 1) as usize, COPY_HIGHLIGHT, Some(&label)),
                };
            }
            if model.count() > 0 {
                model.add_separator();
            }
            model.add_item(INSPECT, Some(&CefString::from(chromium("Inspect").as_str())));
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
            // A web page's link to an internal page goes nowhere, like a click on it.
            let page = CefString::from(&params.page_url()).to_string();
            if !page_may_open(&page, &link) || !page_may_open(&page, &image) {
                return 1;
            }
            let download = |url: String| {
                SAVE_AS.with_borrow_mut(|urls| urls.insert(url.clone()));
                if let Some(host) = browser.and_then(|browser| browser.host()) {
                    host.start_download(Some(&CefString::from(url.as_str())));
                }
            };
            match command_id {
                OPEN_LINK => perform(Action::Run(Command::Open(link))),
                OPEN_LINK_WINDOW => open_link_in_new_window(&link),
                OPEN_LINK_SPLIT => open_in_pane(&link),
                OPEN_LINK_LITTLE => open_little(Some(link)),
                OPEN_LINK_INCOGNITO => open_incognito_window(Some(&link)),
                SAVE_LINK => download(link),
                COPY_LINK => perform(Action::CopyLink(link)),
                OPEN_IMAGE => perform(Action::Run(Command::Open(image))),
                SAVE_IMAGE => download(image),
                #[cfg(target_os = "macos")]
                COPY_IMAGE => crate::mac::copy_image(&image),
                COPY_IMAGE_ADDRESS => perform(Action::CopyLink(image)),
                SEARCH_IMAGE => {
                    let url = format!("https://lens.google.com/uploadbyurl?url={}", arbalete_core::search::encode(&image));
                    perform(Action::Run(Command::Open(url)));
                }
                COPY_HIGHLIGHT => {
                    let page = CefString::from(&params.page_url()).to_string();
                    let selection = CefString::from(&params.selection_text()).to_string();
                    perform(Action::CopyLink(highlight_link(&page, &selection)));
                }
                SEARCH_SELECTION => {
                    let selection = CefString::from(&params.selection_text()).to_string();
                    let query = arbalete_core::search::encode(selection.trim());
                    perform(Action::Run(Command::Open(format!("{}{query}", displayed_search_engine().url()))));
                }
                _ => return 0,
            }
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
                    "arbalete:found",
                    &json!({ "count": count, "active": active_match_ordinal }).to_string(),
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
            status(browser, "", true);
            if is_peek(browser) {
                return send_peek();
            }
            let Some(tab) = page_tab(browser) else {
                return little_changed(browser, |little| little.url = url);
            };
            if tab_has(tab, |tab| tab.url == url) {
                return;
            }
            left_page(self.profile, tab);
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
            // An incognito window keeps no history, and its folder goes with it.
            let incognito = INCOGNITO.with_borrow(|incognito| incognito.contains_key(&self.profile));
            if let Some(frame) = browser.as_ref().and_then(|browser| browser.main_frame()).filter(|_| !incognito) {
                let url = CefString::from(&frame.url()).to_string();
                NEW_TITLES.with_borrow_mut(|titles| titles.insert((self.profile, url), title.clone()));
                save_later();
            }
            let Some(browser) = browser else { return };
            let Some(tab) = page_tab(browser) else {
                return little_changed(browser, |little| little.title = title);
            };
            if tab_has(tab, |tab| tab.title == title) {
                return;
            }
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

        /// The hovered link's address, or none.
        fn on_status_message(&self, browser: Option<&mut Browser>, value: Option<&CefString>) {
            let Some(browser) = browser else { return };
            let address = value.map(CefString::to_string).unwrap_or_default();
            status(browser, &readable_address(&address), false);
        }

        fn on_loading_progress_change(&self, browser: Option<&mut Browser>, progress: f64) {
            if is_shown(browser) {
                update_page(|page| page.progress = progress);
            }
        }

        /// Like Arc's, a page going full screen, like a video, tells how to leave it.
        fn on_fullscreen_mode_change(&self, browser: Option<&mut Browser>, fullscreen: i32) {
            if fullscreen == 1 && is_shown(browser) {
                toast(json!({ "key": "fullscreen", "text": "To exit full screen, press", "keys": "⎋" }));
            }
        }
    }
}

/// Shows `text` at the bottom of `browser`'s page, or hides it without text (native::page_status).
fn status(browser: &Browser, text: &str, instant: bool) {
    if let Some(host) = browser.host() {
        crate::native::page_status(host.window_handle(), text, instant);
    }
}

/// `address` as people read it, like Chromium's status bubble: an international host in its own
/// letters unless they mix scripts whose lookalikes pass for another site, and the characters
/// beyond ASCII decoded but invisible ones. ASCII ones, like `%20` or `%2F`, stay encoded:
/// decoded, they could change what the address means.
fn readable_address(address: &str) -> String {
    let Some(at) = address.find("://") else {
        return decoded(address);
    };
    let (scheme, rest) = address.split_at(at + 3);
    let (authority, tail) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    let (user, host_port) = authority.split_at(authority.rfind('@').map_or(0, |at| at + 1));
    let (host, port) = host_port.split_at(host_port.find(':').unwrap_or(host_port.len()));
    format!(
        "{scheme}{user}{}{port}{}",
        readable_host(host),
        decoded(tail)
    )
}

/// `host` in its own letters when they are all of one script, as far as `script` tells.
fn readable_host(host: &str) -> String {
    let (unicode, valid) = idna::domain_to_unicode(host);
    let mut scripts = unicode.chars().filter_map(script);
    let first = scripts.next();
    if valid.is_err() || scripts.any(|script| Some(script) != first) {
        return host.to_owned();
    }
    unicode
}

/// A letter's script, as far as telling lookalikes apart: Latin, Greek, Cyrillic, or another.
fn script(letter: char) -> Option<u8> {
    match letter {
        'a'..='z' | 'A'..='Z' | '\u{C0}'..='\u{24F}' | '\u{1E00}'..='\u{1EFF}' => Some(0),
        '\u{370}'..='\u{3FF}' | '\u{1F00}'..='\u{1FFF}' => Some(1),
        '\u{400}'..='\u{52F}' => Some(2),
        _ if letter.is_alphabetic() => Some(3),
        _ => None,
    }
}

/// `text` with its percent-encoded characters beyond ASCII decoded, but invisible ones and bytes
/// that are no UTF-8.
fn decoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let bytes: Vec<u8> = rest.as_bytes().chunks(3).map_while(high_byte).collect();
        if bytes.is_empty() {
            out.push('%');
            rest = &rest[1..];
            continue;
        }
        // Each byte is three characters of `run`.
        let (run, after) = rest.split_at(bytes.len() * 3);
        let mut taken = 0;
        for chunk in bytes.utf8_chunks() {
            for character in chunk.valid().chars() {
                let next = taken + character.len_utf8();
                if invisible(character) {
                    out.push_str(&run[taken * 3..next * 3]);
                } else {
                    out.push(character);
                }
                taken = next;
            }
            let next = taken + chunk.invalid().len();
            out.push_str(&run[taken * 3..next * 3]);
            taken = next;
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The byte beyond ASCII that `escape`, like `%D0`, encodes.
fn high_byte(escape: &[u8]) -> Option<u8> {
    let [b'%', high, low] = escape else {
        return None;
    };
    let byte = u8::from_str_radix(std::str::from_utf8(&[*high, *low]).ok()?, 16).ok()?;
    (byte >= 0x80).then_some(byte)
}

/// Whether `character` shows nothing, or could hide or reorder what follows it.
fn invisible(character: char) -> bool {
    character.is_whitespace()
        || character.is_control()
        || matches!(
            character,
            '\u{AD}' | '\u{61C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}'
        )
}

/// Whether the sidebar's tab `tab` is as `is` asks.
pub(super) fn tab_has(tab: NodeId, is: impl FnOnce(&Tab) -> bool) -> bool {
    STATE.with_borrow(|state| {
        state
            .sidebar
            .node(tab)
            .is_some_and(|node| matches!(&node.kind, NodeKind::Tab(tab) if is(tab)))
    })
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

/// Whether a page's popup opens: only one the user asked for, by a click or a key, so a
/// page cannot open tabs or windows on its own, like a script calling `window.open` in a loop.
pub(super) fn popup_allowed(user_gesture: i32) -> bool {
    user_gesture == 1
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
    pub(super) struct BrowserLifeSpan {
        // A tab's page's profile.
        profile: Option<ProfileId>,
        // The profile of a web page, whose popups open in windows of their own.
        opener: Option<ProfileId>,
        // The Little Arbalete window of a page's popup.
        popup: Option<u32>,
    }

    impl LifeSpanHandler {
        /// A popup a page opens with `window.open` and a size, like a sign-in's, opens in a
        /// Little Arbalete window, like Arc's, keeping its opener: it sends its result there and
        /// closes. Others open in tabs.
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            popup_id: i32,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            user_gesture: i32,
            popup_features: Option<&PopupFeatures>,
            window_info: Option<&mut WindowInfo>,
            client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            let Some(url) = target_url.map(CefString::to_string).filter(|url| popup_allowed(user_gesture) && page_may_open(&frame_url(frame), url)) else {
                return 1;
            };
            #[cfg(target_os = "macos")]
            if let (Some(profile), Some(window_info), Some(client)) = (self.opener, window_info, client)
                && target_disposition == WindowOpenDisposition::NEW_POPUP
                && !is_chrome_page(&url)
            {
                let size = popup_features
                    .filter(|features| features.width_set == 1 && features.height_set == 1)
                    .map(|features| (features.width, features.height));
                let Some((id, view)) = open_popup(size) else { return 1 };
                let (width, height) = size.unwrap_or((800, 600));
                *window_info = WindowInfo {
                    parent_view: view,
                    bounds: Rect { x: 0, y: 0, width, height },
                    runtime_style: RuntimeStyle::ALLOY,
                    ..Default::default()
                };
                *client = Some(ContentClient::new(profile, true, Some(id)));
                OPENING_POPUPS.with_borrow_mut(|popups| popups.push((popup_id, id)));
                return 0;
            }
            open_tab(&url);
            1
        }

        /// A popup CEF did not make after all leaves its window.
        fn on_before_popup_aborted(&self, _browser: Option<&mut Browser>, popup_id: i32) {
            let popup = OPENING_POPUPS.with_borrow_mut(|popups| {
                let at = popups.iter().position(|&(id, _)| id == popup_id)?;
                Some(popups.remove(at).1)
            });
            #[cfg(target_os = "macos")]
            if let Some(little) = popup {
                crate::native::little_closed(little);
            }
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            OPEN_BROWSERS.set(OPEN_BROWSERS.get() + 1);
            #[cfg(target_os = "macos")]
            if let (Some(id), Some(profile), Some(browser)) = (self.popup, self.opener, browser.as_deref()) {
                OPENING_POPUPS.with_borrow_mut(|popups| popups.retain(|&(_, little)| little != id));
                popup_created(id, profile, browser);
            }
            if let Some(profile) = self.profile {
                content_created(profile, browser);
            }
        }

        /// A page closing leaves its container, which CEF would otherwise close the window of.
        fn do_close(&self, browser: Option<&mut Browser>) -> i32 {
            if self.profile.is_none() {
                // A Little Arbalete window's page closes its window; Peek's leaves its card.
                #[cfg(target_os = "macos")]
                if let Some(browser) = browser.as_deref() {
                    let replaced = REPLACED_LITTLE_PAGES.with_borrow_mut(|pages| {
                        let at = pages.iter().position(|&page| page == browser.identifier())?;
                        Some(pages.remove(at))
                    });
                    if let (Some(_), Some(host)) = (replaced, browser.host()) {
                        let view = host.window_handle() as usize;
                        post_task(ThreadId::UI, Some(&mut RemoveLittlePage::new(view)));
                        return 1;
                    }
                    if let Some(id) = little_of(browser) {
                        post_task(ThreadId::UI, Some(&mut RemoveLittle::new(id)));
                        return 1;
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_popup_the_user_asked_for_opens() {
        assert!(popup_allowed(1));
        assert!(!popup_allowed(0));
    }

    #[test]
    fn links_to_selected_text_with_a_text_fragment() {
        assert_eq!(
            super::highlight_link("https://a.test/page#top", "Dia  reads-between,\n& more"),
            "https://a.test/page#:~:text=Dia%20reads%2Dbetween%2C%20%26%20more"
        );
    }

    #[test]
    fn shows_a_link_address_readable() {
        // A Cyrillic path, query and fragment, decoded.
        assert_eq!(
            readable_address(
                "https://ru.wikipedia.org/wiki/%D0%9C%D0%BE%D1%81%D0%BA%D0%B2%D0%B0?q=%D0%B0#%D0%B1"
            ),
            "https://ru.wikipedia.org/wiki/Москва?q=а#б"
        );
        // International hosts in their own letters, the user and port kept.
        assert_eq!(
            readable_address("https://u@xn--mnchen-3ya.de:8080/"),
            "https://u@münchen.de:8080/"
        );
        assert_eq!(
            readable_address("http://xn--e1afmkfd.xn--p1ai/"),
            "http://пример.рф/"
        );
        // Both, ASCII escapes kept encoded.
        assert_eq!(
            readable_address("https://xn--e1afmkfd.xn--p1ai/%D0%B0%20b%2Fc"),
            "https://пример.рф/а%20b%2Fc"
        );
        // A Cyrillic "а" in a Latin host stays in punycode.
        assert_eq!(
            readable_address("https://xn--pple-43d.com/"),
            "https://xn--pple-43d.com/"
        );
        // Bytes that are no UTF-8, and invisible characters, stay encoded.
        assert_eq!(
            readable_address("https://a.test/%D0%9C%FF%D0"),
            "https://a.test/М%FF%D0"
        );
        assert_eq!(
            readable_address("https://a.test/a%E2%80%AEb%C2%A0"),
            "https://a.test/a%E2%80%AEb%C2%A0"
        );
        assert_eq!(
            readable_address("https://a.test/100%"),
            "https://a.test/100%"
        );
        assert_eq!(readable_address("mailto:%D0%B0@a.test"), "mailto:а@a.test");
    }
}

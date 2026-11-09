//! Boosts in the pages (decision 0010): each page's DevTools session gives every new document
//! the scripts of its profile's active Boosts, before the page's own, and the open pages of a
//! site take a Boost's changes as they are made.

use super::*;
use crate::command::BoostEditorAction;
use arbalete_core::boosts::{self, Boost, BoostId, Boosts, Editing};

/// The id of the next DevTools message sent: each has its own, as CEF answers one message per id.
const FIRST_MESSAGE: i32 = 27_000_000;

/// A page that gets Boosts, with the scripts its session runs in its new documents.
struct Page {
    profile: ProfileId,
    browser: Browser,
    scripts: Vec<String>,
    /// The messages adding scripts not answered yet, whose answers name them.
    adding: Vec<i32>,
    _registration: Option<Registration>,
}

thread_local! {
    /// Each profile's Boosts, read from its directory the first time they are needed.
    static BOOSTS: RefCell<HashMap<ProfileId, Boosts>> = RefCell::default();
    /// The pages, by browser id.
    static BOOST_PAGES: RefCell<HashMap<i32, Page>> = RefCell::default();
    static NEXT_MESSAGE: Cell<i32> = const { Cell::new(FIRST_MESSAGE) };
}

fn incognito(profile: ProfileId) -> bool {
    INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile))
}

/// A profile's Boosts; none in an incognito window.
pub(super) fn with_boosts<T>(profile: ProfileId, read: impl FnOnce(&Boosts) -> T) -> T {
    BOOSTS.with_borrow_mut(|all| {
        let boosts = all.entry(profile).or_insert_with(|| {
            if incognito(profile) {
                return Boosts::default();
            }
            let path = profile_dir(profile).join(boosts::FILE_NAME);
            boosts::load(&path).unwrap_or_else(|error| {
                eprintln!("cannot read the Boosts: {error}");
                Boosts::default()
            })
        });
        read(boosts)
    })
}

/// Changes a profile's Boosts, saves them, and gives the pages the change: those of `site`
/// restyle at once, and reload when a script changed, which cannot be taken back.
pub(super) fn update_boosts(profile: ProfileId, site: &str, change: impl FnOnce(&mut Boosts)) {
    apply_boosts(profile, site, true, change);
}

/// Changes a profile's Boosts like `update_boosts`; a script changed reloads the pages of `site`
/// only if `reload`, or runs from their next load.
fn apply_boosts(profile: ProfileId, site: &str, reload: bool, change: impl FnOnce(&mut Boosts)) {
    if incognito(profile) {
        return;
    }
    let scripts_before = with_boosts(profile, Boosts::scripts);
    let boosts = BOOSTS.with_borrow_mut(|all| {
        let boosts = all.get_mut(&profile).expect("read above");
        change(boosts);
        boosts.clone()
    });
    if let Err(error) = boosts::save(&boosts, &profile_dir(profile).join(boosts::FILE_NAME)) {
        eprintln!("cannot save the Boosts: {error}");
    }
    let reload = reload && boosts.scripts() != scripts_before;
    let pages: Vec<Browser> = BOOST_PAGES.with_borrow(|pages| {
        pages
            .values()
            .filter(|page| page.profile == profile)
            .map(|page| page.browser.clone())
            .collect()
    });
    for browser in pages {
        add_scripts(&browser, &boosts, false);
        let on_site = browser
            .main_frame()
            .map(|frame| CefString::from(&frame.url()).to_string())
            .and_then(|url| boosts::site(&url))
            .is_some_and(|shown| shown == site);
        if !on_site {
            continue;
        }
        if reload {
            browser.reload();
        } else {
            evaluate(&browser, &boosts.restyle(site));
        }
    }
}

/// Starts giving a page its profile's Boosts.
pub fn page_created(profile: ProfileId, browser: &Browser) {
    let Some(host) = browser.host() else { return };
    let page = browser.identifier();
    let registration = host.add_dev_tools_message_observer(Some(&mut BoostObserver::new(page)));
    BOOST_PAGES.with_borrow_mut(|pages| {
        pages.insert(
            page,
            Page {
                profile,
                browser: browser.clone(),
                scripts: Vec::new(),
                adding: Vec::new(),
                _registration: registration,
            },
        );
    });
    // New documents run the scripts added for them only while the Page domain is enabled.
    send(browser, "Page.enable", json!({}));
    // The page's first document may already be there: they run in it too.
    with_boosts(profile, |boosts| add_scripts(browser, boosts, true));
}

/// Forgets a page as its browser closes.
pub fn page_closed(page: i32) {
    BOOST_PAGES.with_borrow_mut(|pages| pages.remove(&page));
}

/// Replaces the scripts a page's new documents run with those of `boosts`, and runs them in its
/// current one `immediately`.
fn add_scripts(browser: &Browser, boosts: &Boosts, immediately: bool) {
    let old = BOOST_PAGES.with_borrow_mut(|pages| {
        pages
            .get_mut(&browser.identifier())
            .map(|page| std::mem::take(&mut page.scripts))
    });
    for identifier in old.unwrap_or_default() {
        send(
            browser,
            "Page.removeScriptToEvaluateOnNewDocument",
            json!({ "identifier": identifier }),
        );
    }
    for source in std::iter::once(boosts.stylesheet_script()).chain(boosts.scripts()) {
        let id = send(
            browser,
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": source, "runImmediately": immediately }),
        );
        BOOST_PAGES.with_borrow_mut(|pages| {
            if let Some(page) = pages.get_mut(&browser.identifier()) {
                page.adding.push(id);
            }
        });
    }
}

/// Runs a script in a page's main frame, in the page's world.
fn evaluate(browser: &Browser, expression: &str) {
    send(
        browser,
        "Runtime.evaluate",
        json!({ "expression": expression }),
    );
}

/// Sends a page's DevTools session a message, and gives its id, which its answer has.
fn send(browser: &Browser, method: &str, params: serde_json::Value) -> i32 {
    let id = NEXT_MESSAGE.get();
    NEXT_MESSAGE.set(id.checked_add(1).unwrap_or(FIRST_MESSAGE));
    if let Some(host) = browser.host() {
        let message = json!({ "id": id, "method": method, "params": params });
        host.send_dev_tools_message(Some(message.to_string().as_bytes()));
    }
    id
}

wrap_dev_tools_message_observer! {
    struct BoostObserver {
        page: i32,
    }

    impl DevToolsMessageObserver {
        /// Keeps the names of the scripts added, to take them back.
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> i32 {
            let Some(message) = message.and_then(|m| serde_json::from_slice::<serde_json::Value>(m).ok()) else { return 0 };
            let zapped = EDITOR.with_borrow(|editor| {
                editor.as_ref().and_then(|editor| editor.zapping.clone()).filter(|page| page.identifier() == self.page)
            });
            if let Some(page) = zapped {
                zap_message(&page, &message);
            }
            if message["id"] == GLOBALS.get() && let Some(names) = message["result"]["result"]["value"].as_array() {
                send_surface("boost", "arbalete:boost-globals", &json!(names).to_string());
            }
            let Some(id) = message["id"].as_i64().and_then(|id| i32::try_from(id).ok()) else { return 0 };
            BOOST_PAGES.with_borrow_mut(|pages| {
                let Some(page) = pages.get_mut(&self.page) else { return };
                let Some(index) = page.adding.iter().position(|adding| *adding == id) else { return };
                page.adding.remove(index);
                if let Some(identifier) = message["result"]["identifier"].as_str() {
                    page.scripts.push(identifier.to_string());
                }
            });
            0
        }
    }
}

/// The Boost the editor shows, in its profile.
struct Editor {
    profile: ProfileId,
    editing: Editing,
    /// The page the zap picker runs in.
    zapping: Option<Browser>,
    /// The picker gives a selector to the code instead of zapping.
    picking: bool,
}

thread_local! {
    static EDITOR: RefCell<Option<Editor>> = const { RefCell::new(None) };
}

/// The zap picker's world, apart from the page's scripts, and its binding to the app.
const ZAP_WORLD_NAME: &str = "arbalete-zap";
const ZAP_BINDING: &str = "__arbaleteZap";
const ZAP_PICKER: &str = include_str!("zap.js");

/// Makes a Boost for the shown page's site and opens the editor on it.
pub(super) fn new_boost() {
    let profile = displayed_profile();
    let Some(site) = selected_url().as_deref().and_then(boosts::site) else {
        return;
    };
    if incognito(profile) {
        return;
    }
    let mut id = 0;
    update_boosts(profile, &site, |boosts| id = boosts.create(&site));
    edit_boost(id);
}

/// Opens the editor on a Boost of the displayed profile, over a page of its site: the shown one,
/// or a new tab.
pub(super) fn edit_boost(id: BoostId) {
    open_boost(id, false);
}

/// Opens the editor on a Boost over its site in Peek, from the library, which stays, like Arc's.
pub(super) fn peek_boost(id: BoostId) {
    open_boost(id, true);
}

/// Opens the editor on a Boost, which turns it on, over a page of its site: the edited one, or
/// one opened in a tab or in Peek.
fn open_boost(id: BoostId, in_peek: bool) {
    let profile = displayed_profile();
    let Some(site) = with_boosts(profile, |boosts| boosts.get(id).map(|b| b.site.clone())) else {
        return;
    };
    close_editor();
    hide_site_controls();
    update_boosts(profile, &site, |boosts| boosts.set_active(id, true));
    let Some(boost) = with_boosts(profile, |boosts| boosts.get(id).cloned()) else {
        return;
    };
    if edited_site().as_deref() != Some(site.as_str()) {
        let url = format!("https://{site}/");
        if in_peek { peek(&url) } else { open_tab(&url) }
    }
    EDITOR.set(Some(Editor {
        profile,
        editing: Editing::new(boost),
        zapping: None,
        picking: false,
    }));
    send_editor();
    // It is on now, maybe instead of another.
    send_boosts();
    send_state();
}

/// The page the editor works on: Peek's while it shows, like Arc's from its library, otherwise
/// the shown one.
fn edited_page() -> Option<Browser> {
    PEEK.with_borrow(|peek| peek.as_ref().map(|peek| peek.browser.clone()))
        .or_else(content)
}

/// The site of the page the editor works on.
fn edited_site() -> Option<String> {
    let page = edited_page()?;
    let url = CefString::from(&page.main_frame()?.url()).to_string();
    boosts::site(&url)
}

/// Takes the editor's change to its Boost; a gesture's changes undo as one.
pub(super) fn change_boost(boost: Boost, gesture: bool) {
    let Some(profile) = EDITOR.with_borrow_mut(|editor| {
        let editor = editor
            .as_mut()
            .filter(|editor| editor.editing.boost.id == boost.id)?;
        editor.editing.change(boost.clone(), gesture);
        Some(editor.profile)
    }) else {
        return;
    };
    // Code typed is saved as it goes, its JavaScript running once the page reloads, like Arc's.
    apply_boosts(profile, &boost.site, !gesture, |boosts| {
        boosts.update(boost.clone())
    });
    send_editor();
}

/// What the editor's header asks of its Boost.
pub(super) fn boost_editor_action(action: BoostEditorAction) {
    if action == BoostEditorAction::Close {
        return close_editor();
    }
    if let BoostEditorAction::Zap(on) = action {
        set_picking(false);
        return zap(on);
    }
    if action == BoostEditorAction::Pick {
        set_picking(true);
        return zap(true);
    }
    if action == BoostEditorAction::Globals {
        return ask_globals();
    }
    if action == BoostEditorAction::Reload {
        if let Some(page) = edited_page() {
            page.reload();
        }
        return;
    }
    let Some((profile, boost)) = EDITOR.with_borrow_mut(|editor| {
        let editor = editor.as_mut()?;
        match action {
            BoostEditorAction::Undo => editor.editing.undo(),
            BoostEditorAction::Redo => editor.editing.redo(),
            BoostEditorAction::Reset => {
                let boost = &editor.editing.boost;
                let reset = Boost {
                    zaps: Vec::new(),
                    colors: None,
                    font: None,
                    text_case: None,
                    text_size: None,
                    css: String::new(),
                    js: String::new(),
                    ..boost.clone()
                };
                editor.editing.change(reset, false);
            }
            _ => {}
        }
        Some((editor.profile, editor.editing.boost.clone()))
    }) else {
        return;
    };
    update_boosts(profile, &boost.site, |boosts| boosts.update(boost.clone()));
    send_editor();
}

/// Closes the editor; a Boost that changes nothing goes with it.
pub(super) fn close_editor() {
    let Some(editor) = EDITOR.take() else { return };
    if let Some(page) = &editor.zapping {
        stop_zap(page);
    }
    let boost = editor.editing.boost;
    let kept = with_boosts(editor.profile, |boosts| boosts.get(boost.id).cloned());
    if kept.is_some_and(|kept| kept.is_empty()) {
        update_boosts(editor.profile, &boost.site, |boosts| {
            boosts.delete(boost.id)
        });
    }
    send_surface("boost", "arbalete:boost-editor", "null");
}

pub(super) fn set_boost_active(id: BoostId, active: bool) {
    let profile = displayed_profile();
    let Some(site) = with_boosts(profile, |boosts| boosts.get(id).map(|b| b.site.clone())) else {
        return;
    };
    update_boosts(profile, &site, |boosts| boosts.set_active(id, active));
    // Turning one on turns the site's others off, the edited one maybe.
    EDITOR.with_borrow_mut(|editor| {
        if let Some(editor) = editor.as_mut() {
            let kept = with_boosts(profile, |boosts| {
                boosts.get(editor.editing.boost.id).map(|b| b.active)
            });
            editor.editing.boost.active = kept.unwrap_or(false);
        }
    });
    boosts_changed();
}

pub(super) fn rename_boost(id: BoostId, title: &str) {
    let profile = displayed_profile();
    let title = title.trim();
    let Some(mut boost) =
        with_boosts(profile, |boosts| boosts.get(id).cloned()).filter(|_| !title.is_empty())
    else {
        return;
    };
    boost.title = title.to_string();
    update_boosts(profile, &boost.site.clone(), |boosts| boosts.update(boost));
    EDITOR.with_borrow_mut(|editor| {
        if let Some(editor) = editor
            .as_mut()
            .filter(|editor| editor.editing.boost.id == id)
        {
            editor.editing.boost.title = title.to_string();
        }
    });
    boosts_changed();
}

pub(super) fn delete_boost(id: BoostId) {
    let profile = displayed_profile();
    let Some(site) = with_boosts(profile, |boosts| boosts.get(id).map(|b| b.site.clone())) else {
        return;
    };
    if EDITOR.with_borrow(|editor| {
        editor
            .as_ref()
            .is_some_and(|editor| editor.editing.boost.id == id)
    }) {
        close_editor();
        send_surface("boost", "arbalete:boost-editor", "null");
    }
    update_boosts(profile, &site, |boosts| boosts.delete(id));
    boosts_changed();
}

/// The panels listing Boosts show them again: the site controls, the library, and the tabs'
/// site controls' buttons.
fn boosts_changed() {
    send_editor();
    send_state();
    if let Some(url) = selected_url() {
        send_site_controls(&url);
    }
    send_boosts();
}

/// Whether a Boost is on for a page's site, which its site controls' button shows.
pub(super) fn boosted(profile: ProfileId, url: &str) -> bool {
    let Some(site) = boosts::site(url) else {
        return false;
    };
    // Read with the profile's first page, before which none shows.
    BOOSTS.with_borrow(|all| {
        all.get(&profile)
            .is_some_and(|boosts| boosts.for_site(&site).any(|boost| boost.active))
    })
}

/// What the site controls show of the shown page's site's Boosts, `None` when it cannot have any.
pub(super) fn site_boosts(url: &str) -> Option<serde_json::Value> {
    let profile = displayed_profile();
    let site = boosts::site(url).filter(|_| !incognito(profile))?;
    let list: Vec<_> = with_boosts(profile, |boosts| {
        boosts
            .for_site(&site)
            .map(|b| json!({ "id": b.id, "title": b.title, "active": b.active }))
            .collect()
    });
    Some(json!(list))
}

/// Sends the library the displayed profile's Boosts.
pub(super) fn send_boosts() {
    let profile = displayed_profile();
    let list: Vec<_> = with_boosts(profile, |boosts| {
        boosts
            .list()
            .iter()
            .map(|b| json!({ "id": b.id, "site": b.site, "title": b.title, "active": b.active }))
            .collect()
    });
    dispatch(&win().sidebar, "arbalete:boosts", &json!(list).to_string());
}

/// Sends the editor its Boost and where the page is, or closes it once the shown page is of
/// another site; after every command and layout.
pub(super) fn place_boost_editor() {
    let editing =
        EDITOR.with_borrow(|editor| editor.as_ref().map(|e| e.editing.boost.site.clone()));
    let Some(site) = editing else { return };
    let shown = edited_site();
    if shown.is_some_and(|shown| shown != site) {
        return close_editor();
    }
    if let Some(page) = page_frame() {
        let frame = json!({ "x": page.x, "y": page.y, "width": page.width, "height": page.height });
        send_surface("boost", "arbalete:boost-frame", &frame.to_string());
    }
}

fn send_editor() {
    let detail = EDITOR.with_borrow(|editor| {
        editor.as_ref().map_or(json!(null), |editor| {
            json!({
                "boost": editor.editing.boost,
                "canUndo": editor.editing.can_undo(),
                "canRedo": editor.editing.can_redo(),
                "zapping": editor.zapping.is_some(),
            })
        })
    });
    send_surface("boost", "arbalete:boost-editor", &detail.to_string());
    place_boost_editor();
    send_zaps();
}

/// Gives the zap picker, while it runs, the Boost's zaps, which its bar lists to restore them.
fn send_zaps() {
    let Some((page, zaps)) = EDITOR.with_borrow(|editor| {
        let editor = editor.as_ref()?;
        Some((editor.zapping.clone()?, editor.editing.boost.zaps.clone()))
    }) else {
        return;
    };
    if let Some(context) = ZAP_CONTEXT.get() {
        let expression = format!("globalThis.arbaleteZap?.zaps({})", json!(zaps));
        send(
            &page,
            "Runtime.evaluate",
            json!({ "expression": expression, "contextId": context }),
        );
    }
}

/// Starts the zap picker in the shown page, in a world of its own, or stops it.
fn zap(on: bool) {
    let page = EDITOR.with_borrow_mut(|editor| editor.as_mut().map(|editor| editor.zapping.take()));
    let Some(page) = page else { return };
    if let Some(page) = page {
        stop_zap(&page);
    }
    if on && let Some(browser) = edited_page() {
        send(&browser, "Runtime.enable", json!({}));
        send(
            &browser,
            "Runtime.addBinding",
            json!({ "name": ZAP_BINDING, "executionContextName": ZAP_WORLD_NAME }),
        );
        ZAP_FRAMES.set(send(&browser, "Page.getFrameTree", json!({})));
        EDITOR.with_borrow_mut(|editor| {
            if let Some(editor) = editor.as_mut() {
                editor.zapping = Some(browser);
            }
        });
    }
    send_editor();
}

/// Stops the zap picker in a page: its world stays until the next document.
fn stop_zap(page: &Browser) {
    if let Some(context) = ZAP_CONTEXT.take() {
        send(
            page,
            "Runtime.evaluate",
            json!({ "expression": "globalThis.arbaleteZap?.stop()", "contextId": context }),
        );
    }
    send(
        page,
        "Runtime.removeBinding",
        json!({ "name": ZAP_BINDING }),
    );
    send(page, "Runtime.disable", json!({}));
}

thread_local! {
    /// The zap picker's world in the shown page, while it runs.
    static ZAP_CONTEXT: Cell<Option<i64>> = const { Cell::new(None) };
    /// The messages that start the zap picker, step by step: asking for the page's frame, then
    /// for a world of its own there.
    static ZAP_FRAMES: Cell<i32> = const { Cell::new(0) };
    static ZAP_WORLD: Cell<i32> = const { Cell::new(0) };
}

/// The page answered a step of the zap picker's start, or its picker spoke.
fn zap_message(browser: &Browser, message: &serde_json::Value) {
    if message["id"] == ZAP_FRAMES.get()
        && let Some(frame) = message["result"]["frameTree"]["frame"]["id"].as_str()
    {
        ZAP_WORLD.set(send(
            browser,
            "Page.createIsolatedWorld",
            json!({ "frameId": frame, "worldName": ZAP_WORLD_NAME }),
        ));
    } else if message["id"] == ZAP_WORLD.get()
        && let Some(context) = message["result"]["executionContextId"].as_i64()
    {
        ZAP_CONTEXT.set(Some(context));
        send(
            browser,
            "Runtime.evaluate",
            json!({ "expression": ZAP_PICKER, "contextId": context }),
        );
        if EDITOR.with_borrow(|editor| editor.as_ref().is_some_and(|e| e.picking)) {
            let expression = "globalThis.arbaleteZap.pick()";
            send(
                browser,
                "Runtime.evaluate",
                json!({ "expression": expression, "contextId": context }),
            );
        }
        send_zaps();
    } else if message["method"] == "Runtime.bindingCalled"
        && message["params"]["name"] == ZAP_BINDING
    {
        let payload: serde_json::Value = message["params"]["payload"]
            .as_str()
            .and_then(|p| serde_json::from_str(p).ok())
            .unwrap_or_default();
        if let Some(selector) = payload["zap"].as_str() {
            add_zap(selector);
        } else if let Some(selector) = payload["pick"].as_str() {
            send_surface("boost", "arbalete:boost-pick", &json!(selector).to_string());
            set_picking(false);
            zap(false);
        } else if let Some(index) = payload["unzap"].as_u64() {
            unzap(index as usize);
        } else if payload["cancel"] == true {
            zap(false);
        }
    } else if message["method"] == "Page.frameNavigated"
        && message["params"]["frame"]["parentId"].is_null()
    {
        // The picker went with the document.
        let zapping =
            EDITOR.with_borrow(|editor| editor.as_ref().is_some_and(|e| e.zapping.is_some()));
        if zapping {
            ZAP_CONTEXT.set(None);
            zap(false);
        }
    }
}

fn set_picking(on: bool) {
    EDITOR.with_borrow_mut(|editor| {
        if let Some(editor) = editor.as_mut() {
            editor.picking = on;
        }
    });
}

thread_local! {
    /// The message asking the edited page for its own globals.
    static GLOBALS: Cell<i32> = const { Cell::new(0) };
}

/// The page's own globals, which the DOM's types leave out: those a blank frame's window has not.
const PAGE_GLOBALS: &str = "(() => { const frame = document.createElement('iframe'); \
     frame.style.display = 'none'; document.documentElement.append(frame); \
     const standard = new Set(Object.getOwnPropertyNames(frame.contentWindow)); frame.remove(); \
     return Object.getOwnPropertyNames(window) \
     .filter((name) => !standard.has(name) && /^[A-Za-z_$][\\w$]*$/.test(name)).slice(0, 500); })()";

/// Asks the edited page for its own globals, which the code's JavaScript completes.
fn ask_globals() {
    if let Some(page) = edited_page() {
        let params = json!({ "expression": PAGE_GLOBALS, "returnByValue": true });
        GLOBALS.set(send(&page, "Runtime.evaluate", params));
    }
}

fn unzap(index: usize) {
    let Some(mut boost) =
        EDITOR.with_borrow(|editor| editor.as_ref().map(|e| e.editing.boost.clone()))
    else {
        return;
    };
    if index < boost.zaps.len() {
        boost.zaps.remove(index);
        change_boost(boost, false);
    }
}

fn add_zap(selector: &str) {
    let Some(mut boost) =
        EDITOR.with_borrow(|editor| editor.as_ref().map(|e| e.editing.boost.clone()))
    else {
        return;
    };
    if !boost.zaps.iter().any(|zap| zap == selector) {
        boost.zaps.push(selector.to_string());
        change_boost(boost, false);
    }
}

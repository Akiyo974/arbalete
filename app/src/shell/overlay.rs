//! The Command Bar, like Arc's, over the page: its results, its suggestions, and the tab switcher.

use super::*;

/// The Command Bar's width, at most, its search row's height, which its results add to, and
/// the height its box keeps at most, centered in the window, like Arc's.
const OVERLAY_WIDTH: i32 = 766;
pub(super) const OVERLAY_ROW: i32 = 50;
const OVERLAY_BOX: i32 = 338;

/// Editing the address, the Command Bar sits over the docked sidebar's url bar instead, at
/// least as wide as this and at most as `URL_EDIT_MAX`, like Arc's: its left edge and top are
/// the url bar's.
const URL_EDIT_WIDTH: i32 = 350;
const URL_EDIT_MAX: i32 = 720;
const URL_BAR_LEFT: i32 = 8;
const URL_BAR_TOP: i32 = 44;

/// How many tabs selected last the tab switcher lists, at most.
const RECENT_TABS: usize = 10;

/// The open tabs and history pages the Command Bar shows for a text, at most.
const OVERLAY_TABS: usize = 3;
const OVERLAY_PAGES: usize = 5;

/// Puts a tab first among the tabs selected last, which the tab switcher lists.
pub(super) fn remember(recent: &mut Vec<NodeId>, tab: NodeId) {
    recent.retain(|&id| id != tab);
    recent.insert(0, tab);
    recent.truncate(RECENT_TABS);
}

/// The picked tab's index after a step forward or back among `count` tabs, round the list.
fn step(index: usize, count: usize, forward: bool) -> usize {
    if forward {
        (index + 1) % count
    } else {
        (index + count - 1) % count
    }
}

/// ⌃⇥ shows the tabs selected last, still open, the most recent first, in the Command Bar's panel,
/// and picks the one before the shown tab; each ⌃⇥ more picks the next one, ⌃⇧⇥ the one before.
/// Releasing ⌃ shows the picked tab (end_tab_switch).
pub(super) fn switch_tab(forward: bool) {
    let switch = win().switch.take().or_else(|| {
        let tabs: Vec<_> = STATE.with_borrow_mut(|state| {
            win()
                .recent
                .with_borrow(Clone::clone)
                .into_iter()
                .filter(|&id| state.sidebar.tab_mut(id).is_some())
                .collect()
        });
        (tabs.len() > 1).then_some((tabs, 0))
    });
    let Some((tabs, index)) = switch else { return };
    let index = step(index, tabs.len(), forward);
    let rows: Vec<_> = STATE.with_borrow(|state| {
        let profile = |id| state.sidebar.profile_of(id);
        tabs.iter()
            .filter_map(|&id| {
                let NodeKind::Tab(tab) = &state.sidebar.node(id)?.kind else {
                    return None;
                };
                let favicon = favicon_url(profile(id)?, tab);
                Some(json!({ "id": id, "url": tab.url, "title": tab.label(), "favicon": favicon }))
            })
            .collect()
    });
    let detail = json!({ "tabs": rows, "picked": index });
    dispatch(&win().overlay_view, "arbalete:switch", &detail.to_string());
    if let Some(overlay) = win().overlay.with_borrow(Clone::clone)
        && overlay.is_visible() == 0
    {
        win().over_url_bar.set(false);
        place_overlay(OVERLAY_ROW);
        overlay.set_visible(1);
        round_overlay();
    }
    win().switch.set(Some((tabs, index)));
}

/// Shows the tab picked in the switcher, if it is shown.
pub(super) fn end_tab_switch() {
    if let Some((tabs, index)) = win().switch.take() {
        show_overlay(false);
        select_tab(tabs[index]);
    }
}

/// Shows the Command Bar above the page, empty to open a tab, or hides it.
pub(super) fn show_overlay(visible: bool) {
    open_overlay(visible, None);
}

/// Shows the Command Bar holding the selected tab's address, to edit it; without one, empty.
pub(super) fn edit_url() {
    let tab = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let id = sidebar.displayed().selected?;
        let NodeKind::Tab(tab) = &sidebar.node(id)?.kind else {
            return None;
        };
        Some((tab.url.clone(), favicon_url(sidebar.profile_of(id)?, tab)))
    });
    open_overlay(true, tab);
}

/// Shows the Command Bar, or hides it. Holding a tab's address and icon, it edits the address,
/// over the docked sidebar's url bar; empty, it opens a tab, centered on the window.
fn open_overlay(visible: bool, tab: Option<(String, Option<String>)>) {
    let Some(overlay) = win().overlay.with_borrow(Clone::clone) else {
        return;
    };
    if visible {
        // The sidebar shows its url bar while docked, unless the bar above the page holds it.
        let over_url_bar = tab.is_some()
            && win().sidebar_mode.get() == SidebarMode::Shown
            && !app_settings().show_toolbar;
        win().over_url_bar.set(over_url_bar);
        place_overlay(OVERLAY_ROW);
        let (url, favicon) = tab.unzip();
        let engine = STATE.with_borrow(|state| {
            let profile = state.sidebar.displayed().profile;
            let profiles = state.sidebar.profiles();
            profiles
                .iter()
                .find(|p| p.id == profile)
                .map(|p| p.search_engine)
        });
        let engine = engine.unwrap_or_default();
        let search = json!({ "name": engine.name(), "url": engine.url() });
        let empty = url.is_none();
        let detail = json!({ "url": url, "favicon": favicon.flatten(), "search": search });
        dispatch(&win().overlay_view, "arbalete:show", &detail.to_string());
        // Like Arc's, an empty bar offers the sites visited most.
        if empty {
            search_overlay("", "overlay");
        }
    }
    if !visible {
        win().adding_pane.set(false);
        // The page takes the keyboard back from the Command Bar.
        if overlay.is_visible() == 1
            && let Some(tab) = win().shown.get()
        {
            focus_page(tab);
        }
    }
    // Like Arc's, the window keeps the Command Bar's room while it shows, as wide as it was.
    let window = win().window.with_borrow(Clone::clone);
    let min_width = window
        .as_ref()
        .filter(|_| visible && !win().over_url_bar.get())
        .map(|window| {
            window
                .size()
                .width
                .clamp(MIN_WINDOW_WIDTH, OVERLAY_WIDTH + 160)
        });
    if win().overlay_min_width.replace(min_width) != min_width
        && let Some(window) = window
    {
        #[cfg(target_os = "macos")]
        crate::mac::set_min_size(
            window.window_handle(),
            min_width.unwrap_or(MIN_WINDOW_WIDTH),
            MIN_WINDOW_HEIGHT,
        );
    }
    overlay.set_visible(visible.into());
    #[cfg(target_os = "macos")]
    round_overlay();
}

/// Shows the Command Bar where the app placed it, again after it changes size: the native one,
/// on macOS, over the web one, which stays out of sight.
pub(super) fn round_overlay() {
    #[cfg(target_os = "macos")]
    sync_native_surfaces();
}

/// Centers the Command Bar on the window, or places it over the url bar, `height` tall.
pub(super) fn place_overlay(height: i32) {
    let (Some(overlay), Some(size)) = (
        win().overlay.with_borrow(Clone::clone),
        win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.size())),
    ) else {
        return;
    };
    let bounds = if win().over_url_bar.get() {
        let width = URL_EDIT_WIDTH.max(sidebar_width() - 2 * URL_BAR_LEFT);
        Rect {
            x: URL_BAR_LEFT,
            y: URL_BAR_TOP,
            width: width.min(URL_EDIT_MAX).min(size.width - URL_BAR_LEFT - 80),
            height,
        }
    } else {
        // Inset 160 points from the window's sides, or 20 in a narrow window.
        let margin = if size.width >= OVERLAY_WIDTH { 160 } else { 20 };
        let width = OVERLAY_WIDTH.min(size.width - margin);
        Rect {
            x: (size.width - width) / 2,
            y: ((size.height - OVERLAY_BOX) / 2).max(24),
            width,
            height,
        }
    };
    overlay.set_bounds(Some(&bounds));
}

/// Sends the Command Bar the open tabs, in every Space and the favorites, then the displayed
/// Space's profile history pages, best first, whose address or title holds `text`.
pub(super) fn search_overlay(text: &str, surface: &'static str) {
    let wanted = text.trim().to_lowercase();
    let (tabs, profile) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let tabs: Vec<_> = if wanted.is_empty() {
            Vec::new()
        } else {
            let shown = sidebar.displayed().id;
            sidebar
                .tabs()
                // A favorite belongs to no Space: an incognito window has none.
                .filter(|(id, _)| {
                    sidebar
                        .space_of(*id)
                        .map_or(!sidebar.is_incognito(shown), |space| {
                            sidebar.reaches(shown, space)
                        })
                })
                .filter(|(_, tab)| {
                    tab.label().to_lowercase().contains(&wanted)
                        || tab.url.to_lowercase().contains(&wanted)
                })
                .take(OVERLAY_TABS)
                .filter_map(|(id, tab)| Some((id, tab.clone(), sidebar.profile_of(id)?)))
                .collect()
        };
        (tabs, sidebar.displayed().profile)
    });
    // Nothing typed, like Arc's: the sites visited most, one page each.
    let pages = with_history(profile, |history| {
        let limit = if wanted.is_empty() { 8 } else { 1 } * (OVERLAY_PAGES + OVERLAY_TABS);
        history.suggest(text.trim(), limit, now())
    })
    .and_then(Result::ok)
    .unwrap_or_default();
    let pages: Vec<_> = if wanted.is_empty() {
        let mut sites = std::collections::HashSet::new();
        pages
            .into_iter()
            .filter(|page| sites.insert(site(&page.url).trim_start_matches("www.").to_string()))
            .collect()
    } else {
        pages
    };
    // Like Arc's, each page shows its site's icon: the one Chromium found, if any.
    let chromium = favicon::chromium(&profile_dir(profile));
    let pages: Vec<_> = pages
        .iter()
        .filter(|page| tabs.iter().all(|(_, tab, _)| tab.url != page.url))
        .take(OVERLAY_PAGES)
        .map(|page| {
            let icon = chromium
                .as_ref()
                .and_then(|db| favicon::of_page(&profile_dir(profile), db, &page.url));
            let favicon = icon.map(|source| icon_url(profile, &source));
            json!({ "url": page.url, "title": page.title, "favicon": favicon })
        })
        .collect();
    let tabs: Vec<_> = tabs
        .iter()
        .map(|(id, tab, profile)| {
            let favicon = favicon_url(*profile, tab);
            json!({ "id": id, "url": tab.url, "title": tab.label(), "favicon": favicon })
        })
        .collect();
    let detail = json!({ "text": text, "tabs": tabs, "pages": pages });
    send_surface(surface, "arbalete:results", &detail.to_string());
    suggest(profile, text, surface);
}

/// Asks the displayed Space's profile's search engine for searches completing `text`, unless the
/// profile asks for none: the Command Bar shows them once they come (SuggestionsClient). An
/// address typed is not sent, nor any cookie.
fn suggest(profile: ProfileId, text: &str, surface: &'static str) {
    let typed = text.trim();
    let engine = STATE.with_borrow(|state| {
        let profiles = state.sidebar.profiles();
        let profile = profiles.iter().find(|p| p.id == profile)?;
        profile.search_suggestions.then_some(profile.search_engine)
    });
    let Some(base) = engine.and_then(SearchEngine::suggest_url) else {
        return;
    };
    if typed.is_empty() || typed.contains("://") {
        return;
    }
    let Some(request) = request_create() else {
        return;
    };
    let url = format!("{base}{}", arbalete_core::search::encode(typed));
    request.set_url(Some(&CefString::from(url.as_str())));
    request.set_method(Some(&CefString::from("GET")));
    let mut client = SuggestionsClient::new(text.to_string(), surface, std::rc::Rc::default());
    let _ = urlrequest_create(Some(&mut request.clone()), Some(&mut client), None);
}

wrap_urlrequest_client! {
    struct SuggestionsClient {
        text: String,
        surface: &'static str,
        body: std::rc::Rc<RefCell<Vec<u8>>>,
    }

    impl UrlrequestClient {
        fn on_download_data(&self, _request: Option<&mut Urlrequest>, data: *const u8, data_length: usize) {
            if !data.is_null() {
                // SAFETY: CEF hands the chunk received, `data_length` bytes long.
                let chunk = unsafe { std::slice::from_raw_parts(data, data_length) };
                self.body.borrow_mut().extend_from_slice(chunk);
            }
        }

        fn on_request_complete(&self, _request: Option<&mut Urlrequest>) {
            let body = String::from_utf8_lossy(&self.body.borrow()).into_owned();
            let detail = json!({ "text": self.text, "suggestions": arbalete_core::search::suggestions(&body) });
            send_surface(self.surface, "arbalete:suggestions", &detail.to_string());
        }
    }
}

/// Loads an address in the selected tab, or in a new one when none is selected.
pub(super) fn navigate(url: &str) {
    if is_chrome_page(url) {
        return open_chrome_page(url);
    }
    show_overlay(false);
    let selected = STATE.with_borrow_mut(|state| active_tab(&mut state.sidebar).is_some());
    if content().is_some() {
        load(url);
    } else if selected {
        // Its page is not created yet: it opens there.
        update_sidebar(|sidebar| {
            if let Some(tab) = active_tab(sidebar) {
                tab.url = url.to_string();
            }
        });
    } else {
        open_tab(url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_the_tabs_selected_last_the_most_recent_first() {
        let id = |n: u64| -> NodeId { serde_json::from_str(&n.to_string()).unwrap() };
        let mut recent = Vec::new();
        for n in [1, 2, 3, 1] {
            remember(&mut recent, id(n));
        }
        assert_eq!(recent, [id(1), id(3), id(2)]);
        for n in 10..10 + RECENT_TABS as u64 {
            remember(&mut recent, id(n));
        }
        assert_eq!(recent.len(), RECENT_TABS);
    }

    #[test]
    fn the_tab_switcher_steps_round_the_list() {
        assert_eq!(step(0, 3, true), 1);
        assert_eq!(step(2, 3, true), 0);
        assert_eq!(step(0, 3, false), 2);
    }
}

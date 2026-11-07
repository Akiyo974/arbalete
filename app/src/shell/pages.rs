//! Hidden tabs' pages, frozen or unloaded after a while, and the pictures that cover them.

use super::*;

/// How hard the system presses for memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryPressure {
    Normal,
    Warning,
    Critical,
}

/// How long a page stays hidden before it is frozen, then unloaded, in milliseconds: its tab opens
/// it again at its url when shown. Sooner when the system runs low on memory, every hidden page
/// unloading at once when it is critical; and frozen sooner on battery saving, which stops their
/// work.
fn suspend_delays(pressure: MemoryPressure, low_power: bool) -> (i64, i64) {
    const MINUTE: i64 = 60_000;
    match pressure {
        MemoryPressure::Critical => (0, 0),
        MemoryPressure::Warning => (MINUTE, 5 * MINUTE),
        MemoryPressure::Normal if low_power => (MINUTE, 60 * MINUTE),
        MemoryPressure::Normal => (5 * MINUTE, 60 * MINUTE),
    }
}

/// The suspension delays as the system stands now.
fn current_suspend_delays() -> (i64, i64) {
    #[cfg(target_os = "macos")]
    return suspend_delays(crate::mac::memory_pressure(), crate::mac::low_power());
}

/// Asks each page hidden long enough, but not frozen yet or due to unload, whether it plays media
/// (page_playing): a page playing goes on. A frozen page, which plays nothing and answers nothing,
/// unloads once due. A page capturing, or of a site kept awake, is never suspended, nor is any
/// when the settings say so.
pub(super) fn suspend_hidden_pages() {
    let settings = app_settings();
    if !settings.suspend_tabs {
        return;
    }
    let now = now();
    let (freeze_after, unload_after) = current_suspend_delays();
    let (ask, unload): (Vec<_>, Vec<_>) = PAGES.with_borrow(|pages| {
        pages
            .iter()
            .filter(|(tab, live)| {
                !live.capturing
                    && tab_url(**tab)
                        .is_none_or(|url| !settings.awake_sites.iter().any(|s| s == site(&url)))
            })
            .filter_map(|(tab, live)| {
                let hidden = now - live.hidden_since?;
                let due = hidden >= unload_after;
                (due || (hidden >= freeze_after && !live.frozen))
                    .then(|| (*tab, live.browser.clone(), live.frozen && due))
            })
            .partition(|(_, _, unload)| !unload)
    });
    for (tab, _, _) in unload {
        close_page(tab);
        fall_asleep(tab);
    }
    // A player tells the system what it plays through the Media Session API, even without an
    // element in the page, like a Web Audio player.
    let expression = r#"navigator.mediaSession?.playbackState === "playing"
        || [...document.querySelectorAll("video, audio")].some((media) => !media.paused && !media.ended)"#;
    let params = serde_json::json!({ "expression": expression, "returnByValue": true });
    let message = serde_json::json!({ "id": extensions::PLAYING, "method": "Runtime.evaluate", "params": params });
    for (_, browser, _) in ask {
        if let Some(host) = browser.host() {
            host.send_dev_tools_message(Some(message.to_string().as_bytes()));
        }
    }
}

/// Asks the shown page for a picture of itself (page_snapshot), while it draws: a hidden page
/// draws nothing, and would never answer.
pub(super) fn snapshot_shown() {
    let params = serde_json::json!({ "format": "jpeg", "quality": 70 });
    let message = serde_json::json!({ "id": extensions::SNAPSHOT, "method": "Page.captureScreenshot", "params": params });
    if let Some(host) = content().and_then(|browser| browser.host()) {
        host.send_dev_tools_message(Some(message.to_string().as_bytes()));
    }
}

/// Covers a page opening again after it was unloaded with its picture as soon as it is made, or
/// takes the picture away once it has loaded.
#[cfg(target_os = "macos")]
pub(super) fn cover_revived(browser: Option<&Browser>, loading: bool) {
    let Some(browser) = browser else { return };
    // A page being made, which its view does not hold yet, is MAKING's.
    let tab = page_tab(browser).or(MAKING.get());
    let (Some(tab), Some(host)) = (tab, browser.host()) else {
        return;
    };
    if !loading {
        crate::mac::uncover_page(host.window_handle());
    } else if REVIVED.with_borrow_mut(|revived| revived.remove(&tab))
        && let Some(jpeg) = SNAPSHOTS.with_borrow(|snapshots| snapshots.get(&tab).cloned())
    {
        crate::mac::cover_page(host.window_handle(), &jpeg, None);
    }
}

/// Asks a page for the color at its top, like Arc's bar samples it: the background of what
/// shows there, or of the page, unless it is transparent.
pub(super) fn ask_top_color(browser: Option<&Browser>) {
    let expression = r#"(() => {
        const fill = (node) => {
            const color = node && getComputedStyle(node).backgroundColor;
            return color && !/^rgba\(.*, 0\)$|^transparent$/.test(color) ? color : null;
        };
        for (let node = document.elementFromPoint(innerWidth / 2, 1); node; node = node.parentElement) {
            const color = fill(node);
            if (color) return color;
        }
        return fill(document.body) ?? fill(document.documentElement);
    })()"#;
    let params = serde_json::json!({ "expression": expression, "returnByValue": true });
    let message = serde_json::json!({ "id": extensions::TOP_COLOR, "method": "Runtime.evaluate", "params": params });
    if let Some(host) = browser.and_then(Browser::host) {
        host.send_dev_tools_message(Some(message.to_string().as_bytes()));
    }
}

/// Keeps the color at the top of a page, a CSS rgb() or rgba() color, for its tab's bar.
pub fn page_top_color(page: i32, color: &str) {
    let tab = PAGES.with_borrow(|pages| {
        pages
            .iter()
            .find_map(|(tab, live)| (live.browser.identifier() == page).then_some(*tab))
    });
    let Some(hex) = css_hex(color) else { return };
    let Some(tab) = tab else {
        // Peek's page tints its bar.
        let changed = PEEK.with_borrow_mut(|peek| {
            let peek = peek
                .as_mut()
                .filter(|peek| peek.browser.identifier() == page)?;
            (peek.color.as_ref() != Some(&hex)).then(|| peek.color = Some(hex))
        });
        if changed.is_some() {
            send_peek();
        }
        return;
    };
    if TOP_COLORS.with_borrow_mut(|colors| remember(colors, tab, hex, TOP_COLORS_SIZE)) {
        pane_changed(tab);
    }
}

/// How many pages' top colors are kept, the least recently sent dropped beyond: Arc's.
const TOP_COLORS_SIZE: usize = 50;

/// The color at the top of a tab's page, as #rrggbb, if it is known.
pub(super) fn top_color(tab: NodeId) -> Option<String> {
    TOP_COLORS.with_borrow(|colors| {
        let (_, color) = colors.iter().find(|(id, _)| *id == tab)?;
        Some(color.clone())
    })
}

/// Keeps `value` for `key` as the most recent of `cache`, the most recent last, dropping the
/// least recent beyond `size`. Returns whether the value changed.
fn remember<K: PartialEq, V: PartialEq>(
    cache: &mut Vec<(K, V)>,
    key: K,
    value: V,
    size: usize,
) -> bool {
    let old = cache
        .iter()
        .position(|(k, _)| *k == key)
        .map(|index| cache.remove(index).1);
    let changed = old.as_ref() != Some(&value);
    cache.push((key, value));
    let over = cache.len().saturating_sub(size);
    cache.drain(..over);
    changed
}

/// A CSS rgb() or rgba() color as #rrggbb.
fn css_hex(color: &str) -> Option<String> {
    let inside = color.split_once('(')?.1.strip_suffix(')')?;
    let channels: Vec<u8> = inside
        .split(',')
        .take(3)
        .map(|channel| channel.trim().parse::<f64>().ok().map(|c| c.round() as u8))
        .collect::<Option<_>>()?;
    let [r, g, b] = channels[..] else { return None };
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Keeps a page's picture, a JPEG in base64, for its tab.
pub fn page_snapshot(page: i32, jpeg: &str) {
    let tab = PAGES.with_borrow(|pages| {
        pages
            .iter()
            .find_map(|(tab, live)| (live.browser.identifier() == page).then_some(*tab))
    });
    if let Some(tab) = tab {
        SNAPSHOTS.with_borrow_mut(|snapshots| snapshots.insert(tab, jpeg.to_owned()));
    }
}

/// A tab's url, if it is still a tab.
fn tab_url(tab: NodeId) -> Option<String> {
    STATE.with_borrow(|state| match &state.sidebar.node(tab)?.kind {
        NodeKind::Tab(tab) => Some(tab.url.clone()),
        _ => None,
    })
}

/// Dims a tab whose page was frozen or unloaded in the sidebar.
pub(super) fn fall_asleep(tab: NodeId) {
    if ASLEEP.with_borrow_mut(|asleep| asleep.insert(tab)) {
        send_state();
    }
}

/// Makes the frozen pages whose url `wake` picks run again.
pub(super) fn wake_frozen(wake: impl Fn(&str) -> bool) {
    let woken: Vec<_> = PAGES.with_borrow_mut(|pages| {
        pages
            .iter_mut()
            .filter(|(tab, live)| live.frozen && tab_url(**tab).is_some_and(|url| wake(&url)))
            .map(|(tab, live)| {
                live.frozen = false;
                (*tab, live.browser.host())
            })
            .collect()
    });
    for (tab, host) in woken {
        set_frozen(host.as_ref(), false);
        ASLEEP.with_borrow_mut(|asleep| asleep.remove(&tab));
    }
    send_state();
}

/// Freezes or unloads a hidden page that plays no media, as long as it has been hidden; the page
/// shown again meanwhile stays.
pub fn page_playing(page: i32, playing: bool) {
    let found = PAGES.with_borrow(|pages| {
        pages.iter().find_map(|(tab, live)| {
            let browser = live.browser.clone();
            (browser.identifier() == page).then(|| (*tab, live.hidden_since, browser.host()))
        })
    });
    let Some((tab, Some(hidden_since), host)) = found.filter(|_| !playing) else {
        return;
    };
    if now() - hidden_since >= current_suspend_delays().1 {
        close_page(tab);
    } else {
        set_frozen(host.as_ref(), true);
        PAGES.with_borrow_mut(|pages| {
            if let Some(live) = pages.get_mut(&tab) {
                live.frozen = true;
            }
        });
    }
    fall_asleep(tab);
}

/// Freezes a page through Chromium's DevTools, or makes it run again.
pub(super) fn set_frozen(host: Option<&BrowserHost>, frozen: bool) {
    let state = if frozen { "frozen" } else { "active" };
    let message = serde_json::json!({ "id": 0, "method": "Page.setWebLifecycleState", "params": { "state": state } });
    if let Some(host) = host {
        host.send_dev_tools_message(Some(message.to_string().as_bytes()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_the_most_recent_values_dropping_the_least_recent() {
        let mut cache = Vec::new();
        assert!(remember(&mut cache, 1, "a", 2));
        assert!(remember(&mut cache, 2, "b", 2));
        // Sent again, unchanged, it becomes the most recent.
        assert!(!remember(&mut cache, 1, "a", 2));
        assert!(remember(&mut cache, 3, "c", 2));
        assert_eq!(cache, [(1, "a"), (3, "c")]);
        assert!(remember(&mut cache, 1, "d", 2));
        assert_eq!(cache, [(3, "c"), (1, "d")]);
    }

    #[test]
    fn pages_suspend_sooner_on_low_memory_or_battery_saving() {
        let minute = 60_000;
        assert_eq!(
            suspend_delays(MemoryPressure::Normal, false),
            (5 * minute, 60 * minute)
        );
        assert_eq!(
            suspend_delays(MemoryPressure::Normal, true),
            (minute, 60 * minute)
        );
        assert_eq!(
            suspend_delays(MemoryPressure::Warning, true),
            (minute, 5 * minute)
        );
        assert_eq!(suspend_delays(MemoryPressure::Critical, false), (0, 0));
    }
}

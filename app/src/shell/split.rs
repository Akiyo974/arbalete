//! Split View, like Arc's: a split's pages side by side or one above the other, each its own card.

use super::*;

/// Opens a tab beside the selected one, in its split, like Arc's Add Split View; or alone when
/// they cannot share one, like a favorite's or a split already full.
pub(super) fn open_in_pane(url: &str) {
    update_sidebar(|sidebar| {
        let space = sidebar.displayed().id;
        let beside = sidebar.displayed().selected;
        let tab = sidebar.open_tab(space, url).expect("the displayed Space");
        if let Some(beside) = beside {
            let _ = sidebar.split(tab, beside);
        }
        sidebar.select(space, tab).expect("a new tab");
    });
    show_selected();
}

/// Tells that a split already has as many pages as it can, like Arc's.
pub(super) fn tell_split_full() {
    toast(
        json!({ "text": "Cannot Add New Pane", "icon": "exclamationmark.triangle", "error": true }),
    );
}

/// Whether the window shows a split, and then whether one page above the other.
pub fn split_shown() -> Option<bool> {
    win().split.borrow().1.as_ref().map(|layout| layout.stacked)
}

/// Lays the pages of `split`, or of the split shown, out one above the other, or side by side
/// again.
pub(super) fn flip_panes(split: Option<NodeId>) {
    let shown = win().split.borrow().0.first().copied();
    let flipped = STATE.with_borrow_mut(|state| {
        let sidebar = &mut state.sidebar;
        let split = split
            .or_else(|| Some(sidebar.row_of(shown?)))
            .ok_or(Error::UnknownNode)?;
        let stacked = sidebar.split_layout(split).stacked;
        sidebar.set_split_stacked(split, !stacked)
    });
    if flipped.is_ok() {
        save_session();
        show_selected();
    }
}

/// Shows a split's `tabs` as `layout` lays them out, `focused`'s with the keyboard, like Arc's
/// Split View: each its own card, with room between them.
pub(super) fn show_split(focused: NodeId, tabs: &[NodeId], layout: SplitLayout) {
    let same = *win().split.borrow() == (tabs.to_vec(), Some(layout.clone()));
    if !same {
        hide_content();
        win().pane_preview.set(None);
        for &tab in tabs {
            take_page(tab);
            wake_page(tab);
        }
        *win().split.borrow_mut() = (tabs.to_vec(), Some(layout));
        place_card();
    }
    win().shown.set(Some(focused));
    shown_on_card(focused);
}

/// The places of the split shown, in the window's points, in order: each of its pages' by its
/// index, and the empty page a tab dragged over it would open, without one.
fn split_slots() -> Vec<(Option<usize>, Rect)> {
    let (tabs, layout) = win().split.borrow().clone();
    // A page in full screen fills the card alone.
    let (Some(layout), Some(card), false) = (layout, card_rect(), page_full_screen()) else {
        return Vec::new();
    };
    let room = if layout.stacked {
        card.height
    } else {
        card.width
    };
    // A tab dragged over the split: like Arc's, an empty page opens where it would go, all the
    // pages sharing the room evenly.
    let preview = win().pane_preview.get().map(|index| index.min(tabs.len()));
    let count = tabs.len() + usize::from(preview.is_some());
    let lengths = match preview {
        Some(_) => pane_lengths(room, &vec![1.0 / count as f64; count]),
        None => pane_lengths(room, &layout.factors),
    };
    let mut start = 0;
    let mut tab = 0;
    lengths
        .into_iter()
        .enumerate()
        .map(|(at, length)| {
            let rect = if layout.stacked {
                Rect {
                    x: card.x,
                    y: card.y + start,
                    width: card.width,
                    height: length,
                }
            } else {
                Rect {
                    x: card.x + start,
                    y: card.y,
                    width: length,
                    height: card.height,
                }
            };
            start += length + SPLIT_GAP;
            let index = (preview != Some(at)).then(|| {
                tab += 1;
                tab - 1
            });
            (index, rect)
        })
        .collect()
}

/// Where each of the split's pages shows, in the window's points, in order.
pub(super) fn split_rects() -> Vec<Rect> {
    split_slots()
        .into_iter()
        .filter_map(|(index, rect)| index.map(|_| rect))
        .collect()
}

/// How long each of a split's pages is along `room`, by its part, SPLIT_GAP apart; the last
/// takes what is left.
fn pane_lengths(room: i32, factors: &[f64]) -> Vec<i32> {
    let count = factors.len() as i32;
    let shared = f64::from(room - SPLIT_GAP * (count - 1).max(0));
    let mut left = room - SPLIT_GAP * (count - 1).max(0);
    factors
        .iter()
        .enumerate()
        .map(|(index, factor)| {
            let length = if index as i32 == count - 1 {
                left
            } else {
                (shared * factor).round() as i32
            };
            left -= length;
            length.max(1)
        })
        .collect()
}

/// Opens the empty page standing where a tab dragged over the split would go, at `index` among
/// its pages, or closes it.
pub(super) fn preview_pane(index: Option<usize>) {
    if win().pane_preview.replace(index) != index {
        place_card();
    }
}

/// Tells the native interface where the card is and where its pages show, a page alone or a
/// split's, and how each stands, for their containers, bars and the focused one's outline.
pub(super) fn send_panes() {
    #[cfg(target_os = "macos")]
    {
        // A page in full screen shows alone.
        let tabs = if page_full_screen() {
            Vec::new()
        } else {
            win().split.borrow().0.clone()
        };
        let focused = win().shown.get();
        let split = tabs
            .first()
            .map(|&tab| STATE.with_borrow(|state| state.sidebar.row_of(tab)));
        // In the card's view, which holds the pages.
        let card = card_rect().unwrap_or_default();
        let mut panes: Vec<_> = tabs
            .iter()
            .zip(split_rects())
            .map(|(&tab, rect)| {
                let browser = PAGES.with_borrow(|pages| Some(pages.get(&tab)?.browser.clone()));
                let url = STATE
                    .with_borrow_mut(|state| state.sidebar.tab_mut(tab).map(|tab| tab.url.clone()));
                let can = |back: bool| {
                    browser.as_ref().is_some_and(|browser| {
                        (if back {
                            browser.can_go_back()
                        } else {
                            browser.can_go_forward()
                        }) == 1
                    })
                };
                let developer = url.as_deref().is_some_and(developer_mode);
                json!({
                    "tab": tab, "split": split,
                    "x": rect.x - card.x, "y": rect.y - card.y, "width": rect.width, "height": rect.height,
                    "bar": PANE_BAR_HEIGHT,
                    "url": url.unwrap_or_default(),
                    "back": can(true), "forward": can(false),
                    "loading": browser.as_ref().is_some_and(|browser| browser.is_loading() == 1),
                    "focused": focused == Some(tab),
                    "color": top_color(tab),
                    "developer": developer,
                    "toolbar": false, "alone": false, "square": false,
                    "cardX": card.x, "cardY": card.y,
                })
            })
            .collect();
        // A page alone, where the card lays it out.
        if let Some(tab) = focused.filter(|_| tabs.is_empty())
            && card_rect().is_some()
        {
            let bar = shown_toolbar_height();
            panes.push(json!({
                "tab": tab, "split": null,
                "x": 0, "y": bar, "width": card.width, "height": card.height - bar,
                "bar": 0, "url": "", "back": false, "forward": false, "loading": false,
                "focused": true, "color": null, "developer": false, "toolbar": false,
                "alone": true, "square": square_card(), "cardX": card.x, "cardY": card.y,
            }));
        }
        // The empty page standing where a dragged tab would open, known by its tab, -1, which
        // the interface draws.
        if let Some((_, frame)) = split_slots().into_iter().find(|(index, _)| index.is_none()) {
            panes.push(json!({
                "tab": -1, "split": null,
                "x": frame.x - card.x, "y": frame.y - card.y,
                "width": frame.width, "height": frame.height,
                "bar": 0, "url": "", "back": false, "forward": false, "loading": false,
                "focused": false, "color": null, "developer": false, "toolbar": false,
                "alone": false, "square": false, "cardX": card.x, "cardY": card.y,
            }));
        }
        // A page alone has the bar above it in toolbar or developer mode, a view of the card like
        // a split's page's, which the sidebar passes over, like Arc's. It stays without a page:
        // its tab, which the interface reads only for pages, is never null, which it would refuse.
        if tabs.is_empty() && page_toolbar() {
            panes.push(json!({
                "tab": 0, "split": null,
                "x": 0, "y": 0, "width": card.width, "height": card.height,
                "bar": PAGE_TOOLBAR_HEIGHT, "url": selected_url().unwrap_or_default(),
                "back": false, "forward": false, "loading": false, "focused": true,
                // Like Arc's, the bar takes the color at the page's top.
                "color": focused.and_then(top_color),
                "developer": false, "toolbar": true, "alone": false, "square": square_card(),
                "cardX": card.x, "cardY": card.y,
            }));
        }
        // Once the sidebar sent the window's colors, which first show it.
        let shows = win().colored.get() && card_rect().is_some();
        let frame = shows.then_some([card.x, card.y, card.width, card.height]);
        // Only once they change: placing the bars again would end a click on them.
        let sent = json!({ "card": frame, "panes": panes }).to_string();
        if win().panes_sent.borrow().as_ref() == Some(&sent) {
            return;
        }
        let view = crate::native::panes(win().id, &sent);
        if let Some(view) = view {
            win().card_view.set(Some(view));
            // A split's room between its pages shows the window; a page alone has the card's
            // corners.
            if tabs.is_empty() {
                crate::mac::round_card(view, !square_card());
            } else {
                crate::mac::round_split(view);
            }
        }
        // Until the window can show the card, as it opens.
        if view.is_some() || frame.is_none() {
            win().panes_sent.replace(Some(sent));
        }
    }
}

/// Selects the page after the one focused in the split shown, or before it, wrapping around.
pub(super) fn switch_pane(next: bool) {
    let tabs = win().split.borrow().0.clone();
    let Some(index) = tabs.iter().position(|&tab| Some(tab) == win().shown.get()) else {
        return;
    };
    let step = if next { 1 } else { tabs.len() - 1 };
    select_tab(tabs[(index + step) % tabs.len()]);
}

/// Tells each window showing `tab` in a split how its page now stands.
pub(super) fn pane_changed(tab: NodeId) {
    in_each_window(|| {
        if win().split.borrow().0.contains(&tab) {
            send_panes();
        }
    });
}

/// The split's pages, in the window's points, along its axis: where each starts and its length.
fn split_panes() -> Vec<(f64, f64)> {
    let stacked = split_shown() == Some(true);
    split_rects()
        .iter()
        .map(|rect| {
            if stacked {
                (f64::from(rect.y), f64::from(rect.height))
            } else {
                (f64::from(rect.x), f64::from(rect.width))
            }
        })
        .collect()
}

/// Puts the handles that resize a split's pages in the room between them, or takes them away
/// without a split shown.
pub(super) fn place_split_handles() {
    #[cfg(target_os = "macos")]
    {
        let stacked = split_shown() == Some(true);
        let rects = split_rects();
        let handles: Vec<Rect> = rects
            .iter()
            .take(rects.len().saturating_sub(1))
            .map(|rect| {
                if stacked {
                    Rect {
                        y: rect.y + rect.height,
                        height: SPLIT_GAP,
                        ..*rect
                    }
                } else {
                    Rect {
                        x: rect.x + rect.width,
                        width: SPLIT_GAP,
                        ..*rect
                    }
                }
            })
            .collect();
        crate::native::split_handles(win().id, stacked, &handles);
    }
}

/// The least a split's page keeps along its axis, as Arc's.
const MIN_PANE: f64 = 220.0;
/// How close to equal parts a handle moved snaps to them.
const PANE_SNAP: f64 = 14.0;

/// Moves the handle after the split's page at `index` to `point`, along the split, in the
/// window's points: the pages on either side share their room by it, each keeping at least
/// `MIN_PANE`, and snap to equal parts near them, like Arc's.
pub(super) fn resize_panes(index: usize, point: f64) {
    let panes = split_panes();
    if let (Some(&lead), Some(&(_, trail))) = (panes.get(index), panes.get(index + 1)) {
        share_panes(index, lead_part(lead, trail, point));
    }
}

/// The part of the room the page at `lead` (where it starts, its length) keeps of what it shares
/// with the next one, `trail` long, once the handle between them moves to `point`.
fn lead_part((start, lead): (f64, f64), trail: f64, point: f64) -> f64 {
    let room = lead + trail;
    let mut lead = point - f64::from(SPLIT_GAP) / 2.0 - start;
    if room >= 2.0 * MIN_PANE {
        lead = lead.clamp(MIN_PANE, room - MIN_PANE);
    }
    if (lead - room / 2.0).abs() < PANE_SNAP {
        lead = room / 2.0;
    }
    lead.clamp(1.0, room - 1.0) / room
}

/// Gives the pages on either side of the handle after the page at `index` equal parts.
pub(super) fn even_panes(index: usize) {
    share_panes(index, 0.5);
    save_session();
}

/// Gives the page at `index` `part` of the room it shares with the next one, which takes the
/// rest, and keeps it.
fn share_panes(index: usize, part: f64) {
    let (tabs, Some(mut layout)) = win().split.borrow().clone() else {
        return;
    };
    if index + 1 >= layout.factors.len() {
        return;
    }
    let pair = layout.factors[index] + layout.factors[index + 1];
    layout.factors[index] = pair * part;
    layout.factors[index + 1] = pair - layout.factors[index];
    let saved = STATE.with_borrow_mut(|state| {
        let split = state.sidebar.row_of(tabs[0]);
        state.sidebar.set_split_factors(split, &layout.factors)
    });
    if saved.is_ok() {
        win().split.borrow_mut().1 = Some(layout);
        place_card();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_split_shares_its_room_by_the_parts_10_points_apart() {
        assert_eq!(pane_lengths(1010, &[0.5, 0.25, 0.25]), [495, 248, 247]);
        assert_eq!(pane_lengths(810, &[0.5, 0.5]), [400, 400]);
    }

    #[test]
    fn a_split_handle_keeps_the_pages_wide_enough_and_snaps_to_halves() {
        // Two pages of 500 points from 100, the handle 10 points wide between them.
        let part = |point| lead_part((100.0, 500.0), 500.0, point);
        assert!((part(405.0) - 0.3).abs() < 1e-9);
        assert_eq!(part(150.0), 0.22);
        assert_eq!(part(1000.0), 0.78);
        assert_eq!(part(612.0), 0.5);
        assert!((part(625.0) - 0.52).abs() < 1e-9);
    }
}

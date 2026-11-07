//! The find bar above the page, which finds text in it.

use super::*;

/// The find bar's size, and its margin from the card's top right corner (app/macos/Panels.swift).
const FIND_SIZE: Size = Size {
    width: 380,
    height: 32,
};
const FIND_MARGIN: i32 = 10;

/// Shows the find bar, with the text last searched for selected.
pub(super) fn show_find() {
    make_find();
    place_find();
    let Some(controller) = win().find.with_borrow(Clone::clone) else {
        return;
    };
    controller.set_visible(1);
    let text = win()
        .find_text
        .with_borrow(|text| serde_json::to_string(text).expect("a string"));
    dispatch(&win().find_view, "arbalete:show", &text.to_string());
}

/// Makes the find bar's place, once.
fn make_find() {
    if win().find.with_borrow(Option::is_none) {
        let view = place_view(no_width, 1);
        let controller = Spot::default();
        controller.set_visible(0);
        win().find_view.set(Some(view));
        win().find.set(Some(controller));
    }
}

/// Keeps the find bar at the top right corner of the page it searches: Peek's, under its bar, or
/// the card's.
pub(super) fn place_find() {
    let Some(controller) = win().find.with_borrow(Clone::clone) else {
        return;
    };
    let Some(size) = win()
        .window
        .with_borrow(|window| window.as_ref().map(|w| w.size()))
    else {
        return;
    };
    let card = if peek_shown() {
        let frame = peek_frame(&size);
        Rect {
            y: frame.y + PEEK_BAR,
            height: frame.height - PEEK_BAR,
            ..frame
        }
    } else {
        win().sidebar_mode.get().layout(&size).card
    };
    let width = FIND_SIZE.width.min(card.width - 2 * FIND_MARGIN);
    controller.set_bounds(Some(&Rect {
        x: card.x + card.width - width - FIND_MARGIN,
        y: card.y + FIND_MARGIN,
        width,
        height: FIND_SIZE.height,
    }));
}

/// The page find in page acts on: Peek's while it shows, or the shown one.
pub(super) fn find_page() -> Option<Browser> {
    PEEK.with_borrow(|peek| peek.as_ref().map(|peek| peek.browser.clone()))
        .or_else(content)
}

/// Finds text in `browser`, find_page's; the same text again goes to the next match. `None`
/// searches the last text again, or shows the find bar when there is none.
pub(super) fn find(browser: &Browser, text: Option<String>, forward: bool) {
    let text = text.unwrap_or_else(|| win().find_text.with_borrow(Clone::clone));
    if text.is_empty() {
        if let Some(host) = browser.host() {
            host.stop_finding(1);
        }
        win().find_text.with_borrow_mut(String::clear);
        return show_find_if_hidden();
    }
    let next = win().find_text.replace(text.clone()) == text;
    if let Some(host) = browser.host() {
        host.find(
            Some(&CefString::from(text.as_str())),
            forward.into(),
            0,
            next.into(),
        );
    }
}

/// From ⌘G with nothing to search again.
fn show_find_if_hidden() {
    let hidden = win()
        .find
        .with_borrow(|find| find.as_ref().is_none_or(|find| find.is_visible() == 0));
    if hidden {
        show_find();
    }
}

/// Hides the find bar and the page's highlights, keeping the active match selected and the text,
/// which ⌘G searches again; the page gets the keyboard back.
pub(super) fn hide_find() {
    close_find();
    if let Some(host) = PEEK.with_borrow(|peek| peek.as_ref()?.browser.host()) {
        host.set_focus(1);
    } else if let Some(tab) = win().shown.get() {
        focus_page(tab);
    }
}

/// Hides the find bar and the highlights of the page it searches, as Peek opens or closes.
pub(super) fn close_find() {
    if let Some(controller) = win().find.with_borrow(Clone::clone) {
        controller.set_visible(0);
    }
    if let Some(host) = find_page().and_then(|browser| browser.host()) {
        host.stop_finding(0);
    }
}

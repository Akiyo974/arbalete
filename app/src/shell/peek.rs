//! Peek, like Arc's: a link shown over the window, which opens as a tab or beside the page.

use super::*;

/// Peek, like Arc's: a link shown over the window in a card, which its controls close or keep in
/// a tab or a split.
pub(super) struct Peek {
    pub(super) browser: Browser,
    /// The color at the top of its page, as #rrggbb, which tints its bar, once known.
    pub(super) color: Option<String>,
}

/// Peek's card in a window of `size`, like Arc's: 80% of the page's card's width, centered on it,
/// 12 points from the window's top and bottom.
pub(super) fn peek_frame(size: &Size) -> Rect {
    let card = win().sidebar_mode.get().layout(size).card;
    let width = card.width * 4 / 5;
    Rect {
        x: card.x + (card.width - width) / 2,
        y: 12,
        width,
        height: size.height - 24,
    }
}

/// Shows `url` in Peek, in the displayed Space's profile, instead of what it showed.
pub(super) fn peek(url: &str) {
    close_find();
    close_peek();
    let Some(window) = win().window.with_borrow(Clone::clone) else {
        return;
    };
    let frame = peek_frame(&window.size());
    // Its card first, which the page is drawn in, with its scrim and controls.
    #[cfg(target_os = "macos")]
    crate::native::peek(win().id, Some(frame));
    #[cfg(target_os = "macos")]
    let info = WindowInfo {
        parent_view: match crate::native::peek_view(win().id) {
            Some(view) => view,
            None => return,
        },
        // Any size: the card gives the page its own.
        bounds: Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        },
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    };
    let profile = displayed_profile();
    let mut context = profile_context(profile);
    let Some(browser) = browser_host_create_browser_sync(
        Some(&info),
        Some(&mut ContentClient::new(profile, true)),
        Some(&CefString::from(url)),
        Some(&BrowserSettings::default()),
        None,
        context.as_mut(),
    ) else {
        return;
    };
    if let Some(host) = browser.host() {
        host.set_focus(1);
    }
    // The extensions learn it, and it tells the color at its top.
    extensions::page_created(profile, &browser);
    PEEK.set(Some(Peek {
        browser,
        color: None,
    }));
    send_peek();
}

/// Whether `browser` is Peek's page.
pub(super) fn is_peek(browser: &Browser) -> bool {
    PEEK.with_borrow(|peek| {
        peek.as_ref()
            .is_some_and(|peek| browser.is_same(Some(&mut peek.browser.clone())) == 1)
    })
}

/// Gives Peek's bar its page's address, and whether it can go back or forward and loads.
pub(super) fn send_peek() {
    let Some((browser, color)) = PEEK.with_borrow(|peek| {
        peek.as_ref()
            .map(|peek| (peek.browser.clone(), peek.color.clone()))
    }) else {
        return;
    };
    let url = browser
        .main_frame()
        .map(|frame| CefString::from(&frame.url()).to_string())
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    crate::native::peek_bar(
        win().id,
        &shown_url(&url),
        color.as_deref().unwrap_or_default(),
        browser.can_go_back() == 1,
        browser.can_go_forward() == 1,
        browser.is_loading() == 1,
    );
}

/// Does what Peek's bar asks of its page.
pub(super) fn peek_page(action: PeekAction) {
    let Some(browser) = PEEK.with_borrow(|peek| peek.as_ref().map(|peek| peek.browser.clone()))
    else {
        return;
    };
    match action {
        PeekAction::Back => browser.go_back(),
        PeekAction::Forward => browser.go_forward(),
        PeekAction::Reload => browser.reload(),
        PeekAction::Stop => browser.stop_load(),
    }
}

/// Peek follows the window's size.
pub(super) fn place_peek() {
    let size = win()
        .window
        .with_borrow(|window| window.as_ref().map(MainWindow::size));
    let Some(frame) = size.as_ref().map(peek_frame) else {
        return;
    };
    #[cfg(target_os = "macos")]
    if peek_shown() {
        crate::native::peek(win().id, Some(frame));
    }
}

/// Whether Peek shows a link.
pub fn peek_shown() -> bool {
    PEEK.with_borrow(Option::is_some)
}

/// Closes Peek, its page with it.
pub(super) fn close_peek() {
    if !peek_shown() {
        return;
    }
    close_find();
    let Some(peek) = PEEK.take() else { return };
    #[cfg(target_os = "macos")]
    crate::native::peek(win().id, None);
    extensions::page_closed(peek.browser.identifier());
    // Its card's view goes as it closes (BrowserLifeSpan's do_close), which CEF would otherwise
    // close the window of.
    PEEK_CLOSING.set(Some((peek.browser.identifier(), win().id)));
    if let Some(host) = peek.browser.host() {
        host.close_browser(1);
    }
}

/// Opens Peek's page in a tab of its own, or beside the selected tab in a split, and closes it.
pub(super) fn keep_peek(split: bool) {
    let url = PEEK.with_borrow(|peek| {
        let frame = peek.as_ref()?.browser.main_frame()?;
        Some(CefString::from(&frame.url()).to_string())
    });
    close_peek();
    match url.filter(|url| !url.is_empty()) {
        Some(url) if split => open_in_pane(&url),
        Some(url) => open_tab(&url),
        None => {}
    }
}

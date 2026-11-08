//! Where the sidebar, the card and the library go in the window, and how they slide there.

use super::*;

/// Whether the card is square, without margins: a page alone in full screen, like Arc's.
pub(super) fn square_card() -> bool {
    #[cfg(target_os = "macos")]
    return page_full_screen()
        || win().shown.get().is_some()
            && win().split.borrow().0.is_empty()
            && win()
                .window
                .with_borrow(|window| window.as_ref().map(|w| w.window_handle()))
                .is_some_and(crate::mac::full_screen);
}

/// Whether the shown page fills the window, gone full screen like a video.
pub(super) fn page_full_screen() -> bool {
    win().page_full_screen.get().is_some()
}

/// Like Arc's, a page going full screen, like a video, fills the screen: the window goes full
/// screen with it, out of the sidebar's way and the card's margins, and back as the page leaves
/// it. Chromium only fills the page's view with it.
pub(super) fn fill_with_page(full: bool) {
    #[cfg(target_os = "macos")]
    {
        let window = win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.window_handle()));
        let Some(window) = window else { return };
        let window_full = crate::mac::full_screen(window);
        if full {
            if page_full_screen() {
                return;
            }
            win().page_full_screen.set(Some(!window_full));
            if !window_full {
                crate::mac::toggle_full_screen(window);
            }
        } else {
            let Some(entered) = win().page_full_screen.take() else {
                return;
            };
            if entered && window_full {
                crate::mac::toggle_full_screen(window);
            }
        }
        place_card();
        place_native_sidebar();
    }
}

/// Takes the shown page out of full screen, as it hides or the window leaves full screen.
pub(super) fn leave_page_full_screen() {
    if !page_full_screen() {
        return;
    }
    if let Some(host) = content().and_then(|page| page.host()) {
        host.exit_fullscreen(1);
    }
    fill_with_page(false);
}

/// The width of the page's strip the library's Spaces leave at the window's right, like Arc's.
const LIBRARY_STRIP: i32 = 46;

/// The docked sidebar's width: wider for the library; for its Spaces, the window's but for a
/// strip of the page, like Arc's.
pub(super) fn docked_width() -> i32 {
    if fills_window() {
        win()
            .window
            .with_borrow(|window| {
                window
                    .as_ref()
                    .map(|window| window.size().width - LIBRARY_STRIP - CARD_MARGIN)
            })
            .unwrap_or_else(library_width)
    } else if win().library.get() {
        library_width()
    } else {
        sidebar_width()
    }
}

/// The least and the most the sidebar's edge may make it wide.
// Measured on Arc's.
const MIN_SIDEBAR: i32 = 164;
const MAX_SIDEBAR: i32 = 500;
/// The width the sidebar's edge snaps to, like Arc's.
const SNAP_SIDEBAR: i32 = 228;
/// The window's least size, measured on Arc's, whatever the sidebar's width.
pub(super) const MIN_WINDOW_WIDTH: i32 = 470;
pub(super) const MIN_WINDOW_HEIGHT: i32 = 385;

/// The sidebar's width, as dragging its edge set it.
pub(super) fn sidebar_width() -> i32 {
    app_settings().sidebar_width.clamp(MIN_SIDEBAR, MAX_SIDEBAR)
}

/// The docked sidebar's width while it shows the library, wider than its own by as much as
/// `LIBRARY_WIDTH` is wider than `SIDEBAR_WIDTH`.
fn library_width() -> i32 {
    sidebar_width() + LIBRARY_WIDTH - SIDEBAR_WIDTH
}

/// Gives the sidebar the width its edge was dragged to, `x` points from the window's left edge.
/// The cards follow at once; the rest of the window is laid out once the drag ends
/// (end_sidebar_resize).
pub(super) fn resize_sidebar(x: f64) {
    // Like Arc's, dragged well past its least width, the sidebar hides; dragged back within the
    // same drag, it shows again.
    let has_tab = STATE.with_borrow(|state| state.sidebar.displayed().selected.is_some());
    let mode = win().sidebar_mode.get();
    // Like Arc's: under half its least width.
    if x < f64::from(MIN_SIDEBAR / 2) {
        if mode == SidebarMode::Shown && has_tab {
            SIDEBAR_DRAG.set(SIDEBAR_DRAG.get().or(Some(sidebar_width())));
            set_sidebar(SidebarMode::Hidden);
        }
        return;
    }
    if mode == SidebarMode::Hidden && SIDEBAR_DRAG.get().is_some() {
        set_sidebar(SidebarMode::Shown);
    }
    let mut width = (x.round() as i32).clamp(MIN_SIDEBAR, MAX_SIDEBAR);
    // Like Arc's, it snaps to its usual width near it.
    if (width - SNAP_SIDEBAR).abs() < 14 {
        width = SNAP_SIDEBAR;
    }
    if width == sidebar_width() {
        return;
    }
    if SIDEBAR_DRAG.get().is_none() {
        SIDEBAR_DRAG.set(Some(sidebar_width()));
    }
    SETTINGS.with_borrow_mut(|settings| settings.sidebar_width = width);
    in_each_window(|| {
        #[cfg(target_os = "macos")]
        {
            place_native_sidebar();
            place_card();
        }
    });
}

/// Lays the windows out for the sidebar's width, once a drag of its edge ends.
pub(super) fn end_sidebar_resize() {
    if SIDEBAR_DRAG.take().is_some() {
        in_each_window(relayout_library);
    }
    if win().sidebar_mode.get() == SidebarMode::Hidden {
        hint_sidebar_shortcut();
    }
}

/// Like Arc's, the first time a drag hides the sidebar, a toast tells its shortcut, once.
fn hint_sidebar_shortcut() {
    if app_settings().sidebar_hint_shown {
        return;
    }
    SETTINGS.with_borrow_mut(|settings| settings.sidebar_hint_shown = true);
    save_settings();
    toast(json!({
        "text": "You can use",
        "keys": "⌘S",
        "trailing": "to easily show and hide your Sidebar!",
        "icon": "sidebar.left",
    }));
}

/// The docked library shows the Spaces, over the window but for a strip of the card.
fn fills_window() -> bool {
    win().library.get()
        && win().wide_library.get()
        && win().sidebar_mode.get() == SidebarMode::Shown
}

/// Shows the bar above the page, with the navigation buttons and the address, which the sidebar
/// then leaves out; or hides it. Kept across sessions.
pub(super) fn toggle_toolbar() {
    let shown = SETTINGS.with_borrow_mut(|settings| {
        settings.show_toolbar = !settings.show_toolbar;
        settings.show_toolbar
    });
    save_settings();
    sync_page_toolbar();
    #[cfg(target_os = "macos")]
    crate::mac::toolbar_shown(shown);
    send_state();
}

/// Whether the bar above the page shows: in toolbar mode, or over a site in developer mode, as
/// Arc's developer bar; never over a page in full screen.
pub(super) fn page_toolbar() -> bool {
    (app_settings().show_toolbar || selected_url().is_some_and(|url| developer_mode(&url)))
        && !page_full_screen()
}

/// Shows or hides the bar above the page as `page_toolbar` says, but over a split, which shows
/// its pages' own bars.
pub(super) fn sync_page_toolbar() {
    send_panes();
}

/// Moves the sidebar to `mode`. The views it shows on the way are shown first, the others hidden
/// once it gets there (settle). The card and the revealed sidebar slide natively, frame by frame;
/// the revealed sidebar floats in and out in the compositor, which clips it by its own window.
pub(super) fn set_sidebar(mode: SidebarMode) {
    let before = win().sidebar_mode.replace(mode);
    let (Some(size), Some(card), Some(reveal)) = (
        win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.size())),
        // Where it stands now: on its way, or where the mode it leaves put it.
        win().sliding_card.borrow().clone().or_else(|| {
            win()
                .window
                .with_borrow(|window| Some(before.layout(&window.as_ref()?.size()).card))
        }),
        win().reveal.with_borrow(Clone::clone),
    ) else {
        return;
    };
    // Like Arc's, a drag of the sidebar's edge hides and shows it at once, under the pointer.
    #[cfg(target_os = "macos")]
    let duration = if crate::mac::reduce_motion() || SIDEBAR_DRAG.get().is_some() {
        0
    } else {
        slide_duration(before, mode)
    };
    let docked = mode == SidebarMode::Shown;
    if docked != (before == SidebarMode::Shown) {
        let slide = duration > 0 && matches!(before, SidebarMode::Shown | SidebarMode::Hidden);
        let detail = json!({ "docked": docked, "slide": slide });
        dispatch(&win().sidebar, "arbalete:docked", &detail.to_string());
    }
    let floating = |mode| matches!(mode, SidebarMode::Revealed | SidebarMode::Floating);
    if floating(mode) && !floating(before) {
        reveal.set_bounds(Some(&mode.layout(&size).reveal));
        reveal.set_visible((!cfg!(target_os = "macos")).into());
    }
    let from = Layout {
        card,
        reveal: reveal.bounds(),
    };
    let slide = Slide {
        from,
        start: Instant::now(),
        duration: Duration::from_millis(duration.into()),
        docking: docked && floating(before),
    };
    if win().slide.replace(Some(slide)).is_none() {
        step_slide();
    }
}

/// How long the sidebar takes from one mode to another, in milliseconds; 0 for at once.
fn slide_duration(from: SidebarMode, to: SidebarMode) -> u32 {
    match (from, to) {
        (SidebarMode::Shown, SidebarMode::Hidden) | (SidebarMode::Hidden, SidebarMode::Shown) => {
            HIDE_DURATION
        }
        (SidebarMode::Hidden, SidebarMode::Revealed) => REVEAL_DURATION,
        (SidebarMode::Revealed, SidebarMode::Hidden) => CONCEAL_DURATION,
        (SidebarMode::Revealed, SidebarMode::Shown) => DOCK_DURATION,
        (SidebarMode::Floating, SidebarMode::Shown) => FLOATING_DOCK_DURATION,
        _ => 0,
    }
}

/// The mode the sidebar toggle leads to. A Space without a tab shows an empty panel, which has
/// nothing to grow into: the sidebar floats beside it instead of hiding.
pub(super) fn toggled(mode: SidebarMode, has_tab: bool) -> SidebarMode {
    match mode {
        SidebarMode::Shown if has_tab => SidebarMode::Hidden,
        SidebarMode::Shown => SidebarMode::Floating,
        _ => SidebarMode::Shown,
    }
}

/// Moves the card, and the revealed sidebar while it docks, one frame further, until the slide
/// ends.
fn step_slide() {
    let Some((from, start, duration, docking)) = win().slide.with_borrow(|slide| {
        let slide = slide.as_ref()?;
        Some((
            slide.from.clone(),
            slide.start,
            slide.duration,
            slide.docking,
        ))
    }) else {
        return;
    };
    let elapsed = start.elapsed().as_secs_f64();
    let progress = if duration.is_zero() {
        1.0
    } else {
        elapsed / duration.as_secs_f64()
    };
    let size = win()
        .window
        .with_borrow(|window| window.as_ref().map(|w| w.size()));
    let (Some(size), true) = (size, progress < 1.0) else {
        win().slide.set(None);
        win().sliding_card.replace(None);
        settle_sidebar();
        // The bar over the page, which the card hid while it moved.
        send_panes();
        return;
    };
    let eased = crate::motion::ease_out(progress);
    let to = win().sidebar_mode.get().layout(&size);
    win()
        .sliding_card
        .replace(Some(lerp_rect(&from.card, &resting_card(&size), eased)));
    place_card();
    if docking && let Some(reveal) = win().reveal.with_borrow(Clone::clone) {
        reveal.set_bounds(Some(&lerp_rect(&from.reveal, &to.reveal, eased)));
        #[cfg(target_os = "macos")]
        place_native_sidebar();
    }
    post_delayed_task(ThreadId::UI, Some(&mut StepSlide::new()), FRAME_MS);
}

wrap_task! {
    struct StepSlide {}

    impl Task {
        fn execute(&self) {
            step_slide();
        }
    }
}

/// `t` of the way from one rectangle to another.
fn lerp_rect(from: &Rect, to: &Rect, t: f64) -> Rect {
    use crate::motion::lerp;
    Rect {
        x: lerp(from.x, to.x, t),
        y: lerp(from.y, to.y, t),
        width: lerp(from.width, to.width, t),
        height: lerp(from.height, to.height, t),
    }
}

/// Shows the views of the sidebar's mode where it wants them, and hides the others.
fn settle_sidebar() {
    let mode = win().sidebar_mode.get();
    let docked = mode == SidebarMode::Shown;
    if let Some(sidebar) = win().sidebar.with_borrow(Clone::clone) {
        sidebar.set_visible((docked && !cfg!(target_os = "macos")).into());
    }
    place_card();
    place_reveal();
    #[cfg(target_os = "macos")]
    {
        if let Some(window) = win().window.with_borrow(Clone::clone) {
            crate::mac::sidebar_shown(window.window_handle(), docked);
        }
    }
}

/// Insets the card from the window's edges, and from the sidebar while it is docked or floating.
pub(super) fn place_card() {
    place_split_handles();
    send_empty_card();
    send_panes();
    float_beside_empty_card();
}

/// Like Arc's, beside an empty Space's panel, the hidden sidebar floats, which has nothing to
/// give the room to; once a tab shows, it hides again.
fn float_beside_empty_card() {
    let empty = STATE.with_borrow(|state| state.sidebar.displayed().selected.is_none());
    match win().sidebar_mode.get() {
        SidebarMode::Hidden | SidebarMode::Revealed if empty => set_sidebar(SidebarMode::Floating),
        SidebarMode::Floating if !empty => set_sidebar(SidebarMode::Hidden),
        _ => {}
    }
}

/// Moves the card `offset` points right of where it rests beside the docked sidebar, which pages
/// towards the library; the sidebar gets its own width back first. The compositor moves the
/// card's stage, clipped by the card: the page keeps its size.
pub(super) fn shift_card(offset: u32) {
    if win().sidebar_mode.get() != SidebarMode::Shown {
        return;
    }
    widen_for_library(false);
    #[cfg(target_os = "macos")]
    if let Some(view) = card_window() {
        crate::mac::shift(view, f64::from(offset));
    }
}

/// Widens the docked sidebar for the library, the card resting beside it.
pub(super) fn show_library() {
    if win().sidebar_mode.get() != SidebarMode::Shown {
        return;
    }
    widen_for_library(true);
    #[cfg(target_os = "macos")]
    if let Some(view) = card_window() {
        crate::mac::shift(view, 0.0);
    }
}

/// Fills the window with the library while it shows the Spaces, or rests it beside the card.
pub(super) fn widen_library(wide: bool) {
    let from = card_rect();
    if win().wide_library.replace(wide) != wide && win().library.get() {
        relayout_library_sliding(from);
    }
}

/// Gives the docked sidebar the library's width, or its own, and places the card beside it.
fn widen_for_library(library: bool) {
    let from = card_rect();
    if win().library.replace(library) != library {
        relayout_library_sliding(from);
    }
}

/// Lays the window out for the library's new width, the card sliding there from `from`.
fn relayout_library_sliding(from: Option<Rect>) {
    if win().slide.with_borrow(Option::is_none) {
        win().sliding_card.replace(from.clone());
    }
    relayout_library();
    slide_card(from);
}

/// Sizes the docked sidebar and places the card after the library's width changed.
fn relayout_library() {
    if let Some(sidebar) = win().sidebar.with_borrow(Clone::clone) {
        sidebar.invalidate_layout();
    }
    #[cfg(target_os = "macos")]
    place_native_sidebar();
    place_card();
}

/// The native card holding the pages, once the card has shown (send_panes).
#[cfg(target_os = "macos")]
fn card_window() -> Option<*mut std::ffi::c_void> {
    win().card_view.get()
}

/// Places the revealed sidebar above the page, or the strip that reveals it along the window's
/// left edge.
pub(super) fn place_reveal() {
    let Some(reveal) = win().reveal.with_borrow(Clone::clone) else {
        return;
    };
    let mode = win().sidebar_mode.get();
    let size = win()
        .window
        .with_borrow(|window| window.as_ref().map(|w| w.size()));
    let Some(size) = size else { return };
    reveal.set_bounds(Some(&mode.layout(&size).reveal));
    reveal.set_visible((mode != SidebarMode::Shown && !cfg!(target_os = "macos")).into());
    #[cfg(target_os = "macos")]
    place_native_sidebar();
}

/// Puts the native sidebar where the web one would be: docked at the window's left edge, or
/// floating above the page; in neither while hidden.
#[cfg(target_os = "macos")]
fn place_native_sidebar() {
    let (Some(size), Some(reveal)) = (
        win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.size())),
        win().reveal.with_borrow(Clone::clone),
    ) else {
        return;
    };
    let mode = win().sidebar_mode.get();
    let frame = match mode {
        _ if page_full_screen() => None,
        SidebarMode::Shown => Some(Rect {
            x: 0,
            y: 0,
            width: docked_width(),
            height: size.height,
        }),
        SidebarMode::Hidden => None,
        SidebarMode::Revealed | SidebarMode::Floating => Some(reveal.bounds()),
    };
    crate::native::place(
        win().id,
        frame.as_ref(),
        mode != SidebarMode::Shown,
        mode == SidebarMode::Revealed,
    );
}

/// Where the card is in the window: the page's room, or half of it while a tab dragged over a
/// page alone would take the other half, like Arc's; a strip of it while the library shows the
/// Spaces.
pub(super) fn card_rect() -> Option<Rect> {
    if let Some(sliding) = win().sliding_card.borrow().clone() {
        return Some(sliding);
    }
    let size = win()
        .window
        .with_borrow(|window| window.as_ref().map(|w| w.size()))?;
    let mut room = resting_card(&size);
    if !fills_window()
        && split_shown().is_none()
        && let Some((before, stacked)) = win().drop_preview.get()
    {
        if stacked {
            let half = (room.height - SPLIT_GAP) / 2;
            if before {
                room.y += half + SPLIT_GAP;
            }
            room.height = half;
        } else {
            let half = (room.width - SPLIT_GAP) / 2;
            if before {
                room.x += half + SPLIT_GAP;
            }
            room.width = half;
        }
    }
    Some(room)
}

/// Where the card rests in a window of `size`: beside the sidebar; pushed right by the library's
/// Spaces, like Arc's, its size kept, a strip of it showing.
fn resting_card(size: &Size) -> Rect {
    let mut room = win().sidebar_mode.get().layout(size).card;
    if fills_window() {
        room.x = size.width - LIBRARY_STRIP - CARD_MARGIN;
        room.width = size.width - library_width() - CARD_MARGIN;
    }
    room
}

/// Slides the card from `from` to where it rests now, like Arc's library opening, closing, or
/// showing the Spaces: an ease out in about 0.22 s.
fn slide_card(from: Option<Rect>) {
    let (Some(from), Some(reveal)) = (from, win().reveal.with_borrow(Clone::clone)) else {
        return;
    };
    #[cfg(target_os = "macos")]
    if crate::mac::reduce_motion() {
        return;
    }
    let slide = Slide {
        from: Layout {
            card: from,
            reveal: reveal.bounds(),
        },
        start: Instant::now(),
        duration: Duration::from_millis(LIBRARY_SLIDE),
        docking: false,
    };
    if win().slide.replace(Some(slide)).is_none() {
        step_slide();
    }
}

/// How long the card takes to slide as the library opens, closes or shows the Spaces, in
/// milliseconds, like Arc's.
const LIBRARY_SLIDE: u64 = 220;

/// The height of the bar above a page alone, 0 while it hides: over a split, its pages show their
/// own bars.
pub(super) fn shown_toolbar_height() -> i32 {
    if page_toolbar() && split_shown().is_none() {
        PAGE_TOOLBAR_HEIGHT
    } else {
        0
    }
}

/// Paints the window around the sidebar and the card as the sidebar paints itself, with its
/// gradient's `colors` and `grain`, and the card behind its content views. The card shows from the
/// sidebar's first colors on, so that it never flashes another color.
pub(super) fn set_window_colors(card_color: u32, grain: f64, colors: &[u32]) {
    win()
        .window_colors
        .set(Some((card_color, grain, colors.to_vec())));
    // The card first shows once the window has its colors.
    if !win().colored.replace(true) {
        place_card();
    }
}

/// Tells the native interface where the card is, which shades the window around it and washes it
/// while it shows no page; a split's pages are cards of their own.
pub(super) fn send_empty_card() {
    #[cfg(target_os = "macos")]
    {
        let alone = !fills_window() && split_shown().is_none();
        crate::native::card(
            win().id,
            card_rect().filter(|_| alone),
            win().shown.get().is_none(),
        );
    }
}

/// Tells the sidebars the window's width, which their gradient spans like the window's.
pub(super) fn send_window_width() {
    let Some(size) = win()
        .window
        .with_borrow(|window| window.as_ref().map(|w| w.size()))
    else {
        return;
    };
    let width = size.width.to_string();
    dispatch(&win().sidebar, "arbalete:window-width", &width);
    dispatch(&win().reveal_view, "arbalete:window-width", &width);
}

/// Whether a point at `x` from the window's left edge is over the sidebar, shown or revealed,
/// where swipes page between Spaces. The library's Spaces over the window scroll instead; like
/// Arc's, a swipe over its sections' rail, left of them, closes it (LibraryView).
#[cfg(target_os = "macos")]
pub fn is_over_sidebar(x: f64) -> bool {
    /// The library's rail of sections, as wide as LibraryView draws it.
    const LIBRARY_RAIL: f64 = 112.0;
    if fills_window() {
        return x < LIBRARY_RAIL;
    }
    let width = match win().sidebar_mode.get() {
        SidebarMode::Hidden => return false,
        SidebarMode::Shown => docked_width(),
        SidebarMode::Revealed | SidebarMode::Floating => sidebar_width(),
    };
    x < f64::from(width)
}

/// Sends the swipes among the window's scroll events to the sidebar the window shows.
#[cfg(target_os = "macos")]
pub fn scroll(scroll: crate::swipe::Scroll) {
    use crate::swipe::Swipe;
    match win().swipe.with_borrow_mut(|tracker| tracker.track(scroll)) {
        Some(Swipe::Gesture(gesture)) => {
            let current = win();
            let view = &current.sidebar;
            let detail = serde_json::to_string(&gesture).expect("a gesture");
            dispatch(view, "arbalete:swipe", &detail.to_string());
        }
        Some(Swipe::Step(next)) => show_found_space(|s| s.next_space(next)),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(rect: &Rect) -> (i32, i32, i32, i32) {
        (rect.x, rect.y, rect.width, rect.height)
    }

    /// A window, in points.
    const SIZE: Size = Size {
        width: 1000,
        height: 600,
    };

    #[test]
    fn the_toggle_hides_the_sidebar_beside_a_page_and_floats_it_beside_an_empty_panel() {
        assert_eq!(toggled(SidebarMode::Shown, true), SidebarMode::Hidden);
        assert_eq!(toggled(SidebarMode::Shown, false), SidebarMode::Floating);
        for mode in [
            SidebarMode::Hidden,
            SidebarMode::Revealed,
            SidebarMode::Floating,
        ] {
            assert_eq!(toggled(mode, true), SidebarMode::Shown);
            assert_eq!(toggled(mode, false), SidebarMode::Shown);
        }
    }

    #[test]
    fn the_sidebar_slides_quickly_and_floats_beside_an_empty_panel_at_once() {
        use SidebarMode::{Floating, Hidden, Revealed, Shown};
        assert_eq!(slide_duration(Shown, Hidden), slide_duration(Hidden, Shown));
        assert!((80..=90).contains(&slide_duration(Shown, Hidden)));
        assert!(slide_duration(Hidden, Revealed) > slide_duration(Revealed, Hidden));
        assert!(slide_duration(Revealed, Shown) < slide_duration(Shown, Hidden));
        assert!(slide_duration(Floating, Shown) > 0);
        assert_eq!(slide_duration(Shown, Floating), 0);
    }

    #[test]
    fn interpolates_rectangles() {
        let from = Rect {
            x: 269,
            y: 9,
            width: 722,
            height: 582,
        };
        let to = SidebarMode::Hidden.layout(&SIZE).card;
        assert_eq!(rect(&lerp_rect(&from, &to, 0.0)), rect(&from));
        assert_eq!(rect(&lerp_rect(&from, &to, 0.5)), (140, 10, 851, 581));
        assert_eq!(rect(&lerp_rect(&from, &to, 1.0)), rect(&to));
    }
}

//! A Space's theme picker, like Arc's, beside the sidebar or the Space's column.

use super::*;

/// How far the theme picker shows from the Space's column it opened from, like Arc's.
const THEME_GAP: i32 = 13;

/// The theme picker's size, like Arc's.
const THEME_SIZE: Size = Size {
    width: 365,
    height: 520,
};

/// Shows a Space's theme picker beside the sidebar, like Arc's, centered on the pointer, with the
/// Space's theme to edit.
pub(super) fn show_theme_picker(id: SpaceId, beside: Option<(i32, i32)>) {
    let theme = STATE.with_borrow(|state| {
        let space = state.sidebar.spaces().iter().find(|space| space.id == id)?;
        Some(space.theme.clone())
    });
    let (Some(theme), Some(controller)) = (theme, win().theme.with_borrow(Clone::clone)) else {
        return;
    };
    let detail = json!({ "space": id, "theme": theme });
    dispatch(&win().theme_view, "arbalete:theme", &detail.to_string());
    #[cfg(target_os = "macos")]
    win().theme_at.set(crate::mac::pointer_y());
    win().theme_beside.set(beside);
    controller.set_visible(1);
    place_theme_picker();
}

/// Puts the theme picker, if it shows, just right of the sidebar, or like Arc's beside the Space's
/// column it opened from, right of it unless there is no room; centered on where the pointer was,
/// or else above the window's bottom edge; within the window.
pub(super) fn place_theme_picker() {
    let (Some(controller), Some(size)) = (
        win().theme.with_borrow(Clone::clone),
        win()
            .window
            .with_borrow(|window| window.as_ref().map(|w| w.size())),
    ) else {
        return;
    };
    if controller.is_visible() == 0 {
        return;
    }
    let card = win().sidebar_mode.get().layout(&size).card;
    let height = THEME_SIZE.height.min(size.height - 2 * CARD_MARGIN);
    let lowest = size.height - height - 2 * CARD_MARGIN;
    let y = win()
        .theme_at
        .get()
        .map_or(lowest, |at| (at - height / 2).clamp(CARD_MARGIN, lowest));
    let x = match win().theme_beside.get() {
        Some((_, right)) if right + THEME_GAP + THEME_SIZE.width <= size.width - CARD_MARGIN => {
            right + THEME_GAP
        }
        Some((left, _)) => (left - THEME_GAP - THEME_SIZE.width).max(CARD_MARGIN),
        None => card.x + 6,
    };
    controller.set_bounds(Some(&Rect {
        x,
        y,
        width: THEME_SIZE.width,
        height,
    }));
}

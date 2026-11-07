//! Toasts, like Arc's: short notes at the top right of the page (app/macos/Toast.swift).

use super::*;

/// Shows a toast over the shown page, or the card without one. `toast` is an object with its
/// `text` and, if any: a `key`, the toast showing with the same one giving way to it; an `icon`,
/// an SF Symbol; `keys` drawn as keycaps after the text, and the `trailing` words after them; a
/// `sublabel`; a `button`, with a `buttonIcon`; `error`; and the `commands` a click runs.
pub(super) fn toast(toast: serde_json::Value) {
    #[cfg(target_os = "macos")]
    if let Some(page) = page_frame().or_else(card_rect) {
        crate::native::toast(win().id, &page, &toast.to_string());
    }
}

/// The page's zoom level as a percentage, like Chromium's: each level is 1.2 times the one before.
pub(super) fn zoom_percent(level: f64) -> i64 {
    (100.0 * 1.2_f64.powf(level)).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_chromium_zoom_levels_as_percentages() {
        assert_eq!(zoom_percent(0.0), 100);
        assert_eq!(zoom_percent(1.1_f64.ln() / 1.2_f64.ln()), 110);
        assert_eq!(zoom_percent(-1.0), 83);
        assert_eq!(zoom_percent(1.0), 120);
    }
}

//! The native macOS interface (app/macos, in Swift). It gets the events the web UI gets, as their
//! name and JSON detail, and sends the same commands (app/src/command.rs).

use crate::command::Command;
use cef::Rect;
use std::ffi::{CStr, CString, c_char, c_void};

unsafe extern "C" {
    fn arbalete_ui_attach(
        window: u32,
        handle: *mut c_void,
        send: extern "C" fn(u32, *const c_char),
    );
    fn arbalete_ui_event(
        window: u32,
        surface: *const c_char,
        name: *const c_char,
        detail: *const c_char,
    );
    fn arbalete_ui_surface(
        window: u32,
        name: *const c_char,
        shown: bool,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    );
    fn arbalete_ui_detach(window: u32);
    fn arbalete_ui_settings(shown: bool);
    fn arbalete_ui_little(id: u32, handle: *mut c_void);
    fn arbalete_ui_settings_focused() -> bool;
    fn arbalete_ui_make_default_browser();
    fn arbalete_ui_split(window: u32, stacked: bool, frames: *const f64, count: usize);
    fn arbalete_ui_panes(window: u32, panes: *const c_char) -> *mut std::ffi::c_void;
    fn arbalete_ui_page_view(window: u32, tab: i64) -> *mut std::ffi::c_void;
    fn arbalete_ui_page_closed(view: *mut std::ffi::c_void);
    fn arbalete_ui_hand_pages_over(from: u32, to: u32);
    fn arbalete_ui_focus_page(tab: i64);
    fn arbalete_ui_tools_view(tab: i64) -> *mut std::ffi::c_void;
    fn arbalete_ui_tools_room(tab: i64, x: f64, y: f64, width: f64, height: f64);
    fn arbalete_ui_tools_docked(tab: i64, docked: bool);
    fn arbalete_ui_tools_closed(view: *mut std::ffi::c_void);
    fn arbalete_ui_is_default_browser() -> bool;
    fn arbalete_ui_peek(window: u32, shown: bool, x: f64, y: f64, width: f64, height: f64);
    fn arbalete_ui_peek_view(window: u32) -> *mut std::ffi::c_void;
    fn arbalete_ui_peek_closed(window: u32);
    fn arbalete_ui_peek_bar(
        window: u32,
        url: *const c_char,
        color: *const c_char,
        back: bool,
        forward: bool,
        loading: bool,
    );
    fn arbalete_ui_capture(window: u32, shown: bool, x: f64, y: f64, width: f64, height: f64);
    fn arbalete_ui_note(window: u32, text: *const c_char, x: f64, y: f64);
    fn arbalete_ui_copy_png(
        base64: *const c_char,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        viewport: f64,
    );
    fn arbalete_ui_card(
        window: u32,
        shown: bool,
        empty: bool,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    );
    fn arbalete_ui_place_sidebar(
        window: u32,
        visible: bool,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        floating: bool,
        revealed: bool,
    );
}

/// Puts the native interface in the window `handle` is, or is in, which the app knows as `window`.
pub fn attach(window: u32, handle: *mut c_void) {
    // SAFETY: CEF gives a live NSWindow or NSView, and `send` lives as long as the app.
    unsafe { arbalete_ui_attach(window, handle, send) };
}

/// Takes the native interface out of `window`, which closed.
pub fn detach(window: u32) {
    // SAFETY: a plain value.
    unsafe { arbalete_ui_detach(window) };
}

/// Gives a surface of the native interface of `window`, like "sidebar", an event dispatched to
/// its web counterpart: `new CustomEvent("name", { detail: … })` or `new Event("name")`.
pub fn event(window: u32, surface: &str, event: &str) {
    let Some((name, detail)) = parse_event(event) else {
        return;
    };
    let (Ok(surface), Ok(name), Ok(detail)) = (
        CString::new(surface),
        CString::new(name),
        CString::new(detail),
    ) else {
        return;
    };
    // SAFETY: the strings outlive the call, which copies what it needs.
    unsafe { arbalete_ui_event(window, surface.as_ptr(), name.as_ptr(), detail.as_ptr()) };
}

/// An event's name and its detail, `null` without one.
fn parse_event(event: &str) -> Option<(&str, &str)> {
    if let Some(rest) = event.strip_prefix(r#"new Event(""#) {
        return Some((rest.split_once('"')?.0, "null"));
    }
    let (name, rest) = event
        .strip_prefix(r#"new CustomEvent(""#)?
        .split_once('"')?;
    let detail = rest.strip_prefix(", { detail: ")?.strip_suffix(" })")?;
    Some((name, detail))
}

/// Shows the sidebar of `window` at `frame`, in its points from its top left corner, floating
/// above the page or docked; hides it without a frame. Revealed, it hides again once the pointer
/// leaves it.
pub fn place(window: u32, frame: Option<&Rect>, floating: bool, revealed: bool) {
    let frame = frame.cloned().unwrap_or_default();
    // SAFETY: plain values.
    unsafe {
        arbalete_ui_place_sidebar(
            window,
            frame.width > 0,
            frame.x.into(),
            frame.y.into(),
            frame.width.into(),
            frame.height.into(),
            floating,
            revealed,
        );
    }
}

/// Shows a surface of the interface of `window`, like "overlay", at `frame` in its points from
/// its top left corner, or hides it.
pub fn surface(window: u32, name: &str, shown: bool, frame: &Rect) {
    let Ok(name) = CString::new(name) else { return };
    // SAFETY: the string outlives the call.
    unsafe {
        arbalete_ui_surface(
            window,
            name.as_ptr(),
            shown,
            frame.x.into(),
            frame.y.into(),
            frame.width.into(),
            frame.height.into(),
        );
    }
}

/// Gives a Little Arbalete window, `id`, its native bar, or forgets it once closed, without a
/// handle.
pub fn little(id: u32, handle: Option<*mut c_void>) {
    // SAFETY: CEF gives a live NSWindow or NSView, which the window keeps alive.
    unsafe { arbalete_ui_little(id, handle.unwrap_or(std::ptr::null_mut())) };
}

/// Opens the settings window, or closes it.
pub fn settings(shown: bool) {
    // SAFETY: a plain value.
    unsafe { arbalete_ui_settings(shown) };
}

/// Puts the handles that resize a split's pages at `frames`, in the window's points from its top
/// left corner, or takes them away without frames.
pub fn split_handles(window: u32, stacked: bool, frames: &[cef::Rect]) {
    let values: Vec<f64> = frames
        .iter()
        .flat_map(|f| [f.x, f.y, f.width, f.height].map(f64::from))
        .collect();
    // SAFETY: four values per frame, which the call reads before returning.
    unsafe { arbalete_ui_split(window, stacked, values.as_ptr(), frames.len()) };
}

/// Shows the pages of a window as `panes` (JSON) lays them out: where its card is and where each
/// page shows in it, with its bar. Returns the card's content view, once the card shows.
pub fn panes(window: u32, panes: &str) -> Option<*mut std::ffi::c_void> {
    let panes = CString::new(panes).unwrap_or_default();
    // SAFETY: a C string, which the call reads before returning.
    let card = unsafe { arbalete_ui_panes(window, panes.as_ptr()) };
    (!card.is_null()).then_some(card)
}

/// The view a tab's page is drawn in, hidden in the window `window` until it shows, which CEF
/// makes the page's browser in.
pub fn page_view(window: u32, tab: i64) -> Option<*mut std::ffi::c_void> {
    // SAFETY: plain values; the view lives until page_closed.
    let view = unsafe { arbalete_ui_page_view(window, tab) };
    (!view.is_null()).then_some(view)
}

/// Takes a page's container away, its browser, whose view is `view`, closing: the view goes with
/// it, which destroys the browser.
pub fn page_closed(view: *mut std::ffi::c_void) {
    // SAFETY: CEF's view of a browser closing, which the call only compares.
    unsafe { arbalete_ui_page_closed(view) };
}

/// Moves the pages' containers out of the window `from`, which closes, into the window `to`,
/// hidden.
pub fn hand_pages_over(from: u32, to: u32) {
    // SAFETY: plain values.
    unsafe { arbalete_ui_hand_pages_over(from, to) };
}

/// The view a tab's page's developer tools are drawn in, docked under the page, which CEF makes
/// their browser in.
pub fn tools_view(tab: i64) -> Option<*mut std::ffi::c_void> {
    // SAFETY: a plain value; the view lives until tools_closed.
    let view = unsafe { arbalete_ui_tools_view(tab) };
    (!view.is_null()).then_some(view)
}

/// Where a tab's page's docked developer tools leave the page, from their top left corner.
pub fn tools_room(tab: i64, x: f64, y: f64, width: f64, height: f64) {
    // SAFETY: plain values.
    unsafe { arbalete_ui_tools_room(tab, x, y, width, height) };
}

/// Docks a tab's page's developer tools under the page, or shows them in a window of their own.
pub fn tools_docked(tab: i64, docked: bool) {
    // SAFETY: plain values.
    unsafe { arbalete_ui_tools_docked(tab, docked) };
}

/// Takes developer tools away, their browser, whose view is `view`, closing.
pub fn tools_closed(view: *mut std::ffi::c_void) {
    // SAFETY: CEF's view of a browser closing, which the call only compares.
    unsafe { arbalete_ui_tools_closed(view) };
}

/// Gives a tab's page's window the keyboard, if it shows.
pub fn focus_page(tab: i64) {
    // SAFETY: a plain value.
    unsafe { arbalete_ui_focus_page(tab) };
}

/// Shows Peek's scrim and controls around its card at `frame`, in the window's points from its
/// top left corner, or takes them away without a frame.
pub fn peek(window: u32, frame: Option<cef::Rect>) {
    let frame = frame.unwrap_or_default();
    // SAFETY: plain values.
    unsafe {
        arbalete_ui_peek(
            window,
            frame.width > 0,
            frame.x.into(),
            frame.y.into(),
            frame.width.into(),
            frame.height.into(),
        )
    };
}

/// The view Peek's page is drawn in, in the window `window`, while Peek shows: CEF makes the
/// page's browser in it.
pub fn peek_view(window: u32) -> Option<*mut std::ffi::c_void> {
    // SAFETY: a plain value; the view lives until peek_closed.
    let view = unsafe { arbalete_ui_peek_view(window) };
    (!view.is_null()).then_some(view)
}

/// Gives Peek's bar in the window `window` its page's address, the color at its top as #rrggbb
/// (empty while unknown), and its state.
pub fn peek_bar(window: u32, url: &str, color: &str, back: bool, forward: bool, loading: bool) {
    let url = CString::new(url).unwrap_or_default();
    let color = CString::new(color).unwrap_or_default();
    // SAFETY: plain values and strings that outlive the call.
    unsafe { arbalete_ui_peek_bar(window, url.as_ptr(), color.as_ptr(), back, forward, loading) };
}

/// Takes Peek's page's view out of the window `window`, its browser closing, which destroys it.
pub fn peek_closed(window: u32) {
    // SAFETY: a plain value.
    unsafe { arbalete_ui_peek_closed(window) };
}

/// Shows the veil a drag picks the part of the page to capture on, over the page at `frame`, in
/// the window's points from its top left corner, or takes it away without a frame.
pub fn capture(window: u32, frame: Option<Rect>) {
    let frame = frame.unwrap_or_default();
    // SAFETY: plain values.
    unsafe {
        arbalete_ui_capture(
            window,
            frame.width > 0,
            frame.x.into(),
            frame.y.into(),
            frame.width.into(),
            frame.height.into(),
        )
    };
}

/// Shows a short note, like a capture copied, its top right corner at `x` and `y`, in the
/// window's points from its top left corner.
pub fn note(window: u32, text: &str, x: i32, y: i32) {
    let text = CString::new(text).unwrap_or_default();
    // SAFETY: a C string, which the call reads before returning.
    unsafe { arbalete_ui_note(window, text.as_ptr(), x.into(), y.into()) };
}

/// Puts a PNG picture, in base64, on the clipboard, like a page's capture: only the part `area`
/// of it if given, in CSS pixels of a page `viewport` wide, which the picture shows the width of.
pub fn copy_png(base64: &str, area: Option<(i32, i32, i32, i32)>, viewport: f64) {
    let base64 = CString::new(base64).unwrap_or_default();
    let (x, y, width, height) = area.unwrap_or_default();
    // SAFETY: a C string, which the call reads before returning, and plain values.
    unsafe {
        arbalete_ui_copy_png(
            base64.as_ptr(),
            x.into(),
            y.into(),
            width.into(),
            height.into(),
            if area.is_some() { viewport } else { 0.0 },
        )
    };
}

/// Tells where the card is, in the window's points from its top left corner, and whether it shows
/// a page; without a frame, that it does not show alone.
pub fn card(window: u32, frame: Option<cef::Rect>, empty: bool) {
    let frame = frame.unwrap_or_default();
    // SAFETY: plain values.
    unsafe {
        arbalete_ui_card(
            window,
            frame.width > 0,
            empty,
            frame.x.into(),
            frame.y.into(),
            frame.width.into(),
            frame.height.into(),
        )
    };
}

/// Makes Arbalete the default browser, once the user confirms.
pub fn make_default_browser() {
    // SAFETY: no arguments.
    unsafe { arbalete_ui_make_default_browser() };
}

/// Arbalete is the default browser.
pub fn is_default_browser() -> bool {
    // SAFETY: no arguments.
    unsafe { arbalete_ui_is_default_browser() }
}

/// The settings window has the keyboard: ⌘W closes it rather than a tab.
pub fn settings_focused() -> bool {
    // SAFETY: no arguments.
    unsafe { arbalete_ui_settings_focused() }
}

/// Runs a command the interface of `window` sends, on the main thread, which is CEF's UI thread
/// on macOS.
extern "C" fn send(window: u32, path: *const c_char) {
    // SAFETY: the interface passes a string that lives during the call.
    let path = unsafe { CStr::from_ptr(path) };
    if let Some(command) = path.to_str().ok().and_then(Command::parse) {
        crate::shell::run_in(window, command);
    }
}

/// The file behind a url of the app's UI, like a tab's cached icon, for the interface to load;
/// null for none. The caller frees it with arbalete_free_string.
#[unsafe(no_mangle)]
pub extern "C" fn arbalete_ui_file(url: *const c_char) -> *mut c_char {
    // SAFETY: the interface passes a string that lives during the call.
    let url = unsafe { CStr::from_ptr(url) };
    url.to_str()
        .ok()
        .and_then(crate::shell::ui_file_path)
        .and_then(|path| CString::new(path.to_str()?).ok())
        .map_or(std::ptr::null_mut(), CString::into_raw)
}

/// Frees a string this side gave the interface.
#[unsafe(no_mangle)]
pub extern "C" fn arbalete_free_string(string: *mut c_char) {
    if !string.is_null() {
        // SAFETY: made by CString::into_raw in arbalete_ui_file.
        drop(unsafe { CString::from_raw(string) });
    }
}

#[cfg(test)]
mod tests {
    use super::parse_event;

    #[test]
    fn parses_the_events_dispatched_to_the_web_ui() {
        assert_eq!(
            parse_event(r#"new CustomEvent("arbalete:state", { detail: {"space":1} })"#),
            Some(("arbalete:state", r#"{"space":1}"#))
        );
        assert_eq!(
            parse_event(r#"new Event("arbalete:show-history")"#),
            Some(("arbalete:show-history", "null"))
        );
        assert_eq!(parse_event("dispatchEvent()"), None);
    }
}

//! The native macOS interface (app/macos, in Swift). It gets the events the web UI gets, as their
//! name and JSON detail, and sends the same commands (app/src/command.rs).

use crate::command::Command;
use cef::{Rect, Size};
use std::ffi::{CStr, CString, c_char, c_void};

unsafe extern "C" {
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
    fn arbalete_ui_certificate(
        window: u32,
        certificates: *const *const u8,
        sizes: *const usize,
        count: usize,
    );
    fn arbalete_ui_open_little(id: u32) -> *mut c_void;
    fn arbalete_ui_little_closed(id: u32);
    fn arbalete_ui_focused_little() -> i64;
    fn arbalete_ui_little_bar();
    fn arbalete_ui_little_page(id: u32) -> *mut c_void;
    fn arbalete_ui_little_page_closed(view: *mut c_void);
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
    #[allow(clippy::too_many_arguments)]
    fn arbalete_ui_open_window(
        window: u32,
        framed: bool,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        min_width: f64,
        min_height: f64,
        send: extern "C" fn(u32, *const c_char),
    );
    fn arbalete_ui_window_frame(
        window: u32,
        x: *mut f64,
        y: *mut f64,
        width: *mut f64,
        height: *mut f64,
    );
    fn arbalete_ui_window_activate(window: u32);
    fn arbalete_ui_window_close(window: u32);
    fn arbalete_ui_window_handle(window: u32) -> *mut c_void;
    fn arbalete_ui_extension_popup_view(window: u32) -> *mut std::ffi::c_void;
    #[allow(clippy::too_many_arguments)]
    fn arbalete_ui_extension_popup_show(
        window: u32,
        button: bool,
        x: f64,
        y: f64,
        button_width: f64,
        button_height: f64,
        width: f64,
        height: f64,
    );
    fn arbalete_ui_extension_popup_closed(view: *mut std::ffi::c_void);
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

/// A main window, which the native interface makes and keeps (app/macos/MainWindow.swift), as
/// the app knows it, by its id: as CEF's window would, it tells its size and bounds, from the
/// main screen's top left corner, shows, comes in front and closes.
#[derive(Clone)]
pub struct Window(pub u32);

impl Window {
    /// Makes the window `id`, with its interface, at `bounds` if any, else centered; at least
    /// `min` large.
    pub fn open(id: u32, bounds: Option<&Rect>, min: &Size) -> Self {
        let rect = bounds.cloned().unwrap_or_default();
        // SAFETY: plain values, and `send` lives as long as the app.
        unsafe {
            arbalete_ui_open_window(
                id,
                bounds.is_some(),
                rect.x.into(),
                rect.y.into(),
                rect.width.into(),
                rect.height.into(),
                min.width.into(),
                min.height.into(),
                send,
            )
        };
        Window(id)
    }

    pub fn bounds(&self) -> Rect {
        let (mut x, mut y, mut width, mut height) = (0.0, 0.0, 0.0, 0.0);
        // SAFETY: the interface writes the four values during the call.
        unsafe { arbalete_ui_window_frame(self.0, &mut x, &mut y, &mut width, &mut height) };
        let round = |value: f64| value.round() as i32;
        Rect {
            x: round(x),
            y: round(y),
            width: round(width),
            height: round(height),
        }
    }

    /// Its content's size: the window's, its content filling it.
    pub fn size(&self) -> Size {
        let bounds = self.bounds();
        Size {
            width: bounds.width,
            height: bounds.height,
        }
    }

    /// Shows it in front, with the keyboard.
    pub fn show(&self) {
        self.activate();
    }

    pub fn activate(&self) {
        // SAFETY: a plain value.
        unsafe { arbalete_ui_window_activate(self.0) };
    }

    /// Asks it to close, which the app may refuse while its pages close.
    pub fn close(&self) {
        // SAFETY: a plain value.
        unsafe { arbalete_ui_window_close(self.0) };
    }

    /// Its NSWindow, for app/src/mac.rs.
    pub fn window_handle(&self) -> *mut c_void {
        // SAFETY: a plain value.
        unsafe { arbalete_ui_window_handle(self.0) }
    }
}

/// The window `window` resized, went into full screen or out of it.
#[unsafe(no_mangle)]
pub extern "C" fn arbalete_window_resized(window: u32) {
    crate::shell::window_resized(window);
}

/// The window `window` came in front, or left it.
#[unsafe(no_mangle)]
pub extern "C" fn arbalete_window_activated(window: u32, active: bool) {
    crate::shell::window_activated(window, active);
}

/// Whether the window `window` may close now.
#[unsafe(no_mangle)]
pub extern "C" fn arbalete_window_should_close(window: u32) -> bool {
    crate::shell::window_can_close(window)
}

/// The window `window` closed.
#[unsafe(no_mangle)]
pub extern "C" fn arbalete_window_closed(window: u32) {
    crate::shell::window_closed(window);
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

/// Opens the Little Arbalete window `id`, with its bar, and returns the view CEF makes its page's
/// browser in.
pub fn open_little(id: u32) -> Option<*mut c_void> {
    // SAFETY: a plain value; the view lives until little_closed.
    let view = unsafe { arbalete_ui_open_little(id) };
    (!view.is_null()).then_some(view)
}

/// Closes the Little Arbalete window `id`, its page closing: the page's view goes with it, which
/// destroys its browser.
pub fn little_closed(id: u32) {
    // SAFETY: a plain value.
    unsafe { arbalete_ui_little_closed(id) };
}

/// Shows the Command Bar that opens a Little Arbalete window, like Little Arc's.
pub fn little_bar() {
    // SAFETY: no arguments.
    unsafe { arbalete_ui_little_bar() };
}

/// The view the Little Arbalete window `id`'s page is drawn in, which CEF makes a page in.
pub fn little_page(id: u32) -> Option<*mut c_void> {
    // SAFETY: a plain value; the view lives until little_closed.
    let view = unsafe { arbalete_ui_little_page(id) };
    (!view.is_null()).then_some(view)
}

/// Takes a Little Arbalete window's page out of it, its browser, whose view is `view`, closing.
pub fn little_page_closed(view: *mut c_void) {
    // SAFETY: CEF's view of a browser closing, which the call only compares.
    unsafe { arbalete_ui_little_page_closed(view) };
}

/// The Little Arbalete window that has the keyboard, if any.
pub fn focused_little() -> Option<u32> {
    // SAFETY: no arguments.
    u32::try_from(unsafe { arbalete_ui_focused_little() }).ok()
}

/// Whether the Little Arbalete window `id` may close now: its page closes first, which closes it.
#[unsafe(no_mangle)]
pub extern "C" fn arbalete_little_should_close(id: u32) -> bool {
    crate::shell::little_can_close(id)
}

/// Opens the settings window, or closes it.
pub fn settings(shown: bool) {
    // SAFETY: a plain value.
    unsafe { arbalete_ui_settings(shown) };
}

/// Shows a certificate sheet on the window: `chain`, DER encoded, the site's certificate first,
/// then the ones that issued it.
pub fn certificate(window: u32, chain: &[Vec<u8>]) {
    if chain.is_empty() {
        return;
    }
    let certificates: Vec<*const u8> = chain.iter().map(|der| der.as_ptr()).collect();
    let sizes: Vec<usize> = chain.iter().map(Vec::len).collect();
    // SAFETY: the arrays and the bytes they point to outlive the call, which copies them.
    unsafe { arbalete_ui_certificate(window, certificates.as_ptr(), sizes.as_ptr(), chain.len()) };
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

/// A new extension popup's view, a window of its own for the window `window`, hidden until
/// extension_popup_show: CEF makes the page's browser in it.
pub fn extension_popup_view(window: u32) -> Option<*mut std::ffi::c_void> {
    // SAFETY: a plain value; the view lives until extension_popup_closed.
    let view = unsafe { arbalete_ui_extension_popup_view(window) };
    (!view.is_null()).then_some(view)
}

/// Shows the extension popup at its page's `size`: under the site controls' `button`, in the
/// window `window`'s points from its top left corner, or without one over the window's middle.
pub fn extension_popup_show(window: u32, button: Option<&Rect>, size: &Size) {
    let rect = button.cloned().unwrap_or_default();
    // SAFETY: plain values.
    unsafe {
        arbalete_ui_extension_popup_show(
            window,
            button.is_some(),
            rect.x.into(),
            rect.y.into(),
            rect.width.into(),
            rect.height.into(),
            size.width.into(),
            size.height.into(),
        )
    };
}

/// Takes away the extension popup whose page's view is `view`, its browser closing, which
/// destroys it.
pub fn extension_popup_closed(view: *mut std::ffi::c_void) {
    // SAFETY: the interface only compares the address.
    unsafe { arbalete_ui_extension_popup_closed(view) };
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

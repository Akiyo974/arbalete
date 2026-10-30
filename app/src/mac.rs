//! The NSApplication subclass CEF requires on macOS, and the menus.

use crate::command::{Command, Zoom};
use crate::menu::{Action, Entry};
use crate::swipe::{Phase, Scroll};
use block2::RcBlock;
use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
use objc2::{
    AnyThread, ClassType, DefinedClass, Encoding, MainThreadMarker, MainThreadOnly, RefEncode,
    define_class, extern_methods, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool, NSObjectProtocol, Sel},
    sel,
};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSApp, NSApplication, NSColor, NSControlStateValueOn,
    NSDownArrowFunctionKey, NSEvent, NSEventMask, NSEventModifierFlags, NSEventPhase, NSEventType,
    NSF12FunctionKey, NSLeftArrowFunctionKey, NSMenu, NSMenuItem, NSPasteboard,
    NSPasteboardTypeString, NSRightArrowFunctionKey, NSSharingServicePicker, NSTextField,
    NSUpArrowFunctionKey, NSView, NSWindow, NSWindowButton, NSWorkspace,
};
use objc2_foundation::{
    NSAppleEventDescriptor, NSAppleEventManager, NSArray, NSNumber, NSProcessInfo, NSRect,
    NSRectEdge, NSSize, NSString, NSURL, ns_string,
};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::NonNull;

/// The "get URL" Apple event's class and identifier ('GURL'), and the key of its url ('----').
const GET_URL: u32 = u32::from_be_bytes(*b"GURL");
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

/// The page's corner radius, in points.
const CARD_RADIUS: f64 = 8.0;

/// Core Graphics' color, which only passes through here: no need for its bindings.
#[repr(C)]
struct CGColor {
    _private: [u8; 0],
}

// SAFETY: the encoding Objective-C gives a CGColorRef.
unsafe impl RefEncode for CGColor {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGColor", &[]));
}

/// The width of the card's edge, in points: one pixel on a Retina display.
const CARD_EDGE: f64 = 0.5;

thread_local! {
    /// Its title says what it does: hide or show the sidebar.
    static SIDEBAR_ITEM: RefCell<Option<Retained<NSMenuItem>>> = const { RefCell::new(None) };
    /// Its title says what it does: show or hide the bar above the page.
    static TOOLBAR_ITEM: RefCell<Option<Retained<NSMenuItem>>> = const { RefCell::new(None) };
    /// Watches the clicks that open sidebar menus and the tab switcher's keys (watch_events).
    static EVENT_MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    /// The actions of the menu last shown, which its items' tags index.
    static MENU_ACTIONS: RefCell<Vec<Action>> = const { RefCell::new(Vec::new()) };
}

#[derive(Default)]
pub struct AppIvars {
    handling_send_event: Cell<Bool>,
}

define_class!(
    #[unsafe(super(NSApplication))]
    #[ivars = AppIvars]
    pub struct ArbaleteApplication;

    impl ArbaleteApplication {
        #[unsafe(method(sendEvent:))]
        unsafe fn send_event(&self, event: &NSEvent) {
            // Seen on their way to the page, which still scrolls with them.
            if event.r#type() == NSEventType::ScrollWheel {
                crate::shell::scroll(scroll(event));
            }
            let was_sending = self.ivars().handling_send_event.get();
            self.ivars().handling_send_event.set(Bool::YES);
            let _: () = msg_send![super(self), sendEvent: event];
            self.ivars().handling_send_event.set(was_sending);
        }

        // Chromium must leave the run loop to shut down: quitting closes the window first.
        #[unsafe(method(terminate:))]
        unsafe fn terminate(&self, _sender: Option<&AnyObject>) {
            crate::shell::quit();
        }

        // Menu actions reach the application at the end of the responder chain.
        #[unsafe(method(arbaleteNewTab:))]
        fn new_tab(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ShowOverlay);
        }

        #[unsafe(method(arbaleteOpenLocation:))]
        fn open_location(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::EditUrl);
        }

        #[unsafe(method(arbaleteCloseTab:))]
        fn close_tab(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Close(None));
        }

        #[unsafe(method(arbaleteReopenTab:))]
        fn reopen_tab(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Reopen(0));
        }

        #[unsafe(method(arbaleteTogglePin:))]
        fn toggle_pin(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Pin(None));
        }

        #[unsafe(method(arbaleteNewFolder:))]
        fn new_folder(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::NewFolder(None));
        }

        #[unsafe(method(arbaleteImport:))]
        fn import(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ShowImport);
        }

        #[unsafe(method(arbaleteNextTab:))]
        fn next_tab(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::NextTab);
        }

        #[unsafe(method(arbaletePreviousTab:))]
        fn previous_tab(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::PreviousTab);
        }

        // The item's tag is the favorite's number.
        #[unsafe(method(arbaleteFavorite:))]
        fn favorite(&self, sender: Option<&NSMenuItem>) {
            if let Some(item) = sender {
                crate::shell::run(Command::Favorite(item.tag() as usize));
            }
        }

        #[unsafe(method(arbaleteNextSpace:))]
        fn next_space(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::NextSpace);
        }

        #[unsafe(method(arbaletePreviousSpace:))]
        fn previous_space(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::PreviousSpace);
        }

        // The item's tag is the Space's number.
        #[unsafe(method(arbaleteSpace:))]
        fn space(&self, sender: Option<&NSMenuItem>) {
            if let Some(item) = sender {
                crate::shell::run(Command::SpaceNumber(item.tag() as usize));
            }
        }

        #[unsafe(method(arbaleteBack:))]
        fn back(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Back);
        }

        #[unsafe(method(arbaleteForward:))]
        fn forward(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Forward);
        }

        #[unsafe(method(arbaleteToggleSidebar:))]
        fn toggle_sidebar(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ToggleSidebar);
        }

        #[unsafe(method(arbaleteToggleToolbar:))]
        fn toggle_toolbar(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ToggleToolbar);
        }

        // The item's tag indexes the actions of the menu last shown.
        #[unsafe(method(arbaleteMenuItem:))]
        fn menu_item(&self, sender: Option<&NSMenuItem>) {
            let action = sender.and_then(|item| {
                MENU_ACTIONS.with_borrow(|actions| actions.get(item.tag() as usize).cloned())
            });
            if let Some(action) = action {
                crate::shell::perform(action);
            }
        }

        #[unsafe(method(arbaleteReload:))]
        fn reload(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Reload);
        }

        #[unsafe(method(arbaleteForceReload:))]
        fn force_reload(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ForceReload);
        }

        #[unsafe(method(arbaleteViewSource:))]
        fn view_source(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ViewSource);
        }

        #[unsafe(method(arbaleteDevTools:))]
        fn dev_tools(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::DevTools(None));
        }

        #[unsafe(method(arbaleteStop:))]
        fn stop(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Stop);
        }

        #[unsafe(method(arbaleteZoomIn:))]
        fn zoom_in(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Zoom(Zoom::In));
        }

        #[unsafe(method(arbaleteZoomOut:))]
        fn zoom_out(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Zoom(Zoom::Out));
        }

        #[unsafe(method(arbaleteActualSize:))]
        fn actual_size(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Zoom(Zoom::Reset));
        }

        #[unsafe(method(arbaleteFind:))]
        fn find(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ShowFind);
        }

        #[unsafe(method(arbaleteFindNext:))]
        fn find_next(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Find { text: None, forward: true });
        }

        #[unsafe(method(arbaleteFindPrevious:))]
        fn find_previous(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Find { text: None, forward: false });
        }

        #[unsafe(method(arbaleteHistory:))]
        fn history(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ShowHistory);
        }

        #[unsafe(method(arbaleteDownloads:))]
        fn downloads(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ShowDownloads);
        }

        #[unsafe(method(arbaleteNewLittle:))]
        fn new_little(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::NewLittle(None));
        }

        #[unsafe(method(arbaleteOpenLittleInSpace:))]
        fn open_little_in_space(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::OpenLittleInSpace);
        }

        // A link another app asks to open, once Arbalete is the default browser (watch_links).
        #[unsafe(method(arbaleteGetUrl:withReplyEvent:))]
        fn get_url(&self, event: &NSAppleEventDescriptor, _reply: &NSAppleEventDescriptor) {
            let url: Option<Retained<NSAppleEventDescriptor>> =
                unsafe { msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT] };
            if let Some(url) = url.and_then(|url| url.stringValue()) {
                crate::shell::open_link(Some(url.to_string()));
            }
        }

        #[unsafe(method(arbaleteSettings:))]
        fn settings(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ShowSettings);
        }

        #[unsafe(method(arbaleteLibrary:))]
        fn library(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ShowLibraryPage);
        }

        #[unsafe(method(arbaleteCopyUrl:))]
        fn copy_url(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::CopyLink);
        }

        #[unsafe(method(arbaleteClearToday:))]
        fn clear_today(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::ClearToday);
        }

        #[unsafe(method(arbaletePrint:))]
        fn print(&self, _sender: Option<&AnyObject>) {
            crate::shell::run(Command::Print);
        }
    }

    unsafe impl CrAppControlProtocol for ArbaleteApplication {
        #[unsafe(method(setHandlingSendEvent:))]
        unsafe fn set_handling_send_event(&self, value: Bool) {
            self.ivars().handling_send_event.set(value);
        }
    }

    unsafe impl CrAppProtocol for ArbaleteApplication {
        #[unsafe(method(isHandlingSendEvent))]
        unsafe fn is_handling_send_event(&self) -> Bool {
            self.ivars().handling_send_event.get()
        }
    }

    unsafe impl CefAppProtocol for ArbaleteApplication {}
);

impl ArbaleteApplication {
    extern_methods! {
        #[unsafe(method(sharedApplication))]
        fn shared_application() -> Retained<Self>;
    }
}

/// Watches clicks and keys as the application takes them from the system: mouse events never
/// reach sendEvent: above, which Chromium's windows bypass.
/// Opens the links other apps ask the default browser to open, which macOS sends as "get URL"
/// Apple events, in Little Arbalete windows. The bundle declares the http and https schemes
/// (the `app` task in .config/mise.toml).
pub fn watch_links() {
    let manager = NSAppleEventManager::sharedAppleEventManager();
    let app = ArbaleteApplication::shared_application();
    // SAFETY: the app implements the selector, with the arguments Apple events handlers take.
    unsafe {
        let _: () = msg_send![
            &*manager,
            setEventHandler: &*app,
            andSelector: sel!(arbaleteGetUrl:withReplyEvent:),
            forEventClass: GET_URL,
            andEventID: GET_URL
        ];
    }
}

pub fn watch_events() {
    let mask = NSEventMask::RightMouseDown
        | NSEventMask::LeftMouseDown
        | NSEventMask::KeyDown
        | NSEventMask::FlagsChanged;
    let handler = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit gives a valid event.
        let event_ref = unsafe { event.as_ref() };
        if swallows_menu_click(event_ref) || switches_tabs(event_ref) {
            std::ptr::null_mut()
        } else {
            event.as_ptr()
        }
    });
    // SAFETY: the handler returns the event it is given, or null to drop it.
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &handler) };
    EVENT_MONITOR.set(monitor);
}

/// The tab switcher's keys, whatever has the focus, page included: ⌃⇥ and ⌃⇧⇥ pick a tab,
/// releasing ⌃ shows it. Only ⌃⇥ is taken from the page.
fn switches_tabs(event: &NSEvent) -> bool {
    /// The Tab key's virtual key code.
    const TAB: u16 = 48;
    let flags = event.modifierFlags();
    match event.r#type() {
        NSEventType::KeyDown
            if event.keyCode() == TAB
                && flags.contains(NSEventModifierFlags::Control)
                && !flags.contains(NSEventModifierFlags::Command) =>
        {
            let forward = !flags.contains(NSEventModifierFlags::Shift);
            crate::shell::run(Command::SwitchTab(forward));
            true
        }
        NSEventType::FlagsChanged if !flags.contains(NSEventModifierFlags::Control) => {
            crate::shell::run(Command::EndTabSwitch);
            false
        }
        _ => false,
    }
}

/// Keeps right-clicks, and clicks with ⌃, over the sidebar from the page, which would show
/// Chromium's page menu: the sidebar shows its own at once (shell::menu_at), whose tracking takes
/// the release, as a native view's menu does.
fn swallows_menu_click(event: &NSEvent) -> bool {
    let menu_click = match event.r#type() {
        NSEventType::RightMouseDown => true,
        NSEventType::LeftMouseDown => event
            .modifierFlags()
            .contains(NSEventModifierFlags::Control),
        _ => false,
    };
    menu_click && {
        let (x, y) = main_window_point(event);
        crate::shell::menu_at(x, y)
    }
}

/// Only scroll events have phases and deltas.
fn scroll(event: &NSEvent) -> Scroll {
    let phase = event.phase();
    let phase = if phase.is_empty() {
        Phase::None
    } else if phase.contains(NSEventPhase::Began) {
        Phase::Began
    } else if phase.contains(NSEventPhase::Changed) {
        Phase::Changed
    } else if phase.intersects(NSEventPhase::Ended | NSEventPhase::Cancelled) {
        Phase::Ended
    } else {
        Phase::Other
    };
    let (x, y) = main_window_point(event);
    Scroll {
        phase,
        momentum: !event.momentumPhase().is_empty(),
        dx: event.scrollingDeltaX(),
        dy: event.scrollingDeltaY(),
        y,
        time: event.timestamp(),
        inside: crate::shell::is_over_sidebar(x),
    }
}

/// Where an event happened in the main window, from its top left corner: the views above the
/// page, like the card, are windows of their own.
fn main_window_point(event: &NSEvent) -> (f64, f64) {
    let point = event.locationInWindow();
    let mtm = MainThreadMarker::new().expect("main thread");
    let Some(window) = event.window(mtm) else {
        return (point.x, point.y);
    };
    let main = window.parentWindow().unwrap_or_else(|| window.clone());
    let (frame, main) = (window.frame(), main.frame());
    // The system counts from the bottom of the screen.
    let top = main.origin.y + main.size.height;
    (
        frame.origin.x + point.x - main.origin.x,
        top - (frame.origin.y + point.y),
    )
}

pub fn setup_application() {
    let _ = ArbaleteApplication::shared_application();
    let mtm = MainThreadMarker::new().expect("main thread");
    assert!(NSApp(mtm).isKindOfClass(ArbaleteApplication::class()));
}

/// The menus, built in code. Their key equivalents are the app's keyboard shortcuts.
pub fn setup_menu() {
    let mtm = MainThreadMarker::new().expect("main thread");
    let menu_bar = NSMenu::new(mtm);
    for (title, items) in [
        (
            ns_string!("Arbalete"),
            vec![
                item(
                    mtm,
                    ns_string!("Settings…"),
                    sel!(arbaleteSettings:),
                    ns_string!(","),
                ),
                NSMenuItem::separatorItem(mtm),
                item(
                    mtm,
                    ns_string!("Quit Arbalete"),
                    sel!(terminate:),
                    ns_string!("q"),
                ),
            ],
        ),
        (
            ns_string!("File"),
            vec![
                item(
                    mtm,
                    ns_string!("New Tab"),
                    sel!(arbaleteNewTab:),
                    ns_string!("t"),
                ),
                option_item(
                    mtm,
                    ns_string!("New Little Arbalete Window"),
                    sel!(arbaleteNewLittle:),
                    ns_string!("n"),
                ),
                item(
                    mtm,
                    ns_string!("Open in Space"),
                    sel!(arbaleteOpenLittleInSpace:),
                    ns_string!("o"),
                ),
                item(
                    mtm,
                    ns_string!("Open Location…"),
                    sel!(arbaleteOpenLocation:),
                    ns_string!("l"),
                ),
                item(
                    mtm,
                    ns_string!("Close Tab"),
                    sel!(arbaleteCloseTab:),
                    ns_string!("w"),
                ),
                // An uppercase key equivalent adds ⇧.
                item(
                    mtm,
                    ns_string!("Reopen Closed Tab"),
                    sel!(arbaleteReopenTab:),
                    ns_string!("T"),
                ),
                item(
                    mtm,
                    ns_string!("Pin or Unpin Tab"),
                    sel!(arbaleteTogglePin:),
                    ns_string!("d"),
                ),
                // ⇧⌘N is the incognito window's; Arc's New Folder has no shortcut either.
                item(
                    mtm,
                    ns_string!("New Folder"),
                    sel!(arbaleteNewFolder:),
                    ns_string!(""),
                ),
                // No shortcut: imports are rare.
                item(
                    mtm,
                    ns_string!("Import…"),
                    sel!(arbaleteImport:),
                    ns_string!(""),
                ),
                NSMenuItem::separatorItem(mtm),
                item(
                    mtm,
                    ns_string!("Print…"),
                    sel!(arbaletePrint:),
                    ns_string!("p"),
                ),
            ],
        ),
        (ns_string!("Edit"), edit_menu(mtm)),
        (ns_string!("Tabs"), tabs_menu(mtm)),
        (ns_string!("Spaces"), spaces_menu(mtm)),
        (
            ns_string!("View"),
            vec![
                sidebar_item(mtm),
                toolbar_item(mtm),
                NSMenuItem::separatorItem(mtm),
                item(
                    mtm,
                    ns_string!("Stop Loading"),
                    sel!(arbaleteStop:),
                    ns_string!("."),
                ),
                item(
                    mtm,
                    ns_string!("Reload Page"),
                    sel!(arbaleteReload:),
                    ns_string!("r"),
                ),
                item(
                    mtm,
                    ns_string!("Force Reload Page"),
                    sel!(arbaleteForceReload:),
                    ns_string!("R"),
                ),
                NSMenuItem::separatorItem(mtm),
                item(
                    mtm,
                    ns_string!("Actual Size"),
                    sel!(arbaleteActualSize:),
                    ns_string!("0"),
                ),
                // "=" rather than "+", which needs ⇧ on most keyboards.
                item(
                    mtm,
                    ns_string!("Zoom In"),
                    sel!(arbaleteZoomIn:),
                    ns_string!("="),
                ),
                item(
                    mtm,
                    ns_string!("Zoom Out"),
                    sel!(arbaleteZoomOut:),
                    ns_string!("-"),
                ),
                NSMenuItem::separatorItem(mtm),
                option_item(
                    mtm,
                    ns_string!("View Source"),
                    sel!(arbaleteViewSource:),
                    ns_string!("u"),
                ),
                option_item(
                    mtm,
                    ns_string!("Developer Tools"),
                    sel!(arbaleteDevTools:),
                    ns_string!("i"),
                ),
                function_key_item(
                    mtm,
                    ns_string!("Developer Tools"),
                    sel!(arbaleteDevTools:),
                    NSF12FunctionKey,
                ),
            ],
        ),
        (
            ns_string!("History"),
            vec![
                item(
                    mtm,
                    ns_string!("Back"),
                    sel!(arbaleteBack:),
                    ns_string!("["),
                ),
                item(
                    mtm,
                    ns_string!("Forward"),
                    sel!(arbaleteForward:),
                    ns_string!("]"),
                ),
                NSMenuItem::separatorItem(mtm),
                item(
                    mtm,
                    ns_string!("Show All History"),
                    sel!(arbaleteHistory:),
                    ns_string!("y"),
                ),
            ],
        ),
        (ns_string!("Window"), window_menu(mtm)),
    ] {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), title);
        for item in items {
            menu.addItem(&item);
        }
        let menu_item = NSMenuItem::new(mtm);
        menu_item.setSubmenu(Some(&menu));
        menu_bar.addItem(&menu_item);
        // AppKit lists the open windows in it.
        if title == ns_string!("Window") {
            NSApp(mtm).setWindowsMenu(Some(&menu));
        }
    }
    NSApp(mtm).setMainMenu(Some(&menu_bar));
}

/// Standard editing actions: they reach the focused web view, page or UI, through the responder
/// chain, which Chromium answers.
fn edit_menu(mtm: MainThreadMarker) -> Vec<Retained<NSMenuItem>> {
    let paste_and_match_style = item(
        mtm,
        ns_string!("Paste and Match Style"),
        sel!(pasteAndMatchStyle:),
        ns_string!("V"),
    );
    paste_and_match_style.setKeyEquivalentModifierMask(
        NSEventModifierFlags::Command | NSEventModifierFlags::Option | NSEventModifierFlags::Shift,
    );
    vec![
        item(mtm, ns_string!("Undo"), sel!(undo:), ns_string!("z")),
        item(mtm, ns_string!("Redo"), sel!(redo:), ns_string!("Z")),
        NSMenuItem::separatorItem(mtm),
        item(mtm, ns_string!("Cut"), sel!(cut:), ns_string!("x")),
        item(mtm, ns_string!("Copy"), sel!(copy:), ns_string!("c")),
        item(mtm, ns_string!("Paste"), sel!(paste:), ns_string!("v")),
        paste_and_match_style,
        item(mtm, ns_string!("Delete"), sel!(delete:), ns_string!("")),
        item(
            mtm,
            ns_string!("Select All"),
            sel!(selectAll:),
            ns_string!("a"),
        ),
        NSMenuItem::separatorItem(mtm),
        item(
            mtm,
            ns_string!("Copy URL"),
            sel!(arbaleteCopyUrl:),
            ns_string!("C"),
        ),
        NSMenuItem::separatorItem(mtm),
        item(
            mtm,
            ns_string!("Find…"),
            sel!(arbaleteFind:),
            ns_string!("f"),
        ),
        item(
            mtm,
            ns_string!("Find Next"),
            sel!(arbaleteFindNext:),
            ns_string!("g"),
        ),
        item(
            mtm,
            ns_string!("Find Previous"),
            sel!(arbaleteFindPrevious:),
            ns_string!("G"),
        ),
    ]
}

fn window_menu(mtm: MainThreadMarker) -> Vec<Retained<NSMenuItem>> {
    vec![
        item(
            mtm,
            ns_string!("Minimize"),
            sel!(performMiniaturize:),
            ns_string!("m"),
        ),
        item(mtm, ns_string!("Zoom"), sel!(performZoom:), ns_string!("")),
        NSMenuItem::separatorItem(mtm),
        item(
            mtm,
            ns_string!("Library"),
            sel!(arbaleteLibrary:),
            ns_string!("L"),
        ),
        item(
            mtm,
            ns_string!("Downloads"),
            sel!(arbaleteDownloads:),
            ns_string!("J"),
        ),
        NSMenuItem::separatorItem(mtm),
        item(
            mtm,
            ns_string!("Bring All to Front"),
            sel!(arrangeInFront:),
            ns_string!(""),
        ),
    ]
}

fn sidebar_item(mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    let item = item(
        mtm,
        ns_string!("Hide Sidebar"),
        sel!(arbaleteToggleSidebar:),
        ns_string!("s"),
    );
    SIDEBAR_ITEM.set(Some(item.clone()));
    item
}

fn toolbar_item(mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    let item = item(
        mtm,
        ns_string!("Show Toolbar"),
        sel!(arbaleteToggleToolbar:),
        ns_string!("D"),
    );
    TOOLBAR_ITEM.set(Some(item.clone()));
    item
}

/// Updates the menu once the bar above the page is shown or hidden.
pub fn toolbar_shown(shown: bool) {
    if let Some(item) = TOOLBAR_ITEM.with_borrow(Clone::clone) {
        item.setTitle(if shown {
            ns_string!("Hide Toolbar")
        } else {
            ns_string!("Show Toolbar")
        });
    }
}

/// Updates the menu once the sidebar is shown or hidden, and hides the window buttons with it,
/// as they sit in its toolbar. `view` is the window's content view.
pub fn sidebar_shown(view: *mut c_void, shown: bool) {
    if let Some(item) = SIDEBAR_ITEM.with_borrow(Clone::clone) {
        item.setTitle(if shown {
            ns_string!("Hide Sidebar")
        } else {
            ns_string!("Show Sidebar")
        });
    }
    // SAFETY: CEF gives the window's NSView, which the window keeps alive.
    let Some(window) = unsafe { view.cast::<NSView>().as_ref() }.and_then(NSView::window) else {
        return;
    };
    for button in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = window.standardWindowButton(button) {
            button.setHidden(!shown);
        }
    }
}

/// Next and previous tab with ⌘⌥↓ and ⌘⌥↑, or ⌘} and ⌘{, Clear Today with ⇧⌘K, then the
/// favorites with ⌘1 to ⌘9.
fn tabs_menu(mtm: MainThreadMarker) -> Vec<Retained<NSMenuItem>> {
    let mut items = vec![
        arrow_item(
            mtm,
            ns_string!("Next Tab"),
            sel!(arbaleteNextTab:),
            NSDownArrowFunctionKey,
        ),
        arrow_item(
            mtm,
            ns_string!("Previous Tab"),
            sel!(arbaletePreviousTab:),
            NSUpArrowFunctionKey,
        ),
        hidden_item(
            mtm,
            ns_string!("Next Tab"),
            sel!(arbaleteNextTab:),
            ns_string!("}"),
        ),
        hidden_item(
            mtm,
            ns_string!("Previous Tab"),
            sel!(arbaletePreviousTab:),
            ns_string!("{"),
        ),
        item(
            mtm,
            ns_string!("Clear Today"),
            sel!(arbaleteClearToday:),
            ns_string!("K"),
        ),
        NSMenuItem::separatorItem(mtm),
    ];
    for number in 1..=9 {
        let title = if number == 9 {
            "Last Favorite".to_string()
        } else {
            format!("Favorite {number}")
        };
        let item = item(
            mtm,
            &NSString::from_str(&title),
            sel!(arbaleteFavorite:),
            &NSString::from_str(&number.to_string()),
        );
        item.setTag(number);
        items.push(item);
    }
    items
}

/// Next and previous Space with ⌘⌥→ and ⌘⌥←, as Spaces sit side by side, then the Spaces by
/// number with ⌃1 to ⌃9.
fn spaces_menu(mtm: MainThreadMarker) -> Vec<Retained<NSMenuItem>> {
    let mut items = vec![
        arrow_item(
            mtm,
            ns_string!("Next Space"),
            sel!(arbaleteNextSpace:),
            NSRightArrowFunctionKey,
        ),
        arrow_item(
            mtm,
            ns_string!("Previous Space"),
            sel!(arbaletePreviousSpace:),
            NSLeftArrowFunctionKey,
        ),
    ];
    for number in 1..=9 {
        let item = item(
            mtm,
            &NSString::from_str(&format!("Space {number}")),
            sel!(arbaleteSpace:),
            &NSString::from_str(&number.to_string()),
        );
        item.setKeyEquivalentModifierMask(NSEventModifierFlags::Control);
        item.setTag(number);
        items.push(item);
    }
    items
}

/// A menu item the menu does not show, whose shortcut, ⌘ and `key`, sends `action`: a second
/// shortcut for an item shown.
fn hidden_item(
    mtm: MainThreadMarker,
    title: &NSString,
    action: Sel,
    key: &NSString,
) -> Retained<NSMenuItem> {
    let item = item(mtm, title, action, key);
    item.setHidden(true);
    item.setAllowsKeyEquivalentWhenHidden(true);
    item
}

/// A menu item sending `action` with ⌘⌥ and `key`.
fn option_item(
    mtm: MainThreadMarker,
    title: &NSString,
    action: Sel,
    key: &NSString,
) -> Retained<NSMenuItem> {
    let item = item(mtm, title, action, key);
    item.setKeyEquivalentModifierMask(NSEventModifierFlags::Command | NSEventModifierFlags::Option);
    item
}

/// A menu item the menu does not show, whose shortcut is a function `key` alone, like F12: a
/// second shortcut for an item shown.
fn function_key_item(
    mtm: MainThreadMarker,
    title: &NSString,
    action: Sel,
    key: u32,
) -> Retained<NSMenuItem> {
    let key = char::from_u32(key).expect("a function key").to_string();
    let item = item(mtm, title, action, &NSString::from_str(&key));
    item.setKeyEquivalentModifierMask(NSEventModifierFlags::empty());
    item.setHidden(true);
    item.setAllowsKeyEquivalentWhenHidden(true);
    item
}

/// A menu item sending `action` with ⌘⌥ and an arrow `key`.
fn arrow_item(
    mtm: MainThreadMarker,
    title: &NSString,
    action: Sel,
    key: u32,
) -> Retained<NSMenuItem> {
    let key = char::from_u32(key).expect("a function key").to_string();
    let item = item(mtm, title, action, &NSString::from_str(&key));
    item.setKeyEquivalentModifierMask(NSEventModifierFlags::Command | NSEventModifierFlags::Option);
    item
}

/// A menu item sending `action` with ⌘ and `key`.
fn item(
    mtm: MainThreadMarker,
    title: &NSString,
    action: Sel,
    key: &NSString,
) -> Retained<NSMenuItem> {
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            title,
            Some(action),
            key,
        )
    }
}

/// Shows a menu at the pointer until an item is picked, which runs, or the menu is dismissed.
pub fn pop_up(entries: Vec<Entry>) {
    let mtm = MainThreadMarker::new().expect("main thread");
    let mut actions = Vec::new();
    let menu = native_menu(mtm, entries, &mut actions);
    MENU_ACTIONS.set(actions);
    menu.popUpMenuPositioningItem_atLocation_inView(None, NSEvent::mouseLocation(), None);
}

fn native_menu(
    mtm: MainThreadMarker,
    entries: Vec<Entry>,
    actions: &mut Vec<Action>,
) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    for entry in entries {
        let item = match entry {
            Entry::Item(entry) => {
                let item = item(
                    mtm,
                    &NSString::from_str(&entry.title),
                    sel!(arbaleteMenuItem:),
                    ns_string!(""),
                );
                item.setTag(actions.len() as isize);
                if entry.checked {
                    item.setState(NSControlStateValueOn);
                }
                item.setEnabled(entry.enabled);
                actions.push(entry.action);
                item
            }
            Entry::Submenu(title, entries) => {
                let item = NSMenuItem::new(mtm);
                item.setTitle(&NSString::from_str(&title));
                item.setSubmenu(Some(&native_menu(mtm, entries, actions)));
                item
            }
            Entry::Separator => NSMenuItem::separatorItem(mtm),
        };
        menu.addItem(&item);
    }
    menu
}

/// Asks the user to confirm an action named by `button`, which may destroy data.
pub fn confirm(question: &str, detail: &str, button: &str) -> bool {
    let mtm = MainThreadMarker::new().expect("main thread");
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(question));
    alert.setInformativeText(&NSString::from_str(detail));
    alert
        .addButtonWithTitle(&NSString::from_str(button))
        .setHasDestructiveAction(true);
    alert.addButtonWithTitle(ns_string!("Cancel"));
    alert.runModal() == NSAlertFirstButtonReturn
}

/// Asks for a text in a dialog, its field holding `value`, selected: what was entered, unless the
/// dialog was cancelled.
pub fn prompt(question: &str, detail: &str, value: &str, button: &str) -> Option<String> {
    let mtm = MainThreadMarker::new().expect("main thread");
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(question));
    alert.setInformativeText(&NSString::from_str(detail));
    alert.addButtonWithTitle(&NSString::from_str(button));
    alert.addButtonWithTitle(ns_string!("Cancel"));
    let field = NSTextField::textFieldWithString(&NSString::from_str(value), mtm);
    field.setFrameSize(NSSize::new(300.0, 24.0));
    alert.setAccessoryView(Some(&field));
    alert.window().setInitialFirstResponder(Some(&field));
    (alert.runModal() == NSAlertFirstButtonReturn).then(|| field.stringValue().to_string())
}

/// Opens a file with its default app.
pub fn open_file(path: &std::path::Path) {
    NSWorkspace::sharedWorkspace().openURL(&file_url(path));
}

/// Shows a file selected in a Finder window.
pub fn show_file(path: &std::path::Path) {
    let urls = NSArray::from_retained_slice(&[file_url(path)]);
    NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&urls);
}

fn file_url(path: &std::path::Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

/// Shows the system's share menu for a link at the pointer, over the window that has it.
pub fn share(url: &str) {
    let mtm = MainThreadMarker::new().expect("main thread");
    let Some(window) = NSApplication::sharedApplication(mtm).keyWindow() else {
        return;
    };
    let Some(view) = window.contentView() else {
        return;
    };
    let Some(link) = NSURL::URLWithString(&NSString::from_str(url)) else {
        return;
    };
    let items = NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(link))]);
    // SAFETY: the picker takes the items a share menu offers, like links.
    let picker =
        unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items) };
    let point = view.convertPoint_fromView(window.mouseLocationOutsideOfEventStream(), None);
    let rect = NSRect::new(point, NSSize::new(1.0, 1.0));
    picker.showRelativeToRect_ofView_preferredEdge(rect, &view, NSRectEdge::MinY);
}

/// How far down the main window the pointer is, in points, if over it.
pub fn pointer_y() -> Option<i32> {
    let mtm = MainThreadMarker::new()?;
    let window = NSApplication::sharedApplication(mtm).mainWindow()?;
    let point = window.mouseLocationOutsideOfEventStream();
    let height = window.frame().size.height;
    (0.0..=height)
        .contains(&point.y)
        .then_some((height - point.y) as i32)
}

/// Puts a link on the clipboard.
pub fn copy(url: &str) {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    pasteboard.setString_forType(&NSString::from_str(url), unsafe { NSPasteboardTypeString });
}

/// Rounds the corners of the card's window, which draws the page, and outlines it. `view` is
/// its content view, whose layer holds what the window's compositor draws.
pub fn round_card(view: *mut c_void) {
    round(view, CARD_RADIUS);
}

/// Makes a window invisible and lets clicks through it, while a sheet it holds, like Chromium's
/// folder picker, shows. `handle` is the window, or a view in it, as CEF gives it.
pub fn hide_window(handle: *mut c_void) {
    // SAFETY: CEF gives a live NSWindow or NSView, which its window keeps alive.
    let Some(object) = (unsafe { handle.cast::<AnyObject>().as_ref() }) else {
        return;
    };
    let window = if let Some(window) = object.downcast_ref::<NSWindow>() {
        Some(objc2::Message::retain(window))
    } else {
        object.downcast_ref::<NSView>().and_then(NSView::window)
    };
    if let Some(window) = window {
        window.setAlphaValue(0.0);
        window.setIgnoresMouseEvents(true);
    }
}

fn round(view: *mut c_void, radius: f64) {
    // SAFETY: CEF gives the window's NSView, which the window keeps alive.
    let Some(view) = (unsafe { view.cast::<NSView>().as_ref() }) else {
        return;
    };
    // The corners show the window below.
    if let Some(window) = view.window() {
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
    }
    view.setWantsLayer(true);
    let layer: Option<Retained<AnyObject>> = unsafe { msg_send![view, layer] };
    if let Some(layer) = layer {
        let _: () = unsafe { msg_send![&*layer, setCornerRadius: radius] };
        let _: () = unsafe { msg_send![&*layer, setMasksToBounds: Bool::YES] };
        // A darker edge, drawn above the page.
        let edge = NSColor::colorWithWhite_alpha(0.0, 0.25);
        let edge: *const CGColor = unsafe { msg_send![&*edge, CGColor] };
        let _: () = unsafe { msg_send![&*layer, setBorderColor: edge] };
        let _: () = unsafe { msg_send![&*layer, setBorderWidth: CARD_EDGE] };
    }
}

/// The floating sidebar's corner radius, in points.
const FLOAT_RADIUS: f64 = 10.0;

/// The floating sidebar's edge: one point, light.
const FLOAT_EDGE: f64 = 1.0;
const FLOAT_EDGE_ALPHA: f64 = 0.2;

/// Gives the revealed sidebar's window the floating look, rounded and outlined, with a shadow, by
/// `amount` from 0, a plain view, to 1. `view` is its content view, like the card's.
pub fn float(view: *mut c_void, amount: f64) {
    // SAFETY: CEF gives the window's NSView, which the window keeps alive.
    let Some(view) = (unsafe { view.cast::<NSView>().as_ref() }) else {
        return;
    };
    if let Some(window) = view.window() {
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        // The system's shadow follows the rounded corners once they are drawn.
        window.setHasShadow(amount >= 1.0);
        window.invalidateShadow();
    }
    view.setWantsLayer(true);
    let layer: Option<Retained<AnyObject>> = unsafe { msg_send![view, layer] };
    let Some(layer) = layer else { return };
    let edge = NSColor::colorWithWhite_alpha(1.0, FLOAT_EDGE_ALPHA * amount);
    let edge: *const CGColor = unsafe { msg_send![&*edge, CGColor] };
    unsafe {
        let _: () = msg_send![&*layer, setCornerRadius: FLOAT_RADIUS * amount];
        let _: () = msg_send![&*layer, setMasksToBounds: Bool::YES];
        let _: () = msg_send![&*layer, setBorderColor: edge];
        let _: () = msg_send![&*layer, setBorderWidth: FLOAT_EDGE * amount];
    }
}

/// Slides a window's content horizontally through `offsets`, in points from where it rests, spread
/// evenly over `duration` seconds, and keeps it at the last one. The compositor runs it, frame by
/// frame, clipped by the window: the window itself stays in place. No offsets stop the slide.
pub fn slide(view: *mut c_void, offsets: &[f64], duration: f64) {
    let key = ns_string!("slide");
    // SAFETY: CEF gives the window's NSView, which the window keeps alive.
    let Some(view) = (unsafe { view.cast::<NSView>().as_ref() }) else {
        return;
    };
    view.setWantsLayer(true);
    let layer: Option<Retained<AnyObject>> = unsafe { msg_send![view, layer] };
    let Some(layer) = layer else { return };
    let _: () = unsafe { msg_send![&*layer, removeAnimationForKey: key] };
    let Some(class) = AnyClass::get(c"CAKeyframeAnimation") else {
        return;
    };
    if offsets.is_empty() {
        return;
    }
    let values: Vec<_> = offsets
        .iter()
        .map(|&offset| NSNumber::new_f64(offset))
        .collect();
    let values = NSArray::from_retained_slice(&values);
    unsafe {
        let animation: Retained<AnyObject> =
            msg_send![class, animationWithKeyPath: ns_string!("transform.translation.x")];
        let _: () = msg_send![&*animation, setValues: &*values];
        let _: () = msg_send![&*animation, setDuration: duration];
        let _: () = msg_send![&*animation, setFillMode: ns_string!("forwards")];
        let _: () = msg_send![&*animation, setRemovedOnCompletion: Bool::NO];
        let _: () = msg_send![&*layer, addAnimation: &*animation, forKey: key];
    }
}

/// Moves a window's content `x` points right of where it rests, at once, clipped by the window:
/// the window itself stays in place. `view` is any view in the window, like the card's page.
pub fn shift(view: *mut c_void, x: f64) {
    // SAFETY: CEF gives the window's NSView, which the window keeps alive.
    let Some(view) = (unsafe { view.cast::<NSView>().as_ref() }) else {
        return;
    };
    let Some(content) = view.window().and_then(|window| window.contentView()) else {
        return;
    };
    content.setWantsLayer(true);
    let layer: Option<Retained<AnyObject>> = unsafe { msg_send![&*content, layer] };
    let (Some(layer), Some(transaction)) = (layer, AnyClass::get(c"CATransaction")) else {
        return;
    };
    let x = NSNumber::new_f64(x);
    unsafe {
        let _: () = msg_send![transaction, begin];
        let _: () = msg_send![transaction, setDisableActions: Bool::YES];
        let _: () = msg_send![
            &*layer,
            setValue: &*x,
            forKeyPath: ns_string!("transform.translation.x")
        ];
        let _: () = msg_send![transaction, commit];
    }
}

/// The user asked the system for less motion.
pub fn reduce_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// How hard macOS presses for memory, as the kernel tells it: 1 normal, 2 warning, 4 critical.
pub fn memory_pressure() -> crate::shell::MemoryPressure {
    use crate::shell::MemoryPressure;
    unsafe extern "C" {
        fn sysctlbyname(
            name: *const std::ffi::c_char,
            old: *mut c_void,
            old_size: *mut usize,
            new: *mut c_void,
            new_size: usize,
        ) -> std::ffi::c_int;
    }
    let mut level: i32 = 0;
    let mut size = size_of::<i32>();
    // SAFETY: the name is a C string, and `level` an integer as large as `size` says.
    let read = unsafe {
        sysctlbyname(
            c"kern.memorystatus_vm_pressure_level".as_ptr(),
            (&raw mut level).cast(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    match level {
        2 if read == 0 => MemoryPressure::Warning,
        4 if read == 0 => MemoryPressure::Critical,
        _ => MemoryPressure::Normal,
    }
}

/// Whether Low Power Mode is on, saving the battery.
pub fn low_power() -> bool {
    NSProcessInfo::processInfo().isLowPowerModeEnabled()
}

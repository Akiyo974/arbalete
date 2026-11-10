//! The native Linux interface, in GTK 4 ([decision 0011](../../docs/decisions/0011-linux-interface.md)):
//! `native.rs`'s counterpart, with the same functions. It gets events, as their name and JSON
//! detail, and runs commands (app/src/command.rs). Each page is a CEF browser in an X11 container,
//! a child of the window, placed where the app lays the page out.

use crate::command::Command;
use arbalete_core::store::Place;
use cef::{Rect, RuntimeStyle, Size, WindowInfo};
use gtk4::prelude::*;
use gtk4::{gdk, glib};
use serde::Deserialize;
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// What CEF draws a page in: an X11 window.
pub type View = cef::sys::cef_window_handle_t;

/// A browser drawn in the container `view`, `width` by `height` until the container sizes it.
pub fn page_in(view: View, width: i32, height: i32) -> WindowInfo {
    WindowInfo {
        parent_window: view,
        bounds: Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    }
}

/// A main window's interface: the GTK window, and what it lays out in it.
struct Ui {
    id: u32,
    window: gtk4::Window,
    /// Lays the sidebar and the card out where the app places them.
    stage: gtk4::Fixed,
    card: gtk4::Box,
    sidebar: Sidebar,
    command_bar: Rc<CommandBar>,
    /// Each tab's page's container, by the number the app knows the tab by.
    pages: RefCell<HashMap<i64, View>>,
    /// The bars above the pages, made as needed and kept for the next panes.
    bars: RefCell<Vec<PageBar>>,
}

thread_local! {
    static UIS: RefCell<Vec<Rc<Ui>>> = const { RefCell::new(Vec::new()) };
}

fn ui(window: u32) -> Option<Rc<Ui>> {
    UIS.with_borrow(|uis| uis.iter().find(|ui| ui.id == window).cloned())
}

/// The window that has the keyboard, else the first one: where menus and dialogs show.
pub fn front_window() -> Option<gtk4::Window> {
    UIS.with_borrow(|uis| {
        let windows = uis.iter().map(|ui| &ui.window);
        windows
            .clone()
            .find(|window| window.is_active())
            .or_else(|| windows.clone().next())
            .cloned()
    })
}

/// The window an X11 window is, or is in.
pub fn window_of(view: View) -> Option<gtk4::Window> {
    let mut view = view;
    loop {
        let found = UIS.with_borrow(|uis| {
            uis.iter()
                .find(|ui| xid(&ui.window) == Some(view))
                .map(|ui| ui.window.clone())
        });
        if found.is_some() {
            return found;
        }
        view = x11::parent(view)?;
    }
}

/// The X11 window GTK made for a window, once realized.
fn xid(window: &gtk4::Window) -> Option<View> {
    let surface = window.surface()?.downcast::<gdk4_x11::X11Surface>().ok()?;
    Some(surface.xid())
}

/// A main window, which the interface makes and keeps, as the app knows it, by its id: as CEF's
/// window would, it tells its size and bounds, from the screen's top left corner, shows, comes in
/// front and closes.
#[derive(Clone)]
pub struct Window(pub u32);

impl Window {
    /// Makes the window `id`, with its interface, at `place` if any, else centered; at least
    /// `min` large. It shows once the app has laid it out (show).
    pub fn open(id: u32, place: Option<&Place>, min: &Size) -> Self {
        let (width, height) = place.map_or((1280, 800), |place| {
            (place.width as i32, place.height as i32)
        });
        let window = gtk4::Window::builder()
            .title("Arbalete")
            .default_width(width.max(min.width))
            .default_height(height.max(min.height))
            .build();
        window.set_size_request(min.width, min.height);
        if place.is_some_and(|place| place.full_screen) {
            window.fullscreen();
        }

        // The window's background, whose size tells the app the window's.
        let background = gtk4::DrawingArea::new();
        background.add_css_class("arbalete-background");
        background.connect_resize(move |_, _, _| later(move || crate::shell::window_resized(id)));
        let stage = gtk4::Fixed::new();
        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        card.add_css_class("arbalete-card");
        card.set_visible(false);
        stage.put(&card, 0.0, 0.0);
        let sidebar = Sidebar::new(id);
        stage.put(&sidebar.root, 0.0, 0.0);
        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&background));
        overlay.add_overlay(&stage);
        window.set_child(Some(&overlay));
        let command_bar = CommandBar::new(id, &window);

        // The shortcuts, before the focused widget has the key.
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed(|_, key, _, state| {
            if crate::system::run_shortcut(key, state) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(keys);
        window.connect_is_active_notify(move |window| {
            let active = window.is_active();
            later(move || crate::shell::window_activated(id, active));
        });
        window.connect_close_request(move |_| {
            if crate::shell::window_can_close(id) {
                glib::Propagation::Proceed
            } else {
                glib::Propagation::Stop
            }
        });
        window.connect_destroy(move |_| crate::shell::window_closed(id));

        // Its X11 window must exist before the pages' containers.
        WidgetExt::realize(&window);
        WidgetExt::display(&window).sync();
        style();
        UIS.with_borrow_mut(|uis| {
            uis.push(Rc::new(Ui {
                id,
                window,
                stage,
                card,
                sidebar,
                command_bar,
                pages: RefCell::default(),
                bars: RefCell::default(),
            }))
        });
        Window(id)
    }

    fn gtk(&self) -> Option<gtk4::Window> {
        ui(self.0).map(|ui| ui.window.clone())
    }

    /// Where it is, to open there again; `None` before it is made.
    pub fn place(&self) -> Option<Place> {
        let window = self.gtk()?;
        let bounds = self.bounds();
        Some(Place {
            screen: String::new(),
            x: bounds.x.into(),
            y: bounds.y.into(),
            width: bounds.width.into(),
            height: bounds.height.into(),
            full_screen: window.is_fullscreen(),
        })
    }

    pub fn bounds(&self) -> Rect {
        let Some(window) = self.gtk() else {
            return Rect::default();
        };
        let (x, y) = xid(&window).map_or((0, 0), x11::position);
        let (width, height) = match (window.width(), window.height()) {
            (0, _) | (_, 0) => window.default_size(),
            size => size,
        };
        Rect {
            x,
            y,
            width,
            height,
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
        if let Some(window) = self.gtk() {
            window.present();
        }
    }

    /// Asks it to close, which the app may refuse while its pages close.
    pub fn close(&self) {
        if let Some(window) = self.gtk() {
            window.close();
        }
    }

    /// Its X11 window, for app/src/linux.rs.
    pub fn window_handle(&self) -> View {
        self.gtk().as_ref().and_then(xid).unwrap_or_default()
    }
}

/// Runs `work` once the current one is done: GTK tells of changes while the app may be busy
/// with the window, like as it makes it.
fn later(work: impl FnOnce() + 'static) {
    glib::idle_add_local_once(work);
}

/// Runs a command from the interface of the window `window`, given as the app reads it: its name
/// and its arguments (app/src/command.rs).
fn run(window: u32, command: serde_json::Value) {
    match serde_json::from_value::<Command>(command.clone()) {
        Ok(command) => crate::shell::run_in(window, command),
        Err(_) => eprintln!("unknown command {}", command["name"]),
    }
}

/// Shows the About window, with the version of Chromium the app embeds.
pub fn about() {
    use cef::sys::{
        CHROME_VERSION_BUILD, CHROME_VERSION_MAJOR, CHROME_VERSION_MINOR, CHROME_VERSION_PATCH,
    };
    let about = gtk4::AboutDialog::builder()
        .program_name("Arbalete")
        .version(env!("CARGO_PKG_VERSION"))
        .comments(format!(
            "Chromium {CHROME_VERSION_MAJOR}.{CHROME_VERSION_MINOR}.{CHROME_VERSION_BUILD}.{CHROME_VERSION_PATCH}"
        ))
        .website("https://arbalete.co")
        .license_type(gtk4::License::Mpl20)
        .modal(true)
        .build();
    about.set_transient_for(front_window().as_ref());
    about.present();
}

/// Takes the interface out of `window`, which closed.
pub fn detach(window: u32) {
    UIS.with_borrow_mut(|uis| uis.retain(|ui| ui.id != window));
}

/// Gives a surface of the interface of `window`, like "sidebar", an event: its name and its JSON
/// detail, `null` without one.
pub fn event(window: u32, surface: &str, name: &str, detail: &str) {
    if surface == "onboarding" {
        return onboarding_event(name);
    }
    let Some(ui) = ui(window) else { return };
    match surface {
        "sidebar" => {
            if let Some(page) = ui.sidebar.event(name, detail) {
                for bar in ui.bars.borrow().iter() {
                    bar.history(page);
                }
            }
        }
        "overlay" => ui.command_bar.event(name, detail),
        _ => {}
    }
}

/// Shows the sidebar of `window` at `frame`, in pixels from its top left corner, floating above
/// the page or docked; hides it without a frame.
pub fn place(window: u32, frame: Option<&Rect>, _floating: bool, _revealed: bool) {
    let Some(ui) = ui(window) else { return };
    let root = &ui.sidebar.root;
    match frame.filter(|frame| frame.width > 0) {
        Some(frame) => {
            ui.stage.move_(root, frame.x.into(), frame.y.into());
            root.set_size_request(frame.width, frame.height);
            root.set_visible(true);
        }
        None => root.set_visible(false),
    }
}

/// Shows a surface of the interface of `window`, like "overlay", at `frame` in its pixels from
/// its top left corner, or hides it.
pub fn surface(window: u32, name: &str, shown: bool, frame: &Rect) {
    if name != "overlay" {
        return;
    }
    if let Some(ui) = ui(window) {
        ui.command_bar.show(shown, frame);
    }
}

/// Where the card, which holds the pages, shows, and whether it is empty, or that it hides.
pub fn card(window: u32, frame: Option<Rect>, _empty: bool) {
    let Some(ui) = ui(window) else { return };
    match frame.filter(|frame| frame.width > 0) {
        Some(frame) => {
            ui.stage.move_(&ui.card, frame.x.into(), frame.y.into());
            ui.card.set_size_request(frame.width, frame.height);
            ui.card.set_visible(true);
        }
        None => ui.card.set_visible(false),
    }
}

/// A page as the app lays it out (send_panes): its tab, and its frame in the card, under its bar.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pane {
    tab: i64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    /// The height of the bar a split's page has above it.
    bar: i32,
    card_x: i32,
    card_y: i32,
    url: String,
    back: bool,
    forward: bool,
    /// It stands for the bar above a page alone, which has no tab of its own.
    toolbar: bool,
}

#[derive(Deserialize)]
struct Panes {
    panes: Vec<Pane>,
}

/// Shows the pages of a window as `panes` (JSON) lays them out: their containers where each page
/// shows, the others hidden. Returns the window's X11 window, which stands for the card.
pub fn panes(window: u32, panes: &str) -> Option<View> {
    let ui = ui(window)?;
    let panes = serde_json::from_str::<Panes>(panes).ok()?.panes;
    for (&tab, &view) in ui.pages.borrow().iter() {
        match panes.iter().find(|pane| pane.tab == tab) {
            Some(pane) => x11::show(
                view,
                pane.card_x + pane.x,
                pane.card_y + pane.y + pane.bar,
                pane.width,
                pane.height - pane.bar,
            ),
            None => x11::hide(view),
        }
    }
    // The bars above the pages: a split page's own, and the bar above a page alone.
    let mut bars = ui.bars.borrow_mut();
    let mut used = 0;
    for pane in panes.iter().filter(|pane| pane.bar > 0) {
        if used == bars.len() {
            let bar = PageBar::new(window);
            ui.stage.put(&bar.root, 0.0, 0.0);
            bars.push(bar);
        }
        let bar = &bars[used];
        bar.show(pane);
        ui.stage.move_(
            &bar.root,
            (pane.card_x + pane.x).into(),
            (pane.card_y + pane.y).into(),
        );
        bar.root.set_size_request(pane.width, pane.bar);
        used += 1;
    }
    for bar in &bars[used..] {
        bar.root.set_visible(false);
    }
    xid(&ui.window)
}

/// The bar above a page: its navigation buttons and its address, which a click edits.
struct PageBar {
    root: gtk4::Box,
    back: gtk4::Button,
    forward: gtk4::Button,
    address: gtk4::Button,
    /// The tab whose page it is above, but the bar above a page alone, the selected one.
    tab: Rc<Cell<Option<i64>>>,
}

impl PageBar {
    fn new(window: u32) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        root.add_css_class("arbalete-page-bar");
        let tab = Rc::new(Cell::new(None));
        // Each button acts on its page, which it selects first.
        let on_page = |button: &gtk4::Button, name: &'static str| {
            let tab = tab.clone();
            button.connect_clicked(move |_| {
                if let Some(tab) = tab.get() {
                    run(window, json!({ "name": "select", "args": tab }));
                }
                run(window, json!({ "name": name }));
            });
        };
        let back = icon_button("go-previous-symbolic", "Back");
        let forward = icon_button("go-next-symbolic", "Forward");
        let reload = icon_button("view-refresh-symbolic", "Reload");
        let address = gtk4::Button::new();
        address.add_css_class("flat");
        address.set_hexpand(true);
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        ellipsize(&label);
        address.set_child(Some(&label));
        on_page(&back, "back");
        on_page(&forward, "forward");
        on_page(&reload, "reload");
        on_page(&address, "edit-url");
        for widget in [&back, &forward, &reload, &address] {
            root.append(widget);
        }
        Self {
            root,
            back,
            forward,
            address,
            tab,
        }
    }

    fn show(&self, pane: &Pane) {
        self.tab.set((!pane.toolbar).then_some(pane.tab));
        // The bar above a page alone follows the app's state (history).
        if !pane.toolbar {
            self.back.set_sensitive(pane.back);
            self.forward.set_sensitive(pane.forward);
        }
        if let Some(label) = self.address.child().and_downcast::<gtk4::Label>() {
            label.set_label(&pane.url);
        }
        self.root.set_visible(true);
    }

    /// Where the selected page stands in its history, for the bar above a page alone.
    fn history(&self, page: PageState) {
        if self.tab.get().is_none() {
            self.back.set_sensitive(page.can_go_back);
            self.forward.set_sensitive(page.can_go_forward);
        }
    }
}

/// The container a tab's page is drawn in, hidden in the window `window` until it shows, which CEF
/// makes the page's browser in.
pub fn page_view(window: u32, tab: i64) -> Option<View> {
    let ui = ui(window)?;
    let view = x11::container(xid(&ui.window)?)?;
    WidgetExt::display(&ui.window).sync();
    ui.pages.borrow_mut().insert(tab, view);
    Some(view)
}

/// Takes a closing page out of its window: its container, holding `view`, CEF's window, goes,
/// which destroys its browser.
pub fn page_closed(view: View) {
    let Some(container) = x11::parent(view) else {
        return;
    };
    UIS.with_borrow(|uis| {
        for ui in uis {
            ui.pages.borrow_mut().retain(|_, page| *page != container);
        }
    });
    x11::destroy(container);
}

/// The page takes the keyboard from the interface: CEF gives its window the focus.
pub fn focus_page(_tab: i64) {}

/// Hands a window's pages over to another: their containers stay in the window that closes until
/// the app makes them again in the other.
pub fn hand_pages_over(_from: u32, _to: u32) {}

/// The text at the bottom of a page, like a link's address under the pointer: X11 lets nothing
/// cover a page, so it is not shown yet.
pub fn page_status(_view: View, _text: &str, _instant: bool) {}

/// The handles that resize a split's pages: not on Linux yet, whose splits keep their sizes.
pub fn split_handles(_window: u32, _stacked: bool, _frames: &[Rect]) {}

/// A toast, at the bottom of the page: in the window's title for now, as nothing covers a page.
pub fn toast(window: u32, _page: &Rect, toast: &str) {
    let Some(ui) = ui(window) else { return };
    let text = serde_json::from_str::<serde_json::Value>(toast)
        .ok()
        .and_then(|toast| Some(toast.get("text")?.as_str()?.to_string()));
    ui.window
        .set_title(Some(text.as_deref().unwrap_or("Arbalete")));
    let window = ui.window.clone();
    glib::timeout_add_seconds_local_once(4, move || window.set_title(Some("Arbalete")));
}

/// Opens the settings window, on the pane titled `pane` if any, or closes it: not on Linux yet.
pub fn settings(_shown: bool, _pane: Option<&str>) {}

/// Whether the settings window has the keyboard.
pub fn settings_focused() -> bool {
    false
}

thread_local! {
    static ONBOARDING: RefCell<Option<gtk4::Window>> = const { RefCell::new(None) };
}

/// Opens the welcome window, or closes it: a greeting and a button to start, until the import
/// assistant comes to Linux.
pub fn onboarding(shown: bool) {
    if !shown {
        if let Some(window) = ONBOARDING.take() {
            window.destroy();
        }
        return;
    }
    let window = gtk4::Window::builder()
        .title("Welcome to Arbalete")
        .default_width(420)
        .resizable(false)
        .build();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    content.set_margin_top(32);
    content.set_margin_bottom(32);
    content.set_margin_start(32);
    content.set_margin_end(32);
    let title = gtk4::Label::new(Some("Welcome to Arbalete"));
    title.add_css_class("title-1");
    let text = gtk4::Label::new(Some(
        "A sidebar of Spaces and tabs, a Command Bar to open anything: press Ctrl+T.",
    ));
    text.set_wrap(true);
    text.set_justify(gtk4::Justification::Center);
    let start = gtk4::Button::with_label("Get Started");
    start.add_css_class("suggested-action");
    start.connect_clicked(|_| crate::shell::run(Command::FinishOnboarding));
    content.append(&title);
    content.append(&text);
    content.append(&start);
    window.set_child(Some(&content));
    window.connect_close_request(|_| {
        later(|| crate::shell::run(Command::CloseOnboarding));
        glib::Propagation::Stop
    });
    window.present();
    ONBOARDING.set(Some(window));
}

/// The welcome window has nothing to show of the browsers found yet.
fn onboarding_event(_name: &str) {}

/// A certificate's details: Chromium's own dialog is not in CEF on Linux, so not shown yet.
pub fn certificate(_window: u32, _chain: &[Vec<u8>]) {}

/// Little Arbalete's windows: not on Linux yet, whose links open as tabs.
pub fn open_little(_id: u32, _size: Option<(i32, i32)>) -> Option<View> {
    None
}

pub fn little_closed(_id: u32) {}

pub fn little_hotkey(_on: bool) -> bool {
    false
}

pub fn little_bar() {}

pub fn little_page(_id: u32) -> Option<View> {
    None
}

pub fn little_page_closed(_view: View) {}

pub fn focused_little() -> Option<u32> {
    None
}

/// Easels: not on Linux yet, whose Easel tabs show empty.
pub fn easel(_window: u32, _tab: i64, _easel: &str) -> bool {
    false
}

pub fn peek_easel(_window: u32, _tab: i64, _title: &str, _easel: &str) -> bool {
    false
}

pub fn live_capture_view(_easel: i64, _id: i64) -> Option<View> {
    None
}

pub fn live_capture_closed(_view: View) {}

pub fn easel_closed(_tab: i64) {}

/// A capture's editor: not on Linux yet.
pub fn captured(_window: u32, _base64: &str, _mode: u32, _frame: &Rect, _blocks: &str) {}

/// The developer tools docked beside a page: not on Linux yet, where they open in a window.
pub fn tools_view(_tab: i64) -> Option<View> {
    None
}

pub fn tools_room(_tab: i64, _x: f64, _y: f64, _width: f64, _height: f64) {}

pub fn tools_docked(_tab: i64, _docked: bool) {}

pub fn tools_closed(_view: View) {}

/// Peek, a link's page above the card: not on Linux yet, where links open as tabs.
pub fn peek(_window: u32, _frame: Option<Rect>) {}

pub fn peek_view(_window: u32) -> Option<View> {
    None
}

pub fn peek_bar(
    _window: u32,
    _url: &str,
    _color: &str,
    _back: bool,
    _forward: bool,
    _loading: bool,
) {
}

pub fn peek_closed(_window: u32) {}

/// Extensions' popups: not on Linux yet.
pub fn extension_popup_view(_window: u32) -> Option<View> {
    None
}

pub fn extension_popup_show(_window: u32, _button: Option<&Rect>, _size: &Size) {}

pub fn extension_popup_closed(_view: View) {}

/// Makes Arbalete the desktop's browser for web links, through xdg-settings.
pub fn make_default_browser() {
    let _ = std::process::Command::new("xdg-settings")
        .args(["set", "default-web-browser", DESKTOP_FILE])
        .status();
}

pub fn is_default_browser() -> bool {
    std::process::Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == DESKTOP_FILE)
}

/// The desktop entry Arbalete installs, which names it to the desktop.
const DESKTOP_FILE: &str = "arbalete.desktop";

/// The app's stylesheet, once.
fn style() {
    thread_local! {
        static DONE: Cell<bool> = const { Cell::new(false) };
    }
    if DONE.replace(true) {
        return;
    }
    let css = gtk4::CssProvider::new();
    css.load_from_string(
        "
        .arbalete-background { background: alpha(@theme_fg_color, 0.06); }
        .arbalete-card { background: @theme_base_color; border-radius: 8px; }
        .arbalete-sidebar { padding: 8px 6px; }
        .arbalete-sidebar .row { border-radius: 8px; padding: 6px 8px; }
        .arbalete-sidebar .row:hover { background: alpha(@theme_fg_color, 0.07); }
        .arbalete-sidebar .row.selected { background: alpha(@theme_base_color, 0.9);
            box-shadow: 0 1px 2px alpha(black, 0.12); }
        .arbalete-sidebar .address { border-radius: 8px; padding: 6px 10px;
            background: alpha(@theme_fg_color, 0.06); }
        .arbalete-sidebar .space-title { font-weight: bold; padding: 4px 8px; }
        .arbalete-sidebar .favorite { border-radius: 8px; padding: 8px;
            background: alpha(@theme_fg_color, 0.06); }
        .arbalete-sidebar .favorite.selected { background: @theme_base_color; }
        .arbalete-results row { padding: 6px 10px; border-radius: 6px; }
        .arbalete-page-bar { background: @theme_base_color; border-radius: 8px 8px 0 0;
            border-bottom: 1px solid alpha(@theme_fg_color, 0.08); }
        ",
    );
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// The app's state, as it sends it (UiState::to_json in app/src/shell.rs): what the sidebar shows.
#[derive(Deserialize)]
struct State {
    page: PageState,
    space: i64,
    spaces: Vec<Space>,
    pages: Vec<SpacePage>,
}

/// Where the shown page stands in its history.
#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageState {
    can_go_back: bool,
    can_go_forward: bool,
}

#[derive(Deserialize)]
struct Space {
    id: i64,
    title: String,
    emoji: Option<String>,
    theme: Theme,
}

#[derive(Deserialize)]
struct Theme {
    colors: Vec<String>,
}

#[derive(Deserialize)]
struct SpacePage {
    space: i64,
    selected: Option<i64>,
    favorites: Vec<Item>,
    pinned: Vec<Item>,
    today: Vec<Item>,
}

#[derive(Clone, Deserialize)]
struct Tab {
    id: i64,
    url: String,
    title: String,
    favicon: Option<String>,
}

/// A tab, a folder or two tabs split in one row.
#[derive(Deserialize)]
#[serde(untagged)]
enum Item {
    Folder {
        id: i64,
        title: String,
        expanded: bool,
        children: Vec<Item>,
    },
    Split {
        split: Vec<Tab>,
    },
    Tab(Tab),
}

/// The sidebar: the address of the selected tab, the Space's favorites, pinned tabs and today's,
/// and the Spaces to switch to.
struct Sidebar {
    window: u32,
    root: gtk4::Box,
    back: gtk4::Button,
    forward: gtk4::Button,
    address: gtk4::Button,
    title: gtk4::Label,
    favorites: gtk4::FlowBox,
    pinned: gtk4::Box,
    today: gtk4::Box,
    spaces: gtk4::Box,
    /// The Space's color, which tints the sidebar.
    tint: gtk4::CssProvider,
}

impl Sidebar {
    fn new(window: u32) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        root.add_css_class("arbalete-sidebar");

        let navigation = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        let buttons = [
            ("sidebar-show-symbolic", "toggle-sidebar", "Toggle Sidebar"),
            ("go-previous-symbolic", "back", "Back"),
            ("go-next-symbolic", "forward", "Forward"),
            ("view-refresh-symbolic", "reload", "Reload"),
        ]
        .map(|(icon, name, tooltip)| {
            let button = icon_button(icon, tooltip);
            button.connect_clicked(move |_| run(window, json!({ "name": name })));
            navigation.append(&button);
            button
        });
        let [_, back, forward, _] = buttons;
        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        navigation.append(&spacer);
        navigation.append(&app_menu(window));
        root.append(&navigation);

        let address = gtk4::Button::with_label("Search or Enter URL…");
        address.add_css_class("flat");
        address.add_css_class("address");
        if let Some(label) = address.child().and_downcast::<gtk4::Label>() {
            ellipsize(&label);
            label.set_xalign(0.0);
        }
        address.connect_clicked(move |_| run(window, json!({ "name": "edit-url" })));
        root.append(&address);

        let favorites = gtk4::FlowBox::new();
        favorites.set_selection_mode(gtk4::SelectionMode::None);
        favorites.set_max_children_per_line(4);
        favorites.set_min_children_per_line(4);
        favorites.set_homogeneous(true);
        root.append(&favorites);

        let title = gtk4::Label::new(None);
        title.add_css_class("space-title");
        title.set_xalign(0.0);
        root.append(&title);

        let pinned = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let today = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let new_tab = row_button("list-add-symbolic", "New Tab");
        new_tab.connect_clicked(move |_| run(window, json!({ "name": "show-overlay" })));
        let clear = gtk4::Button::with_label("Clear");
        clear.add_css_class("flat");
        clear.add_css_class("dim-label");
        clear.set_halign(gtk4::Align::End);
        clear.connect_clicked(move |_| run(window, json!({ "name": "clear-today" })));
        let separator = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let line = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        line.set_hexpand(true);
        line.set_valign(gtk4::Align::Center);
        separator.append(&line);
        separator.append(&clear);
        let lists = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        lists.append(&pinned);
        lists.append(&separator);
        lists.append(&new_tab);
        lists.append(&today);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_vexpand(true);
        scroll.set_child(Some(&lists));
        root.append(&scroll);

        let spaces = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        spaces.set_halign(gtk4::Align::Center);
        root.append(&spaces);

        root.set_widget_name(&format!("sidebar-{window}"));
        let tint = gtk4::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &tint,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        // Like the macOS one, it asks for the state once it listens.
        later(move || run(window, json!({ "name": "ready" })));
        Self {
            window,
            root,
            back,
            forward,
            address,
            title,
            favorites,
            pinned,
            today,
            spaces,
            tint,
        }
    }

    /// Shows the state the app sends, and returns where the shown page stands in its history.
    fn event(&self, name: &str, detail: &str) -> Option<PageState> {
        if name != "arbalete:state" {
            return None;
        }
        match serde_json::from_str::<State>(detail) {
            Ok(state) => {
                self.show(&state);
                Some(state.page)
            }
            Err(error) => {
                eprintln!("unreadable state: {error}");
                None
            }
        }
    }

    /// Shows the displayed Space's page of the state.
    fn show(&self, state: &State) {
        self.back.set_sensitive(state.page.can_go_back);
        self.forward.set_sensitive(state.page.can_go_forward);
        let Some(page) = state.pages.iter().find(|page| page.space == state.space) else {
            return;
        };
        let space = state.spaces.iter().find(|space| space.id == state.space);
        let label = space.map(|space| {
            let emoji = space.emoji.as_deref().unwrap_or_default();
            format!("{emoji} {}", space.title).trim().to_string()
        });
        self.title.set_label(label.as_deref().unwrap_or_default());
        let color = space.and_then(|space| space.theme.colors.first());
        self.tint
            .load_from_string(&color.map_or(String::new(), |color| {
                format!(
                    "#sidebar-{} {{ background: alpha({color}, 0.18); }}",
                    self.window
                )
            }));

        let tabs: Vec<Tab> = [&page.favorites, &page.pinned, &page.today]
            .into_iter()
            .flatten()
            .flat_map(Item::tabs)
            .collect();
        let selected = tabs.iter().find(|tab| Some(tab.id) == page.selected);
        let address = selected.map_or("Search or Enter URL…", |tab| tab.url.as_str());
        self.address.set_label(address);

        self.favorites.remove_all();
        for favorite in page.favorites.iter().flat_map(Item::tabs) {
            let button = gtk4::Button::new();
            button.add_css_class("flat");
            button.add_css_class("favorite");
            if Some(favorite.id) == page.selected {
                button.add_css_class("selected");
            }
            button.set_child(Some(&favicon(favorite.favicon.as_deref())));
            button.set_tooltip_text(Some(&favorite.title));
            self.on_tab(&button, favorite.id);
            self.favorites.insert(&button, -1);
        }
        self.favorites.set_visible(!page.favorites.is_empty());

        for (list, items) in [(&self.pinned, &page.pinned), (&self.today, &page.today)] {
            clear(list);
            for item in items {
                self.append_item(list, item, page.selected, 0);
            }
        }

        clear(&self.spaces);
        for space in &state.spaces {
            let mark = space
                .emoji
                .clone()
                .filter(|emoji| !emoji.is_empty())
                .unwrap_or_else(|| space.title.chars().take(1).collect());
            let button = gtk4::Button::with_label(&mark);
            button.add_css_class("flat");
            button.add_css_class("circular");
            if space.id != state.space {
                button.add_css_class("dim-label");
            }
            button.set_tooltip_text(Some(&space.title));
            let (window, id) = (self.window, space.id);
            button
                .connect_clicked(move |_| run(window, json!({ "name": "show-space", "args": id })));
            self.spaces.append(&button);
        }
    }

    /// Adds a row for a tab, a folder and its content when expanded, or a split.
    fn append_item(&self, list: &gtk4::Box, item: &Item, selected: Option<i64>, depth: i32) {
        match item {
            Item::Tab(tab) => list.append(&self.tab_row(tab, selected, depth)),
            Item::Split { split } => {
                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
                for tab in split {
                    let tab_row = self.tab_row(tab, selected, depth);
                    tab_row.set_hexpand(true);
                    row.append(&tab_row);
                }
                list.append(&row);
            }
            Item::Folder {
                id,
                title,
                expanded,
                children,
            } => {
                let icon = if *expanded {
                    "folder-open-symbolic"
                } else {
                    "folder-symbolic"
                };
                let button = row_button(icon, title);
                button.set_margin_start(depth * 12);
                let (window, id) = (self.window, *id);
                button.connect_clicked(move |_| {
                    run(window, json!({ "name": "toggle-folder", "args": id }))
                });
                self.on_menu(&button, id);
                list.append(&button);
                if *expanded {
                    for child in children {
                        self.append_item(list, child, selected, depth + 1);
                    }
                }
            }
        }
    }

    /// A tab's row: its icon and title, a click selecting it, and a button closing it.
    fn tab_row(&self, tab: &Tab, selected: Option<i64>, depth: i32) -> gtk4::Box {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        row.add_css_class("row");
        if Some(tab.id) == selected {
            row.add_css_class("selected");
        }
        row.set_margin_start(depth * 12);
        let button = gtk4::Button::new();
        button.add_css_class("flat");
        button.set_hexpand(true);
        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        content.append(&favicon(tab.favicon.as_deref()));
        let title = gtk4::Label::new(Some(if tab.title.is_empty() {
            &tab.url
        } else {
            &tab.title
        }));
        ellipsize(&title);
        title.set_xalign(0.0);
        title.set_hexpand(true);
        content.append(&title);
        button.set_child(Some(&content));
        button.set_tooltip_text(Some(&tab.url));
        self.on_tab(&button, tab.id);
        let close = icon_button("window-close-symbolic", "Close Tab");
        let (window, id) = (self.window, tab.id);
        close.connect_clicked(move |_| run(window, json!({ "name": "close", "args": id })));
        row.append(&button);
        row.append(&close);
        row
    }

    /// A click selects the tab, a middle click closes it, a right click shows its menu.
    fn on_tab(&self, button: &gtk4::Button, id: i64) {
        let window = self.window;
        button.connect_clicked(move |_| run(window, json!({ "name": "select", "args": id })));
        let middle = gtk4::GestureClick::builder().button(2).build();
        middle.connect_released(move |_, _, _, _| {
            run(window, json!({ "name": "close", "args": id }))
        });
        button.add_controller(middle);
        self.on_menu(button, id);
    }

    fn on_menu(&self, widget: &impl IsA<gtk4::Widget>, id: i64) {
        let window = self.window;
        let right = gtk4::GestureClick::builder().button(3).build();
        right.connect_pressed(move |_, _, _, _| {
            run(window, json!({ "name": "menu", "args": { "node": id } }))
        });
        widget.add_controller(right);
    }
}

impl Item {
    fn tabs(&self) -> Vec<Tab> {
        match self {
            Item::Tab(tab) => vec![tab.clone()],
            Item::Split { split } => split.clone(),
            Item::Folder { children, .. } => children.iter().flat_map(Item::tabs).collect(),
        }
    }
}

/// What an item of the app's menu does.
type MenuAction = Box<dyn Fn()>;

/// What macOS's app menu offers, in a button of the sidebar: Linux has no menu bar.
fn app_menu(window: u32) -> gtk4::MenuButton {
    let items = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let popover = gtk4::Popover::new();
    let entries: [(&str, MenuAction); 5] = [
        (
            "New Window",
            Box::new(move || run(window, json!({ "name": "new-window" }))),
        ),
        (
            "Settings",
            Box::new(move || run(window, json!({ "name": "show-settings" }))),
        ),
        ("Set as Default Browser", Box::new(make_default_browser)),
        ("About Arbalete", Box::new(about)),
        ("Quit Arbalete", Box::new(crate::shell::quit)),
    ];
    let mut make_default = None;
    for (title, action) in entries {
        let button = gtk4::Button::with_label(title);
        if title == "Set as Default Browser" {
            make_default = Some(button.clone());
        }
        button.add_css_class("flat");
        if let Some(label) = button.child().and_downcast::<gtk4::Label>() {
            label.set_xalign(0.0);
        }
        let popover = popover.clone();
        button.connect_clicked(move |_| {
            popover.popdown();
            action();
        });
        items.append(&button);
    }
    popover.set_child(Some(&items));
    popover.connect_show(move |_| {
        if let Some(button) = &make_default {
            button.set_sensitive(!is_default_browser());
        }
    });
    let menu = gtk4::MenuButton::new();
    menu.set_icon_name("open-menu-symbolic");
    menu.add_css_class("flat");
    menu.set_tooltip_text(Some("Menu"));
    menu.set_popover(Some(&popover));
    menu
}

/// Cuts a label's text short with an ellipsis where its row is too narrow: asking for no width of
/// its own, as the sidebar's rows take what the app gives the sidebar.
fn ellipsize(label: &gtk4::Label) {
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_max_width_chars(1);
    label.set_hexpand(true);
}

fn clear(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn icon_button(icon: &str, tooltip: &str) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon);
    button.add_css_class("flat");
    button.set_tooltip_text(Some(tooltip));
    button
}

fn row_button(icon: &str, title: &str) -> gtk4::Button {
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    content.append(&gtk4::Image::from_icon_name(icon));
    let label = gtk4::Label::new(Some(title));
    ellipsize(&label);
    label.set_xalign(0.0);
    content.append(&label);
    let button = gtk4::Button::new();
    button.add_css_class("flat");
    button.add_css_class("row");
    button.set_child(Some(&content));
    button
}

/// A tab's icon: the profile's cached one the app serves (shell::ui_file_path), else a page's.
fn favicon(url: Option<&str>) -> gtk4::Image {
    let file = url.and_then(crate::shell::ui_file_path);
    let image = match file {
        Some(file) => gtk4::Image::from_file(file),
        None => gtk4::Image::from_icon_name("web-browser-symbolic"),
    };
    image.set_pixel_size(16);
    image
}

/// The Command Bar, in a popover over the card: what is typed opens as an address or a search,
/// and the app's results for it (open tabs, history, suggestions) list under it.
struct CommandBar {
    window: u32,
    popover: gtk4::Popover,
    entry: gtk4::Entry,
    results: gtk4::ListBox,
    /// What each result row opens.
    rows: RefCell<Vec<serde_json::Value>>,
    /// The bar edits the selected tab's address rather than opening a new tab.
    editing: Cell<bool>,
    search: RefCell<String>,
    /// The app hides it: closing must not tell the app again.
    hiding: Cell<bool>,
}

/// The app's results for what is typed (search_overlay in app/src/shell/overlay.rs).
#[derive(Deserialize)]
struct Results {
    text: String,
    tabs: Vec<Tab>,
    pages: Vec<Page>,
}

#[derive(Deserialize)]
struct Page {
    url: String,
    title: String,
}

impl CommandBar {
    fn new(window: u32, parent: &gtk4::Window) -> Rc<Self> {
        let entry = gtk4::Entry::new();
        entry.set_placeholder_text(Some("Search or Enter URL…"));
        entry.set_width_chars(48);
        let results = gtk4::ListBox::new();
        results.add_css_class("arbalete-results");
        results.set_activate_on_single_click(true);
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        content.append(&entry);
        content.append(&results);
        let popover = gtk4::Popover::new();
        popover.set_child(Some(&content));
        popover.set_has_arrow(false);
        popover.set_position(gtk4::PositionType::Bottom);
        popover.set_parent(parent);
        let bar = Rc::new(Self {
            window,
            popover,
            entry,
            results,
            rows: RefCell::default(),
            editing: Cell::new(false),
            search: RefCell::new("https://www.google.com/search?q=".into()),
            hiding: Cell::new(false),
        });
        let weak = Rc::downgrade(&bar);
        bar.entry.connect_changed(move |entry| {
            if let Some(bar) = weak.upgrade() {
                bar.typed(&entry.text());
            }
        });
        let weak = Rc::downgrade(&bar);
        bar.entry.connect_activate(move |_| {
            if let Some(bar) = weak.upgrade() {
                let row = bar.results.selected_row().map_or(0, |row| row.index());
                bar.open(row as usize);
            }
        });
        let weak = Rc::downgrade(&bar);
        bar.results.connect_row_activated(move |_, row| {
            if let Some(bar) = weak.upgrade() {
                bar.open(row.index() as usize);
            }
        });
        let keys = gtk4::EventControllerKey::new();
        let weak = Rc::downgrade(&bar);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(bar) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let step = match key {
                gdk::Key::Down => 1,
                gdk::Key::Up => -1,
                _ => return glib::Propagation::Proceed,
            };
            bar.move_selection(step);
            glib::Propagation::Stop
        });
        bar.entry.add_controller(keys);
        let weak = Rc::downgrade(&bar);
        bar.popover.connect_closed(move |_| {
            if let Some(bar) = weak.upgrade()
                && !bar.hiding.replace(false)
            {
                run(bar.window, json!({ "name": "hide-overlay" }));
            }
        });
        bar
    }

    fn show(&self, shown: bool, frame: &Rect) {
        if !shown {
            if self.popover.is_visible() {
                self.hiding.set(true);
                self.popover.popdown();
            }
            return;
        }
        let top = gdk::Rectangle::new(frame.x, frame.y, frame.width, 1);
        self.popover.set_pointing_to(Some(&top));
        self.entry.set_width_chars((frame.width / 9).clamp(30, 80));
        // Placed again after every command: it takes the keyboard only as it shows, or typing
        // would replace the text it selects.
        if !self.popover.is_visible() {
            self.popover.popup();
            self.entry.grab_focus();
        }
    }

    fn event(&self, name: &str, detail: &str) {
        match name {
            "arbalete:show" => {
                #[derive(Deserialize)]
                struct Show {
                    url: Option<String>,
                    search: Option<Search>,
                }
                #[derive(Deserialize)]
                struct Search {
                    url: String,
                }
                let show = serde_json::from_str::<Option<Show>>(detail).ok().flatten();
                let url = show.as_ref().and_then(|show| show.url.clone());
                self.editing.set(url.is_some());
                if let Some(search) = show.and_then(|show| show.search) {
                    self.search.replace(search.url);
                }
                self.entry.set_text(url.as_deref().unwrap_or_default());
                self.entry.select_region(0, -1);
                self.list(Vec::new());
            }
            "arbalete:results" => {
                let Ok(results) = serde_json::from_str::<Results>(detail) else {
                    return;
                };
                if results.text != self.entry.text() {
                    return;
                }
                let mut rows = Vec::new();
                for tab in results.tabs {
                    rows.push(
                        json!({ "label": format!("Switch to {}", tab.title), "select": tab.id }),
                    );
                }
                for page in results.pages {
                    let label = if page.title.is_empty() {
                        &page.url
                    } else {
                        &page.title
                    };
                    rows.push(json!({ "label": label, "url": page.url }));
                }
                self.list(rows);
            }
            _ => {}
        }
    }

    /// Asks the app for what matches the text, and offers to open it first.
    fn typed(&self, text: &str) {
        run(
            self.window,
            json!({ "name": "search-overlay", "args": text }),
        );
        self.list(Vec::new());
    }

    /// Lists the row opening what is typed, then `rows`.
    fn list(&self, rows: Vec<serde_json::Value>) {
        let typed = self.entry.text().trim().to_string();
        let mut all = Vec::new();
        if !typed.is_empty() {
            let go = match arbalete_core::search::address(&typed) {
                Some(url) => json!({ "label": url, "url": url }),
                None => json!({
                    "label": format!("Search for “{typed}”"),
                    "url": format!("{}{}", self.search.borrow(), arbalete_core::search::encode(&typed)),
                }),
            };
            all.push(go);
        }
        all.extend(rows);
        self.results.remove_all();
        for row in &all {
            let label = gtk4::Label::new(row["label"].as_str());
            label.set_xalign(0.0);
            ellipsize(&label);
            self.results.append(&label);
        }
        if let Some(first) = self.results.row_at_index(0) {
            self.results.select_row(Some(&first));
        }
        self.results.set_visible(!all.is_empty());
        self.rows.replace(all);
    }

    fn move_selection(&self, step: i32) {
        let count = self.rows.borrow().len() as i32;
        if count == 0 {
            return;
        }
        let current = self.results.selected_row().map_or(0, |row| row.index());
        let next = (current + step).rem_euclid(count);
        self.results
            .select_row(self.results.row_at_index(next).as_ref());
    }

    fn open(&self, index: usize) {
        let Some(row) = self.rows.borrow().get(index).cloned() else {
            return;
        };
        if let Some(tab) = row.get("select") {
            run(self.window, json!({ "name": "select", "args": tab }));
            run(self.window, json!({ "name": "hide-overlay" }));
        } else if let Some(url) = row["url"].as_str() {
            let name = if self.editing.get() {
                "navigate"
            } else {
                "open"
            };
            run(self.window, json!({ "name": name, "args": url }));
        }
    }
}

/// What the interface asks of X11, through the connection GTK opened: the pages' containers.
mod x11 {
    use super::View;
    use std::ffi::{c_int, c_long, c_uint, c_ulong, c_void};

    type Display = c_void;

    #[repr(C)]
    struct SetWindowAttributes {
        background_pixmap: c_ulong,
        background_pixel: c_ulong,
        border_pixmap: c_ulong,
        border_pixel: c_ulong,
        bit_gravity: c_int,
        win_gravity: c_int,
        backing_store: c_int,
        backing_planes: c_ulong,
        backing_pixel: c_ulong,
        save_under: c_int,
        event_mask: c_long,
        do_not_propagate_mask: c_long,
        override_redirect: c_int,
        colormap: c_ulong,
        cursor: c_ulong,
    }

    const INPUT_OUTPUT: c_uint = 1;
    const CW_BACK_PIXEL: c_ulong = 1 << 1;
    const CW_BORDER_PIXEL: c_ulong = 1 << 3;
    const CW_COLORMAP: c_ulong = 1 << 13;

    #[link(name = "X11")]
    unsafe extern "C" {
        fn XDefaultScreen(display: *mut Display) -> c_int;
        fn XDefaultDepth(display: *mut Display, screen: c_int) -> c_int;
        fn XDefaultVisual(display: *mut Display, screen: c_int) -> *mut c_void;
        fn XDefaultColormap(display: *mut Display, screen: c_int) -> c_ulong;
        fn XDefaultRootWindow(display: *mut Display) -> c_ulong;
        #[allow(clippy::too_many_arguments)]
        fn XCreateWindow(
            display: *mut Display,
            parent: c_ulong,
            x: c_int,
            y: c_int,
            width: c_uint,
            height: c_uint,
            border_width: c_uint,
            depth: c_int,
            class: c_uint,
            visual: *mut c_void,
            mask: c_ulong,
            attributes: *mut SetWindowAttributes,
        ) -> c_ulong;
        fn XDestroyWindow(display: *mut Display, window: c_ulong) -> c_int;
        fn XMapWindow(display: *mut Display, window: c_ulong) -> c_int;
        fn XUnmapWindow(display: *mut Display, window: c_ulong) -> c_int;
        fn XMoveResizeWindow(
            display: *mut Display,
            window: c_ulong,
            x: c_int,
            y: c_int,
            width: c_uint,
            height: c_uint,
        ) -> c_int;
        #[allow(clippy::too_many_arguments)]
        fn XQueryTree(
            display: *mut Display,
            window: c_ulong,
            root: *mut c_ulong,
            parent: *mut c_ulong,
            children: *mut *mut c_ulong,
            count: *mut c_uint,
        ) -> c_int;
        #[allow(clippy::too_many_arguments)]
        fn XTranslateCoordinates(
            display: *mut Display,
            from: c_ulong,
            to: c_ulong,
            x: c_int,
            y: c_int,
            to_x: *mut c_int,
            to_y: *mut c_int,
            child: *mut c_ulong,
        ) -> c_int;
        fn XFree(data: *mut c_void) -> c_int;
        fn XFlush(display: *mut Display) -> c_int;
    }

    /// GTK's connection to the X server.
    fn display() -> Option<*mut Display> {
        use gtk4::prelude::*;
        let display = gtk4::gdk::Display::default()?;
        let display = display.downcast::<gdk4_x11::X11Display>().ok()?;
        // SAFETY: GTK's display, open as long as the app runs.
        let x = unsafe { gdk4_x11::ffi::gdk_x11_display_get_xdisplay(display.as_ptr()) };
        (!x.is_null()).then_some(x.cast())
    }

    /// A hidden window of the screen's own visual in `parent`: GTK gives its windows a 32-bit
    /// one, for their transparent corners, in which Chromium cannot make its own.
    pub fn container(parent: View) -> Option<View> {
        let display = display()?;
        // SAFETY: GTK's open display and its window; the attributes outlive the call.
        let view = unsafe {
            let screen = XDefaultScreen(display);
            let mut attributes: SetWindowAttributes = std::mem::zeroed();
            attributes.colormap = XDefaultColormap(display, screen);
            XCreateWindow(
                display,
                parent,
                0,
                0,
                1,
                1,
                0,
                XDefaultDepth(display, screen),
                INPUT_OUTPUT,
                XDefaultVisual(display, screen),
                CW_BACK_PIXEL | CW_BORDER_PIXEL | CW_COLORMAP,
                &mut attributes,
            )
        };
        (view != 0).then_some(view)
    }

    /// Shows a container at a place in its window, its page filling it.
    pub fn show(view: View, x: i32, y: i32, width: i32, height: i32) {
        let Some(display) = display() else { return };
        let (width, height) = (width.max(1) as c_uint, height.max(1) as c_uint);
        // SAFETY: GTK's open display, and windows of the app's or CEF's in it.
        unsafe {
            XMoveResizeWindow(display, view, x, y, width, height);
            for child in children(display, view) {
                XMoveResizeWindow(display, child, 0, 0, width, height);
            }
            XMapWindow(display, view);
            XFlush(display);
        }
    }

    pub fn hide(view: View) {
        let Some(display) = display() else { return };
        // SAFETY: GTK's open display and a window of the app's.
        unsafe {
            XUnmapWindow(display, view);
            XFlush(display);
        }
    }

    pub fn destroy(view: View) {
        let Some(display) = display() else { return };
        // SAFETY: GTK's open display and a window of the app's, which goes with its children.
        unsafe {
            XDestroyWindow(display, view);
            XFlush(display);
        }
    }

    /// The window a window is in, but the screen.
    pub fn parent(view: View) -> Option<View> {
        let display = display()?;
        let (mut root, mut parent, mut children, mut count) = (0, 0, std::ptr::null_mut(), 0);
        // SAFETY: GTK's open display; X allocates the children, freed here.
        let found = unsafe {
            let found = XQueryTree(
                display,
                view,
                &mut root,
                &mut parent,
                &mut children,
                &mut count,
            );
            if !children.is_null() {
                XFree(children.cast());
            }
            found
        };
        (found != 0 && parent != 0 && parent != root).then_some(parent)
    }

    fn children(display: *mut Display, view: View) -> Vec<View> {
        let (mut root, mut parent, mut children, mut count) = (0, 0, std::ptr::null_mut(), 0);
        // SAFETY: GTK's open display; X allocates the children, copied then freed here.
        unsafe {
            if XQueryTree(
                display,
                view,
                &mut root,
                &mut parent,
                &mut children,
                &mut count,
            ) == 0
                || children.is_null()
            {
                return Vec::new();
            }
            let list = std::slice::from_raw_parts(children, count as usize).to_vec();
            XFree(children.cast());
            list
        }
    }

    /// Where a window is on the screen, from its top left corner.
    pub fn position(view: View) -> (i32, i32) {
        let Some(display) = display() else {
            return (0, 0);
        };
        let (mut x, mut y, mut child) = (0, 0, 0);
        // SAFETY: GTK's open display and the app's window.
        unsafe {
            let root = XDefaultRootWindow(display);
            XTranslateCoordinates(display, view, root, 0, 0, &mut x, &mut y, &mut child);
        }
        (x, y)
    }
}

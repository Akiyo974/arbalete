//! What the interface needs of Linux, as `mac.rs` gives it on macOS: the shortcuts, menus,
//! dialogs, the clipboard, opening files and what the desktop says of power and memory, through
//! GTK and GIO ([decision 0011](../../docs/decisions/0011-linux-interface.md)).

use crate::command::{Command, Zoom};
use crate::menu::{Action, Entry};
use crate::native::View;
use crate::settings::{Keys, Priority, Settings};
use cef::{KeyEvent, KeyEventType};
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// Modifiers as the settings keep them, AppKit's mask on every OS: ⌘ stands for Ctrl on Linux,
/// ⌥ for Alt.
const SHIFT: u64 = 1 << 17;
const CONTROL: u64 = 1 << 18;
const ALT: u64 = 1 << 19;
const CTRL: u64 = 1 << 20;

/// A changeable shortcut: its id, the same as on macOS so the settings carry over, what it does,
/// and its default keys.
struct Shortcut {
    id: String,
    title: String,
    key: &'static str,
    modifiers: u64,
    command: fn(usize) -> Command,
    /// Its number, for the shortcuts that go to the n-th sidebar item or Space.
    number: usize,
}

fn shortcut(
    id: &str,
    title: &str,
    key: &'static str,
    modifiers: u64,
    command: fn(usize) -> Command,
) -> Shortcut {
    Shortcut {
        id: id.to_string(),
        title: title.to_string(),
        key,
        modifiers,
        command,
        number: 0,
    }
}

/// The shortcuts, like Chrome's on Linux where macOS uses ⌘.
fn shortcuts() -> Vec<Shortcut> {
    let mut all = vec![
        shortcut("newTab", "New Tab", "t", CTRL, |_| Command::ShowOverlay),
        shortcut("newWindow", "New Window", "n", CTRL, |_| Command::NewWindow),
        shortcut(
            "newIncognito",
            "New Incognito Window",
            "n",
            CTRL | SHIFT,
            |_| Command::NewIncognitoWindow,
        ),
        shortcut("openLocation", "Open Location…", "l", CTRL, |_| {
            Command::EditUrl
        }),
        shortcut("closeTab", "Close Tab", "w", CTRL, |_| Command::Close(None)),
        shortcut("reopenTab", "Reopen Closed Tab", "t", CTRL | SHIFT, |_| {
            Command::Reopen(0)
        }),
        shortcut("togglePin", "Pin Tab", "d", CTRL, |_| Command::Pin(None)),
        shortcut("reload", "Reload Page", "r", CTRL, |_| Command::Reload),
        shortcut(
            "forceReload",
            "Force Reload Page",
            "r",
            CTRL | SHIFT,
            |_| Command::ForceReload,
        ),
        shortcut("back", "Back", "left", ALT, |_| Command::Back),
        shortcut("forward", "Forward", "right", ALT, |_| Command::Forward),
        shortcut("find", "Find…", "f", CTRL, |_| Command::ShowFind),
        shortcut("zoomIn", "Zoom In", "=", CTRL, |_| Command::Zoom(Zoom::In)),
        shortcut("zoomOut", "Zoom Out", "-", CTRL, |_| {
            Command::Zoom(Zoom::Out)
        }),
        shortcut("actualSize", "Actual Size", "0", CTRL, |_| {
            Command::Zoom(Zoom::Reset)
        }),
        shortcut("devTools", "Developer Tools", "i", CTRL | SHIFT, |_| {
            Command::DevTools(None)
        }),
        shortcut("viewSource", "View Source", "u", CTRL, |_| {
            Command::ViewSource
        }),
        shortcut("print", "Print…", "p", CTRL, |_| Command::Print),
        shortcut("history", "History", "y", CTRL, |_| Command::ShowHistory),
        shortcut("downloads", "Downloads", "j", CTRL | SHIFT, |_| {
            Command::ShowDownloads
        }),
        shortcut("settings", "Settings…", ",", CTRL, |_| {
            Command::ShowSettings(None)
        }),
        shortcut("toggleSidebar", "Toggle Sidebar", "s", CTRL, |_| {
            Command::ToggleSidebar
        }),
        shortcut("toggleToolbar", "Toggle Toolbar", "s", CTRL | SHIFT, |_| {
            Command::ToggleToolbar
        }),
        shortcut("copyUrl", "Copy URL", "c", CTRL | SHIFT, |_| {
            Command::CopyLink
        }),
        shortcut("nextTab", "Next Tab", "tab", CONTROL, |_| Command::NextTab),
        shortcut(
            "previousTab",
            "Previous Tab",
            "tab",
            CONTROL | SHIFT,
            |_| Command::PreviousTab,
        ),
        shortcut("nextSpace", "Next Space", "right", CTRL | ALT, |_| {
            Command::NextSpace
        }),
        shortcut(
            "previousSpace",
            "Previous Space",
            "left",
            CTRL | ALT,
            |_| Command::PreviousSpace,
        ),
        shortcut("captureArea", "Capture Area", "2", CTRL | SHIFT, |_| {
            Command::CaptureArea
        }),
        shortcut(
            "newLittle",
            "New Little Arbalete Window",
            "n",
            CTRL | ALT,
            |_| Command::NewLittle(None),
        ),
    ];
    for number in 1..=9 {
        all.push(Shortcut {
            number,
            ..shortcut(
                &format!("sidebarItem-{number}"),
                &format!("Sidebar Item {number}"),
                ["1", "2", "3", "4", "5", "6", "7", "8", "9"][number - 1],
                CTRL,
                Command::SidebarItem,
            )
        });
        all.push(Shortcut {
            number,
            ..shortcut(
                &format!("space-{number}"),
                &format!("Space {number}"),
                ["1", "2", "3", "4", "5", "6", "7", "8", "9"][number - 1],
                CONTROL,
                Command::SpaceNumber,
            )
        });
    }
    all
}

thread_local! {
    /// The keys the settings give the shortcuts that changed, by id (apply_shortcuts).
    static OVERRIDES: RefCell<BTreeMap<String, Keys>> = const { RefCell::new(BTreeMap::new()) };
    /// The shortcut pressed last that a page took first, and until when, in microseconds of the
    /// monotonic clock: pressed again before, a shortcut whose priority is both goes to Arbalete.
    static LAST_SHORTCUT: RefCell<Option<(String, i64)>> = const { RefCell::new(None) };
}

/// Each changeable shortcut's default keys, by id.
pub fn default_shortcuts() -> BTreeMap<String, Keys> {
    shortcuts()
        .into_iter()
        .map(|shortcut| {
            let keys = Keys {
                key: shortcut.key.to_string(),
                modifiers: shortcut.modifiers,
            };
            (shortcut.id, keys)
        })
        .collect()
}

/// Takes the keys the settings give the shortcuts.
pub fn apply_shortcuts(settings: &Settings) {
    let overrides = settings
        .shortcuts
        .iter()
        .filter_map(|(id, change)| Some((id.clone(), change.keys.clone()?)))
        .collect();
    OVERRIDES.set(overrides);
}

/// The shortcut pressed, by its key's name and the settings' modifiers, and its title.
fn find(key: &str, modifiers: u64) -> Option<(Shortcut, String)> {
    OVERRIDES.with_borrow(|overrides| {
        shortcuts().into_iter().find_map(|shortcut| {
            let keys = overrides.get(&shortcut.id);
            let (wanted, mask) = keys.map_or((shortcut.key, shortcut.modifiers), |keys| {
                (keys.key.as_str(), keys.modifiers)
            });
            let title = shortcut.title.clone();
            (!wanted.is_empty() && wanted == key && mask == modifiers).then_some((shortcut, title))
        })
    })
}

/// The key event CEF gives as X11's.
pub type OsEvent<'a> = Option<&'a mut cef::sys::XEvent>;

/// Runs the shortcut a key pressed in a page is, before the page has it, if its priority says
/// Arbalete takes it first; whether it did. CEF's own event tells the key on every OS.
pub fn take_shortcut(
    event: &KeyEvent,
    _os_event: OsEvent,
    priority: impl Fn(&str) -> Priority,
) -> bool {
    if event.type_ != KeyEventType::RAWKEYDOWN {
        return false;
    }
    let Some(key) = key_of_code(event.windows_key_code) else {
        return false;
    };
    let Some((shortcut, title)) = find(&key, cef_modifiers(event.modifiers)) else {
        return false;
    };
    let take = match priority(&shortcut.id) {
        Priority::App => true,
        Priority::Websites => false,
        // Like Arc's: the page's on a first press, Arbalete's on a second within half a second.
        Priority::Both => {
            let again = LAST_SHORTCUT.with_borrow_mut(|last| {
                let now = glib::monotonic_time();
                let again = last
                    .as_ref()
                    .is_some_and(|(pressed, until)| *pressed == shortcut.id && now < *until);
                *last = (!again).then(|| (shortcut.id.clone(), now + 500_000));
                again
            });
            crate::shell::page_offered_shortcut((!again).then_some(title));
            again
        }
    };
    if take {
        crate::shell::run((shortcut.command)(shortcut.number));
    }
    take
}

/// Runs the shortcut a key pressed in the interface is; whether there was one.
pub fn run_shortcut(key: gdk::Key, state: gdk::ModifierType) -> bool {
    let Some(name) = key_of_keyval(key) else {
        return false;
    };
    let Some((shortcut, _)) = find(&name, gdk_modifiers(state)) else {
        return false;
    };
    crate::shell::run((shortcut.command)(shortcut.number));
    true
}

/// Gives the second press of the shortcut a page took first as long as a toast says to press
/// again, `seconds` more.
pub fn wait_second_press(seconds: f64) {
    LAST_SHORTCUT.with_borrow_mut(|last| {
        if let Some((_, until)) = last {
            *until += (seconds * 1_000_000.0) as i64;
        }
    });
}

/// A key's name in the shortcuts from its Windows key code, which CEF gives on every OS.
fn key_of_code(code: i32) -> Option<String> {
    let name = match code {
        0x30..=0x39 | 0x41..=0x5A => char::from(code as u8).to_ascii_lowercase().to_string(),
        0x09 => "tab".into(),
        0x25 => "left".into(),
        0x27 => "right".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0x70..=0x7B => format!("f{}", code - 0x6F),
        _ => return None,
    };
    Some(name)
}

/// A key's name in the shortcuts from GDK's.
fn key_of_keyval(key: gdk::Key) -> Option<String> {
    let name = match key {
        gdk::Key::Tab | gdk::Key::ISO_Left_Tab => "tab".into(),
        gdk::Key::Left => "left".into(),
        gdk::Key::Right => "right".into(),
        gdk::Key::plus => "=".into(),
        _ => match key.to_lower().to_unicode() {
            Some(character) if character.is_ascii_graphic() => {
                character.to_ascii_lowercase().to_string()
            }
            _ => key.name()?.to_lowercase(),
        },
    };
    Some(name)
}

/// The settings' modifiers of CEF's event flags.
fn cef_modifiers(flags: u32) -> u64 {
    // cef_event_flags_t: shift, control, alt, and the Super key as "command".
    [
        (1 << 1, SHIFT),
        (1 << 2, CTRL),
        (1 << 3, ALT),
        (1 << 7, CONTROL),
    ]
    .into_iter()
    .filter(|&(flag, _)| flags & flag != 0)
    .fold(0, |mask, (_, modifier)| mask | modifier)
}

/// The settings' modifiers of GDK's.
fn gdk_modifiers(state: gdk::ModifierType) -> u64 {
    [
        (gdk::ModifierType::SHIFT_MASK, SHIFT),
        (gdk::ModifierType::CONTROL_MASK, CTRL),
        (gdk::ModifierType::ALT_MASK, ALT),
        (gdk::ModifierType::SUPER_MASK, CONTROL),
    ]
    .into_iter()
    .filter(|&(flag, _)| state.contains(flag))
    .fold(0, |mask, (_, modifier)| mask | modifier)
}

/// GTK runs on X11, XWayland on a Wayland desktop: CEF's pages need an X11 parent.
pub fn setup_application() {
    // SAFETY: the app is still single-threaded.
    unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    gtk4::init().expect("GTK starts on X11");
}

/// No menu bar on Linux, like Arc's window: the shortcuts are taken from key events.
pub fn setup_menu() {}

/// The macOS interface watches clicks and swipes its window does not see: GTK gets them itself.
pub fn watch_events() {}

/// The menu's items name the sidebar's state on macOS; Linux has no menu bar.
pub fn sidebar_shown(_window: View, _shown: bool) {}

/// The menu's items name the toolbar's state on macOS; Linux has no menu bar.
pub fn toolbar_shown(_shown: bool) {}

/// The system's language as Chromium names its locales, from the POSIX locale, like "fr_FR.UTF-8".
pub fn chromium_locale() -> String {
    let posix = ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
        .unwrap_or_default();
    let tag = posix
        .split([':', '.', '@'])
        .next()
        .unwrap_or_default()
        .replace('_', "-");
    if tag.is_empty() {
        return "en-US".to_string();
    }
    crate::strings::chromium_locale_of(&tag)
}

/// Chromium keeps its keys in the desktop's keyring, which a development build shares: no mock.
pub fn mock_keychain() -> bool {
    false
}

/// A release build keeps its data in "arbalete", a development one in "arbalete-dev".
pub fn release_bundle() -> bool {
    !cfg!(debug_assertions)
}

/// Whether power saving is on, as the desktop tells it.
pub fn low_power() -> bool {
    gio::PowerProfileMonitor::get_default().is_power_saver_enabled()
}

/// Tells the app each time power saving is turned on or off.
pub fn watch_low_power() {
    let monitor = gio::PowerProfileMonitor::get_default();
    monitor.connect_power_saver_enabled_notify(|monitor| {
        crate::shell::low_power_changed(monitor.is_power_saver_enabled());
    });
    // Kept as long as the app runs: GIO drops the signal with the monitor.
    std::mem::forget(monitor);
}

/// How hard memory is pressed, from the memory the kernel says is available.
pub fn memory_pressure() -> crate::shell::MemoryPressure {
    use crate::shell::MemoryPressure;
    let Ok(info) = std::fs::read_to_string("/proc/meminfo") else {
        return MemoryPressure::Normal;
    };
    let field = |name: &str| -> Option<f64> {
        let line = info.lines().find(|line| line.starts_with(name))?;
        line.split_whitespace().nth(1)?.parse().ok()
    };
    let (Some(total), Some(available)) = (field("MemTotal:"), field("MemAvailable:")) else {
        return MemoryPressure::Normal;
    };
    match available / total {
        share if share < 0.05 => MemoryPressure::Critical,
        share if share < 0.10 => MemoryPressure::Warning,
        _ => MemoryPressure::Normal,
    }
}

/// The user asked the desktop for no animations.
pub fn reduce_motion() -> bool {
    gtk4::Settings::default().is_some_and(|settings| !settings.is_gtk_enable_animations())
}

/// Brings the app's window that had the keyboard in front.
pub fn activate() {
    if let Some(window) = crate::native::front_window() {
        window.present();
    }
}

/// Whether a mouse button is down, as while a click gives a page the focus.
pub fn mouse_down() -> bool {
    let buttons = gdk::ModifierType::BUTTON1_MASK
        | gdk::ModifierType::BUTTON2_MASK
        | gdk::ModifierType::BUTTON3_MASK;
    gdk::Display::default()
        .and_then(|display| display.default_seat())
        .and_then(|seat| seat.pointer())
        .is_some_and(|pointer| pointer.modifier_state().intersects(buttons))
}

/// How far down the front window the pointer is, in pixels, if over it.
pub fn pointer_y() -> Option<i32> {
    let window = crate::native::front_window()?;
    let (_, y) = pointer_in(&window)?;
    (0.0..=f64::from(window.height()))
        .contains(&y)
        .then_some(y as i32)
}

/// Where the pointer is in a window, from its top left corner.
fn pointer_in(window: &gtk4::Window) -> Option<(f64, f64)> {
    let pointer = WidgetExt::display(window).default_seat()?.pointer()?;
    let (x, y, _) = window.surface()?.device_position(&pointer)?;
    Some((x, y))
}

/// Keeps a window at least `width` by `height` pixels.
pub fn set_min_size(window: View, width: i32, height: i32) {
    if let Some(window) = crate::native::window_of(window) {
        window.set_size_request(width, height);
    }
}

/// Whether the window `window` is, or is in, shows in full screen.
pub fn full_screen(window: View) -> bool {
    crate::native::window_of(window).is_some_and(|window| window.is_fullscreen())
}

/// Makes the window `window` is, or is in, go full screen, or leave it.
pub fn toggle_full_screen(window: View) {
    if let Some(window) = crate::native::window_of(window) {
        window.set_fullscreened(!window.is_fullscreen());
    }
}

/// macOS hides a window while Chromium's folder picker shows as its sheet; on Linux the picker is a
/// window of its own.
pub fn hide_window(_window: View) {}

/// macOS covers a page shown in another window with its picture; on Linux its container hides
/// instead, the window's background showing.
pub fn cover_page(_view: View, _jpeg: &str, _tint: Option<u32>) {}

pub fn uncover_page(_view: View) {}

/// The card's corners stay square on Linux: the pages' X11 containers cannot be clipped.
pub fn round_card(_view: View, _rounded: bool) {}

pub fn round_split(_view: View) {}

/// Moves the card along a trackpad swipe between Spaces, which only macOS tells.
pub fn shift(_view: View, _x: f64) {}

/// Puts a link on the clipboard.
pub fn copy(url: &str) {
    if let Some(display) = gdk::Display::default() {
        display.clipboard().set_text(url);
    }
}

/// Puts the image at `url` on the clipboard, read away from the main thread by GIO.
pub fn copy_image(url: &str) {
    gio::File::for_uri(url).load_bytes_async(gio::Cancellable::NONE, |loaded| {
        let Ok((bytes, _)) = loaded else { return };
        if let (Ok(texture), Some(display)) =
            (gdk::Texture::from_bytes(&bytes), gdk::Display::default())
        {
            display.clipboard().set_texture(&texture);
        }
    });
}

/// Opens a file with its default app.
pub fn open_file(path: &std::path::Path) {
    let uri = gio::File::for_path(path).uri();
    let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
}

/// Shows a file selected in the file manager, or its folder when none answers.
pub fn show_file(path: &std::path::Path) {
    let uri = gio::File::for_path(path).uri();
    let shown = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).and_then(|bus| {
        bus.call_sync(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1",
            "ShowItems",
            Some(&(vec![uri.to_string()], "").to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            1000,
            gio::Cancellable::NONE,
        )
    });
    if shown.is_err()
        && let Some(folder) = path.parent()
    {
        open_file(folder);
    }
}

/// Shares a link by email: Linux desktops have no share menu.
pub fn share(url: &str) {
    let body = glib::Uri::escape_string(url, None, false);
    let _ = gio::AppInfo::launch_default_for_uri(
        &format!("mailto:?body={body}"),
        gio::AppLaunchContext::NONE,
    );
}

/// Shows a menu at the pointer until an item is picked, which runs, or the menu is dismissed.
pub fn pop_up(entries: Vec<Entry>) {
    let Some(window) = crate::native::front_window() else {
        return;
    };
    let actions = gio::SimpleActionGroup::new();
    let mut picked = Vec::new();
    let model = menu_model(entries, &actions, &mut picked);
    let picked = Rc::new(picked);
    for (index, _) in picked.iter().enumerate() {
        if let Some(action) = actions.lookup_action(&index.to_string())
            && let Ok(action) = action.downcast::<gio::SimpleAction>()
        {
            let picked = picked.clone();
            action.connect_activate(move |_, _| crate::shell::perform(picked[index].clone()));
        }
    }
    window.insert_action_group("menu", Some(&actions));
    let popover = gtk4::PopoverMenu::from_model(Some(&model));
    popover.set_has_arrow(false);
    popover.set_parent(&window);
    let (x, y) = pointer_in(&window).unwrap_or_default();
    popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    popover.set_position(gtk4::PositionType::Bottom);
    popover.connect_closed(|popover| {
        // Unparented once its item ran: the item's action runs after the popover closes.
        let popover = popover.clone();
        glib::idle_add_local_once(move || popover.unparent());
    });
    popover.popup();
}

/// A GIO menu of entries, each item an action of `actions` named by its index in `picked`.
fn menu_model(
    entries: Vec<Entry>,
    actions: &gio::SimpleActionGroup,
    picked: &mut Vec<Action>,
) -> gio::Menu {
    let menu = gio::Menu::new();
    let mut section = gio::Menu::new();
    for entry in entries {
        match entry {
            Entry::Item(item) => {
                let name = picked.len().to_string();
                let action = if item.checked {
                    gio::SimpleAction::new_stateful(&name, None, &true.to_variant())
                } else {
                    gio::SimpleAction::new(&name, None)
                };
                action.set_enabled(item.enabled);
                actions.add_action(&action);
                section.append(Some(&item.title), Some(&format!("menu.{name}")));
                picked.push(item.action);
            }
            Entry::Submenu(title, entries) => {
                section.append_submenu(Some(&title), &menu_model(entries, actions, picked));
            }
            Entry::Share(url) => {
                let name = picked.len().to_string();
                actions.add_action(&gio::SimpleAction::new(&name, None));
                section.append(Some("Share by Email"), Some(&format!("menu.{name}")));
                picked.push(Action::Share(url));
            }
            Entry::Separator => {
                menu.append_section(None, &section);
                section = gio::Menu::new();
            }
        }
    }
    menu.append_section(None, &section);
    menu
}

/// Asks the user to confirm an action named by `button`, which may destroy data.
pub fn confirm(question: &str, detail: &str, button: &str) -> bool {
    ask(question, detail, None, button).is_some()
}

/// Asks for a text in a dialog, its field holding `value`, selected: what was entered, unless the
/// dialog was cancelled.
pub fn prompt(question: &str, detail: &str, value: &str, button: &str) -> Option<String> {
    ask(question, detail, Some(value), button)
}

/// Shows a dialog over the front window until it is answered, with a text field holding `value`
/// if any: the field's text, or an empty one without a field, unless it was cancelled.
fn ask(question: &str, detail: &str, value: Option<&str>, button: &str) -> Option<String> {
    let dialog = gtk4::Window::builder()
        .modal(true)
        .resizable(false)
        .title(question)
        .default_width(360)
        .build();
    dialog.set_transient_for(crate::native::front_window().as_ref());
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    let title = gtk4::Label::new(Some(question));
    title.add_css_class("title-4");
    title.set_wrap(true);
    content.append(&title);
    if !detail.is_empty() {
        let detail = gtk4::Label::new(Some(detail));
        detail.set_wrap(true);
        content.append(&detail);
    }
    let field = value.map(|value| {
        let field = gtk4::Entry::new();
        field.set_text(value);
        content.append(&field);
        field
    });
    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    buttons.set_halign(gtk4::Align::End);
    let cancel = gtk4::Button::with_label("Cancel");
    let accept = gtk4::Button::with_label(button);
    accept.add_css_class(if value.is_some() {
        "suggested-action"
    } else {
        "destructive-action"
    });
    buttons.append(&cancel);
    buttons.append(&accept);
    content.append(&buttons);
    dialog.set_child(Some(&content));
    dialog.set_default_widget(Some(&accept));
    if let Some(field) = &field {
        field.set_activates_default(true);
    }

    let answer = Rc::new(RefCell::new(None));
    let main_loop = glib::MainLoop::new(None, false);
    accept.connect_clicked({
        let (answer, dialog, field) = (answer.clone(), dialog.clone(), field.clone());
        move |_| {
            let text = field.as_ref().map(|field| field.text().to_string());
            answer.replace(Some(text.unwrap_or_default()));
            dialog.close();
        }
    });
    cancel.connect_clicked({
        let dialog = dialog.clone();
        move |_| dialog.close()
    });
    dialog.connect_close_request({
        let main_loop = main_loop.clone();
        move |_| {
            main_loop.quit();
            glib::Propagation::Proceed
        }
    });
    dialog.present();
    if let Some(field) = &field {
        field.grab_focus();
        field.select_region(0, -1);
    }
    main_loop.run();
    answer.take()
}

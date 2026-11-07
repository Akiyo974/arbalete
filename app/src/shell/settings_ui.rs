//! The settings window, and the settings it and the site controls change.

use super::*;

/// Whether a page's site is in developer mode, like Arc's: as it was turned, or else for
/// `localhost` only, not for any other local host.
pub(super) fn developer_mode(url: &str) -> bool {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return false;
    }
    let site = site(url);
    SETTINGS
        .with_borrow(|settings| settings.developer_sites.get(site).copied())
        .unwrap_or_else(|| localhost(site))
}

/// Whether a site, a host and maybe a port, is the machine itself by name or address.
fn localhost(site: &str) -> bool {
    let host = match site.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(rest),
        None => site.split(':').next().unwrap_or(site),
    };
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host.ends_with(".localhost")
}

/// Turns a site's developer mode on or off, kept only where it differs from its default.
pub(super) fn set_developer_mode(site: String, on: bool) {
    SETTINGS.with_borrow_mut(|settings| {
        if on == localhost(&site) {
            settings.developer_sites.remove(&site);
        } else {
            settings.developer_sites.insert(site, on);
        }
    });
    save_settings();
    send_state();
    if let Some(url) = selected_url() {
        send_site_controls(&url);
    }
}

/// Shows the settings in a window of their own, like Arc's, brought to the front when it is
/// already open. Its page asks for the settings once it listens (Command::SettingsReady).
pub(super) fn show_settings() {
    #[cfg(target_os = "macos")]
    {
        crate::native::settings(true);
        send_settings();
    }
}

/// Closes the settings window, if it is open.
pub(super) fn close_settings() {
    #[cfg(target_os = "macos")]
    crate::native::settings(false);
}

/// The settings window has the keyboard: ⌘W closes it rather than a tab.
pub(super) fn settings_focused() -> bool {
    crate::native::settings_focused()
}

/// Sends the settings window the app's settings, the search engines and each profile's.
pub(super) fn send_settings() {
    let settings = app_settings();
    let engines: Vec<_> = SearchEngine::ALL
        .iter()
        .map(|engine| json!({ "id": engine, "name": engine.name() }))
        .collect();
    let profiles: Vec<_> = STATE.with_borrow(|state| {
        let profiles = state.sidebar.profiles().iter();
        profiles
            .map(|p| json!({ "id": p.id, "name": p.name, "color": p.color, "searchEngine": p.search_engine, "searchSuggestions": p.search_suggestions, "archiveAfter": p.archive_after, "downloadLocation": p.download_location }))
            .collect()
    });
    let delays: Vec<_> = ArchiveAfter::ALL
        .iter()
        .map(|after| json!({ "id": after, "name": after.name() }))
        .collect();
    // URL Routing's destinations: every Space, named with its profile.
    let (spaces, routing) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let spaces: Vec<_> = sidebar
            .spaces()
            .iter()
            .map(|space| {
                let profile = sidebar.profiles().iter().find(|p| p.id == space.profile);
                json!({ "id": space.id, "title": space.title, "emoji": space.emoji, "profile": profile.map(|p| &p.name) })
            })
            .collect();
        (spaces, json!(sidebar.url_routing()))
    });
    // Like Arc's, the setting says when another app has ⌥⌘N.
    let taken = settings.little_hotkey && !LITTLE_HOTKEY.get();
    #[cfg(target_os = "macos")]
    let defaults = crate::mac::default_shortcuts();
    let priorities: std::collections::BTreeMap<_, _> = defaults
        .keys()
        .map(|id| (id.clone(), settings::default_priority(id)))
        .collect();
    let detail = json!({ "settings": settings, "littleHotkeyTaken": taken, "shortcutDefaults": defaults, "shortcutPriorities": priorities, "engines": engines, "delays": delays, "profiles": profiles, "spaces": spaces, "urlRouting": routing });
    SETTINGS_VIEW.with(|view| dispatch(view, "arbalete:settings", &detail.to_string()));
}

/// Gives the menu's shortcuts the keys the settings changed.
pub fn apply_shortcuts() {
    #[cfg(target_os = "macos")]
    crate::mac::apply_shortcuts(&app_settings().shortcuts);
}

/// Changes a shortcut from the settings, saves it, and applies it to the menu.
pub(super) fn edit_shortcut(edit: ShortcutEdit) {
    let mut settings = app_settings();
    let shortcuts = &mut settings.shortcuts;
    match edit {
        ShortcutEdit::Set(id, key, modifiers) => {
            shortcuts.entry(id).or_default().keys = Some(settings::Keys { key, modifiers });
        }
        ShortcutEdit::Remove(id) => {
            let keys = settings::Keys {
                key: String::new(),
                modifiers: 0,
            };
            shortcuts.entry(id).or_default().keys = Some(keys);
        }
        ShortcutEdit::Reset(id) => {
            if let Some(changed) = shortcuts.get_mut(&id) {
                changed.keys = None;
            }
        }
        ShortcutEdit::Priority(id, priority) => {
            shortcuts.entry(id).or_default().priority = Some(priority);
        }
        ShortcutEdit::ResetAll => shortcuts.clear(),
    }
    shortcuts.retain(|_, changed| *changed != settings::ShortcutOverride::default());
    SETTINGS.set(settings);
    save_settings();
    apply_shortcuts();
    send_settings();
}

/// Registers ⌥⌘N as the setting says, which another app may already have.
pub(super) fn apply_little_hotkey() {
    #[cfg(target_os = "macos")]
    LITTLE_HOTKEY.set(crate::native::little_hotkey(app_settings().little_hotkey));
}

/// Turns an app setting on or off and saves it.
pub(super) fn set_setting(setting: Setting, on: bool) {
    let mut settings = app_settings();
    match setting {
        Setting::WarnBeforeQuit => settings.warn_before_quit = on,
        Setting::LittleHotkey => settings.little_hotkey = on,
        Setting::RestoreSession => settings.restore_session = on,
        Setting::SuspendTabs => settings.suspend_tabs = on,
    }
    SETTINGS.set(settings);
    save_settings();
    if setting == Setting::LittleHotkey {
        apply_little_hotkey();
    }
    send_settings();
    if !on && setting == Setting::SuspendTabs {
        wake_frozen(|_| true);
    }
}

/// Keeps a site's pages from being suspended, waking those frozen, or lets them be suspended again.
pub(super) fn keep_awake(site: String, on: bool) {
    SETTINGS.with_borrow_mut(|settings| {
        settings.awake_sites.retain(|awake| *awake != site);
        if on {
            settings.awake_sites.push(site.clone());
            settings.awake_sites.sort();
        }
    });
    save_settings();
    send_settings();
    if on {
        wake_frozen(|url| self::site(url) == site);
    }
    if let Some(url) = selected_url() {
        send_site_controls(&url);
    }
}

/// The app's settings, as they stand.
pub(super) fn app_settings() -> Settings {
    SETTINGS.with_borrow(Clone::clone)
}

pub(super) fn save_settings() {
    let path = DATA_DIR.get().expect("set").join(settings::FILE_NAME);
    if let Err(error) = settings::save(&app_settings(), &path) {
        eprintln!("cannot save the settings: {error}");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn puts_localhost_alone_in_developer_mode_by_default() {
        for url in [
            "http://localhost:3000/",
            "http://127.0.0.1/",
            "http://[::1]:8080/a",
            "https://app.localhost/",
        ] {
            assert!(super::developer_mode(url), "{url}");
        }
        for url in [
            "https://app.runora.local/",
            "https://example.com/",
            "about:blank",
        ] {
            assert!(!super::developer_mode(url), "{url}");
        }
    }
}

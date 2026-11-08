//! The welcome flow, in a window of its own (app/macos/Onboarding.swift): shown at the first
//! launch, or again by hand (docs/decisions/0007-first-launch-onboarding.md).

use super::*;

/// Opens the welcome flow's window, alone at the first launch. Its page asks for the browsers to
/// import from once it listens (Command::OnboardingReady).
pub(super) fn show_onboarding() {
    #[cfg(target_os = "macos")]
    crate::native::onboarding(true);
    send_onboarding();
}

/// Sends the welcome flow the browsers found, as the import assistant lists them: again as it
/// comes back to the front, Full Disk Access given to read Safari's.
pub(super) fn send_onboarding() {
    // The import running reads the browsers found before.
    if !IMPORTING.get() {
        SOURCES.set(browsers::detect(&crate::user_home()));
    }
    event("arbalete:onboarding", &sources());
}

/// Tells the welcome flow what an import made: its Spaces, and how many pages of history came.
pub(super) fn send_imported(spaces: &[SpaceId], pages: usize) {
    event(
        "arbalete:imported",
        &json!({ "spaces": spaces, "pages": pages }),
    );
}

/// Adds a Space for work in a profile of its own, so that work logins, cookies and history stay
/// apart.
pub(super) fn add_work_space() {
    let added = update_sidebar(|sidebar| {
        let profile = sidebar.add_profile("Work");
        let space = sidebar.add_space("Work", profile)?;
        sidebar.set_space_emoji(space, "💼")?;
        sidebar.set_space_theme(space, Color::Blue.into())
    });
    if added.is_ok() {
        send_settings();
    }
}

fn event(name: &str, detail: &serde_json::Value) {
    #[cfg(target_os = "macos")]
    crate::native::event(win().id, "onboarding", name, &detail.to_string());
}

/// Ends the welcome flow: it no longer shows at launch, and the browser takes over.
pub(super) fn finish_onboarding() {
    let mut settings = app_settings();
    settings.onboarded = true;
    SETTINGS.set(settings);
    save_settings();
    close_onboarding();
    toast(
        json!({ "text": "You’re all set", "icon": "checkmark", "keys": "⌘T", "trailing": "opens a page" }),
    );
}

/// Closes the welcome flow's window and brings the main one, which waited behind it at the first
/// launch, as it would have been without it: with the start page when it has no tab.
pub(super) fn close_onboarding() {
    #[cfg(target_os = "macos")]
    crate::native::onboarding(false);
    if STATE.with_borrow_mut(|state| active_tab(&mut state.sidebar).is_none()) {
        open_tab(START_URL);
    }
    if let Some(window) = win().window.with_borrow(Clone::clone) {
        window.show();
    }
}

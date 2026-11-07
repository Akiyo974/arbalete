//! The session, the sidebar and the windows it shows: saved soon after each change, restored at
//! launch.

use super::*;

/// What the windows but the focused one show of the sidebar.
pub(super) fn other_windows() -> Vec<WindowView> {
    let focused = FOCUSED.get();
    WINDOWS.with_borrow(|windows| {
        windows
            .iter()
            .filter(|win| win.id != focused && win.incognito.get().is_none())
            .filter_map(|win| win.view.with_borrow(Clone::clone))
            .collect()
    })
}

/// How long a change waits to be written, at most: a page changing its title many times a
/// second writes the session and the history once a second.
const SAVE_DELAY_MS: i64 = 1000;

/// Saves the sidebar, and what the other windows show of it, once the session is restored: soon,
/// or right away while the app quits.
pub(super) fn save_session() {
    if SESSION_FILE.with_borrow(Option::is_none) {
        return;
    }
    SESSION_CHANGED.set(true);
    if CLOSING.get() {
        save_pending();
    } else {
        save_later();
    }
}

/// Writes the changes waiting to be in a moment, unless it is already planned.
pub(super) fn save_later() {
    if !SAVE_POSTED.replace(true) {
        post_delayed_task(ThreadId::UI, Some(&mut SavePending::new()), SAVE_DELAY_MS);
    }
}

wrap_task! {
    struct SavePending {}

    impl Task {
        fn execute(&self) {
            save_pending();
        }
    }
}

/// Writes the changes waiting to be: the pages' new titles and the session.
pub(super) fn save_pending() {
    SAVE_POSTED.set(false);
    for ((profile, url), title) in NEW_TITLES.take() {
        update_history(profile, |history| history.set_title(&url, &title));
    }
    if SESSION_CHANGED.replace(false) {
        write_session();
    }
}

fn write_session() {
    let Some(path) = SESSION_FILE.with_borrow(Clone::clone) else {
        return;
    };
    // Quitting closes the windows one by one: the session keeps them all.
    let windows = QUIT_WINDOWS
        .with_borrow(Clone::clone)
        .unwrap_or_else(other_windows);
    if let Err(error) = STATE.with_borrow(|state| store::save(&state.sidebar, &windows, &path)) {
        eprintln!("cannot save the session: {error}");
    }
}

/// Sets a data file that cannot be loaded aside, so starting over from defaults does not lose it.
pub fn keep_broken(path: &Path) {
    match store::set_aside(path) {
        Ok(aside) => eprintln!("kept {} as {}", path.display(), aside.display()),
        Err(error) => eprintln!("cannot set {} aside: {error}", path.display()),
    }
}

/// Loads the sidebar saved by the previous session; a file that cannot be read is set aside, so the
/// next save does not overwrite it. The first launch offers to import the browsers found, if any, before the start page.
pub(super) fn restore(path: PathBuf) {
    let first_launch = match store::load(&path) {
        Ok(Some((sidebar, windows))) => {
            STATE.with_borrow_mut(|state| state.sidebar = sidebar);
            // The other windows open again with the first one.
            if app_settings().restore_session {
                RESTORED_WINDOWS.set(windows);
            }
            false
        }
        Ok(None) => true,
        Err(error) => {
            eprintln!("cannot restore the session: {error:?}");
            keep_broken(&path);
            false
        }
    };
    SESSION_FILE.set(Some(path));
    // Without restoring the session, every Space starts without "Today" tabs.
    if !app_settings().restore_session {
        STATE.with_borrow_mut(|state| {
            let spaces: Vec<_> = state.sidebar.spaces().iter().map(|s| s.id).collect();
            for space in spaces {
                let _ = state.sidebar.clear_today(space);
            }
        });
    }
    STATE.with_borrow_mut(|state| favicon::sweep(&mut state.sidebar, profile_dir));
    if first_launch && !browsers::detect(&crate::user_home()).is_empty() {
        IMPORT_AT_LAUNCH.set(true);
    } else if STATE.with_borrow_mut(|state| active_tab(&mut state.sidebar).is_none()) {
        open_tab(START_URL);
    }
}

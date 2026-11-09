//! The session, the sidebar and the windows it shows: saved soon after each change, restored at
//! launch.

use super::*;

/// Where the focused window is, and the windows but it: what they show of the sidebar, and
/// where they are.
pub(super) fn saved_windows() -> (Option<Place>, Vec<store::Window>) {
    let focused = FOCUSED.get();
    WINDOWS.with_borrow(|windows| {
        let others = windows
            .iter()
            .filter(|win| win.id != focused && win.incognito.get().is_none())
            .filter_map(|win| {
                Some(store::Window {
                    view: win.view.with_borrow(Clone::clone)?,
                    place: place_of(win),
                })
            })
            .collect();
        (place_of(&win()), others)
    })
}

/// Where the window is, but an incognito one, which the session forgets.
fn place_of(win: &Win) -> Option<Place> {
    if win.incognito.get().is_some() {
        return None;
    }
    win.window.with_borrow(|window| window.as_ref()?.place())
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

/// Writes the session right away as quitting starts, with where the windows are, which moving
/// or resizing them does not save.
pub(super) fn save_at_quit() {
    if SESSION_FILE.with_borrow(Option::is_some) {
        SESSION_CHANGED.set(true);
    }
    save_pending();
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
    let (place, windows) = QUIT_WINDOWS
        .with_borrow(Clone::clone)
        .unwrap_or_else(saved_windows);
    let session = STATE.with_borrow(|state| store::Session {
        sidebar: state.sidebar.clone(),
        place,
        windows,
    });
    if let Err(error) = store::save(&session, &path) {
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
/// next save does not overwrite it. Until the welcome flow ran, a sidebar as it first is shows it
/// instead of the start page.
pub(super) fn restore(path: PathBuf) {
    match store::load(&path) {
        Ok(Some(session)) => {
            STATE.with_borrow_mut(|state| state.sidebar = session.sidebar);
            // The first window opens where the focused one was, the others with it.
            if app_settings().restore_session {
                win().place.replace(session.place);
                RESTORED_WINDOWS.set(session.windows);
            }
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("cannot restore the session: {error:?}");
            keep_broken(&path);
        }
    }
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
    // Users of an earlier version have changed their sidebar: they never get it at launch.
    let untouched = STATE.with_borrow(|state| is_untouched(&state.sidebar));
    if !app_settings().onboarded && untouched {
        ONBOARD_AT_LAUNCH.set(true);
    } else if STATE.with_borrow_mut(|state| active_tab(&mut state.sidebar).is_none()) {
        open_tab(START_URL);
    }
}

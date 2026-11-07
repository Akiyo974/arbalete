//! Each profile's history: the pages visited, kept as they load, and shown in the library.

use super::*;

/// Changes a profile's history, kept in its directory beside its web data. A history that cannot
/// be written is not worth stopping the browsing for.
pub(super) fn update_history<E: std::fmt::Display>(
    profile: ProfileId,
    change: impl FnOnce(&mut History) -> Result<(), E>,
) {
    if let Some(Err(error)) = with_history(profile, change) {
        eprintln!("cannot write the history: {error}");
    }
}

/// Runs `f` on a profile's history, opened the first time, unless it cannot be opened.
pub(super) fn with_history<R>(profile: ProfileId, f: impl FnOnce(&mut History) -> R) -> Option<R> {
    // An incognito window keeps no history.
    if INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile)) {
        return None;
    }
    HISTORIES.with_borrow_mut(|histories| {
        let history = match histories.entry(profile) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                // An imported profile has no directory before its pages first load.
                let dir = profile_dir(profile);
                if let Err(error) = std::fs::create_dir_all(&dir) {
                    eprintln!("cannot create the profile's directory: {error}");
                    return None;
                }
                match History::open(&dir.join(history::FILE_NAME)) {
                    Ok(history) => entry.insert(history),
                    Err(error) => {
                        eprintln!("cannot open the history: {error}");
                        return None;
                    }
                }
            }
        };
        Some(f(history))
    })
}

/// Shows the history: Chromium's own page, as the library has none, like Arc's.
pub(super) fn show_history() {
    open_chrome_page("chrome://history");
}

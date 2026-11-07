//! The archive, like Arc's: "Today" tabs left unshown long enough leave the sidebar for it.

use super::*;

/// How often the "Today" tabs left unshown are archived.
const ARCHIVE_CHECK_MS: i64 = 60_000;

/// Archives the "Today" tabs left unshown longer than their profile allows, suspends the pages
/// hidden for long and takes a picture of the shown one, then again every minute.
pub(super) fn check_archive() {
    let now = now().try_into().unwrap_or_default();
    let archived = STATE.with_borrow_mut(|state| state.sidebar.archive_due(now));
    if archived > 0 {
        update_sidebar(|_| ());
        send_archive();
    }
    suspend_hidden_pages();
    snapshot_shown();
    post_delayed_task(
        ThreadId::UI,
        Some(&mut CheckArchive::new()),
        ARCHIVE_CHECK_MS,
    );
}

wrap_task! {
    struct CheckArchive {}

    impl Task {
        fn execute(&self) {
            check_archive();
        }
    }
}

/// Sends the library the archived tabs, the most recent first, with their Space's name.
pub(super) fn send_archive() {
    let json = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let spaces = sidebar.spaces();
        let items: Vec<_> = sidebar
            .archive()
            .iter()
            .map(|archived| {
                let space = spaces.iter().find(|space| space.id == archived.space);
                let profile = space.map_or(sidebar.displayed().profile, |space| space.profile);
                json!({
                    "url": shown_url(&archived.tab.url),
                    "title": archived.tab.label(),
                    "favicon": favicon_url(profile, &archived.tab),
                    "space": space.map(|space| space.title.clone()),
                    "archivedAt": archived.archived_at,
                    "manual": archived.manual,
                })
            })
            .collect();
        serde_json::Value::from(items)
    });
    let json = json.to_string();
    in_each_window(|| dispatch(&win().sidebar, "arbalete:archive", &json));
    dispatch(&win().reveal_view, "arbalete:archive", &json);
}

/// Opens an archived tab again, by its rank, in its Space, and shows it.
pub(super) fn restore_archived(rank: usize) {
    let now = now().try_into().unwrap_or_default();
    let restored = update_sidebar(|sidebar| {
        let (space, tab) = sidebar.restore(rank, now)?;
        sidebar.select(space, tab)?;
        Ok::<_, Error>(space)
    });
    if let Ok(space) = restored {
        show_space(space);
    }
    send_archive();
}

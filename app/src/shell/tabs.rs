//! The sidebar's rows: opening, selecting, closing and moving tabs, folders and favorites.

use super::*;

pub(super) fn active_tab(sidebar: &mut Sidebar) -> Option<&mut Tab> {
    let id = sidebar.displayed().selected?;
    sidebar.tab_mut(id)
}

/// Opens a tab in the displayed Space and shows it, in place of the site controls.
pub(super) fn open_tab(url: &str) {
    if is_chrome_page(url) {
        return open_chrome_page(url);
    }
    hide_site_controls();
    update_sidebar(|sidebar| {
        let space = sidebar.displayed().id;
        let tab = sidebar.open_tab(space, url).expect("the displayed Space");
        sidebar.select(space, tab).expect("a new tab");
    });
    show_selected();
}

/// Opens a tab in the displayed Space behind the one shown.
pub(super) fn open_background_tab(url: &str) {
    if is_chrome_page(url) {
        return open_chrome_page(url);
    }
    update_sidebar(|sidebar| {
        let space = sidebar.displayed().id;
        sidebar.open_tab(space, url).expect("the displayed Space");
    });
}

/// Shows a tab, with its Space; a favorite is shown in the displayed Space.
pub(super) fn select_tab(id: NodeId) {
    let selected = update_sidebar(|sidebar| {
        let shown = sidebar.displayed().id;
        let space = sidebar.space_of(id).unwrap_or(shown);
        // An incognito window and the others never show each other's tabs.
        if !sidebar.reaches(shown, space) {
            return Err(arbalete_core::sidebar::Error::UnknownNode);
        }
        sidebar
            .select(space, id)
            .and_then(|()| sidebar.display(space))
    });
    if selected.is_ok() {
        show_selected();
    }
}

/// Shows the tab `find` picks, if any.
pub(super) fn select_found(find: impl FnOnce(&Sidebar) -> Option<NodeId>) {
    if let Some(tab) = STATE.with_borrow(|state| find(&state.sidebar)) {
        select_tab(tab);
    }
}

/// Shows a Space in the sidebar and its selected tab; like a selection, an unknown Space from the
/// UI is ignored.
pub(super) fn show_space(id: SpaceId) {
    let shown = STATE.with_borrow(|state| state.sidebar.displayed().id);
    if !STATE.with_borrow(|state| state.sidebar.reaches(shown, id)) {
        return;
    }
    if update_sidebar(|sidebar| sidebar.display(id)).is_ok() {
        show_selected();
    }
}

/// Shows the Space `find` picks, if any.
pub(super) fn show_found_space(find: impl FnOnce(&Sidebar) -> Option<SpaceId>) {
    if let Some(space) = STATE.with_borrow(|state| find(&state.sidebar)) {
        show_space(space);
    }
}

/// Closes a tab; closing the selected one shows the next tab, or the overlay when none is left.
/// Favorites and open pinned tabs stay where they are: closing one deselects it and takes it back
/// to its saved url, and it cannot be reopened since it was never removed. A pinned tab already
/// closed, which its Space does not show, is removed, and may be reopened.
pub(super) fn close_tab(tab: Option<NodeId>) {
    close_keeping(tab, false);
}

/// Closes a tab as close_tab does; with `keep_pinned`, a closed pinned tab stays too.
fn close_keeping(tab: Option<NodeId>, keep_pinned: bool) {
    let (selected, tab) = STATE.with_borrow_mut(|state| {
        let selected = state.sidebar.displayed().selected;
        let tab = tab
            .or(selected)
            .filter(|&id| state.sidebar.tab_mut(id).is_some());
        (selected, tab)
    });
    let Some(tab) = tab else { return };
    let stays = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        // A pinned tab is open only while it is its Space's selected one.
        let open = sidebar.spaces().iter().any(|s| s.selected == Some(tab));
        sidebar.is_favorite(tab) || (sidebar.is_pinned(tab) && (open || keep_pinned))
    });
    let next = if stays {
        // It opens again at its saved url.
        close_page(tab);
        update_sidebar(|sidebar| {
            let space = sidebar.displayed().id;
            sidebar
                .close_in_place(space, tab)
                .expect("the displayed Space")
        })
    } else {
        remove_tab(tab)
    };
    if selected == Some(tab) {
        show_next(next);
    }
}

/// Removes an open tab, which may be reopened, and returns the tab to show instead.
fn remove_tab(tab: NodeId) -> Option<NodeId> {
    let next = STATE.with_borrow_mut(|state| {
        let profile = state.sidebar.profile_of(tab).expect("an open tab");
        let closed = Closed {
            id: Some(tab),
            tab: state.sidebar.tab_mut(tab).expect("an open tab").clone(),
            profile,
            at: SystemTime::now(),
            beside: state.sidebar.split_neighbor(tab),
        };
        // An incognito window's tabs cannot be reopened, nor are they archived.
        if !INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile)) {
            // Like Arc's, a tab closed goes to the archive too.
            if let Some(space) = state.sidebar.space_of(tab) {
                state
                    .sidebar
                    .archive_closed(closed.tab.clone(), space, now() as u64);
            }
            state.closed.push(closed);
        }
        state.sidebar.next_after_close(tab)
    });
    update_sidebar(|sidebar| sidebar.remove(tab)).expect("an open tab");
    send_archive();
    next
}

/// Shows the tab that replaces the selected one, or, like Arc's, the empty Space's panel when
/// none is left.
fn show_next(next: Option<NodeId>) {
    match next {
        Some(next) => select_tab(next),
        None => hide_content(),
    }
}

/// Closes the tabs of a folder, the selected one last, so the tab shown instead is not one about
/// to close.
pub(super) fn close_folder_tabs(folder: NodeId) {
    let tabs = STATE.with_borrow(|state| {
        let mut tabs = state.sidebar.folder_tabs(folder).ok()?;
        let selected = state.sidebar.displayed().selected;
        tabs.sort_by_key(|&tab| Some(tab) == selected);
        Some(tabs)
    });
    for tab in tabs.unwrap_or_default() {
        close_keeping(Some(tab), true);
    }
}

/// Closes every "Today" tab of the displayed Space, which may be reopened.
pub(super) fn clear_today() {
    close_rows(|sidebar| {
        let space = sidebar.displayed();
        let (id, profile) = (space.id, space.profile);
        Ok((sidebar.clear_today(id)?, profile))
    });
}

/// Closes the rows after a tab's, as clear_today closes "Today" tabs.
pub(super) fn close_tabs_below(tab: NodeId) {
    close_rows(|sidebar| {
        let profile = sidebar.profile_of(tab).ok_or(Error::UnknownNode)?;
        Ok((sidebar.remove_below(tab)?, profile))
    });
}

/// Removes a folder and closes its tabs, as clear_today closes "Today" tabs.
pub(super) fn delete_folder(folder: NodeId) {
    close_rows(|sidebar| {
        let profile = sidebar.profile_of(folder).ok_or(Error::UnknownNode)?;
        Ok((sidebar.remove_folder(folder)?, profile))
    });
}

/// Removes rows with `remove`, which returns their tabs and their profile; the tabs may be
/// reopened. When the selected tab was one of them, shows the tab closing it would have shown,
/// if it is left, or the overlay. Like a selection, an unknown row from a menu is ignored.
pub(super) fn close_rows(
    remove: impl FnOnce(&mut Sidebar) -> Result<(Vec<Tab>, ProfileId), Error>,
) {
    let space = STATE.with_borrow(|state| state.sidebar.displayed().id);
    let (selected, next) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let selected = sidebar.displayed().selected;
        (
            selected,
            selected.and_then(|tab| sidebar.next_after_close(tab)),
        )
    });
    let Ok((removed, profile)) = update_sidebar(remove) else {
        return;
    };
    let now = SystemTime::now();
    // Like Arc's, the tabs closed go to the archive too, but an incognito window's.
    if !INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile)) {
        STATE.with_borrow_mut(|state| {
            for tab in removed.iter().rev() {
                state
                    .sidebar
                    .archive_closed(tab.clone(), space, self::now() as u64);
            }
        });
        send_archive();
    }
    STATE.with_borrow_mut(|state| {
        state.closed.extend(removed.into_iter().map(|tab| Closed {
            id: None,
            tab,
            profile,
            at: now,
            beside: None,
        }))
    });
    send_state();
    let open = |id: NodeId| STATE.with_borrow_mut(|state| state.sidebar.tab_mut(id).is_some());
    if selected.is_some_and(|tab| !open(tab)) {
        show_next(next.filter(|&id| open(id)));
    }
}

/// Removes a favorite for good, unlike closing it; it may be reopened like a closed tab.
pub(super) fn remove_favorite(tab: NodeId) {
    let (favorite, selected) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        (
            sidebar.is_favorite(tab),
            sidebar.displayed().selected == Some(tab),
        )
    });
    if !favorite {
        return;
    }
    let next = remove_tab(tab);
    if selected {
        show_next(next);
    }
}

/// Moves a tab to another Space; the selected tab takes the Space along, so it stays in view.
pub(super) fn move_to_space(id: NodeId, space: SpaceId) {
    // The selected tab, if it moves: the tab itself, or one in the folder.
    let selected = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let selected = sidebar.displayed().selected?;
        let moves = sidebar.row_of(selected) == sidebar.row_of(id)
            || sidebar
                .folder_tabs(id)
                .is_ok_and(|tabs| tabs.contains(&selected));
        moves.then_some(selected)
    });
    // Like a selection, an unknown tab, folder or Space from a menu is ignored.
    if update_sidebar(|sidebar| sidebar.move_to_space(id, space)).is_ok()
        && let Some(selected) = selected
    {
        select_tab(selected);
    }
}

/// Pins or unpins a tab, or the selected one.
pub(super) fn toggle_pin(tab: Option<NodeId>) {
    update_sidebar(|sidebar| {
        let space = sidebar.displayed();
        let (space, tab) = (space.id, tab.or(space.selected));
        if let Some(tab) = tab {
            // Like a selection, an unknown tab from the UI is ignored.
            let _ = sidebar.toggle_pin(space, tab);
        }
    });
}

/// Creates a folder at the end of `parent`, or of the pinned tabs, and asks the sidebar to rename
/// it, even when the menu was used from the page. Like a selection, an unknown parent is ignored.
pub(super) fn new_folder(parent: Option<NodeId>) {
    let folder = update_sidebar(|sidebar| {
        let parent = parent.unwrap_or(sidebar.displayed().pinned);
        // Like Arc's.
        sidebar.add_folder(parent, "Untitled")
    });
    if let Ok(folder) = folder {
        ask("arbalete:rename", &json!(folder));
    }
}

/// Reopens a closed tab, by its rank from the last closed one: back in its split, beside the tab
/// it showed with while that one is open, like Arc's; else in the displayed Space.
pub(super) fn reopen_tab(rank: usize) {
    let closed = STATE.with_borrow_mut(|state| {
        let index = state.closed.len().checked_sub(rank + 1)?;
        Some(state.closed.remove(index))
    });
    let Some(closed) = closed else {
        return;
    };
    let tab = update_sidebar(|sidebar| {
        let shown = sidebar.displayed().id;
        // An incognito window and the others never show each other's tabs.
        let beside = closed.beside.and_then(|(target, before)| {
            let space = sidebar.space_of(target)?;
            sidebar
                .reaches(shown, space)
                .then_some((space, target, before))
        });
        let space = beside.map_or(shown, |(space, ..)| space);
        let tab = sidebar
            .open_tab(space, &closed.tab.url)
            .expect("a known Space");
        if let Some((_, target, before)) = beside {
            // A split already full takes it as a row of its own.
            let _ = sidebar.split_beside(tab, target, before);
        }
        // Another profile has not cached its icon: its page sends it as it loads.
        let same_profile = sidebar.profile_of_space(space) == Some(closed.profile);
        let reopened = sidebar.tab_mut(tab).expect("just opened");
        reopened.title = closed.tab.title;
        if same_profile {
            reopened.favicon = closed.tab.favicon;
        }
        tab
    });
    // The tabs closed beside it go back beside it again.
    STATE.with_borrow_mut(|state| {
        for other in &mut state.closed {
            if let Some((target, _)) = &mut other.beside
                && closed.id == Some(*target)
            {
                *target = tab;
            }
        }
    });
    select_tab(tab);
}

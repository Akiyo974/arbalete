//! The sidebar's rows: opening, selecting, closing and moving tabs, folders and favorites.

use super::*;

pub(super) fn active_tab(sidebar: &mut Sidebar) -> Option<&mut Tab> {
    let id = sidebar.displayed().selected?;
    sidebar.tab_mut(id)
}

/// Opens a tab in the displayed Space and shows it, in place of the site controls.
/// Opens a Chrome Web Store page in a new tab of a profile's first Space, displayed, for
/// extensions::install_from_store: the tab, unless the profile has no Space.
pub fn open_store_tab(profile: ProfileId, url: &str) -> Option<NodeId> {
    hide_site_controls();
    let tab = update_sidebar(|sidebar| {
        let space = sidebar.spaces().iter().find(|s| s.profile == profile)?.id;
        sidebar.display(space).ok()?;
        let tab = sidebar.open_tab(space, url).ok()?;
        sidebar.select(space, tab).ok()?;
        Some(tab)
    });
    show_selected();
    tab
}

/// Closes the tab extensions::install_from_store opened.
pub fn close_store_tab(tab: NodeId) {
    close_tab(Some(tab));
}

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

/// Opens a tab in the displayed Space behind the one shown. Like Arc's, a toast tells, which a
/// click goes to, while the sidebar that would show the new tab is hidden.
pub(super) fn open_background_tab(url: &str) {
    if is_chrome_page(url) {
        return open_chrome_page(url);
    }
    let tab = update_sidebar(|sidebar| {
        let space = sidebar.displayed().id;
        sidebar.open_tab(space, url).expect("the displayed Space")
    });
    if win().sidebar_mode.get() == SidebarMode::Hidden {
        let go = json!({ "name": "select", "args": tab });
        toast(json!({
            "text": "New Tab Created",
            "icon": "plus.square.on.square",
            "commands": [go.to_string()],
        }));
    }
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
/// Favorites and open pinned tabs, selected or not, stay where they are: closing one deselects it
/// and takes it back to its saved url, and it cannot be reopened since it was never removed. A
/// pinned tab already closed, with no page, is removed, and may be reopened.
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
        let open = has_page(tab) || sidebar.spaces().iter().any(|s| s.selected == Some(tab));
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
            place: state.sidebar.place_of(tab),
        };
        // An incognito window's tabs cannot be reopened, nor are they archived.
        if !INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile)) {
            // Like Arc's, a tab closed goes to the archive too.
            if let Some(space) = state.sidebar.space_of(tab) {
                state
                    .sidebar
                    .archive_closed(closed.tab.clone(), space, now() as u64);
            }
            state.keep_closed([closed]);
        }
        state.sidebar.next_after_close(tab)
    });
    update_sidebar(|sidebar| sidebar.remove(tab)).expect("an open tab");
    send_archive();
    // Like Arc's, closing a tab tells nothing: ⌘Z or ⇧⌘T reopens it.
    next
}

/// Tells that `what` closed, like Arc's after clearing tabs or deleting a folder or a Space, with
/// ⌘Z putting it back. Like Arc's, the toast has no button.
pub(super) fn tell_closed(what: &str, count: usize) {
    if count == 0 {
        return;
    }
    toast(json!({
        "key": "closed",
        "text": format!("{what}."),
        "keys": "⌘Z",
        "trailing": "to Undo",
    }));
}

/// Reopens what closed last, like Arc's ⌘Z: the Space deleted, the tabs closed together, as
/// clearing "Today" closes them, or the tab closed last.
pub(super) fn undo_close() {
    if restore_space() {
        return;
    }
    let count = STATE.with_borrow(|state| {
        let last = state.closed.last().map(|closed| closed.at);
        let together = state.closed.iter().rev();
        together
            .take_while(|closed| Some(closed.at) == last)
            .count()
    });
    for _ in 0..count {
        reopen_tab(0);
    }
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
/// to close; ⌘Z reopens them together.
pub(super) fn close_folder_tabs(folder: NodeId) {
    let tabs = STATE.with_borrow(|state| {
        let mut tabs = state.sidebar.folder_tabs(folder).unwrap_or_default();
        let selected = state.sidebar.displayed().selected;
        tabs.sort_by_key(|&tab| Some(tab) == selected);
        tabs
    });
    let since = SystemTime::now();
    for tab in tabs {
        close_keeping(Some(tab), true);
    }
    // Closed at the same time, like the tabs a toast's Undo reopens.
    STATE.with_borrow_mut(|state| {
        let mut together = state.closed.iter_mut().rev().take_while(|c| c.at >= since);
        if let Some(last) = together.next() {
            let at = last.at;
            together.for_each(|closed| closed.at = at);
        }
    });
}

/// Closes every "Today" tab of the displayed Space, which may be reopened.
pub(super) fn clear_today() {
    // Arc's words.
    close_rows(Some("Cleared Tabs"), |sidebar| {
        let space = sidebar.displayed();
        let (id, profile) = (space.id, space.profile);
        Ok((sidebar.clear_today(id)?, profile))
    });
}

/// Closes the rows after a tab's, as clear_today closes "Today" tabs, but like closing a tab,
/// telling nothing.
pub(super) fn close_tabs_below(tab: NodeId) {
    close_rows(None, |sidebar| {
        let profile = sidebar.profile_of(tab).ok_or(Error::UnknownNode)?;
        Ok((sidebar.remove_below(tab)?, profile))
    });
}

/// Removes a folder and closes its tabs, as clear_today closes "Today" tabs.
pub(super) fn delete_folder(folder: NodeId) {
    close_rows(Some("Folder deleted"), |sidebar| {
        let profile = sidebar.profile_of(folder).ok_or(Error::UnknownNode)?;
        Ok((sidebar.remove_folder(folder)?, profile))
    });
}

/// Removes rows with `remove`, which returns their tabs and their profile; the tabs may be
/// reopened, as a toast saying `what` happened offers, if any. When the selected tab was one of
/// them, shows the tab closing it would have shown, if it is left, or the overlay. Like a
/// selection, an unknown row from a menu is ignored.
pub(super) fn close_rows(
    what: Option<&str>,
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
    if let Some(what) = what {
        tell_closed(what, removed.len());
    }
    STATE.with_borrow_mut(|state| {
        state.keep_closed(removed.into_iter().map(|tab| Closed {
            id: None,
            tab,
            profile,
            at: now,
            beside: None,
            place: None,
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

/// Moves a tab, or a folder, to another Space; the selected tab takes the Space along, so it
/// stays in view. A toast tells where it went, like Arc's, and goes there on a click.
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
    if update_sidebar(|sidebar| sidebar.move_to_space(id, space)).is_err() {
        return;
    }
    if let Some(selected) = selected {
        select_tab(selected);
    }
    let (title, tab) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let title = sidebar
            .spaces()
            .iter()
            .find(|found| found.id == space)
            .map(|space| space.title.clone());
        let tab = sidebar
            .node(id)
            .is_some_and(|node| matches!(node.kind, NodeKind::Tab(_)));
        (title.unwrap_or_default(), tab)
    });
    let go = if tab {
        json!({ "name": "select", "args": id })
    } else {
        json!({ "name": "show-space", "args": space })
    };
    toast(json!({
        "text": format!("Moved to {title}."),
        "trailing": "Click to go there.",
        "icon": "arrow.up.right",
        "commands": [go.to_string()],
    }));
}

/// Pins or unpins a tab, or the selected one, which a toast tells, like Arc's.
pub(super) fn toggle_pin(tab: Option<NodeId>) {
    let told = update_sidebar(|sidebar| {
        let space = sidebar.displayed();
        let (space, title, tab) = (space.id, space.title.clone(), tab.or(space.selected)?);
        // Like a selection, an unknown tab from the UI is ignored.
        sidebar.toggle_pin(space, tab).ok()?;
        Some(if sidebar.is_pinned(tab) {
            json!({ "text": format!("Pinned Tab to {title}"), "icon": "pin" })
        } else {
            json!({ "text": format!("Un-Pinned Tab from {title}"), "icon": "pin.slash" })
        })
    });
    if let Some(told) = told {
        toast(told);
    }
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

/// Reopens a closed tab, by its rank from the last closed one, and shows it with its Space: back
/// in its split, beside the tab it showed with while that one is open, like Arc's; else back in
/// its folder or section at its index; else in the displayed Space.
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
        // Else back where it was, like Arc's, while that folder or section is left.
        let place = closed.place.and_then(|(parent, index)| {
            let space = sidebar.space_of(parent)?;
            sidebar
                .reaches(shown, space)
                .then_some((space, parent, index))
        });
        let space = beside
            .map(|(space, ..)| space)
            .or(place.map(|(space, ..)| space))
            .unwrap_or(shown);
        let tab = match place {
            Some((_, parent, index)) => {
                sidebar.add_tab_at(parent, index, &closed.tab.url, &closed.tab.title)
            }
            None => sidebar.open_tab(space, &closed.tab.url),
        }
        .expect("a known Space or folder");
        if let Some((_, target, before)) = beside {
            // A split already full takes it as a row of its own.
            let _ = sidebar.split_beside(tab, target, before);
        }
        // Another profile has not cached its icon: its page sends it as it loads.
        let same_profile = sidebar.profile_of_space(space) == Some(closed.profile);
        let reopened = sidebar.tab_mut(tab).expect("just opened");
        reopened.title = closed.tab.title;
        // A pinned tab goes back to the url it saved, not the one it was on.
        if reopened.saved_url.is_some() && closed.tab.saved_url.is_some() {
            reopened.saved_url = closed.tab.saved_url;
        }
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

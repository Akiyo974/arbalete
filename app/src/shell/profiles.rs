//! Profiles and Spaces: creating and removing them, and each profile's web data.

use super::*;

/// A request context for a profile's web data, sharing its storage with the profile's content
/// view: contexts with the same cache path share it.
pub(super) fn profile_context(profile: ProfileId) -> Option<RequestContext> {
    let dir = profile_dir(profile);
    let settings = RequestContextSettings {
        cache_path: CefString::from(dir.to_str().expect("a UTF-8 path")),
        ..Default::default()
    };
    let context = request_context_create_context(Some(&settings), None)?;
    // Save Link As… and Save Image As… ask where from the profile's downloads folder, like Arc's,
    // rather than the home folder.
    let (downloads, _) = download_folder(Some(profile));
    for name in ["savefile.default_directory", "download.default_directory"] {
        let Some(mut value) = value_create() else {
            break;
        };
        value.set_string(Some(&CefString::from(downloads.to_string_lossy().as_ref())));
        let mut error = CefString::default();
        context.set_preference(
            Some(&CefString::from(name)),
            Some(&mut value),
            Some(&mut error),
        );
    }
    Some(context)
}

/// Where a profile's downloads go, and whether they ask where each time, like Arc's: the
/// Downloads folder unless the profile has another one.
pub(super) fn download_folder(profile: Option<ProfileId>) -> (PathBuf, bool) {
    let location = STATE.with_borrow(|state| {
        let profiles = state.sidebar.profiles();
        let profile = profiles.iter().find(|p| Some(p.id) == profile);
        profile.map(|p| p.download_location.clone())
    });
    let downloads = crate::user_home().join("Downloads");
    match location.unwrap_or_default() {
        DownloadLocation::Downloads => (downloads, false),
        DownloadLocation::Folder(folder) => (PathBuf::from(folder), false),
        DownloadLocation::Ask => (downloads, true),
    }
}

/// Deletes a profile no Space uses, like Arc's, with its web data, history included: like a
/// rename, the Default profile or one in use from the UI is ignored.
pub(super) fn delete_profile(profile: ProfileId) {
    if update_sidebar(|sidebar| sidebar.delete_profile(profile)).is_err() {
        return;
    }
    CONTEXTS.with_borrow_mut(|contexts| contexts.remove(&profile));
    HISTORIES.with_borrow_mut(|histories| histories.remove(&profile));
    NEW_TITLES.with_borrow_mut(|titles| titles.retain(|(of, _), _| *of != profile));
    let dir = profile_dir(profile);
    if let Err(error) = std::fs::remove_dir_all(&dir)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("cannot delete the profile's data: {error}");
    }
    send_settings();
}

/// Creates a profile used by `spaces`, from Arc's Create a new Profile dialog, and shows the
/// displayed Space's page again when it changes profile. Like a rename, a blank or taken name from
/// the UI is ignored.
pub(super) fn create_profile(name: &str, spaces: &[SpaceId]) {
    let displayed = STATE.with_borrow(|state| state.sidebar.displayed().id);
    if update_sidebar(|sidebar| sidebar.create_profile(name, spaces)).is_ok() {
        send_settings();
        if spaces.contains(&displayed) {
            show_selected();
        }
    }
}

/// Creates a Space in the displayed Space's profile and shows it, with `tab` moved into it and
/// selected. Like a rename, a blank title from the UI is ignored, and like a selection, an unknown
/// tab.
pub(super) fn create_space(title: &str, emoji: &str, color: Color, tab: Option<NodeId>) {
    let created = update_sidebar(|sidebar| {
        let displayed = sidebar.displayed();
        let (profile, current) = (displayed.profile, displayed.id);
        let space = sidebar.create_draft_space(title, profile)?;
        // Like Arc's, right after the Space it was created from.
        if let Some(index) = sidebar.spaces().iter().position(|s| s.id == current) {
            sidebar.move_space(space, index + 1)?;
        }
        sidebar.set_space_emoji(space, emoji)?;
        sidebar.set_space_theme(space, color.into())?;
        // A tab moved there is selected; a folder moved there is not a tab to select.
        if let Some(tab) = tab
            && sidebar.move_to_space(tab, space).is_ok()
        {
            let _ = sidebar.select(space, tab);
        }
        Ok::<_, arbalete_core::sidebar::Error>(())
    });
    if created.is_ok() {
        show_selected();
    }
}

/// Gives a Space the icon of the site at `address`: the icon of a tab of its profile at that
/// address, else the one Chromium found for the page or its site, cached. An address without an
/// icon leaves the Space's as it is.
pub(super) fn set_space_site(space: SpaceId, address: &str) {
    let address = address.trim();
    if address.is_empty() {
        return;
    }
    let url = if address.contains("://") {
        address.to_string()
    } else {
        format!("https://{address}")
    };
    let Some((profile, tab_icon)) = STATE.with_borrow(|state| {
        let sidebar = &state.sidebar;
        let profile = sidebar.profile_of_space(space)?;
        let tab_icon = sidebar
            .tabs()
            .find(|&(id, tab)| tab.url == url && sidebar.profile_of(id) == Some(profile))
            .and_then(|(_, tab)| tab.favicon.clone());
        Some((profile, tab_icon))
    }) else {
        return;
    };
    let dir = profile_dir(profile);
    let Some(source) = tab_icon.or_else(|| favicon::of_page(&dir, &favicon::chromium(&dir)?, &url))
    else {
        return;
    };
    let _ = update_sidebar(|sidebar| sidebar.set_space_emoji(space, &format!("site:{source}")));
}

/// Changes the Spaces, then shows the displayed Space's selected tab if the change displays
/// another Space or `space` itself, whose page may change. Like a selection, an unknown Space or
/// profile from the UI or a menu is ignored.
pub(super) fn update_spaces(
    space: SpaceId,
    change: impl FnOnce(&mut Sidebar) -> Result<(), arbalete_core::sidebar::Error>,
) {
    let displayed = STATE.with_borrow(|state| state.sidebar.displayed().id);
    let changed = update_sidebar(change).is_ok();
    let now = STATE.with_borrow(|state| state.sidebar.displayed().id);
    if changed && (now != displayed || now == space) {
        show_selected();
    }
}

/// A profile's web data directory, once CEF has it open. None before, as for a profile just
/// imported: it is opened then, and extensions::profile_ready tells once it is.
pub fn opened_web_data(profile: ProfileId) -> Option<PathBuf> {
    match CONTEXTS.with_borrow(|contexts| contexts.get(&profile).map(Option::is_some)) {
        Some(true) => Some(profile_dir(profile)),
        Some(false) => None,
        None => {
            create_profile_context(profile);
            None
        }
    }
}

/// Opens a profile's web data in its own directory, `Profile <id>`: Chromium only accepts profile
/// directories directly in the root cache path. CEF needs it initialized before a browser can
/// use it (ProfileContextHandler).
pub(super) fn create_profile_context(profile: ProfileId) {
    CONTEXTS.with_borrow_mut(|contexts| contexts.insert(profile, None));
    // Without a directory, an incognito window's web data stays in memory.
    let incognito = INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile));
    let dir = profile_dir(profile);
    let settings = RequestContextSettings {
        cache_path: CefString::from(if incognito {
            ""
        } else {
            dir.to_str().expect("a UTF-8 path")
        }),
        ..Default::default()
    };
    request_context_create_context(
        Some(&settings),
        Some(&mut ProfileContextHandler::new(profile)),
    );
}

pub(super) fn profile_dir(profile: ProfileId) -> PathBuf {
    if let Some(dir) = INCOGNITO.with_borrow(|incognito| incognito.get(&profile).cloned()) {
        return dir;
    }
    let id = serde_json::to_string(&profile).expect("an id");
    let dir = DATA_DIR.get().expect("set when the context is initialized");
    dir.join(format!("Profile {id}"))
}

wrap_request_context_handler! {
    struct ProfileContextHandler {
        profile: ProfileId,
    }

    impl RequestContextHandler {
        /// Keeps the profile's web data for its pages, reads its extensions, and shows the
        /// selected tab if it waited for them.
        fn on_request_context_initialized(&self, context: Option<&mut RequestContext>) {
            let Some(context) = context else { return };
            // Like Arc's, an incognito window's pages show dark.
            if INCOGNITO.with_borrow(|incognito| incognito.contains_key(&self.profile)) {
                context.set_chrome_color_scheme(ColorVariant::DARK, 0);
            }
            CONTEXTS.with_borrow_mut(|contexts| contexts.insert(self.profile, Some(context.clone())));
            extensions::profile_ready(self.profile, &profile_dir(self.profile));
            in_each_window(show_selected);
        }
    }
}

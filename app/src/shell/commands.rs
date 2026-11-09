//! Runs the commands the interface, the menus and the app's callbacks send (app/src/command.rs).

use super::*;

/// Runs a command later on the UI thread, out of the callback that asks for it.
pub fn post(command: Command) {
    post_task(ThreadId::UI, Some(&mut RunCommand::new(command)));
}

/// Runs a command from the UI or a menu, on the UI thread.
/// Runs a command from the window `window`.
#[cfg(target_os = "macos")]
pub fn run_in(window: u32, command: Command) {
    focus(window);
    run(command);
}

pub fn run(command: Command) {
    run_command(command);
    #[cfg(target_os = "macos")]
    sync_native_surfaces();
}

fn run_command(command: Command) {
    let browser = content();
    match (command, browser) {
        (Command::Ready, _) => {
            send_state();
            send_downloads();
            send_window_width();
        }
        (Command::Download(action, id), _) => download_command(action, id),
        (Command::ShowDownloads, _) => show_downloads(),
        (Command::ShowLibraryPage, _) => show_library_page(),
        (Command::SwitchTab(forward), _) => switch_tab(forward),
        (Command::EndTabSwitch, _) => end_tab_switch(),
        (Command::ShowHistory, _) => show_history(),
        (Command::Select(id), _) => select_tab(id),
        (Command::NextTab, _) => select_found(|s| s.next_tab(s.displayed().id, true)),
        (Command::PreviousTab, _) => select_found(|s| s.next_tab(s.displayed().id, false)),
        (Command::SidebarItem(number), _) => {
            let settings = app_settings();
            let (favorites, ninth) = (settings.count_favorites, settings.ninth_sidebar_item);
            select_found(|s| s.sidebar_item(s.displayed().id, number, favorites, ninth))
        }
        (Command::ShowSpace(id), _) => show_space(id),
        (Command::SpaceNumber(number), _) => show_found_space(|s| s.space_at(number)),
        (Command::NextSpace, _) => show_found_space(|s| s.next_space(true)),
        (Command::PreviousSpace, _) => show_found_space(|s| s.next_space(false)),
        (Command::ShowOverlay, _) => show_overlay(true),
        (Command::HideOverlay, _) => show_overlay(false),
        (Command::EditUrl, _) => edit_url(),
        (Command::SearchOverlay(text), _) => search_overlay(&text, "overlay"),
        (Command::SearchLittle(text), _) => search_overlay(&text, "little"),
        (Command::OverlayHeight(height), _) => {
            place_overlay(i32::try_from(height).unwrap_or(OVERLAY_ROW));
            round_overlay();
        }
        (Command::Navigate(url), _) => navigate(&url_to_load(&url)),
        (Command::ToggleSidebar, _) => {
            let has_tab = STATE.with_borrow(|state| state.sidebar.displayed().selected.is_some());
            set_sidebar(toggled(win().sidebar_mode.get(), has_tab));
        }
        (Command::RevealSidebar, _)
            if win().sidebar_mode.get() == SidebarMode::Hidden && !page_full_screen() =>
        {
            set_sidebar(SidebarMode::Revealed);
        }
        (Command::ConcealSidebar, _) if win().sidebar_mode.get() == SidebarMode::Revealed => {
            set_sidebar(SidebarMode::Hidden);
        }
        (Command::RevealSidebar | Command::ConcealSidebar, _) => {}
        (Command::CardOffset(offset), _) => shift_card(offset),
        (Command::ShowLibrary, _) => show_library(),
        (Command::WideLibrary(wide), _) => widen_library(wide),
        (Command::Open(url), _) => {
            let (url, pane) = (url_to_load(&url), win().adding_pane.get());
            show_overlay(false);
            if pane {
                open_in_pane(&url);
            } else {
                open_tab(&url);
            }
        }
        (Command::OpenInBackground(url), _) => open_background_tab(&url),
        // A link the pointer is on in the shown page, like a click on it (page_may_open).
        (Command::OpenInSplit(url) | Command::OpenInLittle(url), _)
            if !page_may_open(&selected_url().unwrap_or_default(), &url) => {}
        (Command::OpenInSplit(url), _) => open_in_pane(&url),
        (Command::OpenInLittle(url), _) => open_little(Some(url)),
        (Command::AddPane(tab), _) => {
            if let Some(tab) = tab {
                select_tab(tab);
            }
            if win().split.borrow().0.len() >= arbalete_core::sidebar::MAX_SPLIT_TABS {
                tell_split_full();
            } else if selected().is_some() {
                show_overlay(true);
                win().adding_pane.set(true);
            }
        }
        (Command::ClosePane, _) if split_shown().is_some() => close_tab(None),
        (Command::ClosePane, _) => {}
        (Command::SwitchPane(next), _) => switch_pane(next),
        (Command::Peek(url), _) => peek(&url),
        (Command::ClosePeek, _) => close_peek(),
        (Command::PeekPage(action), _) => peek_page(action),
        (Command::KeepPeek { split }, _) => keep_peek(split),
        (Command::FlipPanes(split), _) => flip_panes(split),
        (Command::PreviewPane(index), _) => preview_pane(index),
        (Command::PreviewDrop(preview), _) => {
            if win().drop_preview.replace(preview) != preview {
                place_card();
            }
        }
        (Command::SeparateAll(split), _) => {
            if update_sidebar(|sidebar| sidebar.separate_all(split)).is_ok() {
                show_selected();
            }
        }
        (Command::CloseSplit(split), _) => close_rows(None, |sidebar| {
            let profile = sidebar.profile_of(split).ok_or(Error::UnknownNode)?;
            Ok((sidebar.remove_split(split)?, profile))
        }),
        (Command::Close(None), _) if settings_focused() => close_settings(),
        // ⌘W closes the window of Chromium's own pages when it is in front.
        (Command::Close(None), _) if chrome_page_focused() => {
            if let Some((window, _, _)) = CHROME_PAGE.with_borrow(Clone::clone) {
                window.close();
            }
        }
        // Like Arc's, ⌘W closes Peek and ⌘O keeps its page in a tab.
        (Command::Close(None), _) if peek_shown() => close_peek(),
        (Command::OpenLittleInSpace, _) if peek_shown() => keep_peek(false),
        (Command::Close(None), _) if focused_little().is_some() => {
            if let Some(id) = focused_little() {
                close_little(id);
            }
        }
        // Like Arc's, ⌘W closes the window once its Space shows no tab.
        (Command::Close(None), _) if selected().is_none() => close_window(),
        (Command::Close(tab), _) => close_tab(tab),
        (Command::CloseInspector(tab), _) => close_inspector(tab),
        (Command::ResizePane(index, point), _) => resize_panes(index, point),
        (Command::ResizeSidebar(x), _) => resize_sidebar(x),
        // Kept once the drag ends, not on every move, which would stutter.
        (Command::Resized, _) => {
            end_sidebar_resize();
            save_session();
            save_settings();
        }
        (Command::EvenPanes(index), _) => even_panes(index),
        (Command::Reopen(rank), _) => reopen_tab(rank),
        (Command::UndoClose, _) => undo_close(),
        (Command::Pin(tab), _) => toggle_pin(tab),
        (Command::NewFolder(parent), _) => new_folder(parent),
        (Command::NewEasel, _) => new_easel(),
        (Command::ShowLiveCapture(number, id, url, page, scale), _) => {
            show_live_capture((number, id), &url, page, scale)
        }
        (Command::ScaleLiveCapture(number, id, scale), _) => {
            scale_live_capture((number, id), scale)
        }
        (Command::CloseLiveCapture(number, id), _) => close_live_capture((number, id)),
        (Command::AddCaptureToEasel(file, width, height, number, page), _) => {
            add_capture_to_easel(&file, (width, height), number, page)
        }
        (Command::PeekEasel(tab), _) => peek_easel(tab, None),
        (Command::SaveEasel(number, easel), _) => save_easel(number, &easel),
        (Command::AddEaselImage(number, file, name), _) => add_easel_image(number, &file, &name),
        (Command::CloseFolderTabs(folder), _) => close_folder_tabs(folder),
        (Command::RemoveFavorite(tab), _) => remove_favorite(tab),
        // Like a selection, an unknown Space from the UI is ignored.
        (Command::HideFavoritesHint(space), _) => {
            let _ = update_sidebar(|sidebar| sidebar.hide_favorites_hint(space));
        }
        (Command::ClearToday, _) => clear_today(),
        (Command::CopyLink, _) => {
            let url = STATE
                .with_borrow_mut(|state| active_tab(&mut state.sidebar).map(|tab| tab.url.clone()));
            if let Some(url) = url {
                crate::mac::copy(&shown_url(&url));
                toast(json!({ "text": "Copied Current URL", "icon": "link" }));
            }
        }
        (Command::Shortcut(edit), _) => edit_shortcut(edit),
        (Command::MediaSource(source, on), _) => {
            let mut settings = app_settings();
            settings.media_sources.retain(|kept| *kept != source);
            if on {
                settings.media_sources.push(source);
            }
            SETTINGS.set(settings);
            save_settings();
            send_state();
        }
        (Command::SaveCaptureToLibrary(file), _) => save_capture_to_library(&file),
        (Command::CopyMarkdownLink, _) => {
            let link = STATE.with_borrow_mut(|state| {
                active_tab(&mut state.sidebar).map(|tab| format!("[{}]({})", tab.label(), tab.url))
            });
            if let Some(link) = link {
                crate::mac::copy(&link);
                toast(json!({ "text": "Copied Current URL", "icon": "link" }));
            }
        }
        (Command::MoveToSpace(id, space), _) => move_to_space(id, space),
        (Command::DuplicateRow(id), _) => {
            let _ = update_sidebar(|sidebar| sidebar.duplicate_below(id));
        }
        (Command::CloseTabsBelow(tab), _) => close_tabs_below(tab),
        (Command::DeleteFolder(folder), _) => delete_folder(folder),
        (Command::FolderToSpace(folder), _) => {
            if update_sidebar(|sidebar| sidebar.folder_to_space(folder)).is_ok() {
                show_selected();
            }
        }
        (Command::Menu(target), _) => show_menu(target),
        (Command::TabsMenu(ids), _) => {
            let entries = STATE.with_borrow(|state| menu::tabs_menu(&state.sidebar, &ids));
            #[cfg(target_os = "macos")]
            crate::mac::pop_up(entries);
        }
        (Command::MenuTargets { view, targets }, _) => {
            win()
                .menu_targets
                .with_borrow_mut(|all| all[view as usize] = targets);
        }
        (Command::ChooseMenu(target, item), _) => choose_menu(target, &item),
        // Like a selection, an unknown folder or a blank title from the UI is ignored.
        (Command::ToggleFolder(id), _) => {
            let _ = update_sidebar(|sidebar| sidebar.toggle_folder(id));
        }
        (Command::Rename(id, title), _) => {
            let _ = update_sidebar(|sidebar| sidebar.rename(id, &title));
        }
        (Command::SetPinnedUrl(tab, url), _) => {
            let _ = update_sidebar(|sidebar| sidebar.set_saved_url(tab, &url));
        }
        (Command::ToggleMute(tab), _) => {
            if update_sidebar(|sidebar| sidebar.toggle_mute(tab)).is_ok() {
                mute_page(tab);
            }
        }
        (Command::Move(id, parent, index), _) => {
            if update_sidebar(|sidebar| sidebar.move_node(id, parent, index)).is_ok() {
                show_selected();
            }
        }
        (Command::Split(tab, target, before), _) => {
            match update_sidebar(|sidebar| sidebar.split_beside(tab, target, before)) {
                Ok(_) => show_selected(),
                Err(Error::SplitFull) => tell_split_full(),
                Err(_) => {}
            }
        }
        (Command::Duplicate(tab, parent, index), _) => {
            let _ = update_sidebar(|sidebar| sidebar.duplicate(tab, parent, index));
        }
        (Command::SplitDuplicate(tab, target, before), _) => {
            match update_sidebar(|sidebar| sidebar.split_duplicate(tab, target, before)) {
                Ok(_) => show_selected(),
                Err(Error::SplitFull) => tell_split_full(),
                Err(_) => {}
            }
        }
        (Command::Separate(tab), _) => {
            if update_sidebar(|sidebar| sidebar.separate(tab)).is_ok() {
                show_selected();
            }
        }
        (Command::NewProfile, _) => ask("arbalete:new-profile", &json!(null)),
        (Command::CreateProfile { name, spaces }, _) => create_profile(&name, &spaces),
        (Command::DeleteProfile(id), _) => delete_profile(id),
        // Like Arc's, the page shows in front of the browser, the settings closed: in a tab, unless
        // it is another profile's or one tabs do not show.
        (Command::ProfilePage(id, url), _) => {
            if url.starts_with("chrome://") {
                close_settings();
                if id == displayed_profile() && !is_chrome_page(&url) {
                    open_tab(&url);
                } else {
                    open_profile_page(&url, id);
                }
            }
        }
        // Like a folder, an unknown profile or a blank name from the UI is ignored.
        (Command::RenameProfile(id, name), _) => {
            if update_sidebar(|sidebar| sidebar.rename_profile(id, &name)).is_ok() {
                send_settings();
            }
        }
        (Command::SetArchiveAfter(id, after), _) => {
            if update_sidebar(|sidebar| sidebar.set_archive_after(id, after)).is_ok() {
                send_settings();
            }
        }
        (Command::SetDownloadLocation(id, location), _) => {
            if update_sidebar(|sidebar| sidebar.set_download_location(id, location)).is_ok() {
                send_settings();
            }
        }
        (Command::SendArchive, _) => send_archive(),
        (Command::RestoreArchived(rank), _) => restore_archived(rank),
        (Command::SetSearchEngine(id, engine), _) => {
            if update_sidebar(|sidebar| sidebar.set_search_engine(id, engine)).is_ok() {
                send_settings();
            }
        }
        (Command::SetSearchSuggestions(id, on), _) => {
            if update_sidebar(|sidebar| sidebar.set_search_suggestions(id, on)).is_ok() {
                send_settings();
            }
        }
        (Command::ShowSettings(pane), _) => show_settings(pane.as_deref()),
        (Command::ToggleToolbar, _) => toggle_toolbar(),
        // Like Arc's, the menu opens a Command Bar of its own, which opens the window.
        #[cfg(target_os = "macos")]
        (Command::NewLittle(None), _) => crate::native::little_bar(),
        (Command::NewLittle(url), _) => open_link(url),
        (Command::NewWindow, _) => new_window(),
        (Command::NewIncognitoWindow, _) => new_incognito_window(),
        (Command::Little(id, action), _) => little_action(id, action),
        (Command::OpenLittleInSpace, _) => {
            if let Some(id) = focused_little() {
                little_action(id, LittleAction::OpenInSpace(None));
            }
        }
        (Command::HideSettings, _) => close_settings(),
        (Command::SettingsReady, _) => send_settings(),
        (Command::SetSetting(setting, on), _) => set_setting(setting, on),
        (Command::KeepAwake(site, on), _) => keep_awake(site, on),
        (Command::SitePictureInPicture(site, on), _) => allow_picture(site, on),
        (Command::DeveloperMode(site, on), _) => set_developer_mode(site, on),
        (Command::NewBoost, _) => boosts::new_boost(),
        (Command::EditBoost(id), _) => boosts::edit_boost(id),
        (Command::PeekBoost(id), _) => boosts::peek_boost(id),
        (Command::ChangeBoost(boost, gesture), _) => boosts::change_boost(*boost, gesture),
        (Command::BoostEditor(action), _) => boosts::boost_editor_action(action),
        (Command::SetBoostActive(id, on), _) => boosts::set_boost_active(id, on),
        (Command::RenameBoost(id, title), _) => boosts::rename_boost(id, &title),
        (Command::DeleteBoost(id), _) => boosts::delete_boost(id),
        (Command::SendBoosts, _) => boosts::send_boosts(),
        (
            Command::CreateSpace {
                title,
                emoji,
                color,
                tab,
            },
            _,
        ) => create_space(&title, &emoji, color, tab),
        // Like a profile, an unknown Space or a blank title from the UI is ignored.
        (Command::RenameSpace(id, title), _) => {
            let _ = update_sidebar(|sidebar| sidebar.rename_space(id, &title));
        }
        (Command::SetSpaceEmoji(id, emoji), _) => {
            let _ = update_sidebar(|sidebar| sidebar.set_space_emoji(id, &emoji));
        }
        (Command::SetSpaceSite(id, address), _) => set_space_site(id, &address),
        (Command::SetUrlRouting(routing), _) => {
            update_sidebar(|sidebar| sidebar.set_url_routing(routing));
            send_settings();
        }
        (Command::SetSpaceTheme(id, theme), _) => {
            let _ = update_sidebar(|sidebar| sidebar.set_space_theme(id, theme));
        }
        (Command::ToggleSpace(id), _) => {
            let _ = update_sidebar(|sidebar| sidebar.toggle_space(id));
        }
        // The selected tab shows in its new profile's web data.
        (Command::SetSpaceProfile(id, profile), _) => {
            update_spaces(id, |sidebar| sidebar.set_space_profile(id, profile));
        }
        (Command::DeleteSpace(id), _) => delete_space(id),
        (Command::SpaceToFolder(id, into), _) => space_to_folder(id, into),
        (Command::MoveSpace(id, index), _) => {
            let _ = update_sidebar(|sidebar| sidebar.move_space(id, index));
        }
        (Command::ShowOnboarding, _) => show_onboarding(),
        (Command::OnboardingReady, _) => send_onboarding(),
        (Command::AddWorkSpace, _) => add_work_space(),
        (Command::FinishOnboarding, _) => finish_onboarding(),
        (Command::CloseOnboarding, _) => close_onboarding(),
        (Command::ShowImport, _) => show_import(),
        (Command::ImportReady, _) => send_sources(),
        (Command::CloseImport, _) => close_import(),
        (Command::Import(choices), _) => run_import(choices),
        (Command::InstallExtensions(list), _) => {
            let list = list.into_iter().map(|e| (e.profile, e.id)).collect();
            extensions::install_from_store(list);
        }
        (Command::OpenFullDiskAccess, _) => open_full_disk_access(),
        (
            Command::WindowColors {
                card,
                grain,
                colors,
            },
            _,
        ) => set_window_colors(card, grain, &colors),
        (Command::Back, Some(browser)) => browser.go_back(),
        (Command::Forward, Some(browser)) => browser.go_forward(),
        (Command::Reload, Some(browser)) => browser.reload(),
        (Command::ForceReload, Some(browser)) => browser.reload_ignore_cache(),
        (Command::ViewSource, Some(browser)) => {
            if let Some(frame) = browser.main_frame() {
                open_tab(&format!("view-source:{}", CefString::from(&frame.url())));
            }
        }
        (Command::DevTools(at), Some(browser)) => {
            if let Some(tab) = page_tab(&browser) {
                match at {
                    Some((x, y)) => {
                        show_inspector(tab, "elements", Some(InspectorAction::InspectAt(x, y)))
                    }
                    None => toggle_inspector(tab),
                }
            }
        }
        // The bar's picker picks an element in the page, the others show their panel.
        (Command::DevToolsPanel(panel), Some(browser)) => {
            let then = (panel == InspectorPanel::Elements)
                .then(|| InspectorAction::Script("DevToolsAPI.enterInspectElementMode()".into()));
            if let Some(tab) = page_tab(&browser) {
                show_inspector(tab, panel.name(), then);
            }
        }
        (Command::Capture, Some(_)) => capture(CaptureMode::Portrait),
        (Command::CaptureArea, Some(_)) => capture(CaptureMode::Area),
        (Command::CaptureFull, Some(_)) => capture(CaptureMode::Full),
        (Command::ShowSiteSettings, _) => show_site_settings(),
        (Command::ScrollCapture(dx, dy, x, y), Some(_)) => scroll_capture(dx, dy, x, y),
        (Command::Stop, Some(browser)) => browser.stop_load(),
        (Command::ClearSiteData(data), Some(browser)) => clear_site_data(&browser, data),
        (Command::Zoom(zoom), Some(browser)) => {
            if let Some(host) = browser.host() {
                host.zoom(match zoom {
                    Zoom::In => ZoomCommand::IN,
                    Zoom::Out => ZoomCommand::OUT,
                    Zoom::Reset => ZoomCommand::RESET,
                });
                let percent = zoom_percent(host.zoom_level());
                toast(json!({ "key": "zoom", "text": format!("{percent}%") }));
            }
        }
        (Command::ShowFind, _) => show_find(),
        (Command::Find { text, forward }, _) => {
            if let Some(browser) = find_page() {
                find(&browser, text, forward);
            }
        }
        (Command::HideFind, _) => hide_find(),
        (Command::ShowSiteControls(button), _) => show_site_controls(button),
        (Command::HideSiteControls, _) => hide_site_controls(),
        (Command::ShowThemePicker(id, beside), _) => show_theme_picker(id, beside),
        (Command::HideThemePicker, _) => {
            if let Some(controller) = win().theme.with_borrow(Clone::clone) {
                controller.set_visible(0);
            }
        }
        (Command::SiteControlsHeight(height), _) => {
            win().site_height.set(height);
            place_site_controls();
        }
        (Command::AnswerPermission(id, allow), _) => answer_permission(id, allow),
        (Command::SetSitePermission(setting, allow), _) => set_site_permissions(
            Some(setting),
            if allow {
                ContentSettingValues::ALLOW
            } else {
                ContentSettingValues::BLOCK
            },
        ),
        (Command::ResetSitePermissions, _) => {
            set_site_permissions(None, ContentSettingValues::DEFAULT);
        }
        (Command::PermissionPromptHeight(height), _) => {
            win().permission_height.set(height);
            surfaces_changed();
        }
        (Command::AnswerAuth(id, credentials), _) => answer_auth(id, credentials),
        (Command::AuthPromptHeight(height), _) => {
            win().auth_height.set(height);
            surfaces_changed();
        }
        (Command::OpenExtensionPopup(id), _) => open_extension_popup(&id),
        (Command::ShowStore, _) => show_store(),
        (Command::ShowCertificate, _) => show_certificate(),
        // Like Arc's, Chromium's extensions page.
        (Command::ManageExtensions, _) => open_tab("chrome://extensions"),
        (Command::SiteTargets(targets), _) => win().site_targets.set(targets),
        (Command::RemoveExtension(id), _) => {
            let profile = displayed_profile();
            let extension = extensions::list(profile).into_iter().find(|e| e.id == id);
            let Some(extension) = extension else { return };
            // Chromium confirms it otherwise.
            #[cfg(target_os = "macos")]
            if extensions::removes_itself(profile, &id) {
                let question = format!("Remove “{}”?", extension.name);
                let detail = "Its data in this profile is deleted too.";
                if !crate::mac::confirm(&question, detail, "Remove") {
                    return;
                }
            }
            extensions::uninstall(profile, &extension.id);
        }
        (Command::ExtensionOptions(id), _) => open_extension_options(&id),
        (Command::SetExtensionEnabled(id, enabled), _) => {
            let profile = STATE.with_borrow(|state| state.sidebar.displayed().profile);
            extensions::set_enabled(profile, &id, enabled);
            send_state();
        }
        (Command::PinExtension(id, pinned), _) => {
            let profile = STATE.with_borrow(|state| state.sidebar.displayed().profile);
            let _ = update_sidebar(|sidebar| sidebar.pin_extension(profile, &id, pinned));
        }
        (Command::OpenPinnedExtension(id, button), _) => {
            if let Some(button) = button_in_window(button) {
                win().site_button.set(Some(button));
                open_extension_popup(&id);
            }
        }
        (Command::ExtensionMenu(id), _) => show_extension_menu(Some(&id)),
        (Command::ExtensionsMenu(button), _) => {
            if let Some(button) = button_in_window(button) {
                win().site_button.set(Some(button));
                show_extensions_menu();
            }
        }
        (Command::CloseExtensionPopup, _) => close_extension_popup(),
        (Command::Print, Some(browser)) => {
            if let Some(host) = browser.host() {
                host.print();
            }
        }
        (_, None) => {}
    }
}

// Runs a command posted from a callback on the UI thread (post).
wrap_task! {
    struct RunCommand {
        command: Command,
    }

    impl Task {
        fn execute(&self) {
            run(self.command.clone());
        }    }
}

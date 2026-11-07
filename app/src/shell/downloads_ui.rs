//! The downloads as the library lists them, and what the user does with them there.

use super::*;

/// Saves the downloads list. A list that cannot be written is not worth stopping a download for.
pub(super) fn save_downloads() {
    let path = DATA_DIR.get().expect("set").join(downloads::FILE_NAME);
    let list = DOWNLOADS.with_borrow(|list| {
        downloads::lasting(list, |profile| {
            INCOGNITO.with_borrow(|incognito| incognito.contains_key(&profile))
        })
    });
    if let Err(error) = downloads::save(&list, &path) {
        eprintln!("cannot save the downloads: {error}");
    }
}

/// Sends the downloads to both sidebars, which show the recent ones at their bottom and all of
/// them in the library.
pub(super) fn send_downloads() {
    update_media_files();
    let json = DOWNLOADS.with_borrow(|downloads| serde_json::to_string(downloads).expect("JSON"));
    in_each_window(|| dispatch(&win().sidebar, "arbalete:downloads", &json));
    dispatch(&win().reveal_view, "arbalete:downloads", &json);
}

/// The images and videos downloaded and still there, which the library's Media section shows:
/// the UI loads them by their download's id (media_file).
static MEDIA_FILES: Mutex<Vec<(u32, PathBuf, &'static str)>> = Mutex::new(Vec::new());

fn update_media_files() {
    let files = DOWNLOADS.with_borrow(|downloads| {
        downloads
            .iter()
            .filter(|download| download.state == downloads::State::Complete && !download.missing)
            .filter_map(|download| {
                let mime = downloads::media_type(&download.name)?;
                Some((download.id, download.path.clone(), mime))
            })
            .collect()
    });
    *MEDIA_FILES.lock().expect("not poisoned") = files;
}

/// The downloaded image or video a UI path names, `media/<download id>`, with its media type.
pub(super) fn media_file(path: &str) -> Option<(PathBuf, &'static str)> {
    let id: u32 = path.strip_prefix("media/")?.parse().ok()?;
    let files = MEDIA_FILES.lock().expect("not poisoned");
    let (_, file, mime) = files.iter().find(|(media, _, _)| *media == id)?;
    Some((file.clone(), mime))
}

/// Opens the library on its downloads, showing the sidebar first if it is hidden.
pub(super) fn show_downloads() {
    if win().sidebar_mode.get() == SidebarMode::Hidden {
        set_sidebar(SidebarMode::Shown);
    }
    let current = win();
    let view = &current.sidebar;
    dispatch(view, "arbalete:show-downloads", "null");
}

pub(super) fn download_command(action: DownloadAction, id: u32) {
    let path = DOWNLOADS.with_borrow(|downloads| {
        let download = downloads.iter().find(|download| download.id == id)?;
        Some(download.path.clone())
    });
    let Some(path) = path else { return };
    match action {
        DownloadAction::Cancel => {
            if let Some(callback) =
                DOWNLOAD_CALLBACKS.with_borrow(|callbacks| callbacks.get(&id).cloned())
            {
                callback.cancel();
            }
        }
        // A file deleted or moved since is shown missing.
        DownloadAction::Open | DownloadAction::Show if !path.exists() => {
            DOWNLOADS.with_borrow_mut(|list| {
                if let Some(download) = list.iter_mut().find(|download| download.id == id) {
                    download.missing = true;
                }
            });
            send_downloads();
        }
        #[cfg(target_os = "macos")]
        DownloadAction::Open => crate::mac::open_file(&path),
        #[cfg(target_os = "macos")]
        DownloadAction::Show => crate::mac::show_file(&path),
        DownloadAction::Remove => {
            DOWNLOADS.with_borrow_mut(|downloads| downloads.retain(|download| download.id != id));
            save_downloads();
            send_downloads();
        }
    }
}

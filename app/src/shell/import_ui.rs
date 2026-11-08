//! The import assistant, which brings other browsers' data into Arbalete's.

use super::*;

/// The import assistant's size, at most (app/macos/Import.swift).
const IMPORT_SIZE: Size = Size {
    width: 560,
    height: 480,
};

/// The Privacy & Security pane of System Settings that lists the apps with Full Disk Access.
const FULL_DISK_ACCESS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

/// The first launch's sidebar, which nothing has changed yet: an import replaces it.
pub(super) fn is_untouched(sidebar: &Sidebar) -> bool {
    *sidebar == Sidebar::default()
}

/// Shows the import assistant above the page, with the browsers found now.
pub(super) fn show_import() {
    let Some(window) = win().window.with_borrow(Clone::clone) else {
        return;
    };
    // It shows the import running.
    if IMPORTING.get() {
        return;
    }
    SOURCES.set(browsers::detect(&crate::user_home()));
    make_import();
    let Some(controller) = IMPORT.with_borrow(Clone::clone) else {
        return;
    };
    let size = window.size();
    let (width, height) = (
        IMPORT_SIZE.width.min(size.width),
        IMPORT_SIZE.height.min(size.height),
    );
    controller.set_bounds(Some(&Rect {
        x: (size.width - width) / 2,
        y: (size.height - height) / 2,
        width,
        height,
    }));
    controller.set_visible(1);
    // A page still loading asks for them once it listens (Command::ImportReady).
    send_sources();
    // At launch too, out of any command.
    #[cfg(target_os = "macos")]
    sync_native_surfaces();
}

/// Sends the import assistant the browsers to list and the profiles to merge them into.
pub(super) fn send_sources() {
    let detail = sources();
    IMPORT_VIEW.with(|view| dispatch(view, "arbalete:sources", &detail.to_string()));
}

/// The browsers found, as the import assistant and the welcome flow list them, and the profiles
/// to merge them into: none on the first launch, whose sidebar the import replaces.
pub(super) fn sources() -> serde_json::Value {
    let profiles = STATE.with_borrow(|state| {
        if is_untouched(&state.sidebar) {
            return Vec::new();
        }
        let profiles = state.sidebar.profiles().iter();
        profiles
            .map(|p| json!({ "id": p.id, "name": p.name }))
            .collect()
    });
    let browsers = SOURCES.with_borrow(|browsers| {
        let browsers = browsers.iter().map(|browser| {
            let profiles = browser.profiles.iter().map(|profile| {
                let denied = browser.family == Family::Safari
                    && safari::needs_full_disk_access(&profile.path);
                json!({ "name": profile.name, "needsFullDiskAccess": denied })
            });
            json!({
                "name": browser.name,
                "arc": browser.family == Family::Arc,
                "profiles": profiles.collect::<Vec<_>>(),
            })
        });
        browsers.collect::<Vec<_>>()
    });
    json!({ "browsers": browsers, "profiles": profiles })
}

/// Makes the import assistant's place, once.
fn make_import() {
    if IMPORT.with_borrow(Option::is_none) {
        let view = place_view(no_width, 1);
        let controller = Spot::default();
        controller.set_visible(0);
        IMPORT_VIEW.set(Some(view));
        IMPORT.set(Some(controller));
    }
}

fn hide_import() {
    if let Some(controller) = IMPORT.with_borrow(Clone::clone) {
        controller.set_visible(0);
    }
    // Once the import is done too, out of any command.
    #[cfg(target_os = "macos")]
    sync_native_surfaces();
}

/// Closes the import assistant. Closed on the first launch, it leaves the session as it would
/// have been without it: with the start page. It stays while an import runs.
pub(super) fn close_import() {
    if IMPORTING.get() {
        return;
    }
    hide_import();
    if STATE.with_borrow(|state| is_untouched(&state.sidebar)) {
        open_tab(START_URL);
    }
}

/// Imports browser profiles. Their data is read on a thread of its own, as a large history takes
/// long to read: the window keeps answering meanwhile, and the assistant shows the import
/// running. A second import while one runs is ignored.
pub(super) fn run_import(choices: Vec<Choice>) {
    // The browsers found are used by an import: asked again before they are found anew, it has
    // nothing to import, and must not tell the welcome flow it imported nothing.
    if IMPORTING.get() || SOURCES.with_borrow(Vec::is_empty) {
        return;
    }
    IMPORTING.set(true);
    let browsers = SOURCES.take();
    thread::spawn(move || {
        let sources = import::read(&browsers, &choices);
        let found = importable_extensions(&browsers, &choices);
        let read = Arc::new(Mutex::new(Some((sources, found))));
        post_task(ThreadId::UI, Some(&mut ApplyImport::new(read)));
    });
}

/// The Chrome Web Store extensions of each chosen profile of a Chromium browser, Arc included,
/// by its browser's and its own index (decision 0008).
type Found = HashMap<(usize, usize), Vec<extensions::Installed>>;

fn importable_extensions(browsers: &[browsers::Browser], choices: &[Choice]) -> Found {
    let found = choices.iter().filter_map(|choice| {
        let browser = browsers.get(choice.browser)?;
        let profile = browser.profiles.get(choice.profile)?;
        matches!(browser.family, Family::Chromium | Family::Arc).then(|| {
            let list = extensions::importable(&profile.path);
            ((choice.browser, choice.profile), list)
        })
    });
    found.collect()
}

/// The chosen profiles read, and their extensions, handed to the UI thread.
type Read = Arc<Mutex<Option<(Vec<import::Source>, Found)>>>;

wrap_task! {
    struct ApplyImport {
        read: Read,
    }

    impl Task {
        fn execute(&self) {
            let read = self.read.lock().ok().and_then(|mut read| read.take());
            let (sources, found) = read.unwrap_or_default();
            apply_import(sources, &found);
        }
    }
}

/// The extensions to offer once an import is done: those found in each browser profile imported,
/// but not yet in the profile it went into, with the icon in the other browser's folder.
fn offered_extensions(targets: &[(Choice, ProfileId)], found: &Found) -> Vec<serde_json::Value> {
    let mut seen = HashSet::new();
    let mut offered = Vec::new();
    for (choice, profile) in targets {
        let installed = extensions::list(*profile);
        let list = found
            .get(&(choice.browser, choice.profile))
            .into_iter()
            .flatten();
        for extension in list {
            let new = !installed.iter().any(|e| e.id == extension.id);
            if new && seen.insert((*profile, extension.id.clone())) {
                offered.push(json!({
                    "profile": profile,
                    "id": extension.id,
                    "name": extension.name,
                    "icon": extension.icon,
                }));
            }
        }
    }
    offered
}

/// Adds the browser profiles read to the sidebar as it is now, then shows the first Space
/// imported with its first tab, or a blank page when it has none. Their histories are merged on
/// a thread of their own.
fn apply_import(sources: Vec<import::Source>, found: &Found) {
    IMPORTING.set(false);
    let (imported, shown, spaces) = update_sidebar(|sidebar| {
        let mut new = if is_untouched(sidebar) {
            Sidebar::empty()
        } else {
            sidebar.clone()
        };
        let before = new.spaces().len();
        let imported = import::apply(&mut new, sources);
        // Before the sidebar shows them; a few kilobytes each.
        for (profile, favicons) in &imported.favicons {
            for (source, png) in favicons {
                if let Err(error) = favicon::write(&profile_dir(*profile), source, png) {
                    eprintln!("cannot cache the favicon: {error}");
                }
            }
        }
        let shown = new.spaces().get(before).map(|space| space.id);
        if let Some(space) = shown {
            new.display(space).expect("an imported Space");
            if let Some(tab) = new.next_tab(space, true) {
                new.select(space, tab).expect("a tab of the Space");
            }
        }
        let spaces: Vec<_> = new.spaces()[before..].iter().map(|s| s.id).collect();
        // Nothing was imported into an empty sidebar: it keeps the default one.
        if !new.spaces().is_empty() {
            *sidebar = new;
        }
        (imported, shown, spaces)
    });
    for error in &imported.errors {
        eprintln!("cannot import {error}");
    }
    let pages = imported.histories.iter().map(|(_, entries)| entries.len());
    let pages: usize = pages.sum();
    let offered = offered_extensions(&imported.targets, found);
    let histories = imported.histories.into_iter();
    let histories: Vec<_> = histories
        .map(|(profile, entries)| (profile_dir(profile), entries))
        .collect();
    thread::spawn(move || {
        for (dir, entries) in histories {
            if let Err(error) = merge_history(&dir, &entries) {
                eprintln!("cannot merge the history: {error}");
            }
        }
    });
    // The assistant goes on with the extensions found, to add or not.
    let assistant =
        IMPORT.with_borrow(|import| import.as_ref().is_some_and(|i| i.is_visible() == 1));
    if assistant && !offered.is_empty() {
        let detail = json!({ "extensions": offered });
        IMPORT_VIEW.with(|view| dispatch(view, "arbalete:extensions", &detail.to_string()));
    } else {
        close_import();
    }
    if shown.is_some() {
        show_selected();
    }
    // Last: opening the first Space imported holds the main thread for a moment, which would
    // freeze the welcome flow midway to its next step.
    send_imported(&spaces, pages, &offered);
}

/// Merges entries into the history of the profile in `dir`, with a connection of its own: the
/// browsing's may write to it meanwhile.
fn merge_history(dir: &Path, entries: &[history::Entry]) -> Result<(), String> {
    // An imported profile has no directory before its pages first load.
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let history = History::open(&dir.join(history::FILE_NAME));
    let mut history = history.map_err(|error| error.to_string())?;
    history.merge(entries).map_err(|error| error.to_string())
}

pub(super) fn open_full_disk_access() {
    if cfg!(target_os = "macos")
        && let Err(error) = std::process::Command::new("open")
            .arg(FULL_DISK_ACCESS)
            .status()
    {
        eprintln!("cannot open System Settings: {error}");
    }
}

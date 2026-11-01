mod command;
mod downloads;
mod extensions;
#[cfg(target_os = "macos")]
mod mac;
mod menu;
mod motion;
#[cfg(target_os = "macos")]
mod native;
mod scheme;
mod settings;
mod shell;
#[cfg(target_os = "macos")]
mod swipe;

use cef::*;
use std::path::PathBuf;

fn main() {
    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), false);
        assert!(loader.load());
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    #[cfg(target_os = "macos")]
    mac::setup_application();

    let args = args::Args::new();
    if execute_process(Some(args.as_main_args()), None, std::ptr::null_mut()) >= 0 {
        return;
    }

    let root = data_dir();
    // The global context, under `Default`, serves only the app's own UI: each profile's pages use
    // their own context (app/src/shell.rs).
    let settings = Settings {
        root_cache_path: CefString::from(root.to_str().unwrap()),
        cache_path: CefString::from(root.join("Default").to_str().unwrap()),
        // The extensions' bridge runs in a page without a window (shell::make_bridge).
        windowless_rendering_enabled: 1,
        ..Default::default()
    };
    let mut app = shell::ArbaleteApp::new(root.clone());
    assert_eq!(
        initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut()
        ),
        1
    );
    #[cfg(target_os = "macos")]
    mac::setup_menu();
    #[cfg(target_os = "macos")]
    mac::watch_events();
    #[cfg(target_os = "macos")]
    mac::watch_links();

    run_message_loop();
    shutdown();
}

/// Where Arbalete keeps its data, resolved because CEF compares paths after resolving symlinks.
/// `ARBALETE_DATA_DIR` overrides it, to run the app on another session (benchmarks, tests).
fn data_dir() -> PathBuf {
    let dir = std::env::var_os("ARBALETE_DATA_DIR").map_or_else(default_data_dir, PathBuf::from);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn default_data_dir() -> PathBuf {
    let home = home();
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Arbalete")
    } else if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("Arbalete")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map_or_else(|| home.join(".local/share"), PathBuf::from)
            .join("arbalete")
    }
}

/// The user's home directory: its browsers are imported, and its Downloads folder receives the
/// downloads. `ARBALETE_HOME` overrides it, so that tests import their own data and download into
/// their own folder, never the machine's.
fn user_home() -> PathBuf {
    std::env::var_os("ARBALETE_HOME").map_or_else(home, PathBuf::from)
}

fn home() -> PathBuf {
    PathBuf::from(
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .unwrap(),
    )
}

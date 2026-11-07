//! Chrome extensions in Arbalete's Alloy pages (decision 0001): their content scripts and their
//! service workers run, but Chrome's tab and window APIs find no window, as the pages sit in none.
//! A DevTools session the app opens on a page of its own sees every target of the browser: it
//! attaches to each extension's service worker as it starts and injects a shim
//! (extension-shim.js), which answers with the profile's active tab and asks Arbalete to create or
//! update tabs. A helper extension (app/extension), loaded in every profile, tells each page's tab
//! id, which extensions know pages by.

use arbalete_core::sidebar::ProfileId;
use cef::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The helper extension's directory in the bundle, if it is there: `cef` builds outside the
/// bundle run without it.
pub fn helper_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?.join("Resources/arbalete-helper");
    dir.join("manifest.json").exists().then_some(dir)
}

/// An extension the user installed in a profile, as its button shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Installed {
    pub id: String,
    pub name: String,
    /// Its button's popup, a path in the extension, if it has one.
    #[serde(skip)]
    pub popup: Option<String>,
    /// Its button's icon file.
    #[serde(skip)]
    pub icon: Option<PathBuf>,
    /// Its options page, a path in the extension, if it has one.
    #[serde(skip)]
    pub options: Option<String>,
    /// Its options page opens in a tab rather than a window: its manifest asks for it, or it
    /// predates `options_ui`.
    #[serde(skip)]
    pub options_in_tab: bool,
    /// Turned off by the user (or by Chromium), it stays installed but does not run.
    pub enabled: bool,
    pub description: String,
    pub version: String,
    /// What the library details, like Chromium's extensions page.
    #[serde(skip)]
    pub details: Details,
}

/// An extension's details, from its manifest and from Chromium's preferences.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Details {
    /// The browser APIs it asks for, like `storage` or `tabs`.
    pub permissions: Vec<String>,
    /// The sites it may read and change, like `<all_urls>`.
    pub sites: Vec<String>,
    /// Those of its sites its permissions grant, which its content scripts' do not count in.
    pub hosts: Vec<String>,
    pub homepage: Option<String>,
    pub manifest_version: u64,
    /// Installed from the Chrome Web Store.
    pub from_store: bool,
    /// Loaded from a folder of the user's, in developer mode: its path.
    pub unpacked: Option<PathBuf>,
    /// When it was installed and last updated, in milliseconds since the Unix epoch.
    pub installed_at: Option<u64>,
    pub updated_at: Option<u64>,
}

/// A time Chromium's preferences hold, in microseconds since 1601 as text, in milliseconds since
/// the Unix epoch.
fn preference_time(value: &Value) -> Option<u64> {
    const UNIX_EPOCH_SINCE_1601_MICROS: u64 = 11_644_473_600_000_000;
    let micros: u64 = value.as_str()?.parse().ok()?;
    Some(micros.checked_sub(UNIX_EPOCH_SINCE_1601_MICROS)? / 1000)
}

/// Where Chromium installs extensions, as its preferences record it: from the Web Store or
/// elsewhere into the profile (1), unpacked by the user (4), or by other programs (6, 7). Its own
/// components (5) and extensions loaded from the command line (8), like the helper, are not the
/// user's.
const USER_LOCATIONS: [i64; 4] = [1, 4, 6, 7];

/// The extensions installed in a profile's directory, enabled or not, by name.
pub fn installed(profile_dir: &Path) -> Vec<Installed> {
    let preferences = std::fs::read(profile_dir.join("Secure Preferences")).ok();
    let preferences: Value = preferences
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let Some(settings) = preferences["extensions"]["settings"].as_object() else {
        return Vec::new();
    };
    let mut list: Vec<_> = settings
        .iter()
        .filter(|(_, setting)| {
            let location = setting["location"].as_i64().unwrap_or_default();
            USER_LOCATIONS.contains(&location)
        })
        .filter_map(|(id, setting)| {
            let path = Path::new(setting["path"].as_str()?);
            let dir = if path.is_absolute() {
                path.to_path_buf()
            } else {
                profile_dir.join("Extensions").join(path)
            };
            let disabled = setting["disable_reasons"]
                .as_array()
                .is_some_and(|reasons| !reasons.is_empty());
            let extension = read_manifest(id, &dir)?;
            let details = Details {
                from_store: setting["from_webstore"].as_bool().unwrap_or_default(),
                unpacked: (setting["location"] == 4).then(|| dir.clone()),
                installed_at: preference_time(&setting["first_install_time"]),
                updated_at: preference_time(&setting["last_update_time"]),
                ..extension.details
            };
            Some(Installed {
                enabled: !disabled,
                details,
                ..extension
            })
        })
        .collect();
    list.sort_by_key(|extension| extension.name.to_lowercase());
    list
}

/// An extension's name, popup and icon, from its manifest in `dir`.
fn read_manifest(id: &str, dir: &Path) -> Option<Installed> {
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).ok()?).ok()?;
    // Manifest V2 extensions have a browser action instead.
    let action = if manifest["action"].is_object() {
        &manifest["action"]
    } else {
        &manifest["browser_action"]
    };
    let icon = largest_icon(&action["default_icon"]).or_else(|| largest_icon(&manifest["icons"]));
    Some(Installed {
        id: id.to_string(),
        name: localized(dir, &manifest, manifest["name"].as_str()?),
        popup: action["default_popup"]
            .as_str()
            .filter(|popup| !popup.is_empty())
            .map(str::to_string),
        icon: icon.map(|icon| dir.join(icon.trim_start_matches('/'))),
        options: manifest["options_ui"]["page"]
            .as_str()
            .or(manifest["options_page"].as_str())
            .map(str::to_string),
        options_in_tab: manifest["options_ui"]["open_in_tab"] == true
            || manifest["options_ui"]["page"].is_null(),
        enabled: true,
        description: localized(
            dir,
            &manifest,
            manifest["description"].as_str().unwrap_or_default(),
        ),
        version: manifest["version"].as_str().unwrap_or_default().to_string(),
        details: manifest_details(&manifest),
    })
}

/// What an extension's manifest says it asks for: the browser APIs, and the sites its permissions
/// and content scripts reach, each once.
fn manifest_details(manifest: &Value) -> Details {
    let strings = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    };
    let is_site = |entry: &String| entry.contains("://") || entry == "<all_urls>";
    // Manifest V2 lists the sites among its permissions.
    let (mut hosts, permissions): (Vec<_>, Vec<_>) = strings(&manifest["permissions"])
        .into_iter()
        .partition(is_site);
    hosts.extend(strings(&manifest["host_permissions"]));
    let mut sites = hosts.clone();
    let scripts = manifest["content_scripts"].as_array().into_iter().flatten();
    sites.extend(scripts.flat_map(|script| strings(&script["matches"])));
    let mut unique = Vec::new();
    for site in sites {
        if !unique.contains(&site) {
            unique.push(site);
        }
    }
    Details {
        permissions,
        sites: unique,
        hosts,
        homepage: manifest["homepage_url"].as_str().map(str::to_string),
        manifest_version: manifest["manifest_version"].as_u64().unwrap_or(2),
        ..Details::default()
    }
}

/// The path of an icon, or of the largest of several by size.
fn largest_icon(icons: &Value) -> Option<String> {
    if let Some(icon) = icons.as_str() {
        return Some(icon.to_string());
    }
    let icons = icons.as_object()?;
    let largest = icons
        .iter()
        .max_by_key(|(size, _)| size.parse::<u32>().unwrap_or_default());
    largest
        .and_then(|(_, icon)| icon.as_str())
        .map(str::to_string)
}

/// A manifest's text, its `__MSG_<key>__` placeholders read from the extension's default
/// locale's messages, whose keys ignore case.
fn localized(dir: &Path, manifest: &Value, text: &str) -> String {
    let Some(key) = text
        .strip_prefix("__MSG_")
        .and_then(|key| key.strip_suffix("__"))
    else {
        return text.to_string();
    };
    let locale = manifest["default_locale"].as_str().unwrap_or("en");
    let messages = std::fs::read(dir.join("_locales").join(locale).join("messages.json")).ok();
    let messages: Value = messages
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let message = messages.as_object().and_then(|messages| {
        let (_, message) = messages
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))?;
        message["message"].as_str()
    });
    message.unwrap_or(text).to_string()
}

/// The helper extension's service worker, which every extension url is told apart from.
const HELPER_SCRIPT: &str = "/arbalete-helper.js";

const SHIM: &str = include_str!("extension-shim.js");

/// The selected tab of a profile, as extensions see it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ActiveTab {
    /// Its page's browser, once created.
    pub page: Option<i32>,
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
    pub loading: bool,
}

/// A tab's page: its DevTools target, its browser context and, once the helper has told it, its
/// tab id.
struct Content {
    target: String,
    context: String,
    tab_id: Option<i64>,
}

/// What a message sent in the session answers.
enum Pending {
    /// Attaching to an extension's service worker, in a browser context.
    Attach {
        context: String,
        extension: String,
        helper: bool,
    },
    /// The helper's list of targets and their tab ids, in a browser context.
    TabIds(String),
    /// The helper's list of a profile's extensions, as Chromium has them now.
    Extensions(ProfileId),
}

#[derive(Default)]
struct Bridge {
    host: Option<BrowserHost>,
    registrations: Vec<Registration>,
    next_id: i32,
    pending: HashMap<i32, Pending>,
    /// The workers attached to, by target id.
    attached: HashSet<String>,
    /// The extensions' workers' sessions, with their browser context and extension id.
    workers: HashMap<String, (String, String)>,
    /// The helper's session in each browser context.
    helpers: HashMap<String, String>,
    /// The helper's extension id, which its folder's path sets.
    helper_id: Option<String>,
    /// Each profile's browser context.
    contexts: HashMap<ProfileId, String>,
    /// The pages, by their browser's identifier.
    contents: HashMap<i32, Content>,
    /// Each page's own DevTools observer, which asks for its target.
    page_registrations: HashMap<i32, Registration>,
    /// The profiles in front, with their active tab if they have one.
    tabs: HashMap<ProfileId, Option<ActiveTab>>,
}

impl Bridge {
    /// The profile of a browser context.
    fn profile_of(&self, context: &str) -> Option<ProfileId> {
        let (profile, _) = self.contexts.iter().find(|(_, c)| *c == context)?;
        Some(*profile)
    }

    /// The tab id of a profile's active tab, once the helper has told it.
    fn active_tab_id(&self, profile: ProfileId) -> Option<i64> {
        let page = self.tabs.get(&profile)?.as_ref()?.page?;
        self.contents.get(&page)?.tab_id
    }

    /// The state one of a profile's extensions sees: its active tab, with its page's tab id, and
    /// its address, title and icon only if the extension may see them (exposes_tab_details). A
    /// popup the user opened holds `activeTab` for it, as in Chrome.
    fn state(&self, profile: ProfileId, extension: &str, popup: bool) -> Value {
        let tab = self.tabs.get(&profile).and_then(Option::as_ref);
        let details = tab.is_some_and(|tab| {
            let installed = list(profile).into_iter().find(|e| e.id == extension);
            installed.is_some_and(|installed| {
                let details = &installed.details;
                (popup && details.permissions.iter().any(|p| p == "activeTab"))
                    || exposes_tab_details(details, &tab.url)
            })
        });
        let focused = self.tabs.contains_key(&profile);
        state_json(focused, tab, self.active_tab_id(profile), details)
    }
}

thread_local! {
    static BRIDGE: RefCell<Bridge> = RefCell::default();
    /// Each profile's extensions, as their buttons show them, and its directory.
    static INSTALLED: RefCell<HashMap<ProfileId, (PathBuf, Vec<Installed>)>> = RefCell::default();
    /// The open popup's DevTools observer.
    static POPUP: RefCell<Option<Registration>> = const { RefCell::new(None) };
}

/// The extensions' icon files, which the UI loads by their profile and extension id
/// (icon_file), from the IO thread.
static ICONS: Mutex<Vec<(String, PathBuf)>> = Mutex::new(Vec::new());

/// A profile's extensions, as their buttons show them.
pub fn list(profile: ProfileId) -> Vec<Installed> {
    INSTALLED.with_borrow(|installed| {
        installed
            .get(&profile)
            .map(|(_, list)| list.clone())
            .unwrap_or_default()
    })
}

/// The UI path of an extension's icon, if it has one.
pub fn icon_path(profile: ProfileId, extension: &Installed) -> Option<String> {
    let profile = serde_json::to_string(&profile).expect("an id");
    extension
        .icon
        .as_ref()
        .map(|_| format!("extension-icon/{profile}/{}", extension.id))
}

/// The icon file a UI path names (icon_path), with its media type.
pub fn icon_file(path: &str) -> Option<(PathBuf, &'static str)> {
    let key = path.strip_prefix("extension-icon/")?;
    let icons = ICONS.lock().expect("not poisoned");
    let (_, file) = icons.iter().find(|(icon, _)| icon == key)?;
    let mime = match file.extension()?.to_str()? {
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        _ => "image/png",
    };
    Some((file.clone(), mime))
}

/// Learns an extension just installed from its service worker starting, at `url`, in a browser
/// context: Chromium writes its preferences, which list the extensions, only later. The
/// extension shows in its profile's list, and the windows Chromium opened for it close.
fn worker_started(context: &str, url: &str) {
    let Some(id) = extension_id(url) else {
        return;
    };
    let Some(profile) = BRIDGE.with_borrow(|bridge| bridge.profile_of(context)) else {
        return;
    };
    let new = INSTALLED.with_borrow(|installed| {
        let (dir, list) = installed.get(&profile)?;
        (!list.iter().any(|extension| extension.id == id)).then(|| dir.clone())
    });
    let Some(dir) = new else { return };
    let Some(extension) = read_installed(&dir, id) else {
        return;
    };
    INSTALLED.with_borrow_mut(|installed| {
        if let Some((_, list)) = installed.get_mut(&profile) {
            list.push(extension);
            list.sort_by_key(|extension| extension.name.to_lowercase());
        }
    });
    update_icons(profile);
    crate::shell::extensions_changed();
    crate::shell::extension_installed();
}

/// Turns an extension of a profile on or off, through the profile's helper.
pub fn set_enabled(profile: ProfileId, id: &str, enabled: bool) {
    let id_json = serde_json::to_string(id).expect("a string");
    helper_evaluate(
        profile,
        &format!("chrome.management.setEnabled({id_json}, {enabled})"),
    );
    INSTALLED.with_borrow_mut(|installed| {
        let list = installed.get_mut(&profile).map(|(_, list)| list);
        if let Some(extension) = list
            .into_iter()
            .flatten()
            .find(|extension| extension.id == id)
        {
            extension.enabled = enabled;
        }
    });
}

/// The address of an extension's options page, if it has one.
pub fn options_url(profile: ProfileId, id: &str) -> Option<String> {
    let extension = list(profile)
        .into_iter()
        .find(|extension| extension.id == id)?;
    Some(format!(
        "chrome-extension://{id}/{}",
        extension.options?.trim_start_matches('/')
    ))
}

/// Evaluates an expression in a profile's helper, which holds the `management` permission.
fn helper_evaluate(profile: ProfileId, expression: &str) {
    let session = BRIDGE.with_borrow(|bridge| {
        let context = bridge.contexts.get(&profile)?;
        bridge.helpers.get(context).cloned()
    });
    if let Some(session) = session {
        send_in(
            &session,
            "Runtime.evaluate",
            json!({ "expression": expression }),
            None,
        );
    }
}

/// Reads a profile's extensions again, after their page may have changed them.
pub fn reload(profile: ProfileId) {
    let dir = INSTALLED.with_borrow(|all| Some(all.get(&profile)?.0.clone()));
    if let Some(dir) = dir {
        refresh(profile, &dir);
    }
}

/// Reads a profile's extensions again: from Chromium's preferences, which list their files, then
/// as Chromium has them now, from the helper (reconcile). Chromium writes its preferences only
/// seconds after a change: alone, they would bring back an extension just removed or turned off.
fn refresh(profile: ProfileId, dir: &Path) {
    let list = installed(dir);
    INSTALLED.with_borrow_mut(|all| all.insert(profile, (dir.to_path_buf(), list)));
    update_icons(profile);
    crate::shell::extensions_changed();
    let session = BRIDGE.with_borrow(|bridge| {
        let context = bridge.contexts.get(&profile)?;
        bridge.helpers.get(context).cloned()
    });
    if let Some(session) = session {
        let expression = "chrome.management.getAll().then((all) => all.map(({ id, enabled }) => ({ id, enabled })))";
        let params =
            json!({ "expression": expression, "awaitPromise": true, "returnByValue": true });
        send_in(
            &session,
            "Runtime.evaluate",
            params,
            Some(Pending::Extensions(profile)),
        );
    }
}

/// Makes a profile's extensions those Chromium has now, `live`, as the helper lists them: the
/// removed ones go, the others are turned on or off, and those the preferences do not list yet
/// are read from their files.
fn reconcile(profile: ProfileId, live: &[Value]) {
    let Some(dir) = INSTALLED.with_borrow(|all| Some(all.get(&profile)?.0.clone())) else {
        return;
    };
    let changed = INSTALLED.with_borrow_mut(|all| {
        let Some((_, list)) = all.get_mut(&profile) else {
            return false;
        };
        let before = list.clone();
        list.retain(|extension| live.iter().any(|e| e["id"] == extension.id.as_str()));
        for extension in list.iter_mut() {
            let state = live.iter().find(|e| e["id"] == extension.id.as_str());
            extension.enabled = state.is_some_and(|e| e["enabled"] == true);
        }
        for state in live {
            let Some(id) = state["id"].as_str() else {
                continue;
            };
            if list.iter().any(|extension| extension.id == id) {
                continue;
            }
            if let Some(extension) = read_installed(&dir, id) {
                list.push(Installed {
                    enabled: state["enabled"] == true,
                    ..extension
                });
            }
        }
        list.sort_by_key(|extension| extension.name.to_lowercase());
        *list != before
    });
    if changed {
        update_icons(profile);
        crate::shell::extensions_changed();
    }
}

/// An extension installed into a profile's directory, from its files, in a directory named after
/// its version: the helper and extensions loaded from a folder are not there.
fn read_installed(dir: &Path, id: &str) -> Option<Installed> {
    let versions = std::fs::read_dir(dir.join("Extensions").join(id))
        .into_iter()
        .flatten()
        .flatten();
    versions
        .filter_map(|version| read_manifest(id, &version.path()))
        .next()
}

/// Lets the UI load a profile's extensions' icons (icon_file).
fn update_icons(profile: ProfileId) {
    let list = list(profile);
    let id = serde_json::to_string(&profile).expect("an id");
    let mut icons = ICONS.lock().expect("not poisoned");
    icons.retain(|(key, _)| !key.starts_with(&format!("{id}/")));
    icons.extend(
        list.iter().filter_map(|extension| {
            Some((format!("{id}/{}", extension.id), extension.icon.clone()?))
        }),
    );
}

/// The address of an extension's popup, if it has one.
pub fn popup_url(profile: ProfileId, id: &str) -> Option<String> {
    let extension = list(profile)
        .into_iter()
        .find(|extension| extension.id == id)?;
    Some(format!(
        "chrome-extension://{id}/{}",
        extension.popup?.trim_start_matches('/')
    ))
}

/// Prepares an extension's popup once its blank page has loaded, then loads it: the shim and its
/// binding are installed in the page before any of its scripts runs, with the profile's active
/// tab, and the page sizes the popup, like Chrome's, from 25 to 800 by 600 points.
pub fn popup_created(profile: ProfileId, browser: &Browser, url: &str) {
    let Some(host) = browser.host() else { return };
    host.set_auto_resize_enabled(
        1,
        Some(&Size {
            width: 25,
            height: 25,
        }),
        Some(&Size {
            width: 800,
            height: 600,
        }),
    );
    let mut observer = PopupObserver::new(url.to_string());
    let registration = host.add_dev_tools_message_observer(Some(&mut observer));
    POPUP.set(registration);
    let extension = extension_id(url).unwrap_or_default();
    let state = BRIDGE.with_borrow(|bridge| bridge.state(profile, extension, true));
    let source = format!("self.__arbaleteInitial = {state};\n{SHIM}");
    // New documents get the binding only while the Runtime domain is enabled, and the scripts
    // added for them only while the Page domain is.
    for message in [
        json!({ "id": 3, "method": "Runtime.enable" }),
        json!({ "id": 1, "method": "Runtime.addBinding", "params": { "name": "__arbalete" } }),
        json!({ "id": 3, "method": "Page.enable" }),
        json!({ "id": 2, "method": "Page.addScriptToEvaluateOnNewDocument", "params": { "source": source } }),
    ] {
        host.send_dev_tools_message(Some(message.to_string().as_bytes()));
    }
}

/// Forgets the closed popup's observer.
pub fn popup_closed() {
    POPUP.set(None);
}

/// Reads a profile's extensions from its directory, once its web data is ready.
pub fn profile_ready(profile: ProfileId, dir: &Path) {
    refresh(profile, dir);
}

/// Starts watching the extensions from the bridge's browser, which outlives the pages, with the
/// first page, then learns which target each page is.
pub fn page_created(profile: ProfileId, browser: &Browser) {
    let Some(host) = browser.host() else { return };
    let started = BRIDGE.with_borrow(|bridge| bridge.host.is_some());
    if !started && let Some(sidebar) = crate::shell::bridge_host() {
        let registration = sidebar.add_dev_tools_message_observer(Some(&mut BridgeObserver::new()));
        BRIDGE.with_borrow_mut(|bridge| {
            bridge.host = Some(sidebar.clone());
            bridge.registrations.extend(registration);
        });
        let params = json!({ "discover": true });
        send(
            json!({ "method": "Target.setDiscoverTargets", "params": params }),
            None,
        );
    }
    // The page's own session answers with the page's target.
    let page = browser.identifier();
    let registration =
        host.add_dev_tools_message_observer(Some(&mut ContentObserver::new(profile, page)));
    BRIDGE.with_borrow_mut(|bridge| {
        if let Some(registration) = registration {
            bridge.page_registrations.insert(page, registration);
        }
    });
    let message = json!({ "id": CONTENT_INFO, "method": "Target.getTargetInfo" });
    host.send_dev_tools_message(Some(message.to_string().as_bytes()));
}

/// Forgets a page as its browser closes.
pub fn page_closed(page: i32) {
    let registration = BRIDGE.with_borrow_mut(|bridge| {
        bridge.contents.remove(&page);
        bridge.page_registrations.remove(&page)
    });
    drop(registration);
}

/// The id of the message asking a page whether it plays media (crate::shell::page_playing).
pub const PLAYING: i32 = 2;

/// The id of the message asking a page for a picture of itself (crate::shell::page_snapshot).
pub const SNAPSHOT: i32 = 3;

/// The id of the message asking a page for the color at its top (crate::shell::page_top_color).
pub const TOP_COLOR: i32 = 4;

/// The id of the message asking a page's session for its own target, apart from the
/// bridge's, which start above it.
const CONTENT_INFO: i32 = 1;

/// Tells the extensions which profiles are in front, with their active tab, after the sidebar's
/// state changes.
pub fn sync(tabs: HashMap<ProfileId, Option<ActiveTab>>) {
    let changed = BRIDGE.with_borrow_mut(|bridge| {
        let changed: Vec<_> = bridge
            .contexts
            .iter()
            .filter(|(profile, _)| bridge.tabs.get(profile) != tabs.get(profile))
            .map(|(_, context)| context.clone())
            .collect();
        bridge.tabs = tabs;
        changed
    });
    for context in changed {
        push_state(&context);
    }
}

/// Sends a message in the bridge's session, recording first what its answer is for: CEF may
/// answer while sending, calling the observer back.
fn send(mut message: Value, pending: Option<Pending>) {
    let host = BRIDGE.with_borrow_mut(|bridge| {
        bridge.next_id += 1;
        let id = CONTENT_INFO + bridge.next_id;
        message["id"] = json!(id);
        if let Some(pending) = pending {
            bridge.pending.insert(id, pending);
        }
        bridge.host.clone()
    });
    if let Some(host) = host {
        host.send_dev_tools_message(Some(message.to_string().as_bytes()));
    }
}

/// Sends a message in an attached target's session.
fn send_in(session: &str, method: &str, params: Value, pending: Option<Pending>) {
    let message = json!({ "sessionId": session, "method": method, "params": params });
    send(message, pending);
}

/// Gives the extensions' workers of a browser context its profile's active tab, once its tab id
/// is known, each as its extension may see it.
fn push_state(context: &str) {
    let states: Vec<(String, Value)> = BRIDGE.with_borrow(|bridge| {
        let Some(profile) = bridge.profile_of(context) else {
            return Vec::new();
        };
        bridge
            .workers
            .iter()
            .filter(|(_, (worker, _))| worker == context)
            .map(|(session, (_, extension))| {
                (session.clone(), bridge.state(profile, extension, false))
            })
            .collect()
    });
    for (session, state) in states {
        let expression = format!("self.__arbaleteSync?.({state})");
        send_in(
            &session,
            "Runtime.evaluate",
            json!({ "expression": expression }),
            None,
        );
    }
}

/// The state the shim answers with: whether the profile is in front, its window then being the
/// focused one, and its active tab, if any and its tab id is known, with its address, title and
/// icon only with `details`, as Chrome leaves them out.
fn state_json(focused: bool, tab: Option<&ActiveTab>, tab_id: Option<i64>, details: bool) -> Value {
    let tabs: Vec<Value> = match (tab, tab_id) {
        (Some(tab), Some(id)) => {
            let mut tab_json = json!({
                "id": id,
                "index": 0,
                "windowId": 1,
                "active": true,
                "highlighted": true,
                "pinned": false,
                "incognito": false,
                "status": if tab.loading { "loading" } else { "complete" },
                "url": tab.url,
                "title": tab.title,
                "favIconUrl": tab.favicon,
                "discarded": false,
                "autoDiscardable": true,
                "groupId": -1,
            });
            if !details && let Some(fields) = tab_json.as_object_mut() {
                for field in ["url", "title", "favIconUrl"] {
                    fields.remove(field);
                }
            }
            vec![tab_json]
        }
        _ => Vec::new(),
    };
    json!({ "tabs": tabs, "focused": focused })
}

/// Whether an extension may see the address, title and icon of a tab at `url`, as in Chrome: it
/// holds the `tabs` permission, or a host permission matching the address.
fn exposes_tab_details(details: &Details, url: &str) -> bool {
    details.permissions.iter().any(|p| p == "tabs")
        || details.hosts.iter().any(|host| pattern_matches(host, url))
}

/// Whether a match pattern, like `https://*.example.com/*` or `<all_urls>`, matches `url`: its
/// scheme (`*` for http and https), its host (`*` for any, `*.` for a domain and its
/// subdomains), its port if it names one, and its path and query, where `*` matches anything.
fn pattern_matches(pattern: &str, url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let scheme = scheme.to_ascii_lowercase();
    if pattern == "<all_urls>" {
        return ["http", "https", "ws", "wss", "ftp", "file"].contains(&scheme.as_str());
    }
    let Some((pattern_scheme, pattern_rest)) = pattern.split_once("://") else {
        return false;
    };
    let scheme_matches = match pattern_scheme {
        "*" => scheme == "http" || scheme == "https",
        _ => pattern_scheme.eq_ignore_ascii_case(&scheme),
    };
    // A pattern has a path, at least `/`.
    let Some(slash) = pattern_rest.find('/') else {
        return false;
    };
    let (pattern_authority, pattern_path) = pattern_rest.split_at(slash);
    let rest = rest.split('#').next().unwrap_or_default();
    let (authority, path) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let authority = authority.rsplit('@').next().unwrap_or_default();
    let (host, port) = host_and_port(authority);
    let (pattern_host, pattern_port) = host_and_port(pattern_authority);
    let host = host.to_ascii_lowercase();
    let pattern_host = pattern_host.to_ascii_lowercase();
    let host_matches = match pattern_host.strip_prefix("*") {
        Some("") => true,
        Some(domain) => domain
            .strip_prefix('.')
            .is_some_and(|domain| host == domain || host.ends_with(&format!(".{domain}"))),
        None => host == pattern_host,
    };
    let port_matches = pattern_port.is_none_or(|p| p == "*" || Some(p) == port);
    scheme_matches && host_matches && port_matches && glob_matches(pattern_path, &path)
}

/// An authority's host and port, if it names one: `[::1]:8080` has the host `[::1]`.
fn host_and_port(authority: &str) -> (&str, Option<&str>) {
    match authority.rsplit_once(':') {
        Some((host, port)) if !port.contains(']') => (host, Some(port)),
        _ => (authority, None),
    }
}

/// Whether `text` matches `glob`, where `*` matches any run of characters.
fn glob_matches(glob: &str, text: &str) -> bool {
    let mut parts = glob.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<_> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        return rest.is_empty();
    };
    for part in middle {
        let Some(at) = rest.find(part) else {
            return false;
        };
        rest = &rest[at + part.len()..];
    }
    rest.ends_with(last)
}

/// Whether a target is an extension's service worker, and the helper's.
fn worker_kind(target: &Value) -> Option<bool> {
    let url = target["url"].as_str()?;
    (target["type"] == "service_worker" && url.starts_with("chrome-extension://"))
        .then(|| url.ends_with(HELPER_SCRIPT))
}

/// Attaches to an extension's service worker as it starts.
fn target_seen(target: &Value) {
    let (Some(helper), Some(id)) = (worker_kind(target), target["targetId"].as_str()) else {
        return;
    };
    let context = target["browserContextId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if !BRIDGE.with_borrow_mut(|bridge| bridge.attached.insert(id.to_string())) {
        return;
    }
    let url = target["url"].as_str().unwrap_or_default();
    let extension = extension_id(url).unwrap_or_default().to_string();
    if helper {
        BRIDGE.with_borrow_mut(|bridge| bridge.helper_id = Some(extension.clone()));
    } else {
        worker_started(&context, url);
    }
    let params = json!({ "targetId": id, "flatten": true });
    let message = json!({ "method": "Target.attachToTarget", "params": params });
    let pending = Pending::Attach {
        context,
        extension,
        helper,
    };
    send(message, Some(pending));
}

/// Shims an extension's worker once attached, or asks the helper for the tab ids.
fn attached(session: &str, context: String, extension: String, helper: bool) {
    if helper {
        BRIDGE
            .with_borrow_mut(|bridge| bridge.helpers.insert(context.clone(), session.to_string()));
        ask_tab_ids(&context);
        return;
    }
    BRIDGE.with_borrow_mut(|bridge| {
        bridge
            .workers
            .insert(session.to_string(), (context, extension))
    });
    // A worker that stops and starts again keeps its session, with no new target: the binding
    // stays only while the Runtime domain is enabled. The session reports the running script,
    // and each new start's, once loaded (script_loaded).
    send_in(
        session,
        "Runtime.addBinding",
        json!({ "name": "__arbalete" }),
        None,
    );
    send_in(session, "Runtime.enable", json!({}), None);
}

/// Shims a worker once its script has loaded, then gives it its profile's active tab: as its
/// execution context is created, a worker starting has no `chrome` nor `setTimeout` yet.
fn script_loaded(session: &str) {
    let Some(context) =
        BRIDGE.with_borrow(|bridge| bridge.workers.get(session).map(|(c, _)| c.clone()))
    else {
        return;
    };
    send_in(
        session,
        "Runtime.evaluate",
        json!({ "expression": SHIM }),
        None,
    );
    push_state(&context);
}

/// Asks the helper of a browser context which tab id each page has.
fn ask_tab_ids(context: &str) {
    let Some(session) = BRIDGE.with_borrow(|bridge| bridge.helpers.get(context).cloned()) else {
        return;
    };
    let params = json!({ "expression": "self.arbaleteTargets()", "awaitPromise": true, "returnByValue": true });
    let pending = Pending::TabIds(context.to_string());
    send_in(&session, "Runtime.evaluate", params, Some(pending));
}

/// Records the tab id of the pages of a browser context.
fn tab_ids(context: &str, targets: &[Value]) {
    BRIDGE.with_borrow_mut(|bridge| {
        for content in bridge
            .contents
            .values_mut()
            .filter(|c| c.context == context)
        {
            let target = targets.iter().find(|t| t["id"] == content.target.as_str());
            content.tab_id = target.and_then(|t| t["tabId"].as_i64());
        }
    });
    push_state(context);
}

/// Where a shim's call came from, to answer it there: a service worker's session in the
/// bridge, or a popup's browser, with its extension's id.
#[derive(Clone)]
enum Caller {
    Worker(String),
    Popup(BrowserHost, String),
}

impl Caller {
    /// Evaluates an expression where the call came from.
    fn evaluate(&self, expression: &str) {
        let params = json!({ "expression": expression });
        match self {
            Self::Worker(session) => send_in(session, "Runtime.evaluate", params, None),
            Self::Popup(host, _) => {
                let message = json!({ "id": 0, "method": "Runtime.evaluate", "params": params });
                host.send_dev_tools_message(Some(message.to_string().as_bytes()));
            }
        }
    }

    /// The profile the caller runs in.
    fn profile(&self) -> Option<ProfileId> {
        match self {
            Self::Worker(session) => {
                BRIDGE.with_borrow(|bridge| bridge.profile_of(&bridge.workers.get(session)?.0))
            }
            // Popups open in the displayed Space's profile.
            Self::Popup(..) => Some(crate::shell::displayed_profile()),
        }
    }

    /// The id of the caller's extension, from its own worker or page, never from the call.
    fn extension(&self) -> Option<String> {
        match self {
            Self::Worker(session) => {
                BRIDGE.with_borrow(|bridge| Some(bridge.workers.get(session)?.1.clone()))
            }
            Self::Popup(_, extension) => Some(extension.clone()),
        }
    }

    /// The tab id the caller's shim gives the active tab, if it knows one.
    fn active_tab_id(&self) -> Option<i64> {
        let profile = self.profile()?;
        BRIDGE.with_borrow(|bridge| bridge.active_tab_id(profile))
    }
}

/// The extension id of a `chrome-extension://` URL.
fn extension_id(url: &str) -> Option<&str> {
    url.strip_prefix("chrome-extension://")?
        .split('/')
        .next()
        .filter(|id| !id.is_empty())
}

/// Whether an extension may open `url` in a tab: only a web page or a blank one, never a
/// `javascript:` URL, which would run in the active page, nor a local file or a Chrome page.
fn extension_may_open(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.starts_with("http:") || url.starts_with("https:") || url == "about:blank"
}

/// What Chromium asks the user about in the app's Chrome-style window, where only it can: an
/// extension's request for optional permissions, or removing an extension, which Chromium always
/// confirms when another extension, the helper, asks for it.
enum ChromeTask {
    Permissions {
        caller: Caller,
        /// The shim's number for the request, which its answer names.
        request: i64,
        permissions: Value,
    },
    Uninstall {
        profile: ProfileId,
        id: String,
    },
}

thread_local! {
    static CHROME_TASK: RefCell<Option<ChromeTask>> = const { RefCell::new(None) };
    /// The window's observer, kept until the window closes: dropped from its own callback, where
    /// the answer comes, it would free itself while running.
    static CHROME_TASK_OBSERVER: RefCell<Option<Registration>> = const { RefCell::new(None) };
}

/// Asks the user for the optional permissions an extension requests. Chromium only prompts for
/// them from a Chrome-style page: the app opens one on the extension's manifest, a page of the
/// extension that runs none of its code, and asks there (chrome_task_page_loaded). One task at a
/// time: a request meanwhile is refused.
fn request_permissions(caller: Caller, call: &Value) {
    let (Some(request), Some(extension)) = (call["request"].as_i64(), caller.extension()) else {
        return;
    };
    let busy = CHROME_TASK.with_borrow(Option::is_some);
    let Some(profile) = caller.profile().filter(|_| !busy) else {
        caller.evaluate(&format!("self.__arbaleteAnswer({request}, false)"));
        return;
    };
    let permissions = call["permissions"].clone();
    CHROME_TASK.set(Some(ChromeTask::Permissions {
        caller,
        request,
        permissions,
    }));
    let url = format!("chrome-extension://{extension}/manifest.json");
    crate::shell::open_chrome_task_window(profile, &url, "Extension Permissions", false);
}

/// Whether a profile's extension can remove itself, at the app's asking: turned on, its pages
/// run. It then asks no confirmation, which the app's own stands for.
pub fn removes_itself(profile: ProfileId, id: &str) -> bool {
    list(profile)
        .iter()
        .any(|extension| extension.id == id && extension.enabled)
}

/// Removes an extension from a profile. Turned on, it removes itself, which Chromium confirms
/// nothing for, from one of its own pages, its manifest, in a hidden view (removes_itself).
/// Otherwise, as it runs no page, the helper removes it, which holds the `management`
/// permission, once the user confirms it in Chromium's dialog: Chromium always asks when an
/// extension removes another.
pub fn uninstall(profile: ProfileId, id: &str) {
    if removes_itself(profile, id) {
        let expression = "chrome.management.uninstallSelf({ showConfirmDialog: false })";
        let url = format!("chrome-extension://{id}/manifest.json");
        crate::shell::run_in_hidden_page(profile, &url, expression);
        removed(profile, id);
        return;
    }
    let Some(helper) = BRIDGE.with_borrow(|bridge| bridge.helper_id.clone()) else {
        return;
    };
    if CHROME_TASK.with_borrow(Option::is_some) {
        return;
    }
    CHROME_TASK.set(Some(ChromeTask::Uninstall {
        profile,
        id: id.to_string(),
    }));
    let url = format!("chrome-extension://{helper}/manifest.json");
    crate::shell::open_chrome_task_window(profile, &url, "Remove Extension", false);
}

/// Asks Chromium, with a user gesture, once the window's page has loaded: Chromium shows its
/// prompt in it.
pub fn chrome_task_page_loaded(browser: &Browser) {
    let Some(host) = browser.host() else { return };
    let action = CHROME_TASK.with_borrow(|task| match task.as_ref()? {
        ChromeTask::Permissions { permissions, .. } => {
            Some(format!("chrome.permissions.request({permissions})"))
        }
        ChromeTask::Uninstall { id, .. } => {
            let id = serde_json::to_string(id).expect("a string");
            Some(format!(
                "chrome.management.uninstall({id}).then(() => true, () => false)"
            ))
        }
    });
    let Some(action) = action else { return };
    let registration = host.add_dev_tools_message_observer(Some(&mut ChromeTaskObserver::new()));
    CHROME_TASK_OBSERVER.set(registration);
    // The manifest's text would show under the prompt.
    let expression = format!("document.body.textContent = ''; {action}");
    let params = json!({ "expression": expression, "awaitPromise": true, "userGesture": true, "returnByValue": true });
    let message = json!({ "id": 1, "method": "Runtime.evaluate", "params": params });
    host.send_dev_tools_message(Some(message.to_string().as_bytes()));
}

/// Ends the task with the user's answer, `value`, and closes the window.
fn finish_chrome_task(value: &Value) {
    let yes = *value == true;
    match CHROME_TASK.take() {
        Some(ChromeTask::Permissions {
            caller, request, ..
        }) => {
            caller.evaluate(&format!("self.__arbaleteAnswer({request}, {yes})"));
        }
        Some(ChromeTask::Uninstall { profile, id }) if yes => removed(profile, &id),
        Some(ChromeTask::Uninstall { .. }) | None => {}
    }
    crate::shell::close_chrome_task_window();
}

/// Forgets a profile's extension just removed.
fn removed(profile: ProfileId, id: &str) {
    INSTALLED.with_borrow_mut(|installed| {
        if let Some((_, list)) = installed.get_mut(&profile) {
            list.retain(|extension| extension.id != id);
        }
    });
    update_icons(profile);
    crate::shell::extension_removed(profile, id);
}

/// The window closed: an unanswered prompt is a no.
pub fn chrome_task_window_closed() {
    finish_chrome_task(&Value::Null);
    CHROME_TASK_OBSERVER.set(None);
}

wrap_dev_tools_message_observer! {
    struct ChromeTaskObserver {}

    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> i32 {
            if let Some(message) = parse(message).filter(|m| m["id"] == 1) {
                finish_chrome_task(&message["result"]["result"]["value"]);
            }
            0
        }
    }
}

/// Runs what an extension's shim asks for, through its binding: in a popup, closing it too.
fn binding_called(payload: &str, caller: Caller) {
    let Ok(call) = serde_json::from_str::<Value>(payload) else {
        return;
    };
    if call["method"] == "close" {
        crate::shell::post(crate::command::Command::CloseExtensionPopup);
        return;
    }
    if call["method"] == "permissions" {
        request_permissions(caller, &call);
        return;
    }
    let Some(url) = call["props"]["url"]
        .as_str()
        .filter(|url| extension_may_open(url))
    else {
        return;
    };
    match call["method"].as_str() {
        Some("create") => crate::shell::run(crate::command::Command::Open(url.to_string())),
        // The shim shows only the active tab: another tab's update is not for it.
        Some("update")
            if call["tabId"].is_null() || call["tabId"].as_i64() == caller.active_tab_id() =>
        {
            crate::shell::run(crate::command::Command::Navigate(url.to_string()));
        }
        _ => {}
    }
}

fn message_received(message: &Value) {
    match message["method"].as_str() {
        Some("Target.targetCreated" | "Target.targetInfoChanged") => {
            target_seen(&message["params"]["targetInfo"]);
        }
        Some("Target.targetDestroyed") => {
            if let Some(id) = message["params"]["targetId"].as_str() {
                BRIDGE.with_borrow_mut(|bridge| bridge.attached.remove(id));
            }
        }
        Some("Target.detachedFromTarget") => {
            if let Some(session) = message["params"]["sessionId"].as_str() {
                BRIDGE.with_borrow_mut(|bridge| {
                    bridge.workers.remove(session);
                    bridge.helpers.retain(|_, helper| helper != session);
                });
            }
        }
        Some("Inspector.workerScriptLoaded") => {
            if let Some(session) = message["sessionId"].as_str() {
                script_loaded(session);
            }
        }
        Some("Runtime.bindingCalled") if message["params"]["name"] == "__arbalete" => {
            let caller = Caller::Worker(
                message["sessionId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            );
            binding_called(
                message["params"]["payload"].as_str().unwrap_or_default(),
                caller,
            );
        }
        _ => {}
    }
    let Some(id) = message["id"].as_i64().and_then(|id| i32::try_from(id).ok()) else {
        return;
    };
    match BRIDGE.with_borrow_mut(|bridge| bridge.pending.remove(&id)) {
        Some(Pending::Attach {
            context,
            extension,
            helper,
        }) => {
            if let Some(session) = message["result"]["sessionId"].as_str() {
                attached(session, context, extension, helper);
            }
        }
        Some(Pending::TabIds(context)) => {
            let targets = message["result"]["result"]["value"].as_array().cloned();
            tab_ids(&context, &targets.unwrap_or_default());
        }
        Some(Pending::Extensions(profile)) => {
            if let Some(live) = message["result"]["result"]["value"].as_array() {
                reconcile(profile, live);
            }
        }
        None => {}
    }
}

/// Records a page's target, from its own session's answer.
fn content_info(profile: ProfileId, page: i32, message: &Value) {
    let info = &message["result"]["targetInfo"];
    let (Some(target), Some(context)) =
        (info["targetId"].as_str(), info["browserContextId"].as_str())
    else {
        return;
    };
    let content = Content {
        target: target.to_string(),
        context: context.to_string(),
        tab_id: None,
    };
    BRIDGE.with_borrow_mut(|bridge| {
        bridge.contexts.insert(profile, context.to_string());
        // A page closed meanwhile is not recorded.
        if bridge.page_registrations.contains_key(&page) {
            bridge.contents.insert(page, content);
        }
    });
    ask_tab_ids(context);
}

fn parse(message: Option<&[u8]>) -> Option<Value> {
    serde_json::from_slice(message?).ok()
}

wrap_dev_tools_message_observer! {
    struct BridgeObserver {}

    impl DevToolsMessageObserver {
        // Not handled here: CEF still dispatches the message to the other observers.
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> i32 {
            if let Some(message) = parse(message) {
                message_received(&message);
            }
            0
        }
    }
}

wrap_dev_tools_message_observer! {
    struct PopupObserver {
        url: String,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, browser: Option<&mut Browser>, message: Option<&[u8]>) -> i32 {
            let Some(message) = parse(message) else { return 0 };
            // Only the extension's own page may call: not a web page the popup went to.
            if message["method"] == "Runtime.bindingCalled"
                && let Some(extension) = extension_id(&self.url)
                && let Some(frame) = browser.as_deref().and_then(Browser::main_frame)
                && CefString::from(&frame.url()).to_string().starts_with(&format!("chrome-extension://{extension}/"))
                && let Some(host) = browser.as_deref().and_then(Browser::host)
            {
                let caller = Caller::Popup(host, extension.to_string());
                binding_called(message["params"]["payload"].as_str().unwrap_or_default(), caller);
            }
            // The shim is added for new documents: the popup's may load.
            if message["id"] == 2
                && let Some(frame) = browser.and_then(|browser| browser.main_frame())
            {
                frame.load_url(Some(&CefString::from(self.url.as_str())));
            }
            0
        }
    }
}

wrap_dev_tools_message_observer! {
    struct ContentObserver {
        profile: ProfileId,
        page: i32,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> i32 {
            let Some(message) = parse(message).filter(|m| m["sessionId"].is_null()) else { return 0 };
            if message["id"] == CONTENT_INFO {
                content_info(self.profile, self.page, &message);
            } else if message["id"] == PLAYING {
                let playing = message["result"]["result"]["value"].as_bool().unwrap_or(false);
                crate::shell::page_playing(self.page, playing);
            } else if message["id"] == SNAPSHOT
                && let Some(jpeg) = message["result"]["data"].as_str()
            {
                crate::shell::page_snapshot(self.page, jpeg);
            } else if message["id"] == TOP_COLOR
                && let Some(color) = message["result"]["result"]["value"].as_str()
            {
                crate::shell::page_top_color(self.page, color);
            }
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of its own under the system's temporary directory, removed on drop.
    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("arbalete-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, path: &str, content: &str) {
            let path = self.0.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn lists_the_extensions_the_user_installed() {
        let profile = TestDir::new("extensions");
        let preferences = json!({ "extensions": { "settings": {
            "dark": { "location": 1, "path": "dark/1.0_0", "disable_reasons": [], "from_webstore": true, "first_install_time": "13437699171529348" },
            "block": { "location": 1, "path": "block/2.0_0", "disable_reasons": [] },
            "off": { "location": 1, "path": "off/1.0_0", "disable_reasons": [1] },
            "pdf": { "location": 5, "path": "/framework/pdf" },
            "helper": { "location": 8, "path": "/bundle/arbalete-helper" },
        } } });
        profile.write("Secure Preferences", &preferences.to_string());
        profile.write(
            "Extensions/dark/1.0_0/manifest.json",
            r#"{ "name": "Dark Reader", "version": "4.9", "manifest_version": 3, "permissions": ["storage", "*://*/*"], "content_scripts": [{ "matches": ["<all_urls>"] }], "action": { "default_popup": "ui/popup.html", "default_icon": { "19": "i19.png", "38": "i38.png" } } }"#,
        );
        profile.write(
            "Extensions/block/2.0_0/manifest.json",
            r#"{ "name": "__MSG_extName__", "default_locale": "en", "icons": { "16": "/i16.png", "128": "i128.png" }, "options_ui": { "page": "options.html" } }"#,
        );
        profile.write(
            "Extensions/block/2.0_0/_locales/en/messages.json",
            r#"{ "extname": { "message": "uBlock Origin Lite" } }"#,
        );
        profile.write("Extensions/off/1.0_0/manifest.json", r#"{ "name": "Off" }"#);
        let list = installed(&profile.0);
        let names: Vec<_> = list
            .iter()
            .map(|extension| extension.name.as_str())
            .collect();
        assert_eq!(names, ["Dark Reader", "Off", "uBlock Origin Lite"]);
        assert!(list[0].enabled && !list[1].enabled && list[2].enabled);
        let list = [&list[0], &list[2]];
        assert_eq!(list[0].popup.as_deref(), Some("ui/popup.html"));
        assert_eq!(
            list[0].icon,
            Some(profile.0.join("Extensions/dark/1.0_0/i38.png"))
        );
        assert_eq!(list[1].popup, None);
        assert_eq!(list[1].options.as_deref(), Some("options.html"));
        assert_eq!(list[0].options, None);
        let details = &list[0].details;
        assert!(details.from_store && details.unpacked.is_none());
        assert_eq!(details.installed_at, Some(1_793_225_571_529));
        assert_eq!(details.permissions, ["storage"]);
        assert_eq!(details.sites, ["*://*/*", "<all_urls>"]);
        assert_eq!(
            (list[0].version.as_str(), details.manifest_version),
            ("4.9", 3)
        );
        assert_eq!(
            list[1].icon,
            Some(profile.0.join("Extensions/block/2.0_0/i128.png"))
        );
        assert!(installed(&profile.0.join("missing")).is_empty());
    }

    #[test]
    fn lets_extensions_open_only_web_pages_and_a_blank_one() {
        for url in [
            "https://example.com/",
            "http://example.com/",
            " HTTPS://example.com/",
            "about:blank",
        ] {
            assert!(extension_may_open(url), "{url}");
        }
        for url in [
            "javascript:alert(1)",
            " JavaScript:alert(1)",
            "file:///etc/passwd",
            "chrome://settings",
            "data:text/html,<script>alert(1)</script>",
            "chrome-extension://abc/page.html",
            "about:settings",
            "",
        ] {
            assert!(!extension_may_open(url), "{url}");
        }
    }

    #[test]
    fn tells_extension_workers_and_the_helper_from_other_targets() {
        let target = |kind: &str, url: &str| json!({ "type": kind, "url": url });
        assert_eq!(
            worker_kind(&target(
                "service_worker",
                "chrome-extension://abc/background.js"
            )),
            Some(false)
        );
        assert_eq!(
            worker_kind(&target(
                "service_worker",
                "chrome-extension://def/arbalete-helper.js"
            )),
            Some(true)
        );
        assert_eq!(
            worker_kind(&target("page", "chrome-extension://abc/popup.html")),
            None
        );
        assert_eq!(
            worker_kind(&target("service_worker", "https://site.test/sw.js")),
            None
        );
    }

    #[test]
    fn answers_with_the_active_tab_once_its_tab_id_is_known() {
        let tab = ActiveTab {
            page: Some(1),
            url: "https://example.com/".into(),
            title: "Example".into(),
            favicon: None,
            loading: true,
        };
        assert_eq!(
            state_json(true, Some(&tab), None, true),
            json!({ "tabs": [], "focused": true })
        );
        assert_eq!(
            state_json(false, None, Some(7), true),
            json!({ "tabs": [], "focused": false })
        );
        let state = state_json(true, Some(&tab), Some(7), true);
        let shown = &state["tabs"][0];
        assert_eq!(
            (
                &shown["id"],
                &shown["url"],
                &shown["active"],
                &shown["windowId"]
            ),
            (
                &json!(7),
                &json!("https://example.com/"),
                &json!(true),
                &json!(1)
            )
        );
        assert_eq!(shown["title"], "Example");
        assert_eq!(shown["status"], "loading");
        let state = state_json(true, Some(&tab), Some(7), false);
        let hidden = state["tabs"][0].as_object().expect("a tab");
        assert_eq!(hidden["id"], 7);
        for field in ["url", "title", "favIconUrl"] {
            assert!(!hidden.contains_key(field), "{field} is left out");
        }
    }

    #[test]
    fn shows_a_tab_only_to_extensions_with_the_tabs_or_a_host_permission() {
        let details = |manifest: Value| manifest_details(&manifest);
        let url = "https://mail.example.com/inbox";
        let storage = details(json!({ "permissions": ["storage"] }));
        assert!(!exposes_tab_details(&storage, url));
        let tabs = details(json!({ "permissions": ["storage", "tabs"] }));
        assert!(exposes_tab_details(&tabs, url));
        let host = details(json!({ "host_permissions": ["https://*.example.com/*"] }));
        assert!(exposes_tab_details(&host, url));
        assert!(!exposes_tab_details(&host, "https://other.test/"));
        let v2 = details(json!({ "manifest_version": 2, "permissions": ["<all_urls>"] }));
        assert!(exposes_tab_details(&v2, url));
        // Content scripts grant no host permission.
        let scripts = details(json!({ "content_scripts": [{ "matches": ["<all_urls>"] }] }));
        assert!(!exposes_tab_details(&scripts, url));
        // `activeTab` alone grants nothing until the user invokes the extension.
        let active = details(json!({ "permissions": ["activeTab"] }));
        assert!(!exposes_tab_details(&active, url));
    }

    #[test]
    fn matches_urls_with_match_patterns() {
        let cases = [
            ("<all_urls>", "https://a.com/", true),
            ("<all_urls>", "file:///Users/me/a.txt", true),
            ("<all_urls>", "chrome://settings/", false),
            ("*://*/*", "http://a.com/a", true),
            ("*://*/*", "https://a.com/", true),
            ("*://*/*", "ftp://a.com/", false),
            ("*://*/*", "about:blank", false),
            ("https://*.a.com/*", "https://a.com/", true),
            ("https://*.a.com/*", "https://a.b.a.com/x?y", true),
            ("https://*.a.com/*", "https://nota.com/", false),
            ("https://*.a.com/*", "http://a.com/", false),
            ("https://a.com/*", "https://A.com/a", true),
            ("https://a.com/*", "https://www.a.com/", false),
            ("https://a.com/*", "https://a.com.evil.test/", false),
            ("https://a.com/*", "https://user@a.com:8443/", true),
            ("https://a.com/docs/*", "https://a.com/docs/a", true),
            ("https://a.com/docs/*", "https://a.com/blog/a", false),
            ("https://a.com/*.pdf", "https://a.com/a/b.pdf", true),
            ("https://a.com/*.pdf", "https://a.com/b.html", false),
            ("https://a.com/", "https://a.com", true),
            ("https://a.com/", "https://a.com/a", false),
            ("https://a.com/*?q=*", "https://a.com/s?q=1", true),
            ("http://localhost:8080/*", "http://localhost:8080/a", true),
            ("http://localhost:8080/*", "http://localhost:9090/a", false),
            ("http://localhost/*", "http://localhost:9090/a", true),
            ("http://[::1]/*", "http://[::1]:3000/", true),
            ("file:///*", "file:///tmp/a", true),
            ("https://a.com", "https://a.com/", false),
            ("storage", "https://a.com/", false),
        ];
        for (pattern, url, expected) in cases {
            assert_eq!(
                pattern_matches(pattern, url),
                expected,
                "{pattern} on {url}"
            );
        }
    }
}

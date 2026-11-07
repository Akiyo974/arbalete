# 0001 — Web engine: CEF in Alloy style, with an extension compatibility layer

- Status: accepted; its open questions are answered, and later findings are under "Since the decision"
- Date: 2026-10-23

## Context

Arbalete needs, on macOS first and then on Linux and Windows:

1. **One window holding several pages**: the sidebar, the active tab, Split View, overlays above the page
   (Command Bar, Peek).
2. **Chrome extensions installed from the Chrome Web Store**, with password managers and content blockers working.
3. The same engine on every OS, Rust for the logic and an HTML/TypeScript UI.

Requirement 2 rules out the system WebViews (WebKit on macOS and Linux run no Chrome extensions), so Chromium is
the engine. The question was how to embed it.

## What was tested

All tests ran on macOS arm64 with CEF 156 (Chromium 156), the `cef` crate 156.0.0, an MV3 probe extension and four
extensions from the Chrome Web Store: uBlock Origin Lite, Bitwarden, 1Password and Dark Reader.

### CEF has two browser styles

| | Chrome style | Alloy style |
|---|---|---|
| Browser views per window | **One** ("a Chrome style Window can host at most one Chrome style BrowserView") | Several |
| Extension APIs | Complete | Only what CEF's own components need; the maintainer has no plans to add more ([#4011](https://github.com/chromiumembedded/cef/issues/4011), [#4255](https://github.com/chromiumembedded/cef/issues/4255)) |

### Rejected options

| Option | Why it was rejected |
|---|---|
| **Tauri 2** (system WebViews) | No Chrome extensions on macOS and Linux. |
| **Tauri 3 with its CEF runtime** (3.0.0-alpha.6) | Hosts every browser in a native parent view, which CEF forces to Alloy style on macOS ([#3294](https://github.com/chromiumembedded/cef/issues/3294), open since 2022); the runtime states it does not support extensions. |
| **Electron** | Too heavy for the project's goals; the main extension library is GPL-3. |
| **Several Chrome-style views in one window** | Refused by CEF; moving a created Chrome-style view to another window crashes. |
| **One borderless Chrome-style window per tab, glued as a child of the main window** | Extensions work fully, but macOS gives the keyboard and the active title bar to the same window: clicking the page always makes the main window look inactive. Would also make Split View impossible and multiply per-OS window hacks. |
| **One complete window per tab, swapped on tab change** | Not built: one sidebar per tab, no Split View in a window, side effects with fullscreen and Spaces. |
| **Patching CEF or forking Chromium** | Full control, but a Chromium build infrastructure and a patch to rebase every four weeks. Kept as the fallback. |

### Extensions in Alloy style

| Check | Result |
|---|---|
| Content scripts of all four extensions injected | ✅ |
| uBlock Origin Lite blocks an ad request | ✅ |
| Dark Reader themes the page | ✅ |
| `tabs.sendMessage`, `scripting.executeScript`, `tabs.get`, storage, badge | ✅ |
| `tabs.query` | ❌ returns no tab |
| `tabs.create`, `tabs.update` | ❌ "No current window" |
| `action.openPopup` | ❌ |

### Popups hosted by Arbalete, with a shim

The popup page (`chrome-extension://<id>/<popup>`) is loaded in an Alloy view owned by Arbalete. Before it loads,
a script injected through the DevTools protocol (`Page.addScriptToEvaluateOnNewDocument`) makes
`chrome.tabs.query`, `chrome.windows.getCurrent` and `chrome.windows.getLastFocused` answer with the active tab.

| Extension | Without the shim | With the shim |
|---|---|---|
| uBlock Origin Lite | "not a website" | ✅ shows the site, its filtering mode and per-site actions |
| Dark Reader | "This page is protected" | ✅ shows the site once its **service worker** is shimmed too (its popup asks the background script for the active tab) |
| Bitwarden | first-run screen | ✅ popup works; sign-in and autofill work (question 7) |
| 1Password | blank | ❌ still blank, cause unknown |

### Other spike results

- Web Store installs work in Chrome style when the app accepts the `.crx` download silently in its download
  handler; Chrome then installs the extension.
- Two request contexts give two isolated profiles; an extension only sees its own profile's tabs.
- `Page.setWebLifecycleState("frozen")` freezes a page; closing a tab's browser and recreating it from its URL
  saved about 130 MB per tab.

## Decision

**Arbalete embeds Chromium through CEF, driven from Rust with the `cef` crate, with Alloy-style browsers**, in CEF
Views windows: one window holds the sidebar, the tabs and the overlays.

Chrome extensions run in Alloy style, and **a compatibility layer in Rust** fills the gaps through the DevTools
protocol:

- Arbalete shows extension actions in its own UI and hosts their popups.
- A shim injected into extension popups and service workers answers `tabs.query` and `windows.*` with
  Arbalete's state, and forwards `tabs.create` and `tabs.update` to Arbalete.

If the open questions below cannot be resolved, the fallback is a maintained CEF patch allowing several
Chrome-style views per window.

## Consequences

- The window model stays simple: several pages, Split View and overlays in one window, native focus behavior.
- The compatibility layer is our code to maintain, and some extensions may still fail; each one in the acceptance
  list is tested before release.
- CEF stays out of `arbalete-core`: only the app drives it ([0005](0005-repository-layout.md)).
- Two traps of the `cef` crate, found during the spike: unimplemented callbacks return 0, which means
  "cancel" in `on_before_resource_load` and "no" in window options (`can_resize`, `can_minimize`…), and
  a preferred size with a zero dimension is ignored. Every callback Arbalete relies on is implemented explicitly.

## Open questions

To answer before the compatibility layer is built, in this order:

1. **Browser-level DevTools access from the app**, needed to attach to extension service workers as they start.
   The spike did it from an external DevTools client. **Answered (2026-10-28)**: a DevTools session the app
   opens on any of its browsers (`add_dev_tools_message_observer`, `send_dev_tools_message`) sees every target
   of the browser. `Target.getTargets` lists the extensions' service workers, one per profile with its
   `browserContextId`; `Target.setDiscoverTargets` reports them as they start (`targetCreated`);
   `Target.attachToTarget` with `flatten` gives a session in which `Runtime.evaluate` runs in the worker. Only
   `Target.attachToBrowserTarget` is refused ("Not allowed"), so a worker cannot be paused before its first
   line (`waitForDebuggerOnStart`): the shim is installed just after it starts, which covers `tabs.query` and
   `windows.*` called from event handlers. CEF also honors Chromium's `--load-extension` switch, which loads an
   unpacked extension for tests.
2. **The active tab's extension id**: Chromium's internal tab id is not exposed by CEF. Lead: a small built-in
   extension whose content script reports `sender.tab.id`. **Answered (2026-10-28)**, more directly: in the
   service worker of an extension holding the `debugger` permission, `chrome.debugger.getTargets()` lists each
   page of its profile with both its extension tab id (`tabId`) and its DevTools target id (`id`); the app gets
   the target id of its own pages from `Target.getTargetInfo` on their session, which ties each of its pages to
   its tab id. `chrome.tabs.get(tabId)` then works, with `active: false` and `windowId: -1`, as the pages sit
   in no Chromium window: the shim fills in both. Third-party extensions lack the `debugger` permission, so
   Arbalete ships a small helper extension without content scripts, loaded with `--load-extension` from the
   bundle. That switch loads it in every request context, the app's own UI included, which the helper then
   sees but does not touch. Installed extensions reached the UI too: their content scripts ran in it while it
   was served under `https://arbalete.ui/` (Dark Reader restyled the toolbar). The UI is now served from a
   scheme of its own, `arbalete://ui/`, which content scripts never match.
3. **Installing from the Chrome Web Store** when every browser is Alloy style. Lead: a dedicated Chrome-style
   window for the Web Store. **Answered (2026-10-28)**: a Chrome-style window of its own, in the profile's
   request context, on the extension's Web Store page (`chromewebstore.google.com/detail/<id>`), has the
   store's install API (`chrome.webstorePrivate`): "Add to Chrome", then Chromium's confirmation, installs the
   extension in the profile's `Extensions` directory. It then runs in that profile's Alloy pages (uBlock Origin
   Lite blocks an ad script on example.com), and again after a relaunch. The window must be a separate one: CEF
   refuses a Chrome-style view beside Alloy views. Three things to handle when it is built:
   - the store shows "Switch to Chrome?" and a banner to every Chromium-based browser not branded Google Chrome;
     "No thanks" dismisses them, and the "Add to Chrome" button works;
   - a `.crx` loaded straight from Google's update service is refused ("Extensions and apps cannot be added from
     this website"): installs go through the store page;
   - once installed, Chromium opens a full Chromium window (tab strip, address bar, New Tab page) to show its
     "has been added" bubble, as it finds no browser window of its own: Arbalete closes it, or keeps Chromium
     from opening it.
   **Revised (2026-10-29)**: a separate window is not needed. An Alloy page on the store, in an ordinary
   Arbalete tab, has `chrome.webstorePrivate` too, and "Add to Chrome", then Chromium's confirmation, installs
   the extension, like in Arc. The download handler must accept the `.crx` (declined, the install fails with "Download canceled"): it goes
   to a temporary folder, out of Arbalete's downloads. The full Chromium window
   that may open after an install goes too, once the extension's service worker starts (Chromium writes the
   extension's preferences only later), and a page the extension opened in it moves to an Arbalete tab.
4. **Forwarding `tabs.create` and `tabs.update`** to Arbalete. **Answered (2026-10-28)**: in an extension's
   service worker, `Runtime.addBinding` exposes a function whose calls reach the app as `Runtime.bindingCalled`
   events, and Chromium lets the shim replace `chrome.tabs.create` and `chrome.tabs.update` with functions
   calling it. Called as an extension would, the original `create` fails ("No current window"); the shimmed
   `create` makes Arbalete open the tab, and `update` load another address in it. Left to build: a binding only
   goes one way, so the shim awaits the app's answer, which the app gives by evaluating in the same session, to
   return the new tab's real id (question 2); `update` on another tab finds it by its id the same way; the
   binding and the shim are installed again whenever the worker starts again (question 1).
5. **1Password**: find why its popup stays blank. **Partly answered (2026-10-28)**: installed from the Web Store,
   its popup (`popup/index.html`, opened in an Alloy page) is not broken: its React interface mounts, without
   exceptions or failed requests, and shows the 1Password logo. With the shim in the popup and in its service
   worker, `tabs.query` and `windows.*` answer; the popup then calls no other tab or window API and waits for
   the account's state, which it has none of without a signed-in account: the blank popup of the first spike
   was most likely this screen. The full check (vault, autofill) needs a 1Password account, for the acceptance
   tests. Found on the way: the Chromium windows Chrome-style views live in (the Web Store's, the one opened
   after an install) are real Chromium windows, so extensions take their tab for the active one (1Password
   thought it was on the Web Store): Arbalete never leaves one open. Checked on the way with Dark Reader,
   installed the same way: it themes Arbalete's Alloy pages (iana.org goes dark, `data-darkreader-mode`
   "dynamic"), and its popup shows the site and its settings once the shim answers `tabs.query`, `tabs.get`
   and `windows.*` in the popup and in its service worker: its worker calls `tabs.get` on the id `tabs.query`
   gave it, so the shim answers with real tab ids (question 2) rather than made-up ones. On install, Dark
   Reader and 1Password open a welcome page with `tabs.create`, in Chromium windows of their own until the
   shim forwards it to Arbalete (question 4).
6. **Several Alloy views and an overlay in one CEF Views window**: allowed by CEF. **Answered**: the app is
   built this way (the sidebar, the page's card and its overlays in one window).
7. **Bitwarden with an account**, for the acceptance tests. **Answered (2026-10-29)**: installed from the Web
   Store, signed in from its popup with a one-time code from email, it fills Google's sign-in fields from its
   own suggestions in the page. Found on the way:
   - Chromium takes the popup's page for a tab's (`chrome.extension.getViews({ type: "tab" })` lists it), and
     Bitwarden, which sizes itself by where it shows, filled it: its window shrank to the 25-point minimum.
     The shim lists the page among the popups instead, as in Chrome.
   - A sign-in that needs a code from elsewhere loses the popup if it closes on losing the focus, like
     Chrome's: Arbalete's popup window stays open until closed.
   - Bitwarden's "make it the default password manager" step asks for the optional `privacy` permission with
     `chrome.permissions.request`. Chromium shows no prompt for an Alloy page and refuses it, and Bitwarden
     does not ask again. Arbalete has no password manager of its own to turn off, so nothing is lost.
     Chromium does prompt from a Chrome-style page, though: the shim forwards `chrome.permissions.request`
     to Arbalete, which asks again from a small Chrome-style window on the extension's `manifest.json`
     (a page of the extension that runs none of its code), with a user gesture, and gives the answer back.
     Allowing Bitwarden's `privacy` request there grants it.
   - On Google's sign-in page, Chromium shows "Use Passkey from Another Device" above Bitwarden's suggestions:
     the page asks for passkeys from its field (conditional mediation), whatever
     `chrome.privacy.services.passwordSavingEnabled` says. No switch hides that bubble alone, and a script
     holding the requests back in every page was judged too intrusive: Arbalete keeps Chromium's behavior,
     Chrome's and Arc's too.

## Since the decision

- **The interface went native on macOS** ([0003](0003-rust-logic-native-interface.md)), and the pages moved
  from CEF views into native containers ([0002](0002-native-page-containers.md)). Alloy style and the
  extension compatibility layer stay.
- **Proprietary codecs (2026-11-07).** The official CEF builds leave out H.264 and AAC, which sites like Twitch
  stream. [amenophis/cef-codecs](https://github.com/amenophis/cef-codecs) builds the same CEF from source with
  Chromium's proprietary codecs; FFmpeg decodes them under the LGPL 2.1, and macOS decodes HEVC. `mise run cef`
  downloads the build named by the `cef` version and `cef-codecs-revision` in the workspace's `Cargo.toml`,
  checked against its SHA-256, for development, CI and releases alike. Widevine is not in it: it needs an
  agreement with Google.
- **Chromium's own pages in Alloy style (2026-11-07).** Build 2 of that CEF carries a patch letting Alloy-style
  browsers show Chromium's WebUI pages. Its settings, extensions, history, downloads, password manager, flags and
  internals open in ordinary tabs, shown as `arbalete://` addresses (`app/src/shell/chrome_pages.rs`). The pages
  that still crash in a tab, like `chrome://bookmarks`, open in Chromium's window.
- With a CEF build of its own, the fallback above, a patched CEF, is now within reach should Alloy style fall
  short; it has not been needed.

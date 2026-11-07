//! A page's developer tools: Chromium's own front end, which the app hosts in the card.

use super::*;

/// A page's developer tools, like Chrome's and Arc's: Chromium's own DevTools front end in a
/// view of the card, under the page, which it talks to through CEF's DevTools protocol. Docked,
/// the front end fills the card and the page shows in the room it leaves, at its right, left or
/// bottom; undocked, it moves to a window of its own.
pub(super) struct Inspector {
    /// Drawn in its page's container, under the page, or in a window of its own while undocked.
    frontend: Browser,
    docked: Cell<bool>,
    /// The front end said it is ready; until then, what to do once it is.
    ready: Cell<bool>,
    pending: RefCell<Vec<InspectorAction>>,
    /// Keeps the page's protocol messages coming to the front end.
    _observer: Option<Registration>,
}

/// What a page's developer tools do once their front end is ready.
pub(super) enum InspectorAction {
    /// Runs a script in the front end, like showing a panel.
    Script(String),
    /// Shows the element at a point of the page in the Elements panel, like Inspect.
    InspectAt(i32, i32),
}

/// The ids of the app's own protocol messages to a page, from this one on, whose answers its
/// developer tools' front end does not see: asking for the element at a point, and capturing
/// the page.
const APP_MESSAGES: u64 = 1_000_000_000;
const INSPECT_AT_ID: u64 = APP_MESSAGES + 1;
pub(super) const BLOCKS_ID: u64 = APP_MESSAGES + 2;
pub(super) const CAPTURE_ID: u64 = APP_MESSAGES + 3;
pub(super) const SCROLL_ID: u64 = APP_MESSAGES + 4;
pub(super) const LAYOUT_ID: u64 = APP_MESSAGES + 5;

/// The front end's page, told it may dock.
const INSPECTOR_URL: &str = "devtools://devtools/bundled/devtools_app.html?can_dock=true";

/// The front end's own host, which Chromium gives it, stands in for Chrome's: what it would ask
/// Chrome comes to the app as marked console messages, and the app sends it the page's protocol
/// messages through `DevToolsAPI`.
const INSPECTOR_HOST: &str = r#"(() => {
  const tell = (kind, data) => console.debug("\u0001arbalete-inspector:" + kind + ":" + data);
  const take = (host) => {
    if (!host || host.arbalete) return host;
    host.arbalete = true;
    host.sendMessageToBackend = (message) => tell("send", message);
    host.setIsDocked = (docked, done) => { tell("dock", docked ? "1" : "0"); if (done) setTimeout(done, 0); };
    host.setInspectedPageBounds = (bounds) => tell("room", JSON.stringify(bounds));
    host.closeWindow = () => tell("close", "");
    host.bringToFront = () => {};
    const loaded = host.loadCompleted && host.loadCompleted.bind(host);
    host.loadCompleted = () => { tell("ready", ""); if (loaded) loaded(); };
    return host;
  };
  let host = take(window.InspectorFrontendHost);
  try {
    Object.defineProperty(window, "InspectorFrontendHost", {
      configurable: true, get: () => host, set: (value) => { host = take(value); },
    });
  } catch (_) {}
})();"#;

/// Opens a tab's page's developer tools, docked, or closes them.
pub(super) fn toggle_inspector(tab: NodeId) {
    if INSPECTORS.with_borrow(|inspectors| inspectors.contains_key(&tab)) {
        close_inspector(tab);
    } else {
        open_inspector(tab, None, None);
    }
}

/// Shows a tab's page's developer tools on `panel`, opening them if need be, then does `then`,
/// like Chrome's developer tools buttons and Inspect.
pub(super) fn show_inspector(tab: NodeId, panel: &'static str, then: Option<InspectorAction>) {
    match INSPECTORS.with_borrow(|inspectors| inspectors.get(&tab).cloned()) {
        Some(inspector) => {
            act_in_inspector(
                tab,
                &inspector,
                InspectorAction::Script(format!("DevToolsAPI.showPanel({panel:?})")),
            );
            if let Some(then) = then {
                act_in_inspector(tab, &inspector, then);
            }
        }
        None => open_inspector(tab, Some(panel), then),
    }
}

/// Opens a page's developer tools, docked, on `panel` if given, doing `then` once ready.
fn open_inspector(tab: NodeId, panel: Option<&str>, then: Option<InspectorAction>) {
    let Some(host) = PAGES.with_borrow(|pages| pages.get(&tab)?.browser.host()) else {
        return;
    };
    let url = match panel {
        Some(panel) => format!("{INSPECTOR_URL}&panel={panel}"),
        None => INSPECTOR_URL.to_string(),
    };
    #[cfg(target_os = "macos")]
    let Some(parent) = crate::native::tools_view(tab_number(tab)) else {
        return;
    };
    let info = WindowInfo {
        #[cfg(target_os = "macos")]
        parent_view: parent,
        // Any size: the container gives the front end its own.
        bounds: Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        },
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    };
    let Some(frontend) = browser_host_create_browser_sync(
        Some(&info),
        Some(&mut InspectorClient::new(tab)),
        Some(&CefString::from(url.as_str())),
        Some(&BrowserSettings::default()),
        None,
        None,
    ) else {
        return;
    };
    let observer = host.add_dev_tools_message_observer(Some(&mut InspectorObserver::new(tab)));
    INSPECTORS.with_borrow_mut(|inspectors| {
        inspectors.insert(
            tab,
            Rc::new(Inspector {
                frontend,
                docked: Cell::new(true),
                ready: Cell::new(false),
                pending: RefCell::new(then.into_iter().collect()),
                _observer: observer,
            }),
        )
    });
}

/// Does `action` in a page's developer tools, at once if their front end is ready, or once it
/// is.
fn act_in_inspector(tab: NodeId, inspector: &Inspector, action: InspectorAction) {
    if !inspector.ready.get() {
        inspector.pending.borrow_mut().push(action);
        return;
    }
    match action {
        InspectorAction::Script(script) => run_in_inspector(inspector, &script),
        InspectorAction::InspectAt(x, y) => {
            let ask = json!({
                "id": INSPECT_AT_ID,
                "method": "DOM.getNodeForLocation",
                "params": { "x": x, "y": y, "includeUserAgentShadowDOM": false },
            });
            let host = PAGES.with_borrow(|pages| pages.get(&tab)?.browser.host());
            if let Some(host) = host {
                host.send_dev_tools_message(Some(ask.to_string().as_bytes()));
            }
        }
    }
}

/// Closes a page's developer tools, the page taking the card back.
pub(super) fn close_inspector(tab: NodeId) {
    let Some(inspector) = INSPECTORS.with_borrow_mut(|inspectors| inspectors.remove(&tab)) else {
        return;
    };
    // Its view leaves its container (InspectorLifeSpan's do_close).
    if let Some(host) = inspector.frontend.host() {
        host.close_browser(1);
    }
}

/// Runs a script in a page's developer tools' front end.
fn run_in_inspector(inspector: &Inspector, script: &str) {
    if let Some(frame) = inspector.frontend.main_frame() {
        frame.execute_java_script(Some(&CefString::from(script)), None, 0);
    }
}

wrap_task! {
    struct InspectorTask {
        tab: NodeId,
        kind: String,
        data: String,
    }

    impl Task {
        fn execute(&self) {
            inspector_asked(self.tab, &self.kind, &self.data);
        }
    }
}

/// Does what a page's developer tools' front end asked its host: send the page a protocol
/// message, leave it some room, dock or undock, or close.
fn inspector_asked(tab: NodeId, kind: &str, data: &str) {
    let Some(inspector) = INSPECTORS.with_borrow(|inspectors| inspectors.get(&tab).cloned()) else {
        return;
    };
    match kind {
        "send" => {
            let host = PAGES.with_borrow(|pages| pages.get(&tab)?.browser.host());
            if let Some(host) = host {
                host.send_dev_tools_message(Some(data.as_bytes()));
            }
        }
        "room" => {
            #[derive(serde::Deserialize)]
            struct Room {
                x: f64,
                y: f64,
                width: f64,
                height: f64,
            }
            #[cfg(target_os = "macos")]
            if let Ok(room) = serde_json::from_str::<Room>(data) {
                crate::native::tools_room(tab_number(tab), room.x, room.y, room.width, room.height);
            }
        }
        "dock" => {
            let docked = data == "1";
            if docked == inspector.docked.get() {
                return;
            }
            inspector.docked.set(docked);
            #[cfg(target_os = "macos")]
            crate::native::tools_docked(tab_number(tab), docked);
        }
        "ready" => {
            inspector.ready.set(true);
            let pending = std::mem::take(&mut *inspector.pending.borrow_mut());
            for action in pending {
                act_in_inspector(tab, &inspector, action);
            }
        }
        "close" => close_inspector(tab),
        _ => {}
    }
}

// The front end of a page's developer tools.
wrap_client! {
    struct InspectorClient {
        tab: NodeId,
    }

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(InspectorDisplay::new(self.tab))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(InspectorLoad::new())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(InspectorLifeSpan::new())
        }
    }
}

wrap_life_span_handler! {
    struct InspectorLifeSpan {}

    impl LifeSpanHandler {
        /// The front end's own windows, like its device mode's, which Chrome hosts apart, open
        /// nowhere: never as tabs.
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            _target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            1
        }

        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            OPEN_BROWSERS.set(OPEN_BROWSERS.get() + 1);
        }

        /// The front end closing leaves its page's container, which CEF would otherwise close the
        /// window of.
        fn do_close(&self, browser: Option<&mut Browser>) -> i32 {
            let Some(host) = browser.and_then(|browser| browser.host()) else {
                return 0;
            };
            post_task(
                ThreadId::UI,
                Some(&mut RemoveTools::new(host.window_handle() as usize)),
            );
            1
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            browser_closed();
        }
    }
}

wrap_task! {
    struct RemoveTools {
        view: usize,
    }

    impl Task {
        /// Takes a closing front end's view out of the interface, which destroys its browser.
        fn execute(&self) {
            #[cfg(target_os = "macos")]
            crate::native::tools_closed(self.view as *mut std::ffi::c_void);
        }
    }
}

wrap_display_handler! {
    struct InspectorDisplay {
        tab: NodeId,
    }

    impl DisplayHandler {
        /// What the front end asks its host comes as marked console messages, only from the
        /// front end's own scripts: an extension's panel inside it never speaks for it.
        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            _line: i32,
        ) -> i32 {
            let source = source.map(CefString::to_string).unwrap_or_default();
            if !source.starts_with("devtools://devtools/") {
                return 0;
            }
            let message = message.map(CefString::to_string).unwrap_or_default();
            let Some(asked) = message.strip_prefix("\u{1}arbalete-inspector:") else {
                return 0;
            };
            let (kind, data) = asked.split_once(':').unwrap_or((asked, ""));
            post_task(
                ThreadId::UI,
                Some(&mut InspectorTask::new(self.tab, kind.to_string(), data.to_string())),
            );
            1
        }
    }
}

wrap_load_handler! {
    struct InspectorLoad {}

    impl LoadHandler {
        /// The host stands in before the front end's scripts run, under the front end's address
        /// so that its marked console messages come from there (InspectorDisplay).
        fn on_load_start(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _transition_type: TransitionType) {
            if let Some(frame) = frame.filter(|frame| frame.is_main() == 1) {
                let host = CefString::from(INSPECTOR_HOST);
                frame.execute_java_script(Some(&host), Some(&CefString::from(INSPECTOR_URL)), 0);
            }
        }
    }
}

// Brings a page's protocol messages to its developer tools' front end.
wrap_dev_tools_message_observer! {
    struct InspectorObserver {
        tab: NodeId,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> i32 {
            let Some(message) = message.and_then(|message| std::str::from_utf8(message).ok()) else {
                return 0;
            };
            let inspector = INSPECTORS.with_borrow(|inspectors| inspectors.get(&self.tab).cloned());
            // The element at the point Inspect asked about, which the front end reveals as if
            // picked.
            if message.contains(&INSPECT_AT_ID.to_string()) {
                let answer: serde_json::Value = serde_json::from_str(message).unwrap_or_default();
                if answer["id"] == INSPECT_AT_ID {
                    let node = &answer["result"]["backendNodeId"];
                    if let (Some(inspector), false) = (inspector, node.is_null()) {
                        let event = json!({
                            "method": "Overlay.inspectNodeRequested",
                            "params": { "backendNodeId": node },
                        });
                        let event = serde_json::to_string(&event.to_string()).expect("a string");
                        run_in_inspector(&inspector, &format!("DevToolsAPI.dispatchMessage({event})"));
                    }
                    return 1;
                }
            }
            if is_app_message(message) {
                return 0;
            }
            if let Some(inspector) = inspector {
                let message = serde_json::to_string(message).expect("a string");
                run_in_inspector(&inspector, &format!("DevToolsAPI.dispatchMessage({message})"));
            }
            1
        }
    }
}

/// Whether a page's protocol message answers one of the app's own.
pub(super) fn is_app_message(message: &str) -> bool {
    message.starts_with(r#"{"id":1"#)
        && serde_json::from_str::<serde_json::Value>(message)
            .ok()
            .and_then(|message| message["id"].as_u64())
            .is_some_and(|id| id >= APP_MESSAGES)
}

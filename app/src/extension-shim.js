// Injected by Arbalete into every extension's service worker (app/src/extensions.rs). Arbalete's
// pages sit in no Chromium window, so Chrome's tab and window APIs find none: they answer instead
// with the profile's active tab, which the app keeps up to date (__arbaleteSync), its address,
// title and icon only for an extension Chrome would show them to, and creating or
// updating a tab asks Arbalete (__arbalete, a DevTools binding) to do it in its own tabs. Injected
// into an extension's popup too, with the active tab as it opens (__arbaleteInitial): the popup
// closes on Escape, or when it closes itself.
(() => {
  if (self.__arbaleteShimmed) return;
  // Each API is patched on its own, so that one Chromium has not added yet as the worker starts
  // cannot keep the others native: those missing are tried again, and the shim is done only once
  // all are patched.
  const patches = new Map();
  const patch = (api, apply) => patches.set(api, apply);
  const window_ = {
    id: 1,
    focused: true,
    top: 0,
    left: 0,
    width: 1280,
    height: 820,
    incognito: false,
    type: "normal",
    state: "normal",
    alwaysOnTop: false,
  };
  let tabs = self.__arbaleteInitial?.tabs ?? [];
  // Chrome's tab and window events never fire for Arbalete's pages either: the shim dispatches
  // them as the active tab changes, to the listeners the worker added before it was shimmed too.
  // Were Chromium to fire one itself, the shim would leave that event to it.
  const events = {};
  const dispatcher = (api, name) => {
    const event = chrome[api][name];
    let dispatching = false;
    let native = false;
    event.addListener(() => {
      native ||= !dispatching;
    });
    events[name] = (...args) => {
      if (native) return;
      dispatching = true;
      try {
        event.dispatch(...args);
      } finally {
        dispatching = false;
      }
    };
  };
  // What changed since the previous state: the window focused, none (-1) while the profile is not
  // in front, and the active tab, compared with the last one known, as a new page's tab id is
  // known only after it shows. As in Chrome, a tab made active is not updated, and what a tab
  // lost, like its icon, is no change. A worker's first state, as it starts, announces nothing.
  let focused;
  let known;
  const announce = (state) => {
    const windowId = state.focused ? window_.id : -1;
    if (focused !== undefined && windowId !== focused) events.onFocusChanged?.(windowId);
    focused = windowId;
    const [tab] = state.tabs;
    if (!tab) return;
    const was = known;
    known = tab;
    if (!was) return;
    if (tab.id !== was.id) {
      events.onActivated?.({ tabId: tab.id, windowId: tab.windowId });
      return;
    }
    const changes = ["status", "url", "title", "favIconUrl"]
      .filter((key) => tab[key] != null && tab[key] !== was[key])
      .map((key) => [key, tab[key]]);
    if (changes.length) events.onUpdated?.(tab.id, Object.fromEntries(changes), tab);
  };
  self.__arbaleteSync = (state) => {
    tabs = state.tabs;
    announce(state);
  };
  // Chrome's APIs take a callback last, or return a promise without one.
  const reply = (args, value) => {
    const callback = args.find((arg) => typeof arg === "function");
    if (!callback) return Promise.resolve(value);
    callback(value);
  };
  const ask = (message) => self.__arbalete(JSON.stringify(message));
  // Chromium prompts for optional permissions only from a Chrome-style page: Arbalete asks there
  // (request_permissions in app/src/extensions.rs), then gives the answer to __arbaleteAnswer.
  let asked = 0;
  const answers = new Map();
  self.__arbaleteAnswer = (request, granted) => {
    answers.get(request)?.(granted);
    answers.delete(request);
  };
  patch("permissions", () => {
    chrome.permissions.request = (permissions, ...rest) => {
      asked += 1;
      const granted = new Promise((resolve) => answers.set(asked, resolve));
      ask({ method: "permissions", request: asked, permissions });
      return granted.then((value) => reply(rest, value));
    };
  });
  // A popup's page, rather than a service worker.
  if (typeof window !== "undefined") {
    const close = () => ask({ method: "close" });
    window.close = close;
    window.addEventListener("keydown", (event) => {
      if (event.key === "Escape") close();
    });
    // Chromium takes the popup's page for a tab's: extensions that size themselves by where they
    // show, like Bitwarden, would fill a tab. It is the popup, as in Chrome.
    patch("extension", () => {
      const getViews = chrome.extension.getViews.bind(chrome.extension);
      chrome.extension.getViews = (info = {}) =>
        info.type === "popup" ? [window] : getViews(info).filter((view) => view !== window);
    });
  }
  // chrome.windows.WINDOW_ID_CURRENT.
  const CURRENT = -2;
  const fits = (tab, info) =>
    (info.active === undefined || tab.active === info.active) &&
    (info.windowId === undefined || info.windowId === CURRENT || info.windowId === tab.windowId);
  const withTabs = (info) =>
    info && typeof info === "object" && info.populate ? { ...window_, tabs } : window_;
  patch("tabs", () => {
    const get = chrome.tabs.get.bind(chrome.tabs);
    const query = chrome.tabs.query.bind(chrome.tabs);
    // The extension's own pages in Chromium windows, like those chrome.windows.create opens, are
    // found too, as in Chrome: an extension finds its window by its address to close it, like
    // Bitwarden's once a passkey is used.
    const own = chrome.runtime.getURL("");
    const ownTabs = (info) =>
      query(info).then(
        (found) => found.filter((tab) => tab.url?.startsWith(own)),
        () => [],
      );
    Object.assign(chrome.tabs, {
      query: (info = {}, ...rest) =>
        ownTabs(info).then((found) =>
          reply(rest, [...tabs.filter((tab) => fits(tab, info)), ...found]),
        ),
      get: (id, ...rest) => {
        const tab = tabs.find((tab) => tab.id === id);
        return tab ? reply(rest, tab) : get(id, ...rest);
      },
      create: (props = {}, ...rest) => {
        ask({ method: "create", props });
        return reply(rest, { id: -1, index: 0, windowId: window_.id, active: true, url: props.url });
      },
      update: (...args) => {
        const [tabId, props] = typeof args[0] === "number" ? args : [undefined, args[0]];
        ask({ method: "update", tabId, props });
        return reply(args.slice(typeof args[0] === "number" ? 2 : 1), tabs[0]);
      },
    });
    dispatcher("tabs", "onActivated");
    dispatcher("tabs", "onUpdated");
  });
  patch("windows", () => {
    Object.assign(chrome.windows, {
      get: (id, ...rest) => reply(rest, withTabs(rest[0])),
      getCurrent: (...args) => reply(args, withTabs(args[0])),
      getLastFocused: (...args) => reply(args, withTabs(args[0])),
      getAll: (...args) => reply(args, [withTabs(args[0])]),
    });
    dispatcher("windows", "onFocusChanged");
  });
  const run = (tries) => {
    for (const [api, apply] of patches) {
      try {
        apply();
        patches.delete(api);
      } catch (error) {
        if (!tries) console.error(`Arbalete could not patch chrome.${api}:`, error);
      }
    }
    if (!patches.size) self.__arbaleteShimmed = true;
    else if (tries) setTimeout(run, 10, tries - 1);
  };
  run(100);
})();

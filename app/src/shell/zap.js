// The Boost editor's zap picker (app/src/shell/boosts.rs), in a world of its own in the page:
// like Arc's, it outlines the element under the pointer and, on a click, offers to zap that
// element or the ones like it, showing the selector and how many it matches. A bar at the page's
// bottom says so, with Done, which stops it, like Escape.
(() => {
  if (globalThis.arbaleteZap) return;
  const send = (message) => globalThis.__arbaleteZap(JSON.stringify(message));

  const host = document.createElement("arbalete-zap");
  const root = host.attachShadow({ mode: "closed" });
  // A constructed stylesheet: a page's Content Security Policy would block a style element.
  const style = new CSSStyleSheet();
  style.replaceSync(`
    :host { all: initial; position: fixed; inset: 0; z-index: 2147483647; pointer-events: none;
      font: 13px -apple-system, system-ui, sans-serif; }
    .box { position: fixed; box-sizing: border-box; border-radius: 3px;
      background: rgba(250, 204, 21, 0.9); mix-blend-mode: multiply; }
    .more { position: fixed; left: 50%; bottom: 56px; transform: translateX(-50%); padding: 4px 12px;
      border-radius: 12px; background: #f5c518; color: #3a2a00; font-weight: 600; font-size: 12px; }
    .menu { position: fixed; width: 400px; max-width: calc(100vw - 16px); border-radius: 14px; overflow: hidden;
      background: rgba(246, 246, 248, 0.97); pointer-events: auto;
      box-shadow: 0 10px 30px rgba(0, 0, 0, 0.35), 0 0 0 0.5px rgba(0, 0, 0, 0.2); }
    .choices { display: flex; gap: 8px; padding: 10px; }
    button { all: unset; padding: 8px 12px; border-radius: 9px; color: white; cursor: default;
      white-space: nowrap; box-shadow: inset 0 1px rgba(255, 255, 255, 0.25); }
    .one { background: linear-gradient(#f9364f, #e3132f); }
    .all { flex: 1; background: #8e8e93; }
    .cancel { background: linear-gradient(#77777b, #5f5f63); }
    button:hover { filter: brightness(1.08); }
    .selector { display: flex; gap: 10px; padding: 7px 14px; background: rgba(0, 0, 0, 0.05);
      color: #66666b; font: 11px ui-monospace, Menlo, monospace; white-space: nowrap; overflow: hidden;
      text-overflow: ellipsis; }
    .selector b { color: #333; font-weight: 600; }
    .bar { position: fixed; left: 0; right: 0; bottom: 0; height: 44px; display: flex;
      align-items: center; justify-content: center; background: rgba(30, 31, 34, 0.97); color: #e8e8ea;
      pointer-events: auto; box-shadow: 0 -0.5px rgba(255, 255, 255, 0.15); }
    .bar strong { font-weight: 700; margin: 0 4px; }
    .bar button { padding: 5px 12px; color: #1d1d1f; background: #f2efe9; font-weight: 500; }
    .done { position: absolute; right: 8px; }
    .zaps { position: absolute; left: 8px; display: flex; align-items: center; gap: 6px; }
    .zaps button { width: 30px; height: 30px; padding: 0; display: grid; place-items: center; }
    .zaps button:hover { background: #dcd8d0; }
    .zaps span { margin-left: 6px; }
    .dim { position: fixed; inset: 0; width: 100%; height: 100%; }
    .tip { position: fixed; max-width: 320px; padding: 7px 10px; border-radius: 9px;
      background: rgba(32, 26, 20, 0.96); color: #f2ede6; font-size: 12px; font-weight: 600;
      line-height: 1.35; overflow-wrap: anywhere; box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35); }
    [hidden] { display: none !important; }`);
  root.adoptedStyleSheets = [style];
  // Built without markup, which a page's Trusted Types may refuse.
  const make = (tag, className, ...children) => {
    const element = document.createElement(tag);
    element.className = className;
    element.append(...children);
    return element;
  };
  const boxes = make("div", "boxes");
  const menu = make("div", "menu");
  menu.hidden = true;
  const done = make("button", "done", "Done");
  const hint = make("span", "", "Click elements on the page to", make("strong", "", "Zap"), "them");
  // Picking, for the code's selector picker: a choice gives its selector instead of zapping.
  let picking = false;
  const zapped = make("div", "zaps");
  const bar = make("div", "bar", hint, zapped, done);
  // The page dimmed but where the elements restored are, like Arc's.
  const ns = "http://www.w3.org/2000/svg";
  const dim = document.createElementNS(ns, "svg");
  dim.setAttribute("class", "dim");
  const shade = document.createElementNS(ns, "path");
  shade.setAttribute("fill", "rgba(0, 0, 0, 0.55)");
  shade.setAttribute("fill-rule", "evenodd");
  dim.append(shade);
  dim.style.display = "none";
  const tip = make("div", "tip");
  tip.hidden = true;
  const more = make("div", "more");
  more.hidden = true;
  root.append(dim, boxes, more, menu, bar, tip);
  document.documentElement.append(host);

  // Outlines elements, and tells how many more are out of the window.
  const outline = (elements) => {
    const away = elements.filter((element) => {
      const rect = element.getBoundingClientRect();
      return rect.bottom < 0 || rect.top > innerHeight || rect.right < 0 || rect.left > innerWidth;
    }).length;
    more.hidden = away === 0;
    more.textContent = `${away} more`;
    boxes.replaceChildren(
      ...elements.slice(0, 100).map((element) => {
        const rect = element.getBoundingClientRect();
        const box = document.createElement("div");
        box.className = "box";
        Object.assign(box.style, {
          left: `${rect.left}px`, top: `${rect.top}px`,
          width: `${rect.width}px`, height: `${rect.height}px`,
        });
        return box;
      }),
    );
  };
  const all = (selector) => {
    try {
      return [...document.querySelectorAll(selector)];
    } catch {
      return [];
    }
  };

  // Class names that read like words, not ones a build tool made up.
  const classes = (element) =>
    [...element.classList]
      .filter((name) => /^[a-zA-Z_-][\w-]{1,39}$/.test(name) && !/\d{3,}/.test(name))
      .slice(0, 3)
      .map((name) => `.${CSS.escape(name)}`)
      .join("");
  const step = (element) => {
    const tag = element.localName;
    const same = [...(element.parentElement?.children ?? [])].filter((e) => e.localName === tag);
    const nth = same.length > 1 ? `:nth-of-type(${same.indexOf(element) + 1})` : "";
    return `${tag}${classes(element)}${nth}`;
  };
  // The element alone: by its id when it is the only one, otherwise by its path from the nearest
  // ancestor that is.
  const only = (element) => {
    const path = [];
    for (let at = element; at && at !== document.documentElement; at = at.parentElement) {
      if (at.id && all(`#${CSS.escape(at.id)}`).length === 1) {
        path.unshift(`#${CSS.escape(at.id)}`);
        break;
      }
      path.unshift(step(at));
      if (all(path.join(" > ")).length === 1) break;
    }
    return path.join(" > ");
  };
  // The elements like it, from the closest to the widest, like Arc's: in each of its ancestors in
  // turn, the elements of its kind, and the ancestor's children like the one it is in; each
  // covering more of the page than the one before, and not most of it.
  const kind = (element) => `${element.localName}${classes(element)}`;
  const area = (elements) =>
    elements.reduce((sum, element) => {
      const rect = element.getBoundingClientRect();
      return sum + rect.width * rect.height;
    }, 0);
  const candidates = (element) => {
    const page = document.documentElement.scrollWidth * document.documentElement.scrollHeight;
    const options = [];
    const add = (selector) => {
      const elements = all(selector);
      const holds = elements.some((other) => other === element || other.contains(element));
      if (elements.length < 2 || elements.length > 500 || !holds) return;
      const covered = area(elements);
      if (covered < page * 0.6) options.push({ selector, count: elements.length, covered });
    };
    let inner = element;
    for (let at = element.parentElement, depth = 0; at && at !== document.body && depth < 8; at = at.parentElement, depth++) {
      const scope = only(at);
      add(`${scope} ${kind(element)}`);
      add(`${scope} > ${kind(inner)}`);
      inner = at;
    }
    options.sort((one, other) => one.covered - other.covered);
    return options.filter((option, index) => index === 0 || option.covered > options[index - 1].covered * 1.5);
  };

  let picked = null;
  // The element under the pointer; a wrapper filling its parent, like a link's text, stands for
  // the parent, as in Arc.
  const under = (event) => {
    let element = document.elementsFromPoint(event.clientX, event.clientY).find((e) => e !== host);
    while (element?.parentElement && element.parentElement !== document.body) {
      const parent = element.parentElement;
      const [inner, outer] = [element.getBoundingClientRect(), parent.getBoundingClientRect()];
      const fills = Math.abs(inner.width - outer.width) < 3 && Math.abs(inner.height - outer.height) < 3;
      if (!fills || parent.textContent.trim() !== element.textContent.trim()) break;
      element = parent;
    }
    return element;
  };
  const close = () => {
    picked = null;
    menu.hidden = true;
    boxes.replaceChildren();
  };
  const choice = (label, className, selector, shown) => {
    const button = make("button", className, label);
    button.addEventListener("mouseenter", () => {
      outline(selector ? all(selector) : [picked]);
      shown(selector);
    });
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      close();
      if (selector) send(picking ? { pick: selector } : { zap: selector });
    });
    return button;
  };
  // "Zap all related elements" is a slider, like Arc's: the further right the pointer, the wider
  // the elements it zaps, the button filling up to the pointer.
  const slider = (related, shown) => {
    const button = make("button", "all", picking ? "Use all related elements" : "Zap all related elements");
    let chosen = related[0];
    button.addEventListener("mousemove", (event) => {
      const rect = button.getBoundingClientRect();
      const fraction = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
      chosen = related[Math.min(related.length - 1, Math.floor(fraction * related.length))];
      const fill = `${(fraction * 100).toFixed(1)}%`;
      button.style.background = `linear-gradient(90deg, #f7943a 0 ${fill}, #8e8e93 ${fill} 100%)`;
      outline(all(chosen.selector));
      shown(chosen.selector);
    });
    button.addEventListener("mouseleave", () => (button.style.background = ""));
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      close();
      send(picking ? { pick: chosen.selector } : { zap: chosen.selector });
    });
    return button;
  };
  // Like Arc's: zap the element, zap the ones like it, or cancel; the selector a choice would
  // zap, with how many it matches, under them.
  const offer = (element, event) => {
    picked = element;
    const one = only(element);
    const related = candidates(element);
    const line = make("div", "selector");
    const shown = (selector) => {
      const matches = all(selector ?? one).length;
      line.replaceChildren(make("b", "", String(matches)), selector ?? one);
    };
    const choices = make("div", "choices", choice(picking ? "Use this" : "Zap this", "one", one, shown));
    if (related.length) choices.append(slider(related, shown));
    choices.append(choice("Cancel", "cancel", null, shown));
    shown(one);
    menu.replaceChildren(choices, line);
    menu.hidden = false;
    const x = Math.min(event.clientX - menu.offsetWidth / 2, innerWidth - menu.offsetWidth - 8);
    const below = event.clientY + 16;
    const y = below + menu.offsetHeight > innerHeight - 48 ? event.clientY - menu.offsetHeight - 16 : below;
    Object.assign(menu.style, { left: `${Math.max(8, x)}px`, top: `${Math.max(8, y)}px` });
    outline([element]);
  };
  // The zaps, like Arc's, as buttons at the bar's left: hovering one shows its elements again
  // over the page dimmed, and a click restores them.
  const UNZAP = "data-arbalete-unzap";
  const previewing = () => dim.style.display !== "none";
  const preview = (selector) => {
    for (const element of document.querySelectorAll(`[${UNZAP}]`)) element.removeAttribute(UNZAP);
    dim.style.display = selector ? "" : "none";
    if (!selector) return;
    const elements = all(selector);
    for (const element of elements) element.setAttribute(UNZAP, "");
    const holes = elements.map((element) => {
      const r = element.getBoundingClientRect();
      return `M${r.left} ${r.top}h${r.width}v${r.height}h${-r.width}Z`;
    });
    shade.setAttribute("d", [`M0 0H${innerWidth}V${innerHeight}H0Z`, ...holes].join(""));
  };
  // The restore tip, at once above its button, the system's coming too late.
  const showTip = (button, text) => {
    tip.textContent = text;
    tip.hidden = false;
    const rect = button.getBoundingClientRect();
    Object.assign(tip.style, {
      left: `${Math.max(8, rect.left)}px`,
      top: `${rect.top - tip.offsetHeight - 8}px`,
    });
  };
  const glyph = () => {
    const ns = "http://www.w3.org/2000/svg";
    const svg = document.createElementNS(ns, "svg");
    svg.setAttribute("width", "15");
    svg.setAttribute("height", "15");
    svg.setAttribute("viewBox", "0 0 16 16");
    const path = document.createElementNS(ns, "path");
    path.setAttribute("d", "M3 2.5h10a.5.5 0 0 1 .5.5v10a.5.5 0 0 1-.5.5H3a.5.5 0 0 1-.5-.5V3a.5.5 0 0 1 .5-.5ZM2 2l12 12");
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", "#3a3a3c");
    path.setAttribute("stroke-width", "1.3");
    svg.append(path);
    return svg;
  };
  const showZaps = (zaps) => {
    if (picking) return;
    preview(null);
    hint.hidden = zaps.length > 0;
    zapped.replaceChildren(
      ...zaps.map((selector, index) => {
        const button = make("button", "", glyph());
        button.addEventListener("mouseenter", () => {
          preview(selector);
          showTip(button, `Restore ${selector}`);
        });
        button.addEventListener("mouseleave", () => {
          preview(null);
          tip.hidden = true;
        });
        button.addEventListener("click", (event) => {
          event.stopPropagation();
          preview(null);
          tip.hidden = true;
          send({ unzap: index });
        });
        return button;
      }),
    );
    if (zaps.length) zapped.append(make("span", "", "← Click to unzap"));
  };
  done.addEventListener("click", (event) => {
    event.stopPropagation();
    send({ cancel: true });
  });

  // Events from the picker's own closed root reach the window retargeted to its host.
  const inMenu = (event) => event.target === host;
  const block = (event) => {
    if (inMenu(event)) return;
    event.preventDefault();
    event.stopImmediatePropagation();
  };
  const listeners = {
    mousemove: (event) => {
      if (picked || inMenu(event) || previewing()) return;
      const element = under(event);
      outline(element ? [element] : []);
    },
    click: (event) => {
      if (inMenu(event)) return;
      block(event);
      if (picked) return close();
      const element = under(event);
      if (element && element !== document.documentElement && element !== document.body) {
        offer(element, event);
      }
    },
    keydown: (event) => {
      if (event.key !== "Escape") return;
      block(event);
      if (picked) return close();
      send({ cancel: true });
    },
    scroll: () => picked && outline([picked]),
  };
  for (const name of ["mousedown", "mouseup", "pointerdown", "pointerup", "auxclick", "dblclick"]) {
    listeners[name] = block;
  }
  for (const [name, listener] of Object.entries(listeners)) {
    addEventListener(name, listener, { capture: true, passive: false });
  }
  globalThis.arbaleteZap = {
    pick() {
      picking = true;
      hint.replaceChildren("Click an element to insert its selector");
      zapped.hidden = true;
      hint.hidden = false;
    },
    zaps: showZaps,
    stop() {
      preview(null);
      for (const [name, listener] of Object.entries(listeners)) {
        removeEventListener(name, listener, { capture: true });
      }
      host.remove();
      delete globalThis.arbaleteZap;
    },
  };
})();

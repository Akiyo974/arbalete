# 0010 — Boosts

- Status: proposed
- Date: 2026-11-09

## Context

Arc's Boosts change how a site looks or behaves, for the user only: hide parts of the page, recolor it, change
its font, text case or size, or run the user's own CSS and JavaScript on it. The website lists them as not
supported yet; a Chrome extension such as Stylus covers only part of them.

What Arc does, from its app bundle (never launched, no Arc data read):

- **A Boost** belongs to one site (its host), with a title and an on/off state; a site can have several. It holds
  any of: zaps (CSS selectors of hidden elements), a color scheme (a color wheel: a primary hue and saturation,
  a secondary hue offset, an "invert lightness" switch), a font family, a text case (capitalize, upper, lower),
  a text size (five steps around normal), custom CSS and custom JavaScript.
- **Injection** is Arc's own engine: per host, one stylesheet per kind (user CSS, font, text case, zaps), one
  script, a color filter for the colors and a zoom factor for the text size. The script runs as a plain page
  script, in the page's own JavaScript world, like a userscript; no isolated-world path shows. Zaps are
  `display: none !important` rules; the font and text case rules leave icon fonts alone.
- **The editor**, from the app and a recording of it: a narrow light panel against the page's right edge, a
  header (close, the Boost's name over a menu to rename, shuffle, reset or delete it, undo), a color square with
  two pucks that tones the whole page (a pastel background, dark text, the accent's color) with invert, advanced
  controls (contrast, brightness, original saturation) and reset, a grid of fonts, Size and Case buttons that
  step through their values, Zap and Code; Code shows CSS and JS in the panel's place. Zap highlights the element
  under the pointer and, on a click, offers to zap this element or all the related ones, with the selector and
  how many it matches, a bar at the page's bottom saying so with Done.
- **Where they show**: a Boost tile and a Boosts section (each Boost with its switch and an edit button) in the
  site controls, New Boost in the menus and the Command Bar, and a Boosts page in Arc's library.
- **Sharing and sync** go through Arc's servers; Boosts holding JavaScript cannot be shared.

## Decision

Boosts like Arc's, every kind included, for the user only.

- **Model** (`arbalete-core`, `boosts`): a profile's Boosts, each with an id, a site, a title, on or off, and its
  zaps, colors, font, text case, text size, CSS and JavaScript. The site is the host without a leading `www.`,
  and a Boost applies to that host and its `www.` form only, like Arc's one host per Boost. The core turns the
  site's active Boosts into one stylesheet and one script, so every rule is tested there.
- **Storage**: `boosts.json` in the profile's directory, apart from `session.json`, which is saved on every tab
  change while Boosts change rarely and can be long. Incognito windows get no Boosts and cannot make one.
  No sharing and no sync; importing Arc's Boosts is left for later (Arc keeps them on its servers).
- **Injection, in CEF Alloy**: through each page's DevTools session, which Arbalete already opens
  (extensions).
  `Page.addScriptToEvaluateOnNewDocument` installs, in every frame of every new document and before the page's
  own scripts, a small script carrying the profile's active Boosts by site. It picks those of its frame's host,
  applies their CSS at once as a constructed stylesheet (`document.adoptedStyleSheets`), which needs no element
  and which a page's Content Security Policy does not block, so nothing flashes, and runs their JavaScript in the
  page's world once the DOM is ready. A change to the Boosts replaces that script for the next documents and
  re-applies the CSS live to the open pages of the site, from the same session (`Runtime.evaluate`); a change
  to the JavaScript reloads them, since a script cannot be taken back. Only `http` and `https` pages get Boosts.
  Frames of another site in another process (site isolation) are not reached, as their DevTools target differs:
  a Boost is meant for the site's own pages.
- **Colors** have no engine filter in CEF: they become an SVG filter on the root element, which tones the page
  from a dark color to a light one, like Arc's: the text takes the accent's hue, darkly, the background its
  complement, softly, keeping a share of the page's own colors ("original saturation"); then CSS `contrast` and
  `brightness`. CSS reaches only filters of the document itself (a `data:` URL is ignored), so the stylesheet's
  script adds a hidden `<svg>` with the filter, built with DOM calls, which a page's Trusted Types do not
  refuse. "Invert lightness" alone is `invert` with a `hue-rotate(180deg)`, pictures turned back. The root gets a
  white background at zero specificity, as the canvas behind it is not filtered. The text size is CSS `zoom` on
  the root element, apart from the page zoom the user sets.
- **The editor**, like Arc's: a light panel against the page's right edge, a view of the main window like the
  other panels: a header with close, the Boost's name over its menu (rename, shuffle, reset, delete, all
  Boosts) and undo; the color square with the accent's puck and its complement's; invert, the advanced
  controls (contrast, brightness, original saturation) and reset; a grid of twenty fonts; Size and Case, which
  step through their values; Zap, with the count of zaps; and Code, which shows the CSS and the JavaScript in
  the panel's place. The code is in Monaco (MIT), Arc's own choice, in a WebKit web view: its CSS completes
  properties and values, its JavaScript the DOM's types through TypeScript and the page's own globals, read from
  the page through its DevTools session. `mise run monaco` downloads Monaco's npm package, pinned and checked
  against its checksum, and the bundle carries its AMD build (about 25 MB, mostly the TypeScript worker) with its
  license and notices; a scheme of the app's own serves its files, which lets its workers load. The CSS applies
  as it is typed; the JavaScript is saved as typed too and runs once the page reloads. Under the code, the selector picker inserts
  an element's selector at the cursor (the zap picker, picking) and the hammer opens the developer tools. Zap
  runs a picker in the page's isolated world, through the DevTools session: it outlines the element under the
  pointer, and a click offers to zap that element or, with a slider, its related ones from the closest to the
  widest within their block, showing the selector and how many it matches; a bar at the page's bottom lists the
  zaps, each restoring its elements, with Done. Undo keeps the editor's states in the core. While a Boost is on
  for the shown page, the site controls' button takes the Space's color, and a site has one Boost on at a time.
- **Entry points**: the site controls' second tile becomes the Boost tile (Copy Link moves to the "…" menu), and
  a Boosts section lists the site's Boosts with a switch and an edit button; New Boost in the Plus menu and the
  Command Bar; a Boosts section in the Library lists all of them, a click opening one over its site in Peek.
- **User JavaScript** runs with the page's rights, as if the site had shipped it: it is the user's own code, never
  fetched or shared, so it is trusted like a bookmarklet. The code window says so above the JavaScript tab.

## Consequences

- Arc users find their Boost kinds, editor and places, but must make their Boosts again.
- Colors look close to Arc's, not the same: a two-color tone of the page, pictures included, with a share of its
  own colors; a filter on the whole page costs some drawing on long, busy pages.
- Every page gets one more script before its own, carrying the profile's active Boosts: a few kilobytes, read
  once per document.
- A page that resets `document.adoptedStyleSheets` drops the Boost's CSS until the next load; few do.
- Boosts depend on the DevTools session Arbalete opens on each page, like the extensions' tab ids already do.

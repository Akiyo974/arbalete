# 0002 — Pages in native containers on macOS

- Status: accepted, migration planned in phase 14
- Date: 2026-11-04

## Context

[0001](0001-web-engine.md) chose Alloy-style browsers in CEF Views windows: the window's layout, the card, Split
View and the developer tools are CEF views, laid out from Rust. On macOS the interface around the pages went
native (SwiftUI), and everything it draws over the pages has to stand outside CEF's views:

- **Chromium draws CEF's views in a window of its own**, the card's, and keeps every click inside it. The
  sidebar, the panels and the bars above the pages are child windows, or views added over Chromium's, which the
  app places where it laid out empty CEF views standing for them.
- **Two systems place one block.** A page's bar and the page under it are placed apart: the page by CEF's
  layout, the bar by the interface, from the frames the app sends it. While a split's pages are resized, the page
  moves first and its bar trails it by a frame, which shows. Making the bar follow its page's own view did not
  help: the page shows at its new place before macOS shows the bar's.
- **Workarounds pile up**: windows raised again after CEF orders its own above them, drops that lose the drag to
  Chromium, frames sent back and forth to draw a drag's room over a split, corners cut out of what CEF draws.

## What was tested

A spike (branch `spike/native-containers`, 2026-11-04) showed two pages in a window of its own, side by side in an
`NSSplitView`. Each is a native container: a SwiftUI bar on top, and under it a view CEF draws the page in, the
browser created as its child (`WindowInfo.parent_view`, Alloy style) rather than as a CEF view.

- Both pages load and show.
- Dragging the split's divider and resizing the window move each bar and its page as one block: AppKit lays
  both out in the same pass.
- Chromium logs two `observer_list` checks at start, without crashing; their cause is still to find.

Not yet checked in the spike: clicks taken at once, the keyboard, and the focus moving between pages.

## Decision

**On macOS, each page is a CEF browser drawn in a native container**, a view of the interface holding the page's
bar and the view the browser draws in. AppKit and SwiftUI lay out the window: the sidebar, the card, Split View
and its handles, the developer tools docked beside the page, and the panels above it. Rust keeps the logic and
tells the interface which pages to show where; it no longer lays out views.

Chromium stays the engine, in Alloy style, with the extension compatibility layer of 0001.

## Consequences

- A bar and its page move together, and what shows over or between pages (a split's outline, a drag's room, the
  handles) is a native view among them, with no frames to send.
- Most child windows and their reordering go: the sidebar, panels and bars are views of the main window.
- The pages' layout moves from Rust (`app/src/shell.rs`: the card, `lay_out_split`, the inspector's stage) to
  the interface, which grows; the C interface carries which page shows where instead of frames.
- Linux and Windows will need their own containers, as they need their own interface already.
- The migration touches the most used paths: it goes step by step (phase 14), each step shipped alone, the
  CEF views path kept until its last user moves.

## Open questions

To answer in the first step of the migration, in this order:

1. **Clicks, keyboard and focus**: a click in a page is taken at once, typing reaches the focused page, and the
   focus moves between pages, the Command Bar and the sidebar as it does now.
2. **The `observer_list` checks** Chromium logs: harmless or a sign of a misuse.
3. **What relies on CEF views today** and must work again: developer tools docked in the card, Peek, extension
   popups, tabs put to sleep and woken, page captures, full screen.

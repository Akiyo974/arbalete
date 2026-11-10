# 0011 — The Linux interface: GTK 4, pages in X11 containers

- Status: accepted
- Date: 2026-11-10

## Context

[0003](0003-rust-logic-native-interface.md) gives each OS a native interface over the same Rust logic, and left
Linux for later. The macOS interface gets the app's state and events as JSON and sends commands as JSON
(`app/src/native.rs`, `app/src/command.rs`); its pages are CEF browsers drawn in views it holds
([0002](0002-native-page-containers.md)).

On Linux, CEF draws a windowed browser in an X11 window, the child of a window the app gives it
(`WindowInfo.parent_window`). GTK 4 no longer embeds foreign windows (GTK 3's `GtkSocket` is gone) and has no
X11 window per widget, only one per toplevel.

## What was tested

A spike (2026-11-10, CEF 156, GTK 4.14, Xvfb) put a GTK window and a CEF page side by side:

- GTK and CEF share one event loop: Chromium's UI thread runs GLib's main loop on Linux, so
  `run_message_loop` also runs GTK's. Chromium must load GTK 4 too (`--gtk-version=4`): two GTK versions in one
  process crash.
- A browser made directly in the GTK window's X11 window fails: GTK 4 gives it a 32-bit visual, for its
  transparent corners, and Chromium creates its own window with the screen's default one, which X refuses
  (`BadMatch`).
- A browser made in an X11 window of the default visual, created by the app as a child of the GTK window, with
  its own colormap and border pixel, shows and draws.
- GTK's X11 connection is not Chromium's: the app syncs GTK's before CEF uses a window it made.

## Decision

- **The Linux interface is GTK 4**, through gtk-rs (`gtk4`, `gdk4-x11`), in Rust: `app/src/gtk.rs` is
  `native.rs`'s counterpart, with the same functions, and `app/src/linux.rs` is `mac.rs`'s, for the clipboard,
  menus, dialogs and opening files. The rest of the app does not tell the two apart.
- **Each page is a CEF browser in an X11 container**: a window of the default visual, child of the GTK window,
  which the interface places where the app lays the page out, as macOS places a page's view.
- **X11 only**: GTK runs on X11 (`GDK_BACKEND=x11`), XWayland on a Wayland desktop. CEF's windowed browsers need
  an X11 parent; Wayland would need off-screen rendering into GTK, slower and a project of its own.
- **Nothing is drawn over a page**: a container covers the GTK window where it is. The Command Bar and the
  panels open in popovers, which GTK makes windows of their own.

## Consequences

- The Linux interface reaches the macOS one's features step by step, each with its tests and a screenshot under
  Xvfb; the first gives the window, the sidebar, the address and the pages.
- Linux builds run `mise run check` in CI on Ubuntu, beside macOS.
- A Wayland session runs Arbalete through XWayland.

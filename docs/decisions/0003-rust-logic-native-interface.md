# 0003 — Rust for the logic, a native interface per OS

- Status: accepted
- Date: 2026-11-07

## Context

Arbalete is built on Chromium through CEF ([0001](0001-web-engine.md)), driven from Rust. Its first interface
was a React/TypeScript page in a CEF view beside the tabs. CEF paints its windowed browsers opaque, so a web
sidebar cannot show the desktop through the window the way Arc's does (an `NSVisualEffectView` blending behind
the window, under the Space's colors), and every panel over the page had to be a web view of its own. Its
measurements ([docs/perf/phase-0.md](../perf/phase-0.md)) met the performance targets: the window's look ruled
it out.

## Decision

- **Rust holds all the logic**: the model, persistence, profiles, auto-archive, URL routing, history, imports,
  extensions, and which page shows where.
- **Each OS gets a native interface.** On macOS it is SwiftUI and AppKit (`app/macos`): the window, the sidebar,
  the library, the Command Bar, the panels above the page and the settings. It gets the app's state and events as
  JSON (`app/src/native.rs`) and sends commands as JSON (`app/src/command.rs`), through a C interface.
- **Linux and Windows** come later, with an interface of their own; how it shows the desktop through the window
  is still open.

The web interface was removed once macOS went native.

## Consequences

- The interface keeps a copy of the state and never decides alone: the UI rules in
  [CONTRIBUTING.md](../../CONTRIBUTING.md#ui-rules) follow from it.
- The pages live in native views of the interface too ([0002](0002-native-page-containers.md)).
- Each new OS means a new interface; the logic is shared.
- `app/build.rs` compiles the interface with the Command Line Tools' `swiftc`, which cannot expand SwiftUI's
  macros: views spell their `State` out, and models are `ObservableObject`s.

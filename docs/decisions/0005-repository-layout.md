# 0005 — Repository layout: the app and one core crate

- Status: accepted
- Date: 2026-11-07

## Context

The doctrine ([CONTRIBUTING.md](../../CONTRIBUTING.md), rule 10) splits code only when it calls for it. The
logic must still be testable without launching the app or CEF.

## Decision

```text
app/                    the desktop app: windows, commands, CEF and extensions (app/src, app/src/shell)
app/macos/              the native macOS interface (SwiftUI)
crates/arbalete-core/   the model (profiles, Spaces, the tab tree, Favorites) and pure logic, without CEF
```

`arbalete-core` is a crate of its own for one reason: its tests run without the engine. Persistence,
auto-archive, URL routing, history and imports are modules of these two crates; a module becomes a crate only
once it hurts readability or build times. Only the app touches CEF; an engine abstraction comes only with a
second engine.

## Consequences

- Tooling stays Cargo's: `rustfmt`, `clippy`, the tests and `cargo-deny` for licenses and advisories, all run by
  `mise run check` and by CI on macOS.
- New code that needs no CEF goes into `arbalete-core`, with its tests.

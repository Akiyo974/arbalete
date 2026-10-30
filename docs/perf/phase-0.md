# Phase 0 performance measurements

Measured on 2026-10-24 with `mise run bench` (`perf/measure.mjs`, removed with the web interface): Apple M5, 32 GB,
macOS 27.0.1, on mains power. CEF 156.0.7 and the UI are release builds; `bundle-cef-app` only builds the
Rust app in the debug profile on macOS, which does not take part in the UI's rendering.

Scenario: the `bench-session` example, 5 Spaces of 100 tabs (500 tabs), half of them in 5 folders per
Space. The sidebar shows the first Space's 50 "Today" tabs: Spaces and folders are not displayed yet.
20 runs per measure, three runs of the script gave the same results within a few milliseconds.

| Measure | Target | Median | p95 | |
|---|---|---|---|---|
| Cold start to a usable sidebar | < 1 s | 270–340 ms | | ✅ |
| Switch tab, UI part: click → app → state → active tab painted | < 16 ms | 7.5 ms | 11 ms | ✅ |
| State update rendered (one tab title changed) | < 5 ms | 7.1 ms | 13 ms | ⚠️ |
| Open the overlay: command → field focused and painted | < 50 ms | 1.6 ms | 6 ms | ✅ |
| UI JavaScript heap | < 150 MB | sidebar 2 MB, overlay 1 MB | | ✅ |
| Shortcuts while the page has focus | work | ⌘T, ⌘R, ⌘[, ⌘], ⌘Q work | | ✅ |
| Overlay above the page | displays | displays, opaque | | ✅ |
| Page scrolling | as smooth as Chrome | smooth on mains power | | ✅ |

Resident memory of all processes, for context: browser 190 MB, GPU 120 MB, utilities 160 MB, four renderers
(sidebar, overlay, one web page, one spare) 90 to 145 MB each. Resident sizes count the shared Chromium
framework in every process.

Not measured yet, because the features do not exist: switching Spaces, dragging tabs, typing in the Command
Bar over 10,000 history entries, and switching to an already loaded page (one content view reloads the
selected tab for now).

## Findings

- The HTML/TS UI meets the targets that can be measured today: phase 1 can start on this UI.
- A state update re-renders the whole sidebar: 7 ms for 50 rows is above the 5 ms target and grows with the
  tab count. Sending changes instead of the whole state, and memoizing tab rows, should fix it in phase 1.
- Choppy scrolling seen in the first builds came from the Mac's low battery, not from Arbalete.
- 2026-10-24, phase 1: the 7 ms above was mostly the wait for the next frame, which the measure
  included. Timed until the sidebar is laid out again, a state update takes 0.1 ms median, 0.3 ms
  p95, before and after memoizing tab rows. Memoizing matters as tabs grow: with 500 rows, one
  renamed tab went from 2.6 ms to 1.1 ms median.
- 2026-10-24, phase 1: cold starts of about 1.8 s came from launching each new build for the first
  time, when macOS checks its code, whatever the signing identity. Later launches start in about
  300 ms, as in phase 0. `mise run bench` now reports the two separately.

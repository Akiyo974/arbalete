# 0004 — Resource saving: pages frozen, then unloaded

- Status: accepted
- Date: 2026-11-07

## Context

Users coming from Arc keep many tabs open, and a live Chromium page costs tens of megabytes. Arc manages each
tab's page through four states (alive, frozen, discarded, closed), with timers, a reaction to memory pressure
and to Low Power Mode, and sites that are never suspended.

## Decision

Arbalete follows the same model, in the app (`app/src/shell/pages.rs`):

| State | What it means | How (CEF) |
|---|---|---|
| **Shown** | The page on screen | A live browser |
| **Hidden** | Loaded, another page shown | A live browser; Chromium throttles it on its own |
| **Frozen** | Loaded, JavaScript and timers paused, memory kept | DevTools protocol `Page.setWebLifecycleState("frozen")` |
| **Unloaded** | The tab keeps its URL, title, icon and a picture of its page | The browser closed, created again when the tab shows |

- A tab's page opens the first time it shows: restored tabs start unloaded.
- A page hidden 5 minutes is frozen, then unloaded after an hour. Under memory pressure, after a minute and 5
  minutes, all at once when it is critical; in Low Power Mode, frozen after a minute.
- Never suspended: pages playing media (their elements or the Media Session) or capturing, and the sites kept
  awake from the site controls. A setting turns suspension off.
- An unloaded page shows its last picture while it loads again, and the sidebar dims the tabs asleep.

This is separate from auto-archive: an unloaded tab stays in the sidebar, an archived tab leaves it.

## Consequences

Measured on 2026-11-01 on macOS, 100 English Wikipedia articles, each shown once, every process of the app
summed:

| State | Processes | Total | Renderers | GPU | Browser |
|---|---|---|---|---|---|
| Every page alive | 112 | ≈ 7.8 GB | 5.86 GB | 1.47 GB | 391 MB |
| Hidden pages unloaded | 12 | ≈ 0.66 GB | 169 MB | 150 MB | 296 MB |

Unloading saves about 92 %. A live page costs about 57 MB in its renderer, and about 13 MB in the GPU process
once hidden, where Chromium keeps its last frame to show it again at once.

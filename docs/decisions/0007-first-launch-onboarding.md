# 0007 — First-launch onboarding

- Status: accepted
- Date: 2026-11-08

## Context

Arbalete opens straight onto an empty sidebar: nothing offers to bring another browser's data, to set up the
first Space or to make Arbalete the default browser, and the main shortcuts are only in the menus (issue #215).
Arc runs a long first-launch wizard in its own window: an account, a demo video, then import, favourites,
bookmarks, passwords, theme, ad blocker, default browser and a member card. Arbalete has no account and wants
people browsing within a minute.

## Decision

A short, playful flow in a borderless window of its own, 1000 × 660 pt, centred, shown before the main window.
The app's text stays in English, like the rest of Arbalete.

| Step | Shown when | What it offers | Skip |
|---|---|---|---|
| **Welcome** | Always | The app icon, the name written letter by letter, a line of intent, "Let's go" | "Skip setup" ends the whole flow |
| **Import** | Another browser is found | The browsers and profiles the importers detect (Arc, Chromium, Firefox, Safari), as checkboxes: several can be ticked, and each becomes its own Space with its own profile, as the import assistant does. Arc is ticked at first. A Safari profile without Full Disk Access is locked, with the steps to unlock it and a button to System Settings; the others still import | "Not now" |
| **Import done** | Something was imported | What came in: Spaces and profiles, tabs, bookmarks, history | — |
| **Your Spaces** | The import was not only Arc's | One tab per Space the import made: name, icon (emoji picker) and colour (the app's theme picker) for each | "Keep defaults" |
| **First Space** | Nothing was imported | Name, icon, colour (the theme picker), and "Also make a Work Space", which adds a second Space in a profile of its own | "Keep defaults" |
| **Default browser** | Arbalete is not the default yet | A button that triggers macOS's own confirmation | "Not now" |
| **Shortcuts** | Always | Four tiles: open or search (⌘T), hide the sidebar (⌘S), next Space (⌘→), copy the URL (⇧⌘C); each lights up when its shortcut is pressed or clicked | "Start browsing" works at any time |

- **Components**: the Spaces steps embed the theme picker of `ThemePicker.swift` as it is (pad with the
  appearance and −/+ buttons, palette pages, intensity wave, grain dial); the end uses the toasts of
  `Toast.swift`, at the page's top right. Imports go through the existing import code; Bookmarks bars go to
  Favorites, the assistant's default.
- **Keys**: the tiles read their keys from the menus, so a changed shortcut shows its new keys. Pressed during
  the flow, the shortcuts only light their tile: no tab opens.
- **Choices apply** when their step is confirmed, through the existing import, Space and profile code.
- **Background**: every step shows the selected Space's theme as the main window does, the desktop blurred
  through the window under the theme's colours, intensity and grain, following the picker at once.
- **A new Space's theme**: the logo's three hues (assets/icon.svg) at one saturation and lightness, which the
  picker's pad shows as they are: #a352ff, #5289ff, #ff5252, intensity 0.8, grain 0.4. It is the theme of the
  first launch's Space and of the Spaces an import makes; a Space made by hand keeps the colour picked for it.
- **Motion**: the name's letters rise from a blur, 0.5 s each, 70 ms apart; the Welcome text and button
  follow 0.6 s after 0.85 s; steps slide 24 pt and fade, 0.45 s; a key tile presses in 0.15 s and ticks in
  0.2 s; at the end the flow's window fades out over the main window (0.6 s), already showing the chosen
  Space, and a toast drops in: all set, ⌘T opens a page. With Reduce Motion, steps only fade.
- **Moving between steps**: Return for the main button, Escape for the step's skip; a bar at the bottom holds
  a Back button and the step dots, each clickable; the mouse's back and forward buttons and a two-finger
  swipe move a step, like pages, forward never past the last.
- **When it shows**: on the very first launch only, with no Arbalete data. Existing users never get it on
  update. Quitting midway shows it again from the start next launch.
- **Stored**: one `onboarded` flag in the settings, set at the end or by "Skip setup". No step is remembered,
  nothing is sent anywhere.
- **Again by hand**: Help › Welcome to Arbalete… and "Show Welcome" in the Command Bar. On existing data
  nothing is replaced: the import asks which profile to merge into, the Spaces step edits the current Space,
  and the Work box adds a Space and a profile.
- **No reminders**: unlike Arc, the default browser question is not asked again on later days; it stays in
  Settings.

From Arc, Arbalete keeps the separate window, the order (import, then the Spaces and their colours, then the
default browser), a skip on every step, dropping the import steps when there is nothing to import, and keys
that light up as they are pressed. It leaves out the account, the video, favourites, passwords, the ad
blocker and the member card. None of Arc's text, images or assets is used.

## Consequences

- With nothing to import, the flow is four screens (Welcome, First Space, Default browser, Shortcuts); an Arc
  import skips the Spaces step.
- The import code, the theme and emoji pickers and the toasts are reused, not duplicated.
- The browser of a new user opens in the logo's colours, the same as the flow, with no jump between them.
- Suggested extensions, the Bookmarks bar choice and localized text are left for later.

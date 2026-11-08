# 0008 — Importing extensions

- Status: accepted
- Date: 2026-11-09

## Context

The import (decision 0007 and the import assistant) brings Arc's sidebar and other browsers' bookmarks and
history, but not their extensions: after importing a Chrome or Arc profile, the user installs each extension
again by hand. Arc imports them: its importer reads the source profile's `Secure Preferences`, keeps the
extensions installed from the Chrome Web Store that the browser did not install by default, lists them with a
switch each, and installs those left on from the Web Store, without asking anything more.

That silent install is Chromium's `WebstoreInstallWithPrompt` with its prompt turned off: it fetches the item and
its manifest from the store, downloads the CRX, and installs it with an approval that skips the confirmation,
still checking the store's signature, its blocklist and that the CRX asks for no more than the store's manifest.
CEF does not expose it: through the store page, a Web Store install always ends on Chromium's confirmation
(decision 0001, question 3). Arbalete already builds its own CEF (amenophis/cef-builds), with patches.

## Decision

- **Which**: from a Chromium-family profile (Arc included), the extensions its `Secure Preferences` lists as
  installed from the Web Store (`from_webstore`) and not by the browser or the computer's maker
  (`was_installed_by_default`, `was_installed_by_oem`), with their name and icon from their folder in
  `Extensions`. Those already in the Arbalete profile imported into are left out. Firefox and Safari
  extensions are not Chrome's: none are offered.
- **Where**: after an import that found some, a list with a switch per extension, all on: a step of its own in
  the welcome flow, after "Import done", and the import assistant's next screen. "Add" installs those left on,
  "Not now" skips. That list is the user's consent: nothing more is asked.
- **How, with Arbalete's CEF**: a patch in cef-builds adds one function to CEF,
  `arbalete_install_webstore_extension(cache_path, id, done, context)`, which runs Chromium's installer without
  its prompt and without the "extension added" bubble, in the profile at that path. Arbalete finds it at run
  time and installs the chosen extensions one at a time, in the background, with no window, as soon as they are picked, the welcome flow
  going on meanwhile. It is a plain C
  export rather than a CEF API, so the `cef` crate needs no change, and only native code in Arbalete's own
  process can call it: no page or extension can trigger a silent install.
- **How, with the official CEF** (the function is missing): as before, one at a time in a tab of the main
  window on the extension's Web Store page, in the first Space of the profile imported into. Once the page is
  ready, Arbalete presses its "Add to Chrome" button, and Chromium asks the user to confirm, once per
  extension. The tab then moves to the next, and closes after the last. Picked in the welcome flow, they start
  once it ends. A new profile meets Google's cookie consent page first, which only the user answers. An
  extension already installed, or that the store no longer offers or shows a button for, is skipped.
- **Either way**: the windows Chromium opens by itself during the installs, for an extension's welcome page,
  stay invisible and close after the last, their page moving to a tab.
- **Nothing copied**: the extensions' files and their settings stay in the other browser. Copying the folders
  and writing Chromium's preferences would skip the store, but Chromium checks those preferences against a
  signature and turns off extensions it cannot vouch for.

## Consequences

- With Arbalete's CEF, an imported Chrome or Arc profile ends with its extensions without a click more, like
  Arc's: no store page, no Google consent page, no confirmation.
- An extension's own first-run page, which it opens itself once installed, still opens, as in Chrome and Arc.
- Extensions come back with the store's current version, without their own settings, like Arc's.
- One more local CEF patch to carry on each CEF update: a new file of about 70 lines on a Chromium class stable
  for years, plus a line in CEF's build file. A drift fails the CEF build rather than the install.
- Two install paths to keep: the fallback through the store's page depends on that page, and if Google changes
  it, the user presses "Add to Chrome" in the same tab instead. Closing the store's tab stops the installs left.

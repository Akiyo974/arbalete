# 0006 — Releases: a self-signed app, a tap of our own and Sparkle

- Status: accepted
- Date: 2026-11-07

## Context

Arbalete ships as a free download for macOS. Notarization needs an Apple developer account ($99 a year), and
Homebrew's main cask list takes only notarized apps. macOS ties the permissions it gave an app (camera,
microphone, folders) to its signature, and an ad hoc signature changes with every build.

## Decision

- **Signing**: a self-signed code signing certificate, "Arbalete Release", valid for ten years and the same for
  every release, so permissions survive updates. It is stored as repository secrets (`SIGN_P12`,
  `SIGN_P12_PASSWORD`), with a copy kept by its owner. No notarization for now.
- **Distribution**: a disk image on GitHub releases, and a cask in a tap of our own, `amenophis/homebrew-tap`
  (`brew install --cask amenophis/tap/arbalete`). The cask removes the quarantine after installing, since adding
  the tap already trusts it; a disk image downloaded by hand opens once through Open Anyway (README).
- **Releases**: SemVer from 0.1.0. A release is a commit bumping `app/Cargo.toml`, then its `v*` tag; the release
  workflow checks the tag, builds and signs the disk image, publishes it with notes generated from the commits,
  and updates the cask with a token allowed to write to the tap only.
- **Updates inside the app** with [Sparkle](https://sparkle-project.org) 2 (MIT): the app reads the appcast the
  website builds from the releases and installs only a disk image signed with the EdDSA key whose public half is
  `packaging/sparkle/public-key`. The cask says `auto_updates true`; development builds never update themselves.
- **Licenses**: the app carries Arbalete's, Chromium's and CEF's (`CREDITS.html`) and its Rust crates'
  (`Rust-CREDITS.html`), which redistributing them requires.

## Consequences

- Losing the release certificate costs every user their permissions once; losing the Sparkle private key stops
  updates from inside the app.
- A Developer ID and notarization would remove the first-launch step and open Homebrew's main cask list.
- Builds are Apple Silicon only.

# Arbalete

A cross-platform browser inspired by Arc: vertical sidebar, Spaces, profiles and link routing.
Built with Rust on Chromium, with a native SwiftUI interface on macOS.

> **Status:** early development, preview builds for macOS. Website: [arbalete.co](https://arbalete.co).

## Goals

- A vertical sidebar with Favorites, pinned tabs and today's tabs.
- Spaces, each with its own tabs and theme, attached to isolated profiles.
- Chrome extensions, installed from the Chrome Web Store.
- Automatic archiving of inactive tabs, and resource saving for background tabs.
- Link routing: send links opened from other apps to the right Space.
- A Command Bar for URLs, search, tabs and history.
- Import from Arc and from the browsers installed on your system.
- macOS first, then Linux and Windows.

See [docs/PLAN.md](docs/PLAN.md) for the full plan.

## Install

On a Mac with Apple Silicon and macOS 14 or later, with [Homebrew](https://brew.sh):

```sh
brew install --cask amenophis/tap/arbalete
```

Or download the disk image from the [releases](https://github.com/amenophis/arbalete/releases). Arbalete is not
notarized by Apple yet, so macOS blocks the first launch of a downloaded disk image (Homebrew's cask clears that
itself). Open it once from System Settings → Privacy & Security → Open Anyway, or run:

```sh
xattr -dr com.apple.quarantine /Applications/Arbalete.app
```

## Development

The tools (Rust, CMake, Ninja) are pinned in [.config/mise.toml](.config/mise.toml). On macOS,
the native interface also needs Swift, from the Xcode Command Line Tools (`xcode-select --install`).
With [mise](https://mise.jdx.dev/) installed:

```sh
mise install     # install the tools
mise run setup   # download CEF and Sparkle, and install bundle-cef-app, the CEF bundling tool
mise run run     # bundle the app and run it
```

`mise run setup` downloads CEF (about 350 MB) to `~/cef` once, and Sparkle, the updater, to `~/sparkle`; every build uses them through `CEF_PATH` and `SPARKLE_PATH`.

Development builds are "Arbalete Dev": their own bundle identifier (`io.github.amenophis.arbalete.dev`) and data
folder (`~/Library/Application Support/Arbalete Dev`), so an installed Arbalete keeps its own data, preferences and
permissions. `ARBALETE_DATA_DIR` points either to another folder.

In Visual Studio Code, install the recommended extensions and open the repository from a shell where mise is
active (`code .`), so that rust-analyzer builds with the same toolchain and `CEF_PATH`. The mise tasks are VS Code
tasks, and "Debug the app" builds and debugs the app on a session of its own in the temporary folder.

On macOS, builds are signed ad hoc, so the system asks again for each permission (such as access to the
Documents folder) after every build. To keep permissions across builds, create a code signing certificate in
Keychain Access (Certificate Assistant → Create a Certificate, type "Code Signing", trusted for code signing)
and name it in `.config/mise.local.toml`, which is not committed:

```toml
[env]
ARBALETE_SIGN_IDENTITY = "Arbalete Dev"
```

### Releasing

`mise run release` bundles an optimized app, signed with `ARBALETE_SIGN_IDENTITY`, in
`target/dmg/Arbalete-<version>-arm64.dmg`. To publish one, bump the version in `app/Cargo.toml` on `main`, then
push its tag (`v0.1.0`): the [release workflow](.github/workflows/release.yml) builds the disk image signed with the
release certificate, publishes it on GitHub and updates the cask in
[amenophis/homebrew-tap](https://github.com/amenophis/homebrew-tap). It needs four secrets: `SIGN_P12` (the
certificate and its key, in base64), `SIGN_P12_PASSWORD`, `TAP_TOKEN` (a token allowed to write to the tap) and
`SPARKLE_PRIVATE_KEY`.

The app updates itself with [Sparkle](https://sparkle-project.org): it reads the appcast the website builds from
the releases (`https://arbalete.co/appcast.xml`) and installs only a disk image signed with the release key, an
EdDSA key pair made once with Sparkle's `generate_keys` (`mise run sparkle` downloads it):

1. `~/sparkle/<version>/bin/generate_keys --account arbalete` creates the pair in your login keychain and prints
   the public key.
2. Put the public key, alone, in [packaging/sparkle/public-key](packaging/sparkle/public-key) and commit it.
3. `~/sparkle/<version>/bin/generate_keys --account arbalete -x sparkle-private-key` exports the private key:
   store it in the `SPARKLE_PRIVATE_KEY` secret and in a password manager, then delete the file. Never commit
   it. Losing it means users can no longer update from inside the app.

`mise run release` fails while `packaging/sparkle/public-key` holds no key. A release workflow without the
`SPARKLE_PRIVATE_KEY` secret fails before publishing anything.

## Logo

The logo is a bolt seen from behind: three vanes, one color each. The app icon is
[assets/icon.svg](assets/icon.svg), with [icon-light.svg](assets/icon-light.svg) for light backgrounds and
[icon-mono.svg](assets/icon-mono.svg) for single-color use. The mark alone is [mark.svg](assets/mark.svg) on dark
backgrounds and [mark-light.svg](assets/mark-light.svg) on light ones; [favicon.svg](assets/favicon.svg) follows the
system theme, and [loader.svg](assets/loader.svg) animates it. The macOS icon, `app/resources/mac/arbalete.icns`, is
rendered from icon.svg (16 to 1024 px) with `iconutil`; development builds use
[icon-dev.svg](assets/icon-dev.svg), the same with a dashed amber edge, rendered into `arbalete-dev.icns`. The proposals it was chosen from are in
[assets/logo-history](assets/logo-history).

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) first: simplicity is the project's doctrine.
To report a security issue, see [SECURITY.md](SECURITY.md).

## License

[Mozilla Public License 2.0](LICENSE).

Arbalete embeds CEF built with Chromium's proprietary codecs (H.264, AAC, HEVC), from
[amenophis/cef-codecs](https://github.com/amenophis/cef-codecs). FFmpeg, part of Chromium, decodes
them under the LGPL 2.1; macOS decodes HEVC. Their licenses, and those of Chromium's other
components, are in the app's `CREDITS.html`. These formats may be covered by patents in some
countries.

Arbalete is an independent project, not affiliated with or endorsed by The Browser Company.
Arc is a trademark of its owner.

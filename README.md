# Arbalete

A cross-platform browser inspired by Arc: vertical sidebar, Spaces, profiles and link routing.
Built with Rust on Chromium, with a native SwiftUI interface on macOS.

> **Status:** early development. Nothing is usable yet.

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

## Development

The tools (Rust, CMake, Ninja) are pinned in [.config/mise.toml](.config/mise.toml). On macOS,
the native interface also needs Swift, from the Xcode Command Line Tools (`xcode-select --install`).
With [mise](https://mise.jdx.dev/) installed:

```sh
mise install     # install the tools
mise run setup   # download CEF and install bundle-cef-app, the CEF bundling tool
mise run run     # bundle the app and run it
```

`mise run setup` downloads CEF (about 350 MB) to `~/cef` once; every build uses it through `CEF_PATH`.

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

## Logo

The app icon is [assets/icon.svg](assets/icon.svg), with a single-color version in
[assets/icon-mono.svg](assets/icon-mono.svg). The macOS icon, `app/resources/mac/arbalete.icns`, is rendered
from it (16 to 1024 px) with `iconutil`. The proposals it was chosen from are in [assets/logo-history](assets/logo-history).

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) first: simplicity is the project's doctrine.
To report a security issue, see [SECURITY.md](SECURITY.md).

## License

[Mozilla Public License 2.0](LICENSE).

Arbalete is an independent project, not affiliated with or endorsed by The Browser Company.
Arc is a trademark of its owner.

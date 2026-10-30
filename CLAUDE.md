# Arbalete

Browser inspired by Arc: Rust on Chromium (CEF), with a native SwiftUI interface (`app/macos`), macOS only for
now. Plan: [docs/PLAN.md](docs/PLAN.md).

## Mandatory rules

- **Simplicity first**: apply the doctrine in [CONTRIBUTING.md](CONTRIBUTING.md). Before proposing code, ask
  "can it be simpler?" and simplify if so.
- **Open-sourceable**: everything committed must be publishable (MPL-2.0). Follow the "Open source" section of
  [CONTRIBUTING.md](CONTRIBUTING.md): compatible dependency licenses, no proprietary material from Arc or other
  products, no secrets or personal data.
- **English only**: code, comments, documentation, commit messages and pull requests.
- **Commits**: follow [CONTRIBUTING.md](CONTRIBUTING.md#commits). Sole author `Jérémy LEHERPEUR <amenophis@leherpeur.net>`.
  **No mention of Claude or any tool** in commits and pull requests: no `Co-Authored-By`, no "Generated with" line.
- Every commit builds and passes `mise run check`.

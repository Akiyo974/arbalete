# Contributing to Arbalete

## Doctrine: the simplest possible code

Simplicity is the project's first requirement, ahead of elegance and ahead of generality.
Simple code reads quickly, is easy to test and can be deleted without fear.

1. **Solve today's need.** No code for a hypothetical need: no option, parameter, extension point or generic case
   "just in case".
2. **No abstraction before the second real use.** A trait, a generic or a layer of indirection is introduced only
   once at least two concrete implementations or callers exist. Until then, a well-bounded module is enough.
3. **As little code as possible.** Prefer deleting to adding. No dead code, no commented-out code, no TODO without
   an issue.
4. **Names that explain.** A comment explains *why*, never *what* the code does. If the code needs a comment to be
   understood, rewrite it first.
5. **Short, direct functions.** A function does one thing. Little nesting: early returns rather than cascading `if`s.
6. **Simple data structures.** Plain `struct`s and `enum`s rather than hierarchies. State lives in one place (Rust);
   the UI only keeps a copy.
7. **Few dependencies.** Every dependency (crate or npm package) must be justified in the commit that adds it.
   Standard library first.
8. **No optimization without measurement.** The only exceptions are the UI rules in [docs/PLAN.md](docs/PLAN.md).
9. **Test behavior, not implementation.** A test must survive a refactoring that does not change behavior.
10. **Split when needed, not before.** One more file, module or crate only when the existing one becomes hard to
    read or test.

**Mandatory review question: "Can it be simpler?"** If yes, simplify before merging.

## Open source

The repository is meant to be public. Everything committed must be publishable.

- **License**: Arbalete is under the [Mozilla Public License 2.0](LICENSE). By contributing, you agree that your
  contribution is licensed under the same terms.
- **Dependencies**: only licenses compatible with MPL-2.0 (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0…). No GPL or
  AGPL dependency. `cargo-deny` checks this in CI.
- **No third-party proprietary material**: no code, assets, icons or text copied from Arc or any other proprietary
  product. Other browsers' file formats are documented only as far as needed to import users' own data.
- **Own names**: features use Arbalete's names, not another product's brand names.
- **No secrets or personal data**: no keys, tokens, local paths or personal data in code, tests or history.
  Test data is generated.

## Language

Everything in the repository is written in English: code, comments, documentation, commit messages and pull
requests.

## Commits

### Identity

Commits are authored only by the project author:

```text
Jérémy LEHERPEUR <amenophis@leherpeur.net>
```

No other author or tool mention (no `Co-Authored-By`, no tool signature in the message).

### Format: Conventional Commits

```text
<type>(<scope>): <imperative summary, lowercase, no trailing period, 72 characters max>

<optional body: why the change, not how; lines wrapped at 72 characters>
```

| Type | Use |
|---|---|
| `feat` | New user-visible feature |
| `fix` | Bug fix |
| `perf` | Measured performance improvement |
| `refactor` | Structural change with no behavior change |
| `test` | Tests only |
| `docs` | Documentation only |
| `build` | Dependencies, build configuration, packaging |
| `ci` | Continuous integration |
| `chore` | Maintenance with no effect on the product |

Scopes: `core`, `store`, `engine`, `ui`, `app`, `import`, `routing`, `archive`, `platform`, `perf`, `ci`, `docs`.

Examples:

```text
feat(core): add profiles and spaces to the sidebar model
fix(ui): keep tab selection when switching spaces
build(app): add a single-instance lock for external links
```

### Rules

- **One commit = one coherent change** that stands on its own and can be summarized in one line.
- **Every commit builds and passes the checks**: `mise run check` (`rustfmt`, `clippy`, Rust tests,
  `cargo-deny`).
- **Code and its tests go in the same commit.**
- **No mixing**: a refactoring and a feature are two commits.
- **An added dependency** is justified in the commit body.

### Branches

- `main` is always in a working state.
- One short-lived branch per topic: `<type>/<topic>` (e.g. `feat/sidebar-drag-and-drop`).
- Integration through pull requests, merged by **rebase** (no merge commits, no squash), so history keeps the
  atomic commits.
- A release is a `vX.Y.Z` tag (SemVer).

#!/bin/sh
# Writes a release's notes from its commits since the previous tag that change the app or what
# ships it (its code, its crates, its icon, its packaging; not the website, CI or docs), sorted the way the
# website's release notes page labels them: New (feat), Improved (perf) and Fixed (fix). Other
# commits (docs, ci, build, refactor…) stay out.
#
#   packaging/release-notes.sh v0.1.1 > notes.md
set -eu
tag=$1
previous=$(git describe --tags --abbrev=0 "$tag^" 2>/dev/null || true)
git log --no-merges --format=%s "${previous:+$previous..}$tag" -- app crates assets packaging Cargo.toml Cargo.lock | awk '
  function add(list, count) { sub(/^[^:]*: /, ""); list[count] = toupper(substr($0, 1, 1)) substr($0, 2) }
  /^feat(\([^)]*\))?!?: / { add(new, ++n); next }
  /^perf(\([^)]*\))?!?: / { add(improved, ++i); next }
  /^fix(\([^)]*\))?!?: / { add(fixed, ++f); next }
  function section(title, list, count,   k) {
    if (!count) return
    printf "%s### %s\n\n", (printed++ ? "\n" : ""), title
    for (k = 1; k <= count; k++) printf "- %s\n", list[k]
  }
  END {
    section("New", new, n); section("Improved", improved, i); section("Fixed", fixed, f)
    if (!printed) print "Maintenance release."
  }'

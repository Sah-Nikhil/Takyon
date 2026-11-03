#!/usr/bin/env bash
# CI's `changes` job: which slow jobs a pull request has reason to run.
# Writes `search`, `focus` and `app` as true/false to $GITHUB_OUTPUT. Anything
# but a pull request runs everything. Paths and their reasons: docs/tbc/0017.
#
# Local check: FILES=$'a\nb' EVENT=pull_request bash scripts/ci-changes.sh
set -euo pipefail

out="${GITHUB_OUTPUT:-/dev/stdout}"

if [ "${EVENT:-}" != "pull_request" ]; then
  printf 'search=true\nfocus=true\napp=true\n' >> "$out"
  exit 0
fi

files="${FILES:-$(git diff --name-only "$BASE...$HEAD")}"

# Changing CI or the Rust build itself can break anything: everything runs.
everything='^(\.github/workflows/ci\.yml|scripts/ci-changes\.sh|rust-toolchain\.toml|apps/desktop/src-tauri/(Cargo\.(toml|lock)|build\.rs))$'

# `!s`: retrieval, WinHTTP, the DuckDuckGo parser, and its live test.
search='^apps/desktop/src-tauri/(src/search/|tests/web_search\.rs)'

# Focus's live tests: the browser driver, what it matches, the enforcer that
# calls it, the block page, the site-icon fetch, and the two live test files.
focus='^apps/desktop/src-tauri/(src/focus/(browser|site|runtime|icons)\.rs|focus/blocked\.html|tests/focus(_browser)?\.rs)'

# The bench measures the built app: any change to its code, assets or
# dependencies, and the bench itself. Docs, tests and other scripts do not.
app='^(apps/desktop/(src/|src-tauri/src/|src-tauri/tauri.*\.json|src-tauri/capabilities/|index\.html|vite\.config\.ts|package\.json)|packages/shared/src/|package\.json|bun\.lock|scripts/bench)'

hit() {
  if grep -Eq "$1" <<<"$files" || grep -Eq "$everything" <<<"$files"; then
    echo true
  else
    echo false
  fi
}

{
  echo "search=$(hit "$search")"
  echo "focus=$(hit "$focus")"
  echo "app=$(hit "$app")"
} >> "$out"

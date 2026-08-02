#!/usr/bin/env bash
# scripts/commit-generated.sh
#
# Stage-and-commit body for the `commit` make target.
#
# `make commit` materializes first through the pipeline's single producer
# (`check-sync` in update mode over every output family, run as a recipe step
# rather than a prerequisite because a prerequisite edge cannot carry that mode),
# then lists every generator-owned path via `gmeow-dev sync --list-paths`,
# stages whichever of those paths actually exist, and commits the result. A
# clean tree (nothing generator-owned changed) is not an error condition the
# caller silently swallows — it HARD-FAILS with an explicit message, because a
# human running `make commit` expects a commit to be made.
#
# Inputs are explicit environment variables, not re-derived here, so the
# Makefile stays the single place `gmeow-dev` and the commit message text are
# configured:
#
#   GMEOW_DEV   the gmeow-dev invocation (e.g. `cargo run -q -p gmeow-dev-cli --`).
#               This is intentionally expanded UNQUOTED below so a multi-word
#               invocation word-splits into its executable + arguments, exactly
#               as the Makefile's own bare `$(GMEOW_DEV)` expansions already do.
#   MESSAGE     the commit message text.
#
# This script commits ONLY generator-owned paths under a message that says so.
# A caller who already has unrelated work staged (for a different, unrelated
# commit) must never have that work silently swept into this one — the repo's
# fail-closed style is to refuse loudly, naming the offending paths, rather
# than to quietly narrow or reinterpret what gets committed.
set -euo pipefail

: "${GMEOW_DEV:?GMEOW_DEV must be set to the gmeow-dev invocation}"
: "${MESSAGE:?MESSAGE must be set to the commit message}"

if ! git diff --cached --quiet; then
  echo "Refusing to commit: the index already has staged changes unrelated to" >&2
  echo "generator-owned paths. This script only commits generated artifacts;" >&2
  echo "commit or unstage the following first:" >&2
  git diff --cached --name-only >&2
  exit 1
fi

# shellcheck disable=SC2086 # GMEOW_DEV is a command line, not a single token.
REGENERATED_PATHS=$(GMEOW_CONSOLE=silent $GMEOW_DEV sync --list-paths)

# Line-oriented, not `for p in $REGENERATED_PATHS`: word-splitting the whole
# blob would also glob-expand any path containing `*`/`?`/`[`, and mishandle
# any path containing whitespace. `--` stops `git add` from treating a path
# that happens to start with `-` as an option.
while IFS= read -r p; do
  [ -n "$p" ] || continue
  if [ -e "$p" ]; then git add -- "$p"; fi
done <<<"$REGENERATED_PATHS"

if git diff --cached --quiet; then
  echo "Nothing to commit."
  exit 1
fi

git commit -m "$MESSAGE"

git diff --quiet || echo "Warning: unstaged changes remain. Stage them separately if needed."

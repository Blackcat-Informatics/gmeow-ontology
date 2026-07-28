#!/usr/bin/env bash
# scripts/commit-generated.sh
#
# Stage-and-commit body for the `commit` make target.
#
# `make commit` regenerates first (as a SUB-make from the Makefile, so the
# regen-guard's MAKELEVEL check never fires — see the comment in the Makefile's
# `commit` recipe for why that recursion must not be a same-level prerequisite),
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
set -euo pipefail

: "${GMEOW_DEV:?GMEOW_DEV must be set to the gmeow-dev invocation}"
: "${MESSAGE:?MESSAGE must be set to the commit message}"

# shellcheck disable=SC2086 # GMEOW_DEV is a command line, not a single token.
REGENERATED_PATHS=$(GMEOW_CONSOLE=silent $GMEOW_DEV sync --list-paths)

for p in $REGENERATED_PATHS; do
  if [ -e "$p" ]; then git add "$p"; fi
done

if git diff --cached --quiet; then
  echo "Nothing to commit."
  exit 1
fi

git commit -m "$MESSAGE"

git diff --quiet || echo "Warning: unstaged changes remain. Stage them separately if needed."

#!/bin/sh
# scripts/lint-issue-refs.sh
# Reject issue/PR number references of the form #NNN in Rust comments and
# developer-facing Markdown documentation.
#
# This script is intentionally POSIX-shell and ripgrep-based so it adds no
# Python/Rust runtime dependency beyond what the repository already uses.
set -eu

status=0

# --- Rust source comments -----------------------------------------------------
# Match #NNN tokens inside //, ///, //! and /* */ style comments.  We do not
# try to fully parse Rust; the tree is already clean, so simple regexes are
# sufficient to catch regressions.  Any false positives can be handled by
# refining the patterns or the allow-list below.
rust_matches=$(rg -n --type rust \
    -e '//.*#\d{3,}' \
    -e '/\*.*#\d{3,}' \
    crates/ \
    || true)

if [ -n "$rust_matches" ]; then
    echo "Found issue/PR number references in Rust comments:" >&2
    echo "$rust_matches" >&2
    status=1
fi

# --- Markdown documentation ---------------------------------------------------
# Scan developer-facing Markdown.  Exclude GitHub-process artifacts and the
# brand colour palette, whose all-digit hex codes are legitimate #NNNNNN
# values, not issue references.
md_matches=$(rg -n -e '#\d{3,}' \
    --glob '*.md' \
    --glob '!docs/BRAND.md' \
    --glob '!.github/**' \
    README.md AGENTS.md CLAUDE.md CODE_OF_CONDUCT.md CONSTITUTION.md \
    CONTRIBUTING.md LICENSING.md SECURITY.md \
    docs/ \
    conformance/ \
    crates/ \
    coverage/ \
    bench/README.md \
    fuzz/README.md \
    || true)

if [ -n "$md_matches" ]; then
    echo "Found issue/PR number references in Markdown docs:" >&2
    echo "$md_matches" >&2
    status=1
fi

exit "$status"

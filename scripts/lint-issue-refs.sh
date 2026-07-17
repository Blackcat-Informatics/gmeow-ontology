#!/bin/sh
# scripts/lint-issue-refs.sh
# Reject issue/PR number references of the form #NNN in Rust comments and
# developer-facing Markdown documentation.
#
# This script is intentionally POSIX-shell and ripgrep-based so it adds no
# Python/Rust runtime dependency beyond what the repository already uses.
set -eu

command -v rg >/dev/null 2>&1 || { echo "ripgrep (rg) is required" >&2; exit 2; }

status=0

# --- Rust source comments -----------------------------------------------------
# Match #NNN tokens inside //, ///, //! style comments and across lines of
# /* */ block comments.  We do not try to fully parse Rust; the tree is
# already clean, so simple regexes are sufficient to catch regressions.  Any
# false positives can be handled by refining the patterns or the allow-list
# below.
rust_line_code=0
rust_line_matches=$(rg -n --type rust \
    -e '//.*#\d{3,}' \
    crates/) || rust_line_code=$?
if [ "$rust_line_code" -eq 2 ]; then exit 2; fi

rust_block_code=0
rust_block_matches=$(rg -n --type rust -U \
    -e '(?m)(?:^|[^\S\n])/\*[\s\S]*?#\d{3,}[\s\S]*?\*/' \
    crates/) || rust_block_code=$?
if [ "$rust_block_code" -eq 2 ]; then exit 2; fi

if [ -n "$rust_line_matches" ] || [ -n "$rust_block_matches" ]; then
    echo "Found issue/PR number references in Rust comments:" >&2
    if [ -n "$rust_line_matches" ]; then
        echo "$rust_line_matches" >&2
    fi
    if [ -n "$rust_block_matches" ]; then
        echo "$rust_block_matches" >&2
    fi
    status=1
fi

# --- Markdown documentation ---------------------------------------------------
# Scan developer-facing Markdown recursively.  Exclude GitHub-process artifacts,
# generated artifacts (their issue references come from canonical sources), and
# the brand colour palette, whose all-digit hex codes are legitimate #NNNNNN
# values, not issue references.
md_code=0
md_matches=$(rg -n -e '#\d{3,}' \
    --glob '*.md' \
    --glob '!docs/BRAND.md' \
    --glob '!.github/**' \
    --glob '!generated/**' \
    --glob '!vendor/**' \
    .) || md_code=$?
if [ "$md_code" -eq 2 ]; then exit 2; fi

if [ -n "$md_matches" ]; then
    echo "Found issue/PR number references in Markdown docs:" >&2
    echo "$md_matches" >&2
    status=1
fi

exit "$status"

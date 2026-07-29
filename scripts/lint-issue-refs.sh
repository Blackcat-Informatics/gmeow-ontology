#!/bin/sh
# scripts/lint-issue-refs.sh
# Reject issue/PR number references of the form #NNN in Rust comments,
# developer-facing Markdown documentation, and TOML configuration.
#
# TOML is covered because a manifest comment is exactly where a tracker number
# hides from a source/docs-only lint: `Cargo.toml` rationale comments are prose
# about the code, carry the same "explain the constraint, not the ticket"
# obligation as a Rust comment, and are read by anyone auditing a dependency.
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

# --- TOML configuration -------------------------------------------------------
# Every `.toml` in the tree, which is a superset of every `Cargo.toml`: the root
# workspace manifest, each member manifest, and the standalone tool configs
# (`rust-toolchain.toml`, `mutants.toml`, ...).  Scanning whole files rather than
# only `#`-comment lines matches the Markdown lane above and is what makes a
# `description = "... (#123)"` package field — which is not a comment at all, and
# ships to crates.io — a failure too.
#
# `#\d{3,}` deliberately does not match a two-digit token, so standards
# references such as `UTS #39` stay legal; it also cannot match `#[case]`,
# `#[cfg(...)]`, or `#[global_allocator]`, whose `#` is followed by `[`.  The
# same generated/vendored/GitHub-process exclusions as the Markdown lane apply:
# those trees either mirror a canonical source or are the one place a process
# reference legitimately belongs.
toml_code=0
toml_matches=$(rg -n -e '#\d{3,}' \
    --glob '*.toml' \
    --glob '!.github/**' \
    --glob '!generated/**' \
    --glob '!vendor/**' \
    --glob '!target/**' \
    --glob '!**/node_modules/**' \
    .) || toml_code=$?
if [ "$toml_code" -eq 2 ]; then exit 2; fi

if [ -n "$toml_matches" ]; then
    echo "Found issue/PR number references in TOML configuration:" >&2
    echo "$toml_matches" >&2
    status=1
fi

exit "$status"

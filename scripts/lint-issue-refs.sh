#!/bin/sh
# scripts/lint-issue-refs.sh
# Reject issue/PR number references of the form #NNN in Rust comments,
# developer-facing Markdown documentation, the build surface (Makefiles and shell
# scripts), and TOML manifests/config.
#
# The build surface and TOML were both added after references survived in them
# precisely because nothing scanned them: process-flow information belongs in
# GitHub, not in the repository, and a rule enforced on only some file types is a
# rule that migrates to the unenforced ones.
#
# This script is intentionally POSIX-shell and ripgrep-based so it adds no
# Python/Rust runtime dependency beyond what the repository already uses.
#
# Usage: lint-issue-refs.sh [ROOT]   (ROOT defaults to the current directory;
# an explicit ROOT lets the gate be exercised against a fixture tree.)
set -eu

command -v rg >/dev/null 2>&1 || { echo "ripgrep (rg) is required" >&2; exit 2; }

cd "${1:-.}"

status=0

# --- Rust source comments -----------------------------------------------------
# Match #NNN tokens inside //, ///, //! style comments and across lines of
# /* */ block comments.  We do not try to fully parse Rust; the tree is
# already clean, so simple regexes are sufficient to catch regressions.  Any
# false positives can be handled by refining the patterns or the allow-list
# below.
rust_line_matches=''
rust_block_matches=''
if [ -d crates ]; then
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
fi

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

# --- Build surface: Makefiles and shell scripts --------------------------------
# Makefile and shell comments are documentation too, and they were the blind spot
# that let references persist.  Exclude the vendored and generated trees for the
# same reason Markdown does.
build_code=0
build_matches=$(rg -n -e '#\d{3,}' \
    --glob 'Makefile' \
    --glob '*.mk' \
    --glob '*.sh' \
    --glob '!.github/**' \
    --glob '!generated/**' \
    --glob '!vendor/**' \
    .) || build_code=$?
if [ "$build_code" -eq 2 ]; then exit 2; fi

if [ -n "$build_matches" ]; then
    echo "Found issue/PR number references in Makefiles or shell scripts:" >&2
    echo "$build_matches" >&2
    status=1
fi

# --- TOML manifests and config -------------------------------------------------
# Scan Cargo manifests and other repo-authored TOML (mutants.toml,
# rust-toolchain.toml, ...) for the same banned issue/PR references. Exclude
# generated artifacts, vendored trees, and GitHub workflow config, mirroring the
# Markdown section above. Cargo.lock is excluded defensively even though it is
# not named *.toml and so would not match the glob anyway: it is a machine-written
# lockfile, never a place to author prose. No other exclusion is needed — unlike
# Markdown's brand-colour hex codes, no legitimate `#NNN`-shaped content (hex
# colours, port numbers, etc.) exists in any tracked TOML file today.
toml_code=0
toml_matches=$(rg -n -e '#\d{3,}' \
    --glob '*.toml' \
    --glob '!Cargo.lock' \
    --glob '!.github/**' \
    --glob '!generated/**' \
    --glob '!vendor/**' \
    .) || toml_code=$?
if [ "$toml_code" -eq 2 ]; then exit 2; fi

if [ -n "$toml_matches" ]; then
    echo "Found issue/PR number references in TOML manifests:" >&2
    echo "$toml_matches" >&2
    status=1
fi

exit "$status"

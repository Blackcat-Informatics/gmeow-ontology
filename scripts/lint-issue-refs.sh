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

# --- The whole branch diff -----------------------------------------------------
# The two scans above are FILE-SCOPED: Rust comments under crates/, and Markdown
# anywhere.  Everything else a branch touches -- Turtle modules, the Makefile, CI
# workflows, shell scripts, JSON evidence, TOML manifests -- was outside the rule
# entirely, so an issue number could land in a slice's skos:definition (which SHIPS,
# as ontology content, inside gmeow.gts) and no gate would see it.
#
# The fix is to widen THIS rule rather than to add a second one: a second
# file-scoped checker would be a second source of truth for one policy, and the two
# would drift on exclusions the moment either grew one.  So the same #NNN pattern
# and the same exclusion set now also run over every file the branch changed
# relative to its merge base with origin/main.
#
# Tri-state on the comparand, matching the repository's standing discipline:
#   * origin/main missing entirely  -> a LOUD skip (a fresh clone has no upstream);
#   * origin/main present but the merge base unresolvable -> HARD FAIL (the leg
#     cannot perform the comparison it is defined to perform, and passing there
#     would let a changed file through unseen);
#   * resolved -> scan.
if git rev-parse --verify --quiet origin/main >/dev/null 2>&1; then
    base=$(git merge-base HEAD origin/main 2>/dev/null) || base=""
    if [ -z "$base" ]; then
        echo "lint-issue-refs: origin/main exists but 'git merge-base HEAD origin/main'" >&2
        echo "  did not resolve, so the branch-diff leg cannot run. Fetch origin and retry." >&2
        exit 2
    fi
    # The unit is the ADDED LINE, not the changed file.  A file-level scan would red
    # on history the branch merely stood next to -- a Cargo.toml comment written
    # three years ago -- which is not a defect this branch can fix and would train
    # everyone to bypass the hook.  What the branch is answerable for is what it
    # WROTE, so `--unified=0` plus the hunk headers gives an exact `file:line` for
    # every introduced line, and untracked files are scanned whole (every line of a
    # new file is an added line).
    #
    # Committed changes AND the working tree, so the rule fires before a commit is
    # made rather than only after.
    diff_matches=$(
        {
            # --src-prefix/--dst-prefix are pinned rather than defaulted: a checkout
            # with `diff.noprefix = true` emits `+++ Makefile`, and a parser that
            # assumed `+++ b/Makefile` would silently attribute every finding to an
            # empty filename instead of failing.
            git diff --unified=0 --diff-filter=ACMR \
                --src-prefix=a/ --dst-prefix=b/ "$base" -- . 2>/dev/null
            for path in $(git ls-files --others --exclude-standard 2>/dev/null); do
                [ -f "$path" ] || continue
                printf '+++ b/%s\n@@ -0,0 +1 @@\n' "$path"
                sed 's/^/+/' -- "$path"
            done
        } | awk '
            /^[+][+][+] / { file = substr($0, 7); line = 0; next }
            /^@@ / {
                # @@ -a,b +c,d @@ -- take c, then count added lines from there.
                split($3, plus, ",")
                line = substr(plus[1], 2) - 1
                next
            }
            /^[+]/ {
                line = line + 1
                if ($0 ~ /#[0-9][0-9][0-9]/) { printf "%s:%d:%s\n", file, line, substr($0, 2) }
            }
        ' | grep -v -e '^generated/' -e '^vendor/' -e '^\.github/' -e '^docs/BRAND\.md:'
    ) || true
    if [ -n "$diff_matches" ]; then
        echo "Found issue/PR number references in lines this branch ADDED:" >&2
        echo "$diff_matches" >&2
        status=1
    fi
else
    echo "lint-issue-refs: origin/main is not present in this checkout, so the" >&2
    echo "  branch-diff leg was skipped. The file-scoped Rust/Markdown scans ran." >&2
fi

exit "$status"

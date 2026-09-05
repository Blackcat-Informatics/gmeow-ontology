#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
#
# Reject tracker and review-process provenance in authored prose. One policy is
# applied to Rust comments, Markdown, branch-added lines, build/shell sources,
# and TOML. An explicit ROOT makes the same gate usable by hermetic fixtures.
set -euo pipefail

script_dir=$(dirname -- "${BASH_SOURCE[0]}")
# shellcheck source=build-support/common.sh
source "$script_dir/../build-support/common.sh"

gmeow_require_command rg "ripgrep (rg)"
gmeow_require_command perl
gmeow_require_command git

cd "${1:-.}"

# Assemble tool names so this shell source does not become its own finding.
# Only the review-bot *authorship* pattern is spelled out here, because that is
# the policy itself — review-process provenance in prose is the thing being
# rejected. The bots' on-disk state directories are NOT named: they are
# untracked and ignored, so the derived exclusion below covers them.
review_name_a='Code'
review_name_b='Rabbit'
review_bot_a='code'
review_bot_b='rabbit'
review_bot_c='ai'
review_name_c='Gem'
review_name_d='ini'
REVIEW_TOOL_PATTERN="(?:${review_name_a}${review_name_b}s?|${review_bot_a}${review_bot_b}${review_bot_c}(?:\\[bot\\])?|${review_name_c}${review_name_d}(?:[- ]code[- ]assist)?s?(?:\\[bot\\])?)"

# Shared by every scan leg. The process-code alternative is deliberately
# qualified by a keyword; standalone competency identifiers remain technical.
POLICY_PATTERN="(?i:(?:\\b(?:issue|pull[ -]?request|pr)[[:space:]]*(?:#[[:space:]]*)?[0-9]{2,}\\b|\\b[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+#[0-9]{2,}\\b|(?<![[:alnum:]_])#[0-9]{2,}\\b|\\b(?:tasks?|gap|finding|review(?:er)?)[[:space:]#:-]*[A-Z]?[0-9]+(?:\\.[0-9]+|[a-z])?\\b|\\b${REVIEW_TOOL_PATTERN}\\b))"
export POLICY_PATTERN REVIEW_TOOL_PATTERN

# Foreign trees are DERIVED, never enumerated by vendor name. A path that is
# both untracked AND ignored is, by construction, not authored content of this
# repository: a local tool checkout, agent runtime state, a vendored cache.
# Naming each such tool inside a policy gate is a maintenance leak — the gate
# grows an entry per tool and the repository ends up carrying the identity of
# software it does not ship.
#
# `--exclude-standard` reads .gitignore, .git/info/exclude, and
# core.excludesFile, so a purely local tool is excluded by purely local
# configuration, which is where a local tool belongs. `--directory` collapses a
# wholly ignored tree to a single entry. The result is a deterministic function
# of the working tree: same tracked set and same ignore configuration, same
# exclusions, on every machine.
#
# The rule is untracked AND ignored, never merely ignored. A TRACKED file is
# always scanned even when an ignore rule matches it — the scan uses
# `--no-ignore` deliberately, and that behaviour is pinned by
# `tracked_authored_files_are_scanned_even_when_an_ignore_rule_matches`.
#
# NUL-delimited so a path containing a newline cannot forge a list entry. The
# early globs and the canonical `filter_contexts` policy are fed from the same
# read, which is what keeps the two in the parity the fixtures require.
DERIVED_GLOBS=()
DERIVED_EXCLUDE_LIST=''
while IFS= read -r -d '' derived_path; do
  derived_path="${derived_path%/}"
  [[ -n "$derived_path" ]] || continue
  DERIVED_GLOBS+=(--glob "!${derived_path}" --glob "!${derived_path}/**")
  DERIVED_EXCLUDE_LIST+="${derived_path}"$'\n'
done < <(git ls-files -z --others --ignored --exclude-standard --directory)
export DERIVED_EXCLUDE_LIST

# Hidden authored files are in scope. These globs are an early performance and
# traversal bound only; every scan leg then passes through `filter_contexts`, the
# canonical exclusion and contextual-control policy. The parity fixtures require
# every early exclusion to be accepted by that final policy at any matching depth.
rg_authored() {
  rg --hidden --no-ignore --pcre2 \
    --glob '!.git/**' --glob '!**/.git/**' \
    --glob '!.worktrees/**' --glob '!**/.worktrees/**' \
    --glob '!.venv/**' --glob '!**/.venv/**' \
    --glob '!__pycache__/**' --glob '!**/__pycache__/**' \
    --glob '!.pytest_cache/**' --glob '!**/.pytest_cache/**' \
    --glob '!.ruff_cache/**' --glob '!**/.ruff_cache/**' \
    --glob '!.mypy_cache/**' --glob '!**/.mypy_cache/**' \
    --glob '!.tox/**' --glob '!**/.tox/**' \
    --glob '!.cache/**' --glob '!**/.cache/**' \
    --glob '!generated/**' --glob '!**/generated/**' \
    --glob '!target/**' --glob '!**/target/**' \
    --glob '!build/**' --glob '!**/build/**' \
    --glob '!out/**' --glob '!**/out/**' \
    --glob '!dist/**' --glob '!**/dist/**' \
    --glob '!ontology-docs/**' --glob '!docs/_generated/**' \
    --glob '!htmlcov/**' --glob '!**/htmlcov/**' \
    --glob '!*.egg-info/**' --glob '!**/*.egg-info/**' \
    --glob '!*.py[cod]' --glob '!*.snap.new' \
    --glob '!.coverage' --glob '!lcov.info' --glob '!llms.txt' \
    --glob '!rustc-ice-*.txt' --glob '!.DS_Store' --glob '!*.swp' \
    --glob '!.stamps/**' --glob '!.tmp/**' \
    --glob '!.gmeow-tmp-*/**' --glob '!node_modules/**' \
    --glob '!**/node_modules/**' --glob '!mutants.out*/**' \
    --glob '!pipeline/**' --glob '!.mcp.json' \
    --glob '!.worktree' --glob '!keys/*.secret' \
    --glob '!keys/*.secret.asc' --glob '!keys/*.tmp' \
    --glob '!catalog-v001.xml' \
    --glob '!packages/python/gmeow_models/**' \
    --glob '!crates/xtask/tests/issue_refs_lint.rs' \
    "${DERIVED_GLOBS[@]}" \
    "$@"
}

# Remove only contextual technical controls, then re-run the same policy over
# the remaining text. A line containing both a control and a violation still
# fails. The original line is printed for diagnostics.
filter_contexts() {
  perl -ne '
        BEGIN {
            # Mirror of the derived glob set, so the canonical policy accepts
            # every early exclusion. Trailing slashes are stripped so a tree
            # entry and a file entry compare the same way.
            @DERIVED = map { my $p = $_; $p =~ s{/\z}{}; $p }
                       grep { length }
                       split /\n/, ($ENV{DERIVED_EXCLUDE_LIST} // q{});
        }

        my $original = $_;
        my $audit = $_;
        my ($path) = $audit =~ m{^([^:\n]+):[0-9]+:};
        $path //= q{};
        $path =~ s{^\./}{};

        next if $path eq q{crates/xtask/tests/issue_refs_lint.rs};
        next if $path =~ m{(?:^|/)(?:\.git|\.worktrees|\.venv|__pycache__|\.pytest_cache|\.ruff_cache|\.mypy_cache|\.tox|\.cache|generated|target|build|out|dist|ontology-docs|htmlcov|[^/]+\.egg-info|node_modules|mutants\.out[^/]*)(?:/|$)};
        next if $path =~ m{^pipeline(?:/|$)};
        next if $path =~ m{(?:^|/)(?:docs/_generated|\.stamps|\.tmp|\.gmeow-tmp-[^/]+)(?:/|$)};
        next if $path =~ m{^packages/python/gmeow_models(?:/|$)};
        next if $path =~ m{(?:^|/)(?:\.coverage|lcov\.info|llms\.txt|\.DS_Store|\.worktree|\.mcp\.json|catalog-v001\.xml)$};
        next if $path =~ m{(?:^|/)(?:[^/]+\.py[cod]|[^/]+\.snap\.new|[^/]+\.swp|rustc-ice-[^/]+\.txt)$};
        next if $path =~ m{^keys/[^/]+\.(?:secret|secret\.asc|tmp)$};
        next if grep { $path eq $_ || index($path, "$_/") == 0 } @DERIVED;

        my $tool = qr/$ENV{REVIEW_TOOL_PATTERN}/i;
        $audit =~ s{\b(?:UAX|UTS)[[:space:]]*#[0-9]+\b}{}gi;
        $audit =~ s{https?://[^[:space:]<>"`]+}{}gi;
        $audit =~ s{\]\(#[0-9]+-[a-z0-9][^)]*\)}{]}gi;

        if ($path eq q{slices/grounding/logic/design/LOGIC-REFERENCES.md}) {
            $audit =~ s{\bReview[[:space:]]+[0-9]{1,3}\([0-9]+\)}{}gi;
        }

        if ($path eq q{.deficiencies}) {
            $audit =~ s{\bAudit pointer:[[:space:]]*[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+#[0-9]{2,}\b}{}g;
        }

        if ($path eq q{AGENTS.md} && $audit =~ /Read both automated .* and human reviews/) {
            $audit =~ s{$tool}{}g;
        }

        if ($path eq q{metadata/references.ttl}) {
            $audit =~ s{GitHub PR review comment discussion_r[0-9]+ by[[:space:]]+$tool}{}gi;
            $audit =~ s{GitHub comment issuecomment-[0-9]+ by[[:space:]]+$tool}{}gi;
            if ($audit =~ /(?:rdfs:label|gmeow:authority)/) {
                $audit =~ s{$tool}{}g;
            }
        }

        if ($path =~ /\.(?:css|scss|less)$/ || $path eq q{docs/BRAND.md}
            || $audit =~ /\b(?:color|background|border|outline|fill|stroke)[-A-Za-z]*[[:space:]]*:/i) {
            $audit =~ s{(?<![0-9A-Fa-f])#[0-9A-Fa-f]{8}(?![0-9A-Fa-f])}{}g;
            $audit =~ s{(?<![0-9A-Fa-f])#[0-9A-Fa-f]{6}(?![0-9A-Fa-f])}{}g;
            $audit =~ s{(?<![0-9A-Fa-f])#[0-9A-Fa-f]{4}(?![0-9A-Fa-f])}{}g;
            $audit =~ s{(?<![0-9A-Fa-f])#[0-9A-Fa-f]{3}(?![0-9A-Fa-f])}{}g;
        }

        print $original if $audit =~ /$ENV{POLICY_PATTERN}/;
    '
}

status=0

report_scan() {
  heading=$1
  shift
  scan_code=0
  raw_matches=$(rg_authored "$@") || scan_code=$?
  if [ "$scan_code" -eq 2 ]; then
    exit 2
  fi
  matches=$(printf '%s\n' "$raw_matches" | filter_contexts)
  if [ -n "$matches" ]; then
    echo "$heading" >&2
    echo "$matches" >&2
    status=1
  fi
}

report_scan "Found process provenance in Rust line comments:" \
  -n --type rust -e "//[^\n]*(?:${POLICY_PATTERN})" .

report_scan "Found process provenance in Rust block comments:" \
  -n -U --type rust \
  -e "(?s)/\\*(?:(?!\\*/).)*?(?:${POLICY_PATTERN})(?:(?!\\*/).)*?\\*/" .

report_scan "Found process provenance in authored Markdown:" \
  -n -e "$POLICY_PATTERN" --glob '*.md' .

report_scan "Found process provenance in Makefiles or shell sources:" \
  -n -e "$POLICY_PATTERN" --glob 'Makefile' --glob '*.mk' \
  --glob '*.sh' --glob '*.bash' --glob '*.zsh' .

report_scan "Found process provenance in TOML:" \
  -n -e "$POLICY_PATTERN" --glob '*.toml' --glob '!Cargo.lock' .

# Scan every added line relative to origin/main, including untracked files. The
# comparison is tri-state: a missing remote is a loud skip; an unresolvable merge
# base is a hard failure.
if git rev-parse --verify --quiet origin/main > /dev/null 2>&1; then
  base=$(git merge-base HEAD origin/main 2> /dev/null) || base=''
  if [ -z "$base" ]; then
    echo "lint-issue-refs: origin/main exists but its merge base is unavailable" >&2
    exit 2
  fi
  diff_lines=$(
    {
      git diff --unified=0 --diff-filter=ACMR \
        --src-prefix=a/ --dst-prefix=b/ "$base" -- . 2> /dev/null
      git ls-files --others --exclude-standard 2> /dev/null |
        while IFS= read -r path; do
          [ -f "$path" ] || continue
          printf '+++ b/%s\n@@ -0,0 +1 @@\n' "$path"
          awk '{ print "+" $0 }' < "$path"
        done
    } | awk '
            /^[+][+][+] / { file = substr($0, 7); line = 0; next }
            /^@@ / {
                split($3, plus, ",")
                line = substr(plus[1], 2) - 1
                next
            }
            /^[+]/ {
                line = line + 1
                printf "%s:%d:%s\n", file, line, substr($0, 2)
            }
        '
  )
  diff_scan_code=0
  diff_candidates=$(printf '%s\n' "$diff_lines" |
    rg --no-line-number --pcre2 -e "$POLICY_PATTERN") || diff_scan_code=$?
  if [ "$diff_scan_code" -gt 1 ]; then
    echo "lint-issue-refs: branch-added-line policy scan failed" >&2
    exit 2
  fi
  diff_matches=$(printf '%s\n' "$diff_candidates" | filter_contexts)
  if [ -n "$diff_matches" ]; then
    echo "Found process provenance in lines this branch added:" >&2
    echo "$diff_matches" >&2
    status=1
  fi
else
  echo "lint-issue-refs: origin/main is absent; branch-added-line scan skipped" >&2
fi

exit "$status"

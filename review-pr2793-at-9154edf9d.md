VERDICT: PASS 9154edf9dec1c6d28eb05e24accc5832c45d066c

# Review of PR 2793 at 9154edf9d (corrections to the merged agent guide)

Read-only review of `.worktrees/agents-guide-consolidation`.

- HEAD is `9154edf9d`, on branch `paudley/agents-guide-corrections`. The forge (`gh api .../pulls/2793`) reports the same head sha and branch, state open, base `main`.
- The head is one commit whose parent is `2629810da`, which is current `origin/main` (fetched for this review) and is the squash of PR 2792.
- Nothing was built or run.

## What was checked

- `git diff origin/main 9154edf9d --stat` touches only `AGENTS.md` (+57/-32).
- `git diff 915ba9564 9154edf9d -- AGENTS.md` is exactly one line. "Older modules still author `rdfs:` in two ways, and neither is a pattern: subsumption" becomes "Older modules still author two semantic `rdfs:` forms, and neither is a pattern: subsumption". That is optional finding O1 of `review-pr2792-at-915ba9564.md`, and it removes the reading that `rdfs:` is otherwise not authored (the annotation rule at `AGENTS.md:126` requires `rdfs:label` and `rdfs:isDefinedBy`).
- `AGENTS.md` on `main` is byte-identical to the one at `0b01c193e` (empty diff). The changed lines of `git diff origin/main 9154edf9d -- AGENTS.md` equal those of `git diff 0b01c193e 915ba9564 -- AGENTS.md` except for that one line. So the PR delivers exactly the corrections reviewed at `789eb9729` and `915ba9564`.

## Findings against the merged defects

The four REQUIRED findings of `review-pr2792-at-0b01c193e.md` are on `main` now. This PR corrects all of them, as verified at `789eb9729` and `915ba9564`:
- R1: the lint claim is limited to `owl:`, and the `rdfs:` subsumption and signature status matches `docs/PROJECTION-VOCABULARY-RATCHET.md:80-86`.
- R2: dependency inheritance is stated as a rule for new work.
- R3: there is no claim of a cargo wrapper or a build lock in the repository, and the gate's host-wide lock with no override matches `docs/GATE-AND-PIPELINE.md:143-146`.
- R4: the twelve-hour report is removed and the limit of three names whom it binds.

The optional findings taken at `789eb9729` are carried unchanged. The "left alone by the reset" clause stays removed.

REQUIRED: none.
OPTIONAL: the one cosmetic wrap noted at `915ba9564` (the `.stage/` bullet breaks after "goes when the"). Line length is not linted.

## Limits

When read, CI on this head showed `license/cla` pass; `lint`, `helpers`, `producer-build` and `rust-prebuild` pending; CodeRabbit skipping. Markdown lint, spelling and the issue-reference lint on this content are shown only by those runs.

## Coverage

Read:
- `git rev-parse HEAD`, the commit's parent, and `origin/main` after `git fetch origin main`.
- `gh api .../pulls/2793` (head, branch, state, base).
- `git diff 915ba9564 9154edf9d` (changed lines).
- `git diff origin/main 9154edf9d --stat`.
- `git diff 0b01c193e origin/main -- AGENTS.md` (empty).
- A line-set comparison of the corrections in the two diffs.
- `gh pr checks 2793`.

The unchanged body of `AGENTS.md` was reviewed at `0b01c193e`, `789eb9729` and `915ba9564` and was not re-read here.

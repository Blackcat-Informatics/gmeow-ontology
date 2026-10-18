# Requirement candidates — issue #1762

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 21 | list-under-requirement-heading | - No production-reachable `Result<_, String>` remains in the reported surfaces. |
| B2 | body | 22 | list-under-requirement-heading | - Prover discovery/execution failures retain command, artifact, version, exit-status, stdout/stderr and path evidence where applicable. |
| B3 | body | 23 | list-under-requirement-heading | - External model parsing and numeric preparation failures retain their typed operation/locus and original detail. |
| B4 | body | 24 | list-under-requirement-heading | - Existing public CLI rendering and exit behavior remains deterministic. |
| B5 | body | 25 | list-under-requirement-heading | - Focused failure-path tests assert stable diagnostic codes and evidence. |
| B6 | body | 26 | list-under-requirement-heading | - `make crate-check` no longer reports the string-error rule. |
| C1.1 | comment 1 (paudley, 2026-09-22T04:50:28Z) | 24 | modal | 5. Open a non-draft PR closing #1762 and publish the plan/evidence. Address every human and CodeRabbit thread under stage-2, repeating validation/commit/push/issue updates per logical remediation. Merge current origin/main into the branch, re-run required evidence, write squash notes, and merge only with `/home/paudley/stage/root/bin/ghprsq`. Verify PR merge, issue closure and main before removing only this worktree. |
| C3.1 | comment 3 (paudley, 2026-10-15T21:57:12Z) | 5 | modal | Next steps: settle those known source prerequisites, verify the affected CLI and diagnostic consumers, then perform the required qualification and review before ordinary commits and ghprsq integration. Principles 4, 7, 18. |
| C5.1 | comment 5 (paudley, 2026-10-17T01:32:01Z) | 58 | modal | remain required. Neither 1762 nor 1770 is complete, and the full logic issue/PR |

## Comments that matched no rule

- comment 2 (paudley, 2026-09-22T08:56:51Z)
- comment 4 (paudley, 2026-10-16T22:14:30Z)

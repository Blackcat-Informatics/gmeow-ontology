# Requirement candidates — issue #1825

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| C1.1 | comment 1 (paudley, 2026-10-16T16:28:01Z) | 2 | modal | by docs-print/src/lib.rs and called by render.rs. It must not be deleted. |
| C1.2 | comment 1 (paudley, 2026-10-16T16:28:01Z) | 16 | modal | Regression coverage must include implicit workspace identities, generated owners, |

## Comments that matched no rule

None.

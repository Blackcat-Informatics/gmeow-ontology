# Requirement candidates — issue #1438

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| C1.1 | comment 1 (coderabbitai, 2026-07-08T03:56:00Z) | 18 | checkbox | - [ ] <!-- {"checkboxId": "8d4f2b9c-3e1a-4f7c-a9b2-d5e8f1c4a7b9"} --> Create Plan |
| C2.1 | comment 2 (paudley, 2026-09-05T03:07:12Z) | 14 | modal | Flagging rather than concluding: whoever picks this up should first re-baseline which of the five projections genuinely exist, because the issue is currently scoped as if none do. That re-baseline may shrink this from complexity-hard to something much smaller. |

## Comments that matched no rule

- comment 3 (paudley, 2026-10-20T09:40:04Z)

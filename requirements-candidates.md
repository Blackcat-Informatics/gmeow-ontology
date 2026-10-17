# Requirement candidates — issue #1845

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 3 | modal | 1. Positive existential guards are rejected by direct atom lowering. The co-occurrence frontend actually emits such guards. Lower them with capture-avoiding binder renaming, preserving outer variables and sibling scopes. Universal guards must remain unsupported; do not restore indiscriminate quantifier stripping. |

## Comments that matched no rule

None.

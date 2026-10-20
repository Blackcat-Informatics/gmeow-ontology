# Requirement candidates — issue #1657

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 11 | modal | `math:ExtendedRealValueConstraint` enforces the non-literal-must-be-a-pole case, but the guarded `logic:` fragment has no numeric-datatype test, so a non-numeric literal in an extended-real position passes. |
| B2 | body | 28 | modal | - Closed-form leaf-linkage runtime enforcement is a `tests/structural.ttl` MUST cell over authored instances rather than a live `logic:Constraint`, because transitive AST traversal exceeds the guarded fragment. |
| B3 | body | 32 | checkbox | - [ ] Give the extended-real gate a numeric-datatype test, or record why the guarded fragment cannot carry one. |
| B4 | body | 33 | checkbox | - [ ] Decide how to forbid a finite-negative `measureResult` without breaking the literal-or-pole span. |
| B5 | body | 34 | checkbox | - [ ] Register the radial scenes as flagship scenarios, or state that the non-flagship surface is sufficient. |
| C1.1 | comment 1 (coderabbitai, 2026-07-27T00:19:05Z) | 18 | checkbox | - [ ] <!-- {"checkboxId": "8d4f2b9c-3e1a-4f7c-a9b2-d5e8f1c4a7b9"} --> Create Plan |
| C3.1 | comment 3 (paudley, 2026-09-06T02:53:08Z) | 45 | modal | That is a vocabulary addition, not a one-line tightening, which is why I have not bundled it into the current run of small fixes. Item 2 (`math:MeasureResultNonNegativeConstraint`) plausibly wants the same builtin plus `logic:termGreaterEqual`, so the two should be designed together rather than separately. |

## Comments that matched no rule

- comment 2 (paudley, 2026-09-04T16:20:51Z)
- comment 4 (paudley, 2026-10-20T07:59:11Z)

# Branch under review — issue #1746

Branch `paudley/1746-logic-non-entailment-obligation` against `origin/main`. Gathered by `stagectl brief
--mode review`; every hash and path came from this repository.

## Commits (2)

- `6592e22b6` test: separate authenticated verifier and native inventory coverage  _2026-10-20_
- `63893810c` fix: retain authored obligation identities in native inventories  _2026-10-20_

## Files changed

```
crates/logic/src/obligations/assessment.rs    |  43 ++++++---
 crates/logic/src/obligations/program_tests.rs | 124 ++++++++++++++++++++++++++
 2 files changed, 156 insertions(+), 11 deletions(-)
```

## ADRs the changed files cite

None of the changed files cite an ADR.

These are the settled decisions this branch touches. The question
for stage2 is whether the change fits them, not merely whether it
compiles.


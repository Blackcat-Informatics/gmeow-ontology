# Branch under review — issue #1829

Branch `paudley/1829-fixtures-goal-directed-aggregate` against `origin/main`. Gathered by `stagectl brief
--mode review`; every hash and path came from this repository.

## Commits (2)

- `ff3cfe1c6` fix(logic): supply executable legs for the lossy recovery refutation  _2026-10-16_
- `693caea59` fix(pipeline): persist the goal-directed proof contribution  _2026-10-16_

## Files changed

```
crates/logic/src/correspondence_exec.tests.rs | 39 +++++++++++++++++++++++++++
 crates/pipeline/src/fixture.tests.rs          | 12 +++++++++
 crates/pipeline/src/stages/goal_directed.rs   | 12 ++++-----
 imports/supersession-correspondences.ttl      | 10 +++++++
 slices/core/pipeline/module.ttl               |  2 +-
 5 files changed, 68 insertions(+), 7 deletions(-)
```

## ADRs the changed files cite

None of the changed files cite an ADR.

These are the settled decisions this branch touches. The question
for stage2 is whether the change fits them, not merely whether it
compiles.


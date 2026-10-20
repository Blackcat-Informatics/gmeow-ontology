# Branch under review — issue #1646

Branch `paudley/1646-schemas-recover-the-developer-surface` against `origin/main`. Gathered by `stagectl brief
--mode review`; every hash and path came from this repository.

## Commits (1)

- `5bbb43a03` fix: route imported template model types  _2026-10-20_

## Files changed

```
crates/pipeline/src/stages/compile_logic.rs     |  28 +++-
 crates/pipeline/src/stages/pydantic.rs          |   2 +-
 crates/pipeline/src/stages/pydantic.tests.rs    | 166 ++++++++++++++++++++++++
 crates/pipeline/src/stages/pydantic/topology.rs |  50 +++++--
 docs/schema-projections.md                      |   7 +
 5 files changed, 237 insertions(+), 16 deletions(-)
```

## ADRs the changed files cite

None of the changed files cite an ADR.

These are the settled decisions this branch touches. The question
for stage2 is whether the change fits them, not merely whether it
compiles.


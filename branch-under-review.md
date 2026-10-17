# Branch under review — issue #1834

Branch `paudley/1834-repair-shared-helper-ownership-and` against `origin/paudley/1829-fixtures-goal-directed-aggregate`. Gathered by `stagectl brief
--mode review`; every hash and path came from this repository.

## Commits (5)

- `f033b9f3a` docs(logic): clarify diagnostic contracts and native terminology  _2026-10-16_
- `12380e2df` fix(logic): remove facet string errors and ground service refusals  _2026-10-16_
- `c3123db1a` docs(helpers): explain source and diagnostic regression contracts  _2026-10-16_
- `425eb65de` fix(build): own shared diagnostic macro inputs  _2026-10-16_
- `c74f5e804` fix(helpers): share RDF emission and diagnostic registration  _2026-10-16_

## Files changed

```
crates/affect-ingest/src/lib.rs                   | 119 +++++-----------------
 crates/build-inputs/src/modules.rs                |   1 +
 crates/build-inputs/tests/inventory.rs            |  23 +++++
 crates/errors/src/diag.rs                         |  60 +++++++----
 crates/errors/src/diag.tests.rs                   |  49 +++++++++
 crates/errors/src/error.tests.rs                  |   7 +-
 crates/gmeow-cli/src/commands/reasoning_input.rs  |  84 +++++++++++----
 crates/gmeow-cli/src/commands/reasoning_report.rs |  48 +++++++--
 crates/gmeow-cli/src/error.rs                     |  36 +++----
 crates/logic-compile/src/frontend.rs              |  34 ++++---
 crates/logic-compile/src/tptp/contracts.tests.rs  |  14 +--
 crates/logic/src/error.rs                         |   1 +
 crates/logic/src/reasoner_services.tests.rs       |  10 ++
 crates/math-lift/src/sink.rs                      |  24 +++--
 crates/math-lift/src/sink.tests.rs                |  26 ++++-
 crates/math/src/lib.rs                            |   3 +
 helpers-census-debt.json                          |  50 ---------
 slices/grounding/logic/i18n/fr.po                 |  56 ++++++++++
 slices/grounding/logic/i18n/zh.po                 |  56 ++++++++++
 slices/grounding/logic/module.ttl                 |  26 +++++
 20 files changed, 479 insertions(+), 248 deletions(-)
```

## ADRs the changed files cite

- ADR-0042 (not on disk)

These are the settled decisions this branch touches. The question
for stage2 is whether the change fits them, not merely whether it
compiles.


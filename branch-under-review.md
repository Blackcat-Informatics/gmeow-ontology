# Branch under review — issue #1657

Branch `paudley/1657-math-tighten-the-signed-radial-field` against `origin/main`. Gathered by `stagectl brief
--mode review`; every hash and path came from this repository.

## Commits (1)

- `c290ab573` fix: execute compound native numeric integrity laws  _2026-10-20_

## Files changed

```
crates/logic-compile/src/relational_core.rs        | 280 ++++++++++++++++-----
 crates/logic-compile/src/relational_core.tests.rs  |  28 ++-
 crates/logic/src/counterfactual.rs                 |  10 +
 crates/logic/src/physical/builtin_eval.rs          |  46 ++++
 crates/logic/src/physical/magic.rs                 |  13 +
 crates/logic/src/physical/plan.rs                  |  12 +
 crates/logic/src/physical/seminaive.rs             |  10 +-
 crates/logic/src/query_ir.rs                       |  11 +
 .../logic/src/reason/enactment/term_kind.tests.rs  | 106 ++++++++
 crates/logic/src/reference_resolver.rs             |  11 +
 crates/logic/src/relational_core.rs                |  10 +
 .../examples/signed-radial-field-closed-form.ttl   |   2 +-
 .../examples/signed-radial-field-qualitative.ttl   |   2 +-
 slices/grounding/math/tests/competency.ttl         |   4 +-
 .../grounding/math/tests/example-conformance.ttl   |   4 +-
 slices/grounding/math/tests/structural.ttl         |   4 +-
 16 files changed, 474 insertions(+), 79 deletions(-)
```

## ADRs the changed files cite

None of the changed files cite an ADR.

These are the settled decisions this branch touches. The question
for stage2 is whether the change fits them, not merely whether it
compiles.


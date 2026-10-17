# Branch under review — issue #1803

Branch `paudley/1803-split-gmeow-purrdf-responsibilities` against `origin/main`. Gathered by `stagectl brief
--mode review`; every hash and path came from this repository.

## Commits (2)

- `7db11af84` fix(pipeline): select recorded worlds after complete object assembly  _2026-10-16_
- `0327fb993` feat(pipeline): consume recorded reasoning inputs without a producer  _2026-10-16_

## Files changed

```
Cargo.lock                                         |  62 ++---
 Cargo.toml                                         |  10 +-
 Makefile                                           |   7 +
 crates/bundle-view/src/diagnostics_reader.rs       |   6 +-
 crates/conformance/Cargo.toml                      |   2 +-
 crates/conformance/src/compare.rs                  |   2 +-
 crates/mcp/src/lib.rs                              |  20 +-
 crates/mcp/src/tests.rs                            |   2 +-
 crates/pipeline/Cargo.toml                         |   5 +
 .../examples/recorded-class-consistency.rs         | 271 +++++++++++++++++++++
 crates/pipeline/src/cache.rs                       |  14 +-
 crates/pipeline/src/error.rs                       |  15 ++
 crates/pipeline/src/medium/corpus.rs               |   4 +-
 crates/pipeline/src/medium/sweep.rs                |   4 +-
 crates/pipeline/src/scheduler.rs                   |   8 +-
 .../stages/carrier.coherence_certificate_tests.rs  |   4 +-
 .../src/stages/carrier.docs_try_it_golden.rs       |   6 +-
 .../src/stages/carrier.logic_graph_golden_tests.rs |   7 +-
 crates/pipeline/src/stages/carrier.rs              |  51 ++--
 .../pipeline/src/stages/carrier_rooting_tests.rs   |  12 +-
 crates/pipeline/src/stages/compile_logic.tests.rs  |   8 +-
 .../stages/compile_logic/native_carrier/tests.rs   |   6 +-
 crates/pipeline/src/stages/goal_directed.tests.rs  |   4 +-
 crates/pipeline/src/stages/lpg.rs                  |   4 +-
 crates/pipeline/src/stages/lpg.tests.rs            |   4 +-
 crates/pipeline/src/stages/medium_dictionaries.rs  |   2 +-
 crates/pipeline/src/stages/reason.tests.rs         |  10 +-
 crates/pipeline/src/stages/reason/witness_tests.rs |   8 +-
 crates/pipeline/src/stages/slice_brief.tests.rs    |   4 +-
 crates/pipeline/src/stages/superset.rs             |   2 +-
 crates/pipeline/src/stages/validate.tests.rs       |   8 +-
 crates/pipeline/src/stages/validate/norm_claims.rs |   4 +-
 crates/pipeline/src/stages/verify_attestation.rs   |   8 +-
 .../src/stages/verify_attestation.tests.rs         |   4 +-
 crates/pipeline/tests/norm_claims_shacl.rs         |   2 +-
 .../pipeline/tests/support/authenticated_bundle.rs |   4 +-
 crates/validate/tests/conformance_support/mod.rs   |   2 +-
 slices/core/pipeline/module.ttl                    |   2 +-
 38 files changed, 488 insertions(+), 110 deletions(-)
```

## ADRs the changed files cite

None of the changed files cite an ADR.

These are the settled decisions this branch touches. The question
for stage2 is whether the change fits them, not merely whether it
compiles.


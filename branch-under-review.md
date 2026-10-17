# Branch under review — issue #1770

Branch `paudley/1770-certify-and-precompile-the-complete` against `origin/paudley/1834-repair-shared-helper-ownership-and`. Gathered by `stagectl brief
--mode review`; every hash and path came from this repository.

## Commits (4)

- `816a604cd` fix(logic): prepare exact questions for native proof checking  _2026-10-16_
- `10c6e175f` refactor(logic): retain native DL certificates and proofs  _2026-10-16_
- `4b733c636` docs(logic): describe admission ownership and archive guarantees  _2026-10-16_
- `41d204773` feat(logic): publish complete-source admission evidence  _2026-10-16_

## Files changed

```
crates/bundle-view/src/bundle_blobs.rs             |  17 ++
 .../src/bundle_blobs/reasoned_gates.tests.rs       |  72 ++++++++
 crates/gmeow-cli/src/commands.rs                   |  53 +++---
 .../logic-compile/src/frontend/source_admission.rs |  38 ++--
 .../src/frontend/source_admission/origins.rs       |  71 ++++++++
 .../src/frontend/source_admission/origins.tests.rs | 124 +++++++++++++
 crates/logic-compile/src/tptp/canon_admission.rs   |  83 +++++++++
 .../src/tptp/canon_admission.tests.rs              | 117 ++++++++++++
 crates/logic-compile/src/tptp/mod.rs               |   5 +
 crates/logic-compile/src/tptp/tests.rs             |   6 +
 crates/logic/src/reasoner_services.rs              | 198 ++++++---------------
 crates/logic/src/reasoner_services.tests.rs        | 140 +++++++++++++--
 crates/pipeline/src/stages/carrier.rs              | 127 ++-----------
 crates/pipeline/src/stages/carrier.ustar_tests.rs  |  18 ++
 crates/pipeline/src/stages/compile_logic.rs        |  11 +-
 slices/grounding/logic/design/LOGIC-CONTRACT.md    |  16 ++
 16 files changed, 771 insertions(+), 325 deletions(-)
```

## ADRs the changed files cite

None of the changed files cite an ADR.

These are the settled decisions this branch touches. The question
for stage2 is whether the change fits them, not merely whether it
compiles.


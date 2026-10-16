# Branch under review — issue #1757

Branch `paudley/1757-perf-logic-finish-merged-pipeline` against `origin/main`. Gathered by `stagectl brief
--mode review`; every hash and path came from this repository.

## Commits (53)

- `a794c70b3` fix(slices): qualify datatype-property counts by a data range, not owl:Thing  _2026-10-05_
- `f06f59e5c` fix(slices): state logic:message values as the plain strings its range declares  _2026-10-05_
- `5d43eb77f` refactor(1757): memoize class clashes through the map entry  _2026-10-05_
- `4e5196fd0` perf(1757): evaluate cardinality bound clashes once per class  _2026-10-05_
- `a261317e2` fix(1757): report native family cost and an oversized receipt's composition  _2026-10-05_
- `f0ae39bba` fix(1757): count qualified-count qualifiers by meaning, not spelling  _2026-10-05_
- `96b1ac63a` fix(1757): name the obstruction that withholds completed native reads  _2026-10-05_
- `a66869a65` fix(1757): keep blocked native reads visible on the reasoning verdict  _2026-10-05_
- `2a942b70f` fix(1757): admit xsd:anyURI as an intrinsic primitive datatype  _2026-10-04_
- `f5b0c00df` fix(1757): name the datatype node that lacks a complete constructor  _2026-10-04_
- `61fc549fb` fix(1757): admit definition writes that only re-derive asserted statements  _2026-10-04_
- `3238773a2` fix(1757): admit clash commitments under a source-bound contextual grammar  _2026-10-04_
- `bce830888` fix(1757): name the write that changes a selected definition grammar  _2026-10-04_
- `a95adedf4` fix(1796): read a Thing filler as any successor in restricted triggers  _2026-10-04_
- `1f80fdcb0` perf(1796): trace why a restricted trigger closure does not block  _2026-10-04_
- `c3e9669f0` perf(1796): name the members of a remaining restricted cycle  _2026-10-04_
- `d677fbe1b` fix(1796): read joint acyclicity off the weak-acyclicity position graph  _2026-10-04_
- `e0cda306c` perf(1796): report restricted-candidate sizes before the trigger checks  _2026-10-04_
- `ce276d4e8` perf(1796): skip restricted-trigger closures that cannot reach the head  _2026-10-04_
- `4c6c7844e` perf(1796): check restricted triggers only inside joint-acyclicity components  _2026-10-04_
- `770e4dcd8` perf(1796): build restricted joint acyclicity on the shared joint graph  _2026-10-04_
- `3093dfd18` perf(1796): index restricted-trigger closures by body predicate  _2026-10-04_
- `92068d629` refactor(1757): name the joint-graph frontier type  _2026-10-04_
- `895166f8b` perf(1757): compute joint acyclicity in linear Horn propagation  _2026-10-04_
- `c7c5a20c8` fix(1796): read witness-family summaries per rule on the position rungs  _2026-10-04_
- `da8d326b4` fix(1796): let exact count-1 witness families climb the polynomial ladder  _2026-10-04_
- `4e7c7f2a7` fix(1795): bind termination specializations from closed witness-free relations  _2026-10-04_
- `7a285ccfb` fix(1795): encode termination statements per atom with fresh-null keys  _2026-10-04_
- `a465c9970` feat(1796): certify restricted joint acyclicity  _2026-10-04_
- `12e5c4f8a` feat(1796): apply Datalog-first to every chase executor and producer  _2026-10-04_
- `0e9a889a4` feat(1796): run the restricted chase Datalog-first  _2026-10-04_
- `51b7abd40` fix(1789): specialize termination analysis without combinatorial products  _2026-10-03_
- `14e32bf7e` wip(1789): project immutable join solutions before specializing  _2026-10-03_
- `0c82e4d08` fix(1789): size the input-specific termination certificate for production  _2026-10-03_
- `71a47243e` fix(1789): join wildcard class positions only along real flows  _2026-10-03_
- `286d6aebf` fix(1789): restore source-constant enrichment from every selector row  _2026-10-03_
- `fccef9ccb` feat(1789): name the heaviest predicates of an undecided termination bound  _2026-10-03_
- `f65f12cb4` style(1757): clear the branch's clippy debt in value flow and chase  _2026-10-03_
- `a5a0a0bb9` fix(1789): decide the input-specific termination certificate at production scale  _2026-10-03_
- `fee7e66b5` perf(1757): hash-cons value-flow support rows  _2026-10-03_
- `82f5c3250` feat(1757): trace support bytes and transposed dense rows per large head  _2026-10-03_
- `1401ffbec` feat(1757): trace the resident footprint of large value-flow closures  _2026-10-03_
- `cbecc6f6e` test(1757): the class-partitioned closure equals the per-candidate closure  _2026-10-03_
- `01ee8f0d3` feat(1757): name the world and boundary of a refused native source  _2026-10-03_
- `7934ff3a5` perf(1757): identify support rows by nonzero words, not members  _2026-10-03_
- `5c8e931ca` perf(1757): refine value classes over live values only  _2026-10-03_
- `2eb436874` perf(1757): keep dense value-flow rows word-level in refinement  _2026-10-03_
- `9f9b2ac14` perf(1757): refine value classes by partition, not materialized signatures  _2026-10-03_
- `40588d5d8` perf(1757): evaluate one representative per exchangeable value class  _2026-10-03_
- `fa479b8fc` perf(1757): close value-flow refinement in topological component order  _2026-10-03_
- `af3c69190` perf(1757): batch rectangle supports in value-flow refinement  _2026-10-03_
- `74b9417d1` fix(1757): certify chase termination over firing producers only  _2026-10-02_
- `8b5f885c0` perf(1757): trace and restructure the native reasoner critical path  _2026-10-02_

## Files changed

```
crates/logic/src/physical/chase.rs                 | 1425 +++++++++++++++++---
 crates/logic/src/physical/chase.tests.rs           |  523 +++++++
 crates/logic/src/physical/effects.rs               |   51 +-
 crates/logic/src/physical/effects/tests.rs         |  138 ++
 crates/logic/src/physical/effects/value_flow.rs    |   89 ++
 crates/logic/src/physical/seminaive/joint.rs       |  172 ++-
 .../logic/src/physical/seminaive/joint/families.rs |   49 +
 .../physical/seminaive/joint/families/analysis.rs  |   31 +-
 crates/logic/src/physical/seminaive/joint/input.rs |   24 +-
 .../physical/seminaive/joint/input/admission.rs    |  972 +++++++++++--
 .../seminaive/joint/input/admission_tests.rs       |  174 +++
 .../src/physical/seminaive/joint/input/tests.rs    |  357 +++++
 crates/logic/src/physical/seminaive/property.rs    |   42 +-
 .../src/physical/seminaive/property/datatype.rs    |   12 +-
 .../physical/seminaive/property/datatype.tests.rs  |   44 +
 .../physical/seminaive/property/datatype/extent.rs |    1 +
 .../src/physical/seminaive/property/minimum.rs     |    8 +
 crates/logic/src/reason/dl.rs                      |   28 +-
 crates/logic/src/reason/program/list_tests.rs      |   77 ++
 crates/logic/src/reason/program/minimum_tests.rs   |   35 +
 crates/logic/src/reason/refute/counting.rs         |  219 ++-
 crates/logic/src/reason/refute/native/tests.rs     |   56 +
 crates/logic/src/reason/value.rs                   |   19 +
 crates/logic/src/result.rs                         |   10 +
 crates/logic/src/result_rdf/native.rs              |   48 +-
 crates/pipeline/src/stages/docs_render.rs          |   12 +-
 slices/core/affect/module.ttl                      |    2 +-
 slices/core/agentic/module.ttl                     |    4 +-
 slices/core/calendar/module.ttl                    |    4 +-
 slices/core/gts/module.ttl                         |    4 +-
 slices/core/norms/module.ttl                       |    4 +-
 slices/extensions/images/module.ttl                |    2 +-
 slices/grounding/lang/module.ttl                   |   18 +-
 slices/grounding/logic/design/LOGIC-PERFORMANCE.md |   22 +-
 slices/grounding/logic/module.ttl                  |   20 +-
 slices/grounding/math/module.ttl                   |    6 +-
 slices/vocabulary.ttl                              |   14 +-
 37 files changed, 4272 insertions(+), 444 deletions(-)
```

## ADRs the changed files cite

- ADR-0042 (not on disk)

These are the settled decisions this branch touches. The question
for stage2 is whether the change fits them, not merely whether it
compiles.


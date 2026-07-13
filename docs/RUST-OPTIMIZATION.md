<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics(R) Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Rust Optimization Doctrine

This document is the project-level guide for Rust performance and language-feature
work in GMEOW. It complements the always-on gate policy in
[`docs/rust-test-budget.md`](./rust-test-budget.md), the Rust/GTS toolchain policy
in [`docs/rust-gts-integration.md`](./rust-gts-integration.md), and the core
workflow rules in [`AGENTS.md`](../AGENTS.md).

The short version: optimize the Rust core by changing data shape, ownership shape,
and dispatch shape first. Compiler flags and clever syntax are secondary. Every
optimization must preserve determinism, generated-artifact reproducibility, and the
Rust-first / Python-surface boundary.

## Required Constraints

- Work in a branch worktree under `.worktrees/`; never edit the top-level checkout.
- Use Makefile targets for validation and benchmark lanes.
- Do not add Python code, Python tests, or Python orchestration unless explicitly
  authorized.
- Do not hand-edit generated files. If an optimization changes canonical output,
  regenerate from the canonical source and commit the reproducible artifacts.
- Preserve `debug-assertions = true` and `overflow-checks = true` for test and CI
  builds. Runtime checks are part of the gate.
- Do not restore debug symbols in local dev/test builds. Full debug-symbol trees
  are banned because they produce tens of GB of useless artifacts per worktree.
- Keep build-memory measurements in the optimization evidence. Do not enable
  profile-wide options that create high-RSS opt-level/LTO/codegen shapes.

## Measurement Rules

Optimization work needs evidence. Before changing a hot path, capture the current
behavior with the narrowest useful command, then rerun the same command after the
change.

The dependency lock is part of that measurement identity. The current baseline is
the lockstep `purrdf` **0.5.0** family; do not present a measurement taken against a
0.4.x build as current evidence. The 0.5.0 release already supplies ownership and
allocation reductions in the RDF IR, SPARQL/XSD, reasoning, SHACL/ShEx, GTS, slice,
and binding layers, so GMEOW benchmarks should measure on top of those gains rather
than claim them again. Its owned `QuadPatternCursor` and borrowed dataset-term
writers are additional cross-crate seams: prefer them when a GMEOW boundary would
otherwise collect a result-sized quad vector or materialize owned term trees. See
the [purrdf 0.5.0 release notes](https://github.com/Blackcat-Informatics/purrdf/releases/tag/rust-v0.5.0).

Useful lanes:

```bash
make bench           # criterion hot-path benchmark suite, report-only
make bench-compare   # live criterion run compared to bench/baseline.json
make rust-test       # always-on Rust gate, including duration budget
make check           # full Docker-free, Java-free local gate
```

For targeted investigation, use crate-local `cargo bench`, `cargo test`, `perf`,
`hyperfine`, or profiler output only as evidence for the local change. The final
verification still uses the repo Makefile targets.

Report exactly what ran. Do not invent percentages, extrapolate from unrelated
inputs, or claim a speedup from a benchmark that did not exercise the changed path.

## Preferred Rust Optimization Shapes

### 1. Static Iterator Seams

Prefer static dispatch and borrow-preserving iterator APIs on hot read paths.
`DatasetView` is the model: it returns `impl Iterator` over `QuadIds` /
`QuadRef` without allocation. Where hot logic or SPARQL paths still return
`Box<dyn Iterator>`, first ask whether the object-safe seam is actually required.

Use GATs, RPITIT, enums over iterator variants, or concrete callback-style walkers
when they remove allocation or dynamic dispatch without making the API brittle.
Keep object-safe traits only at real plugin/FFI/remote boundaries.

### 2. Dense Typed IDs Over String Keys

Prefer dense IDs and interned symbols for repeated joins, indexes, and membership
tests. The `rdf-core` `TermId` pattern is the reference: an opaque local ID with a
compact layout, no cross-dataset identity leak, and borrowed resolution at the edge.

Native logic code should avoid repeatedly rendering RDF terms to strings for
internal fact keys, predicate indexes, or join probes when an interned symbol table
or typed ID can carry the same semantics. String rendering belongs at ingestion,
diagnostics, provenance serialization, and final deterministic output boundaries.

### 3. Const Generics And Type-State For Real Invariants

Use const generics when arity is genuinely fixed and repeated at runtime:
binary relation rows, triple/quad slots, fixed profile dimensions, or bounded small
vectors. Do not use const generics merely to make code look more advanced.

Use type-state when it prevents invalid phase transitions, such as parsed ->
validated -> frozen, or source carrier -> checked carrier -> terminal snapshot.
A type-state transition is worthwhile when it deletes runtime checks or makes an
invalid call unrepresentable.

### 4. SIMD Only Where Data Is Already Dense

SIMD is appropriate for dense numeric, bitset, sorted-ID, or lane-parallel equality
work. It is usually the wrong first move for string-heavy or allocation-heavy code.

Before adding more `std::simd`, make the data contiguous and ID-based. Benchmark
the scalar dense version first, then compare SIMD against it. Keep the portable
SIMD feature gate narrow and documented.

### 5. Sealed Traits For Internal Contracts

For internal extension points, prefer sealed traits when downstream crates must not
invent implementations. This is especially useful for validated carriers, frozen
datasets, profile markers, and pipeline stage contracts.

Public trait flexibility is not free: every external implementation becomes a
compatibility burden. In this repo, correctness and future cleanup normally matter
more than downstream implementability.

## Current In-Tree Examples

Use these as the reference points before adding a new advanced-language-feature
optimization. The RDF examples live in the external lockstep `purrdf` family and
are consumed through its public Rust API:

- Static iterator seams: `purrdf::DatasetView` returns `impl Iterator` over
  borrowed quad views instead of boxed iterator objects; `QuadPatternCursor`
  provides the owned, lazy equivalent when a cursor must pin an `Arc<RdfDataset>`.
- Dense typed IDs: `purrdf::{TermId, QuadIds}` and the dataset's quad indexes keep
  hot joins on compact local IDs.
- Const generics: purrdf's internal `AxisOrder<const N>` carries fixed quad-axis
  arity in the helper type instead of spreading parallel `[T; 4]` conventions
  through the index code.
- Type-state: `purrdf::ValidatedRdfDatasetBuilder` makes validated -> frozen an
  explicit phase boundary while preserving the one-shot
  `RdfDatasetBuilder::freeze()` API.
- Compile-don't-interpret: `crates/logic/src/physical/plan.rs` lowers rules through
  parsed -> stratified -> planned -> executable type states into an immutable owned
  plan. Its identity is `(contract_hash, canonical_rule_hash, solver_version)`;
  forward and demand-transformed evaluation share the same plan type through a
  bounded cache. Positive atoms carry stable variable slots, a deterministic
  binding-aware SIPS order, a guaranteed index shape, and a preselected
  variable/constant kernel shape, while scan mode dispatch enters a const-generic
  kernel once per operator rather than branching per tuple.
- Bounded provenance algebra: `crates/logic/src/provenance.rs` defines the checked
  semiring seam used by the native Record lane's minimal-proof-height carrier and
  the incremental core's signed Z-weight counting carrier. Record stores one typed
  height per fact; Skip stores none. `crates/logic/src/explain.rs` indexes row
  identity once and descends only a queried witness subtree, so no eager proof-tree
  forest is materialized.
- Opaque score-carrying evaluation: `crates/logic/src/annotation.rs` owns the public
  algebra/contract/result types, while `physical/annotation.rs` evaluates annotation
  equations through the existing planned semi-naive joins. `dispatch_query_annotated`
  reuses the ordinary magic-transformed fact closure and treats magic relations as
  unit-valued control; `materialize_program_annotated` applies the same rule-firing
  algebra to canonical-IR materialization. Keep score combination in this physical
  seam: joining scores onto completed answers loses alternative-derivation lineage and
  duplicates evaluation work.
- Incremental non-monotone grounding:
  `crates/logic/src/physical/incremental_grounding.rs` reuses the signed recursive
  session for the positive candidate universe and differentiates the ground-rule
  projection with the same telescoping product law. The WFS/stable-model wrappers
  cache an answer only for an unchanged EDB+ground-rule slice; a changed slice
  explicitly records and runs the still-non-incremental solver from scratch.
- Deterministic intra-world parallelism:
  `crates/logic/src/physical/seminaive.rs` evaluates the immutable per-rule work of
  a semi-naive round into independent Rayon task buffers, consumes task results in
  executable program order, quality-merges duplicate heads, and mutates the stores
  only at the existing lexical `FactKey` commit. Budgeted and unbounded runs therefore
  observe the identical committed prefix, provenance, and completion frontier; one-rule
  strata and the one-worker allocation-measurement pool retain the direct path. The
  permanent balanced evidence fixture separately enters the real four-worker path and
  drift-gates its output/provenance and budget-cut parity, candidate-row critical path,
  and merge-buffer row bound. Those are scheduler-independent structural counts, not a
  wall-time speedup or byte-level memory claim.
- SIMD: `purrdf-iri` uses safe portable SIMD only for dense ASCII delimiter scans;
  semantic validation remains scalar and byte-exact.
- Sealed traits: `purrdf::DatasetView` and `DatasetMut` are sealed to the purrdf
  dataset types so downstream crates cannot invent invalid storage contracts.

## Determinism Doctrine

Performance changes must not change output order accidentally.

- Do not rely on raw `HashMap` iteration order for emitted artifacts, diagnostics,
  or golden outputs.
- If switching to a faster hash map, preserve deterministic output by sorting at the
  boundary or using a fixed-seed deterministic hasher where appropriate.
- Keep first-wins rules explicit. If an optimized join changes first-wins behavior,
  it is a semantic change and needs a golden/parity update with explanation.
- Keep generated artifact diffs reproducible through `make regenerate`.

## Build Profile Doctrine

The existing profile layout is part of the optimization surface:

- `profile.dev` and `profile.test` keep debug assertions and overflow checks on,
  disable debug symbols, strip residual symbols, and use `opt-level = 1`.
- Release builds use thin LTO and first-party `codegen-units = 1`.
- Bench builds intentionally drop thin LTO to keep iteration and memory costs bounded.

Do not change these profiles opportunistically. Profile changes require a before /
after measurement that includes wall time, peak memory where relevant, and the
effect on the unified native extension.

## Validation Expectations

For a narrow Rust hot-path optimization, use focused tests first, then the relevant
gate:

- Iterator/API shape only: `cargo test -p <crate> ...`, then `make rust-test`.
- Reasoning behavior: focused `gmeow-logic` tests and relevant conformance tests.
  Use `make reason-verify` when both native reports are needed; use `make reason`
  or `make verify` only to isolate one side.
- Validation behavior: focused `gmeow-validate` / `gmeow-shacl` tests, then
  `make validate`.
- Generated-output behavior: `make regenerate` followed by `make check-generated`.
- Final branch confidence: `make check`.

If the change is intentionally performance-only, the semantic output should be
byte-identical. If it is not byte-identical, explain the semantic reason and update
goldens through the documented review path.

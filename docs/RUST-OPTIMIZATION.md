<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Rust Optimization Doctrine

This document is the project-level guide for Rust performance and language-feature
work in GMEOW. It complements the test-performance policy in
[`docs/rust-test-performance.md`](./rust-test-performance.md), the Rust/GTS toolchain policy
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
- Keep build-memory measurements in the optimization evidence. The dedicated
  producer pays the full-LTO build cost once per admitted recipe; keep its
  compilation separate from the test and debugging profiles.

## Measurement Rules

Optimization work needs evidence. Before changing a hot path, capture the current
behavior with the narrowest useful command, then rerun the same command after the
change.

The dependency lock is part of that measurement identity. The current baseline is
the exact `purrdf` release selected in `Cargo.lock` within the manifest compatibility
requirement, including its fallible
paged-query view and succinct-pack `DatasetView` contracts; do not present a
measurement from an older lock as current evidence. Those contracts already supply
borrowed/dense RDF access, page-pattern translation, typed provider/budget/cancellation
outcomes, and deterministic request evidence. GMEOW benchmarks must measure on top of
those gains rather than claim them again. Prefer `DatasetView` pattern/cardinality
pushdown and operation-scoped `FallibleDatasetView` checkpoints whenever a GMEOW
boundary would otherwise collect a result-sized quad vector, touch irrelevant pages,
or confuse incomplete provider access with semantic absence.

Useful lanes:

```bash
make bench           # criterion hot-path benchmark suite, report-only
make bench-compare   # live criterion run compared to bench/baseline.json
make rust-test       # always-on Rust correctness gate
make check           # synchronize outputs, then run the local gate DAG (every task)
make heavy           # CI-ONLY breadth lane (wasm parity, transpile acceptance, golden soak)
```

`make check` runs every task in its DAG on every invocation; the gate's wall time
comes from scheduling, not from selection. Each task declares `sync` as a
prerequisite only when it genuinely reads a `generated/` artifact, so the gates over
authored sources start in the first wave, and the Rust surface runs as four
concurrent siblings under one `rust-build` instead of one serial node. `--explain`
prints that wave plan without running anything (and without taking the host gate
lock); `--timings-json` records per-task wall time. Both are observational surfaces
only and never enter artifacts, cache keys, or correctness decisions.

`make heavy` carries the lanes whose cost is set by breadth or by repetition rather
than by the change under test. It is CI-only by construction (it hard-fails without
`CI=true` plus a CI-vendor marker) and runs on every PR, so moving a task there is a
scheduling decision and never a coverage cut.

Local `make check` owns update-mode synchronization, so a developer does not need
to run `make check` first. The whole-run manifest makes a clean fixed point a fast
no-op; after relevant source changes, check pays the one required regeneration.
CI invokes the internal `make check-sync` target in read-only mode so committed
drift remains a hard failure.

For targeted investigation, use crate-local `cargo bench`, `cargo test`, `perf`,
`hyperfine`, or profiler output only as evidence for the local change. The final
verification still uses the repo Makefile targets.

Report exactly what ran. Do not invent percentages, extrapolate from unrelated
inputs, or claim a speedup from a benchmark that did not exercise the changed path.

Cold CI producers select
`GMEOW_LOG=warn,pipeline_phase=info,pipeline_reasoning_detail=info,pipeline_memory=info`
so the reasoning boundaries, large-input value-flow details and glossary/snapshot
subphases remain visible before a stage completes. The `pipeline_memory` target emits
the per-stage memory record: the process id, its resident size and its lifetime peak
resident size, and the allocator's live and free byte totals, at each stage's dispatch,
publication and carrier release, with the
product's quad, term and artifact-byte counts, the bytes of its dataset's term text
and record tables as the dataset states them (`dataset_payload_bytes`, which leaves out
lookup indexes, allocator overhead and every typed handle payload), and what a release
removed. A record is
also made at the start and end of each phase a stage runs through `measure_stage_phase`
and at the end of each timed phase of the snapshot serializer, whose stratum
canonicalization also has a start record. A phase that is only
logged makes no record of its own: that includes the phases the logic crate logs inside
reasoning and projection, the receipt encode among them, which fall inside the enclosing
measured stage phase. No trim follows a release directly, so resident size in a release
record includes pages the allocator kept. Freed pages are returned to the system once
after each stage's publication and the releases it caused, and inside the reasoning
stage after its input is prepared and after `reason-all`; each return makes a `reclaim`
record with the resident size immediately before it, the resident size after it and the
time it took. In the log these are `reclaim_rss_before_kib`, `rss_kib` and
`reclaim_elapsed_us`; in the timings JSON they are `reclaim.rss_before_kib`, `rss_kib`
and `reclaim.elapsed_us`. Returns never overlap, but other threads go on allocating and
freeing between the two readings, so their difference is what the return gave back net
of that activity, and it can be small or negative beside a busy stage. A return
frees whole pages only, so fragmented free bytes stay resident. The allocator totals
are glibc's `mallinfo2`: live bytes are what it reports as in use, and free bytes are
the free chunks the allocator keeps, including pages a return already handed back. The
resident size and the live bytes measure different scopes: the resident size also
counts code, thread stacks and resident file and shared mappings, which the allocator
totals do not. The resident size less the live bytes is therefore an indication of the
resident memory the allocator holds unused, and is neither a bound on it nor a measure
of what a return can recover. It reads high by the resident memory the allocator does
not own. It reads low because glibc counts chunks freed into its per-thread caches as
still in use, so they are inside the live bytes, and because live pages that are swapped
out or never touched are not resident; the two readings are also not taken at one
moment. Each record
costs one read of `/proc/self/status` and one `mallinfo2` call, which walks each
arena's free lists under that arena's lock; a `reclaim` record reads the status file
twice. The records of a run make at most five such reads per executed stage plus two
per measured phase execution, plus two for each return inside the reasoning stage. The
scheduler's records, and the phase records crossed on a thread that
is running a stage body, are made whether or not the target is selected, and a completed
`--timings-json` run carries those, resident sizes included, as
`observations.stage_memory`; on a host without Linux procfs the resident figures are
null, and on a target without glibc the allocator totals are null and a return of freed
pages does nothing. A measured phase crossed on a pool thread that is running no stage body makes no
kept record: with the target selected it is logged, and with the target off it is
dropped. Selecting the target writes each record to the log as it is made, and
`GMEOW_STAGE_MEMORY_LOG=<file>` appends each one to that file as one line whatever
`GMEOW_LOG` selects; the hosted observer points the variable at its resource log, so a
producer that is killed part-way still leaves them in the uploaded resource artifact.
A start without an end identifies unfinished
work; an end records elapsed time, not semantic acceptance. These observations never enter generated
bytes or cache identities. Keep the same selected inputs and producer profile when
comparing runs, and distinguish time spent discovering sources, rendering RDF,
composing the carrier and publishing native handles.

The validation timing JSON's `example-shacl.example_work` separates shared base
projection and shape preparation from each example's parse, canonical projection,
view binding and complete validation. Durations use microseconds; parallel example
durations overlap, while the enclosing phase's `elapsed_ms` measures wall time.
An authenticated verdict hit has `execution: null`: observations from the original
execution are never replayed as current work. A partial miss still lists every
selected example, in deterministic source-path order, and carries `execution`
only for the examples evaluated in the current run.

Each completed example reports PurRDF's composite copy, freeze and materialization
counters, retained payload and adapter bookkeeping charges. These counters cover
the composite and its Core/query adapters; they do not measure source parsing,
GMEOW's canonical projection allocations, allocator overhead or process RSS.
Retained payload includes the shared base for each binding, and Core/query adapters
may also share storage. Do not sum those charges as resident memory. Pair these
observations with separately scoped peak-memory measurements on the authenticated
O3/full-LTO producer. They explain conversion removal and reuse; they cannot stand
in for fusion or scheduling measurements, semantic certificates, or output parity.

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
tests. purrdf's `TermId` pattern is the reference: an opaque local ID with a
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
- Direct logic views: `crates/logic/src/seam.rs` keeps the external/provider boundary
  object-safe with a callback visitor while statically dispatching the underlying
  `DatasetView`. Backward plans push named-world plus subject/predicate/object patterns
  and cardinality estimates before any RDF row is owned. The fallible dispatch and
  selected-materialization boundaries sample PurRDF before and after execution, discard
  partial internal rows on operational failure, and return source/page/generation
  evidence with the certified result.
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
- Shared source ingestion: the source stage parses each authored document once
  through the native adapter and retains its original dataset, role and spans.
  The base catalog, authored default, import graph, provenance projection and
  source-span table consume that same collection. Import transport no longer
  serializes and reparses N-Quads. Public import-span suppression remains a
  projection policy; the original source records stay separate. The collection
  lives only for the producer stage and does not cache cumulative carriers.
- Typed action identity: product and selected-graph keys bind complete native
  handle fields alongside the governed RDF graph. Typed CBOR streams into the
  digest without retaining a serialization buffer; published commitments are
  reused by scheduling and receipts. Cached handles restore their complete typed
  values without reparsing RDF. Publication retains the projection-binding checks,
  and hydration verifies the recomputed whole-product commitment. Releasing a
  transient span table preserves published identity, so warm fixture admission
  needs no separate hydration or transition-witness action.
- Diagnostic metadata: each immutable value retains only canonical JSON lexical
  evidence, preserving exact numbers and sorted last-wins objects. CBOR and its
  streamed typed commitment borrow that text directly. JSON/SARIF projections
  parse temporary native values on demand; they do not retain parallel trees or
  hide read/allocation errors as absent metadata. This is an evidence-storage
  contract, with projection costs, not a measured performance advantage.
- Native compiler canonicalization: `graphutil::canonicalize_blank_nodes` applies
  PurRDF's `canonical_relabel` directly to the typed dataset. Label issuance and
  RDF 1.2 statement preservation stay upstream-owned; the compiler no longer
  allocates canonical N-Quads and immediately reparses them. Compiler projection
  goldens verify the GMEOW output contract without duplicating RDF conformance.
- Shared program preparation: `crates/logic/src/program_analysis.rs` lowers the
  canonical rules and full formulas once for program-driven reasoning and
  materialization. Predicate selection and selected-view execution borrow the same
  immutable rules and loss evidence. A process-local LRU retains at most 64 small
  preparations, with independent rule-count, atom-count and rendered
  payload bounds. Its digest streams the full typed program, including contextual
  scope and provenance, directly into the hasher. No dataset, result or serialized
  carrier is retained; eviction recomputes the same lowering and cannot weaken it.
- Native reasoning input ownership: `prepare_reasoning_input` and source-edit
  refresh retain one ordered set of executable `FactKey` tuples, then move their
  fields into the final `Fact` vectors. Original native statements, source
  contexts, ingress identity and contextual preparation remain separate from
  executable deduplication. A synthetic all-IRI input with 4,096 distinct rows,
  the default world and a declared empty world retained 4,096 keys without a
  second 4,096 `Fact` values. At the final conversion boundary, initialized
  key/value bytes fell from 1,507,328 to 753,664 and owned IRI string capacity
  from 671,744 to 335,872 bytes. A real source assertion refreshed 4,097 rows:
  initialized key/value bytes fell from 1,507,696 to 753,848 and IRI capacity
  from 671,908 to 335,954 bytes. Both paths preserved the exact ingress and
  ordered-fact digests. These selected live-entry observations exclude B-tree
  node slack and links, original-source evidence, contextual storage, transient
  and probe allocations, allocator overhead and process RSS; they establish no
  full-pipeline memory reduction.
- Bounded termination analysis: witness-free relation closure admits at most
  262,144 pending candidate occurrences in both the initial and semi-naive passes.
  Exact native fact payloads are shared in first-seen order; a borrowed-field index
  holds only row positions. Logical occurrence accounting remains separate from
  payload storage: the cap is checked before duplicate lookup, so even another
  duplicate at the full bound stops the analysis with the same producer, committed
  count and `pending_rows` occurrence count. The index and count reset at the
  complete batch drain; unique facts enter the unchanged committed-row accounting
  in first-seen order. The existing immutable-only analysis remains available and
  retains the bound observation on refusal; incomplete closure never becomes a
  termination certificate. The bounds count occurrences and committed rows, not
  their byte size, and establish no full-pipeline memory reduction.
  A synthetic two-producer 512 by 256 fanout retained 262,144 logical occurrences
  with one shared payload instead of 262,144 copies. At the drain/refusal checkpoint,
  row-vector capacity fell from 262,144 to 4 (48,234,496 to 736 backing bytes), and
  IRI string capacity from 7,340,032 to 28 bytes; the index held capacity 3 and
  allocated 52 bytes. An all-distinct control with 131,072 outputs retained the same
  vector capacity and 24,117,248 backing bytes plus 5,289,472 IRI string capacity
  bytes, and added an index of capacity 229,376 with 2,359,312 allocated bytes.
  These selected pending-storage observations exclude other stores, transient
  grounding and probe allocations, allocator overhead and process RSS. They show
  duplicate-payload sharing and its distinct-output index overhead, not universal
  memory savings.
- Selected reasoned projections: `crates/logic/src/verify.rs` builds asserted plus
  native-derived closure, checks DL completeness and the observed-not-derived
  boundary, and attaches expression identity through one shared materializer.
  Full verification extends that base with the authored math-dimension and
  enactment-integrity programs. The expression-substrate producer selects its
  exact operator-cardinality law from the canonical math module and consumes the
  checked base directly, so its small acceptance scenes do not execute unrelated
  repository-wide gate programs. The independent verify-gates producer remains
  the authority that runs those programs and their required checks.
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
- Keep generated artifact diffs reproducible through `make check`.

## Build Profile Doctrine

The existing profile layout is part of the optimization surface:

- `profile.dev` and `profile.test` keep debug assertions and overflow checks on,
  disable debug symbols, strip residual symbols, and use `opt-level = 3` for both
  first-party crates and dependencies. Dependency runtime checks remain off; workspace
  crates retain them. This is the measured checks-on O3 contract, not release mode.
- The repository producer uses `profile.pipeline`: O3, full (fat) LTO, one codegen
  unit, no incremental compilation, and no debug symbols. Workspace assertions and
  overflow checks remain enabled; dependency checks retain their existing policy.
  Its native C/C++ dependencies also compile at O3. `make producer-build` admits the
  resolved runtime dependency graph and stamps the linked executable with that
  recipe. CI caches and transfers the executable together with its receipt;
  producer commands authenticate both before accessing the corpus. A test/debug
  executable cannot enter a producer operation.
  Reusing that pair in another checkout retains the selected checkout as the
  runtime root: fixture workers bind it before opening corpus stores, and each
  immutable worker copy carries the same authenticated executable receipt.
  Documentation asset gates also read the selected checkout's witnesses and pins.
  Action owners combine their own implementation inputs with the recipe's resolved
  compilation policy. The executable's wider source identity authenticates the
  binary without making a CLI-only edit invalidate unrelated corpus products.
- Consumer release builds use thin LTO and first-party `codegen-units = 1`.
- Bench builds intentionally drop thin LTO to keep iteration and memory costs bounded.

Do not change these profiles opportunistically. Profile changes require a before /
after measurement that includes wall time, peak memory where relevant, and the
effect on the production pipeline. CI records producer build wall time, CPU time,
and peak child-process RSS separately from pipeline execution. A compiler-profile
change alone is not evidence of a runtime speedup.

## Validation Expectations

For a narrow Rust hot-path optimization, use focused tests first, then the relevant
gate:

- Iterator/API shape only: `cargo test -p <crate> ...`, then `make rust-test`.
- Reasoning behavior: focused `gmeow-logic` tests and relevant conformance tests.
  Use `make reason-verify` when reasoning and reasoned-graph verification are both
  needed from one closure; use `make reason` or `make verify` to isolate one side.
- Validation behavior: focused `gmeow-validate` / `gmeow-shacl` tests, then
  `make validate`.
- Generated-output behavior: `make check` (it materializes through the single
  producer and then gates the result — never a separate regenerate pass first).
- Final branch confidence: `make check`.

If the change is intentionally performance-only, the semantic output should be
byte-identical. If it is not byte-identical, explain the semantic reason and update
goldens through the documented review path.

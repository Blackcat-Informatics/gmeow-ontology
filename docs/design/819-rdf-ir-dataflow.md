<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# RFC: `gmeow-rdf` as the optimal IR & dataflow stream

This is the design RFC for `gmeow-rdf` as the optimal IR and dataflow stream.
Parent: the reference RDF-1.2 stack (META-EPIC). Builds on the Rust
reasoning authority work.

This is a design RFC. No engine code lands with this document; the architecture is
realized through the staged children C0–C8 below. **C0 (a semantic RFC + loss matrix)
is a mandatory first child** — see [Semantics](#the-ir--an-immutable-value-interned-rdfdataset)
and [Ordering](#epic-decomposition).

## Context

The Rust acceleration work finishes moving the logic compiler, reasoning, SHACL, and validation
paths off Python/rdflib into Rust. The declared end-state — per the north-star goals
(rust-first, maximal information flow, overcome-not-inherit constraints, SOTA/greenfield)
and the logic-stack-retirement doctrine ("Rust is the authority") — is that **all**
Python is eventually replaced by Rust.

Designing for that end-state changes what "optimal" means. Today three RDF
representations fight inside the Rust code, and the Python boundary forces text
round-trips.

1. **Legacy owned-quad reader (kernel model, `crates/rdf/src/model.rs`, `store.rs`)** —
   owned-`String` terms, no value-interning. Its boxed iterator **cloned every quad per
   iteration** and cloned the whole lookaside each call. It was a lowest-common-denominator
   *conversion shim*, not a working IR, and it has since been removed.
2. **oxigraph `Store` (the de-facto working IR)** — `logic`, `shacl`, `validate`, and
   `slicetest` all materialized into oxigraph and SPARQL-query it. The transitional
   owned-quad path used to feed oxigraph materialization; a subsequent cleanup replaced that with
   `RdfDataset`-native ingress.
3. **GTS folded `Graph` (an efficient transport)** — an ID-addressed physical
   representation (`terms: Vec<Term>` + id-tuple `quads`/`reifiers`,
   `gmeow-gts model.rs:222`). **Important correction:** GTS term IDs are *segment-local
   compression artifacts, never identity*; cross-segment equality is by resolved term
   value, blank-node scopes are separate, and the GTS writer deterministically *reorders*
   terms without necessarily deduplicating one semantic term per value. GTS is therefore
   an excellent **transport ingress**, not a ready-made interned IR.

Every stage still pays a conversion tax: GTS → `RdfQuad` (clone) → oxigraph (clone
again). The historical Python FFI also added text round-trips for logic and SHACL.
Those logic paths now consume typed RDF values directly in the native engine.

**Intended outcome:** `gmeow-rdf` exposes a single **immutable, RDF-semantics-defined,
value-interned dataset IR** with GTS as an efficient transport ingress, ID-native
consumers, and explicit compatibility backends at the edges — replacing the clone shim
and collapsing both the GTS↔`RdfQuad`↔oxigraph conversion and (once Python is gone) the
text FFI. The best version of this design is **not merely "GTS-shaped"**; it defines RDF
identity itself and treats GTS IDs as a physical detail remapped on import.

## Target architecture

### The IR — an immutable, value-interned `RdfDataset`

Because the structure contains quads and named graphs, the canonical type is
**`RdfDataset`** (not `RdfGraph`). It is built once through a fallible builder, validated,
frozen, and shared as `Arc<RdfDataset>`:

```text
RdfDatasetBuilder
    ├── validates (positional constraints, ID references, cycles)
    ├── value-interns terms (RDF identity, not GTS IDs)
    ├── deduplicates RDF quads
    ├── resolves blank-node scopes
    └── freezes
             ↓
       Arc<RdfDataset>
```

**Identity & invariants (the C0 semantic contract — stated normatively):**

- `TermId` is **local to one frozen `RdfDataset`**; it is *not* persistent, serializable,
  or meaningful across dataset merges.
- GTS IDs are **remapped through RDF value identity** during import (segment-local IDs are
  discarded as identity).
- **Blank-node scope participates in the interning key.**
- **Triple terms** (RDF-1.2 quoted triples) are identified *structurally* by their
  resolved `(s, p, o)`.
- **Reifier resources are stored separately**; many reifiers may point to one triple term.
- **Predicate and graph-name positional constraints** are validated before freeze.
- **All ID references are valid and triple-term cycles are rejected** before any consumer
  sees the dataset.
- **Literal identity is specified, not inherited from a backend**: default-datatype
  expansion, language-tag handling, base direction, and lexical spelling are defined by
  the IR. Oxigraph is explicitly **not** the identity oracle — its store may canonicalize
  typed-literal lexical forms.

The frozen dataset gives stable IDs for its lifetime, infallible allocation-free
iteration, safe sharing across SHACL/logic/validate/Python, lazy indexes and lazy
oxigraph caches without invalidation complexity, capability flags computed once, and a
single natural point for hard failures on malformed structure.

### Separate the hot dataset from the transport envelope

"One canonical IR" must **not** mean blobs, signatures, segment ledgers, opaque frames,
suppressions, and RDF quads share one hot structure:

```text
RdfBundle
├── dataset:  RdfDataset            // compact term / quad / reification / annotation /
│                                   // sparse source-location tables — the hot path
└── envelope: RdfEnvelope (RdfLookaside)   // GTS evidence & package material
```

This is already a fidelity concern: `RdfLookaside` owns substantial nested metadata, yet
its blob records do **not** contain the actual blob bytes — so the current GTS writer
explicitly cannot preserve blobs. The RFC therefore defines **two distinct gates**:

1. **RDF dataset round-trip fidelity** (the hot graph: terms, quads, reifiers,
   annotations, named graphs, directional literals, nested triple terms).
2. **Full GTS bundle/envelope round-trip fidelity** (blobs, signatures, suppressions,
   opaque frames, segment ledgers).

They are not the same promise and must not be conflated.

### RDF 1.2 fidelity blockers and the loss ledger

At least two current GTS↔RDF mismatches are named directly, because "RDF 1.2 fidelity"
cannot be claimed until they are resolved:

- The RDF model supports **literal base direction**, but the GTS term schema has no
  direction field and the writer drops it.
- GTS permits **several reifiers for the same `(s,p,o)`**, but the current writer rejects
  a second distinct explicit reifier for that triple.

The fidelity gate must therefore either **(a)** first extend the GTS representation to
carry direction and correct multi-reifier handling, or **(b)** define a **formal,
machine-readable loss ledger** and stop calling that conversion lossless. Every
intentional conversion loss is enumerated and tested.

### Iteration surface (infallible, ID-native + borrowed)

The clone-per-item, `Result`-yielding `Box<dyn Iterator<Item = Result<RdfQuad, _>>>`
(`store.rs:40`) is retired. The frozen dataset exposes both:

- `QuadIds` — a small `Copy` row (`[TermId; 4]`-shaped) for ID-native consumers.
- `QuadRef<'a>` — a borrowed resolved view (`&Term`) for consumers that need lexical
  values.

The canonical iterator does **not** return `Result`: validation/diagnostics belong to
*ingestion* (the builder), not to iteration over an already-frozen dataset.

### Performance properties — measurable, not slogans

"Columnar" and "zero-copy" are removed as architectural commitments and replaced with
benchmark-selected layouts and operational requirements:

- **Layout is chosen by benchmark**, not asserted. The GTS-shaped `Vec<(s,p,o,g)>` is
  *row-oriented*, not columnar; candidates to measure include array-of-structures quad
  rows, structure-of-arrays columns, predicate-grouped adjacency tables, and sorted rows
  plus secondary indexes.
- **Operational requirements** (these replace the absolute "zero-copy" claim):
  - zero heap allocations per canonical quad iteration;
  - zero term-string clones during iteration;
  - at most one import/interning pass per external representation;
  - no repeated conversion between the canonical IR and oxigraph;
  - GTS import **moves** owned strings where possible rather than cloning them.
- Literal zero-copy across *every* backend is not a realistic contract: oxigraph owns its
  own compatibility representation.

#### Layout decision — bench-driven, not asserted (C1 Task 7)

The benchmark gate above is now realized by `crates/rdf/benches/ir_layout.rs` (the
`make bench` lane; excluded from `make check`). It builds a deterministic, representative
few-thousand-quad dataset (IRIs, blanks across scopes, typed + language-tagged literals, a
named graph, reifiers + annotations, nested triple terms) and reports the operational
metrics the gate demands **beyond quads/sec**:

- **Total allocated bytes, allocation count, and an allocator high-water mark** for one full
  *build* (intern + push + freeze), one full AoS *iteration*, and one full *resolution* pass.
  These come from a process-global counting `#[global_allocator]` (the same pattern as
  `tests/ir_zero_alloc.rs`, extended with byte and net-live high-water tracking) whose
  thread-local counters are snapshotted around each measured region and printed as deltas. A
  true peak-RSS read is impractical in-process, so the high-water mark of net-live allocated
  bytes approximates peak memory — stated, not hidden.
- **Index-build cost:** there is no standalone secondary index in the shipped dataset today.
  The only non-linear structure is the sparse, handle-sorted source-location table built at
  freeze, so its build cost is already folded into the *build* group rather than benched as a
  separate pass; the bench notes this explicitly.
- **Layout comparison, measured head-to-head on the SAME frozen quads:** the bench builds a
  bench-local **structure-of-arrays** (`{ s, p, o, g: Vec<…> }`) shim and a bench-local
  **predicate-grouped adjacency** shim and benchmarks the identical iteration on all three, so
  the choice is *measured*. The shims are measurement-only and are NOT wired into the real
  dataset.

**Decision: the array-of-structures `QuadRow` layout (`Box<[QuadRow]>`) is retained** as the
shipped layout. The harness measures AoS vs SoA vs predicate-adjacency, so the choice is
bench-driven, not asserted. AoS keeps the quad row a contiguous `Copy` value that the resolved
hot path (`quad_refs()`/`resolve()`) borrows directly, confirms the zero-allocation iteration
and resolution requirements operationally (both regions report **0 allocations**), and the
predicate-adjacency shim is materially slower to iterate. The SoA shim's narrow edge on the
pure ID-fold micro-iteration is recorded by the bench as standing evidence: if a future
ID-native consumer makes that edge material under a real workload, this harness is the
instrument to revisit the decision against, rather than re-asserting it.

### ID-native consumers

- **SHACL** — the expensive seam was that the former store entrypoint materialized the *entire*
  source into oxigraph, and the Core constraint/path engine is typed around
  `oxigraph::Store`/`Term`. Ordinary SHACL targets already use oxigraph *pattern lookups*;
  only `SPARQLTarget` invokes SPARQL. So the work is to port **SHACL Core graph access,
  paths, target resolution, and Core constraints** to an ID-native `ShaclDataGraph` over
  `RdfDataset`, retaining the lazy oxigraph backend **only** for SHACL-SPARQL targets and
  constraints. (Porting target selection alone would not eliminate the materialization.)
- **Logic** — the full seam is typed end-to-end. The native engine consumes compiler IR and
  typed EDB facts, emits typed derived rows with provenance, and uses direct world/graph IDs.
  Production evaluation performs no executable rule-text or fact-string round-trip.

### Streaming — reuse the existing GTS event contract

`gmeow-gts` already defines `StreamingSink` and `read_to_sink` with term, quad, reifier,
annotation, suppression, blob, signature, and diagnostic events (its docs correctly warn
that IDs are segment-local and that the current implementation still folds each segment
enough for validation). Rather than add a parallel `QuadSource`, generalize this existing
vocabulary:

- Name it `RdfEventSource` / `RdfEventSink` (ID-addressed quads require term
  declarations).
- Sink methods return `Result<ControlFlow<…>, E>` for failure **and** cancellation.
- Add batch methods such as `quads(&[QuadIds])`.
- Preserve segment / blank-node-scope boundaries explicitly.
- `RdfDatasetBuilder` **implements the sink** (evented ingestion freezes into a dataset).
- Separate "evented ingestion" from "bounded-memory end-to-end processing" — a reasoner
  may still require the complete EDB in memory.

### Lazy, policy-keyed oxigraph backend (a migration bridge, landed early)

The lazy oxigraph backend is not an end-state cleanup; it is the bridge that keeps every
SPARQL-dependent consumer working while they migrate, so it lands **shortly after** the
immutable dataset and borrowed APIs. Critical constraint: the cache is **keyed by
projection policy**, because the repo needs at least two incompatible projections —
*preserve named graphs* (world-indexed logic) and *flatten to default graph* (current
SHACL). A single unqualified `OnceLock<Store>` would silently hand one consumer the wrong
dataset semantics; cache separate policy-specific stores (or a preserved store plus an
explicit dataset view). `RdfDataset` remains the **lexical source of truth** — never
re-serialize through oxigraph after it has normalized terms.

### FFI ownership model

Python cannot safely hold a Rust `&RdfDataset` across calls. The model is ownership by
`Arc`, borrow per call:

```text
Python object / versioned capsule
        owns Arc<RdfDataset>
                 ↓
Rust operations borrow &RdfDataset during each call
```

The goal is to **retire text exchange**, not necessarily the capsule: a *versioned opaque
handle* with a defined destructor and `Arc` ownership is a perfectly good zero-copy
transitional ABI.

## Epic decomposition

| Child | Scope |
| ----- | ----- |
| **C0** | **Semantic RFC + loss matrix (mandatory first):** term identity, blank-node scope, triple terms, multiple reifiers, literal direction/normalization, duplicate policy, dataset-vs-envelope split, `TermId` lifetime, graph-policy semantics. |
| **C1** | Immutable `RdfDatasetBuilder → Arc<RdfDataset>`: typed IDs, validation, sparse source-location tables, borrowed terms, infallible zero-allocation iteration (`QuadIds` + `QuadRef`). |
| **C2** | GTS consuming importer + event-sink importer: reuse the existing streaming events; remap segment IDs by value; move strings where possible. |
| **C3** | Lazy, policy-keyed oxigraph compatibility backend (the migration bridge — keeps behavior available while consumers move). |
| **C4** | Native SHACL Core graph interface (`ShaclDataGraph`), paths, target resolution, Core constraints, and indexes over `RdfDataset`; oxigraph retained for SHACL-SPARQL only. |
| **C5** | Typed native logic bridge: typed EDB injection, typed derived rows, handle-based provenance, direct world IDs. |
| **C6** | General evented output / projection sinks (`RdfEventSink`) for chase output, SHACL results, projections. |
| **C7** | `Arc`-backed Python handle; remove text-exchange FFI paths. |
| **C8** | Delete the old owned store shim and redundant stores. |

Ordering rationale: C0 fixes semantics *before* code; C1 is the foundation; C2 brings data
in; **C3 lands early as a bridge**; C4/C5 are the ID-native consumer ports (C5 gated on its
spike); C6 generalizes output; C7/C8 are end-state cleanups gated on the Python cutover.

## Critical files / anchors

- Kernel IR & surface: `crates/rdf/src/model.rs`, `store.rs`, `oxigraph.rs`, `gts.rs`,
  `gts_write.rs`, `lookaside.rs`, `diagnostic.rs`, `py_store.rs`, `lib.rs`.
- GTS transport & streaming to reuse: `gmeow-gts` `model.rs` (`Graph`/`Term`),
  `StreamingSink` / `read_to_sink`.
- Consumers to migrate: `crates/logic/src/store.rs`, `reason/mod.rs`, `provenance.rs`;
  `crates/shacl/src/engine.rs` (`validate`, `validate_dataset`, `parse_shapes`, target
  helpers `:64-110`), `report.rs`; `crates/validate/src/store.rs`, `validate_all.rs`;
  `crates/slicetest/src/stores.rs`.
- FFI scaffolding to retire: `src/gmeow_tools/oracles/native_rl_rdflib.py`, `logic_runner.py`,
  `tests/_graph_nt.py` (the RL-closure shim `native_rl.py` is already retired).

## Verification — stronger acceptance gates

The criterion + property-test plan must include, in addition to staying green on
`make check` / `make test` and the logic **derivation-graph goldens**:

1. `RdfDataset::quads()` performs **zero allocations**; iterating a quad never clones or
   formats a term.
2. GTS **multi-segment** tests prove blank-node scope isolation and correct ID remapping
   (segment-local ID → value identity).
3. Dedicated tests for: directional literals, nested triple terms, multiple reifiers per
   `(s,p,o)`, duplicate annotations, named graphs, explicit-vs-default datatypes, and
   malformed/cyclic ID references.
4. The **structural comparator operates directly on `RdfDataset`**; oxigraph is *not* the
   sole equality oracle (it canonicalizes lexical forms). Use the pyoxigraph star
   comparator where RDF-1.2 `<<>>` comparison is needed — `rdf_compare` cannot parse it.
5. The lazy backend is constructed **at most once per projection policy**; cold and warm
   lazy-backend timings are measured separately.
6. Benchmarks report **total allocated bytes, allocation count, peak memory, index-build
   cost, and end-to-end SHACL/reasoning latency** — not only quads/sec — for the
   GTS→logic, GTS→shacl, and GTS→validate paths.
7. **Every intentional conversion loss is machine-readable** (the loss ledger) **and
   tested**; "fidelity" is asserted only where the ledger is empty.

## Doctrines honored

Greenfield / no-backcompat (replace the clone shim; no fallback); no-optionality /
hard-fail (malformed structure fails at freeze; orphan references rejected);
one-PR-at-a-time (decomposed; lands child by child). The best version of this design is an
immutable, RDF-semantics-defined dataset IR with GTS as an efficient transport ingress,
native consumers on IDs, and explicit compatibility backends at the edges.

## Appendix C0 — Ratified semantic decisions (normative)

This appendix **ratifies** the C0 semantic contract sketched in
[Identity & invariants](#the-ir--an-immutable-value-interned-rdfdataset). The bullets above
are descriptive; the rules below are **normative**. Conforming implementations of the
`gmeow-rdf` IR **MUST** obey them. Keywords (MUST, MUST NOT, SHALL, MAY) are used per
RFC 2119. Each conversion loss permitted by these rules is enumerated in the
machine-readable loss ledger (`crates/rdf/src/loss.rs`, rendered to
`generated/rdf-loss-matrix.json`); the loss codes referenced below are the stable contract.

### C0.1 Literal identity

- The IR **SHALL** define literal identity itself; oxigraph (or any external store) is
  **NOT** the identity oracle. An external store **MAY** canonicalize typed-literal lexical
  forms; the IR **MUST NOT** adopt that canonicalization as identity.
- A literal with no explicit datatype **SHALL** be expanded to its default datatype at
  intern time: a plain literal expands to `xsd:string`, and a language-tagged literal
  expands to `rdf:langString`. After expansion every literal **MUST** carry an explicit
  datatype in its identity key.
- Language tags **SHALL** be lowercased for the identity key (BCP 47 case-insensitive
  comparison). The original-cased tag **MAY** be retained for emission but **MUST NOT**
  affect identity.
- A literal's **base direction** (`ltr` / `rtl`, RDF 1.2) **SHALL** participate in the
  identity key. Two literals that differ only in base direction are **distinct**.
- The **lexical spelling** of a literal **SHALL** be preserved verbatim; the IR **MUST NOT**
  rewrite lexical forms (no number/date canonicalization at intern).

### C0.2 Blank-node scope

- Blank-node scope **SHALL** participate in the interning key. Two blank nodes from
  different scopes are **distinct** even when they share a label; two blank nodes in the
  same scope with the same label are the **same** node.
- GTS term IDs are segment-local compression artifacts and **SHALL NOT** be treated as
  identity. On import, segment-local IDs **MUST** be discarded and terms **MUST** be
  remapped through RDF value identity.
- A non-streaming GTS read (`gmeow_gts::reader::read()`) folds all segments into one term
  table and therefore collapses per-segment blank-node scope. This is an intentional,
  enumerated loss (`bnode-scope-flatten`); per-segment scope is recoverable **only** via the
  streaming-event importer, which **MUST** preserve segment boundaries.

### C0.3 Triple terms (RDF 1.2 quoted triples)

- A triple term **SHALL** be identified **structurally** by its resolved `(s, p, o)`. Two
  triple terms with equal resolved components are the **same** term.
- Triple-term reference cycles **MUST** be rejected before freeze. No consumer **SHALL** ever
  observe a dataset containing a triple-term cycle.

### C0.4 Reifiers

- Reifier resources **SHALL** be stored separately from the triple terms they describe.
- Many reifiers **MAY** point to one triple term; the IR **MUST** permit several distinct
  reifiers for the same `(s, p, o)`.
- The current GTS writer rejects a second distinct explicit reifier for the same `(s, p, o)`
  (`rdf-conflicting-reifier`). Projecting an IR dataset that uses multiple reifiers per
  triple to GTS is therefore an intentional, enumerated loss (`multi-reifier-collapsed`)
  until the GTS schema is extended. Fidelity **SHALL NOT** be claimed for such a projection.

### C0.5 Duplicate quads

- The dataset is a **set** of quads. Duplicate quads (and duplicate annotations)
  **SHALL** collapse to a single member at freeze. Quad multiplicity is **NOT** semantically
  significant and **MUST NOT** be preserved.

### C0.6 Dataset vs. envelope split

- The hot dataset (terms, quads, reifiers, annotations, named graphs, sparse source
  locations) **SHALL** be kept distinct from the transport envelope (`RdfLookaside`: blobs,
  signatures, suppressions, opaque frames, segment ledgers). They are two **separate**
  round-trip promises and **MUST NOT** be conflated.
- `RdfLookaside` blob records carry blob metadata but **not** the blob bytes. Projecting blob
  payloads to GTS is therefore impossible today; this is an intentional, enumerated loss
  (`blob-bytes-absent`).

### C0.7 Literal base direction → GTS

- The GTS term schema has no literal base-direction field. The GTS writer **SHALL** drop a
  literal's base direction; the lexical form, datatype, and language tag are preserved. This
  is an intentional, enumerated loss (`direction-dropped`). RDF 1.2 fidelity **SHALL NOT** be
  claimed for any conversion whose loss ledger is non-empty; "fidelity" is asserted **only**
  where the relevant ledger (`LossLedger::is_empty`) is empty.

### C0.8 `TermId` lifetime

- `TermId` is **local to one frozen `RdfDataset`**. It **SHALL NOT** be persisted,
  serialized, or compared across datasets, and it is **NOT** merge-stable. Any consumer
  needing a durable identifier **MUST** resolve the term to its RDF value rather than
  retaining a `TermId`.

### C0.9 Graph policy

- Two projection policies are defined and **SHALL** be selected explicitly:
  - `PreserveNamedGraphs` (**default**): named graphs are retained; the quad's graph name is
    part of its identity.
  - `FlattenToDefaultGraph` (**opt-in**): all quads are projected into the default graph;
    graph-name distinctions are intentionally discarded.
- A lazy oxigraph backend **SHALL** be keyed by projection policy. A single unqualified cache
  **MUST NOT** be shared across policies, because the two projections carry incompatible
  dataset semantics.

## Appendix Z — PR implementation status & C5 spike finding (paudley/819-rdf-ir-dataflow)

This appendix records the state of each child as delivered on the
`paudley/819-rdf-ir-dataflow` branch, and the **C5 feasibility spike** result the
RFC required as C5's first deliverable.

### Delivered

- **C0/C1/C2** — semantic contract + loss ledger; immutable value-interned
  `Arc<RdfDataset>` with zero-alloc iteration; GTS event-sink + consuming-graph
  importers.
- **Loss ledger flips** — gmeow-gts 0.9.4 added `Term.direction`, reifier-id-keyed
  `Graph.reifiers`, and `decoded_blobs`, letting *literal base direction* and
  *multiple reifiers per (s,p,o)* round-trip losslessly. Blobs are an intentional
  **by-reference** entry: the IR holds the content-addressed `blob_id` digest +
  origin (never the payload — blobs may be multi-terabyte); payload streaming
  origin→destination is a follow-up.
- **C3 bridge** — the retired dataset-to-owned-quad adapter threaded each quad's source
  **location** into the owned model and remapped location `QuadHandle`s across the
  freeze-time sort (two LSP-correctness fixes), and the IR gained
  `reifiers_of`/`annotations_of` accessors.
- **C4** — the SHACL Core engine is generic over a `ShaclDataGraph` trait with an
  IR-native backend; `validate_dataset` runs on the IR with no whole-store
  oxigraph materialization (oxigraph retained only for SHACL-SPARQL). Conformance
  is held by a 16-case oxigraph-vs-IR differential equivalence test.
- **C6** — `RdfEventSink`, the ID-addressed evented OUTPUT dual of the import sink.
- **Tasks 8–11 (Python→Rust cutovers)** — GTS production (incl. the signed
  `gmeow.gts`), canonical Turtle normalization (a native IR serializer that
  improves on rdflib `longturtle`), the language-tag policy core, and Crossref
  deposit-XML are all Rust-authored now (with byte-identity or isomorphism gates).
- **Task 12 (SARIF/annotation threading)** — IR source locations thread
  end-to-end into SARIF `physicalLocation`; the SARIF normalizer contract
  (repo-relative URIs, no angle brackets, related-locations carry
  physicalLocation, fallback anchor) is locked by regression tests.

### C5 outcome — typed native bridge landed

Callers build a dictionary-interned `TypedFactSet` (`crates/logic/src/facts.rs`)
and drive the native physical engine directly. The columnar `RelationStore` keys
and indexes on interned `TermId`s; derived rows and provenance remain typed
through the public fold. The former string adapter and external runtime dependency
have been removed.

### Deferred (with reason)

- **C7 (remove text-exchange FFI)** — the foundation (`PyRdfDataset`, an
  `Arc<RdfDataset>` Python handle) shipped in Task 8, but full removal is blocked
  by the **rdflib-based Python validation/DSL pipeline**: callers hold rdflib
  graphs, so passing the IR handle still requires an rdflib→N-Triples→IR
  conversion — the seam moves, it is not removed. True removal needs the Python
  pipeline (audit/acceptance/dsl_validate/sparql) to go rdflib-free over the IR, a
  separate effort.
- **C5 typed-bridge implementation** — LANDED (typed fact set + typed chase
  surface + interned columnar store; see above). Only the adapter-internal
  programmatic `add_fact` swap remains open.
- **Sub-file SARIF `region` line/col** — gated on oxigraph 0.5's parser, which
  exposes source positions only on the *error* path, not for successfully parsed
  triples. The `RdfLocation`/builder/renderer plumbing is ready; a positional
  parser is the unblock.
- **Annotation threading — DATA path done, LINT-consumer is the follow-up.**
  RDF 1.2 statement annotations already flow through the whole pipeline at the
  data level: the IR carries them (`reifiers`/`annotations` tables, with
  multi-reifier now *preserved*), `gts_write` writes them into the GTS bundle's
  `annot` frames, and the reader folds them back. The IR exposes the efficient
  read surface — `reifiers_of` (linear; small table) and `annotations_of`
  (`O(log n)`, contiguous run) — tested and ready. The one un-done piece is the
  validate **lints auto-consuming** those accessors to enrich diagnostics; the
  lints run on `oxigraph::Store` today, so wiring them to read the IR accessors
  is C4-class work (port the lints to the IR), tracked as a follow-up. This is a
  gated consumer, not a data-flow gap.
- **C8 (delete the owned store shim)** — the production goal is **met**: the
  IR is the sole production working store. A final cleanup removed
  the legacy trait, backend adapters, compat bridge, and owned fixture store from both
  production and tests.

### CodeRabbit review — fixes applied + remaining deferrals

Correctness fixes landed in this review pass (all gated):

- **BlankScope preserved across the owned/oxigraph boundaries.** The former compat bridge
  and the SHACL IR→oxigraph conversion (`shacl/src/data.rs`) now
  scope-qualify a non-default blank label via the shared `BlankScope::qualify_label`
  helper (`ir/term.rs`): default scope keeps the bare label (byte-unchanged for real
  single-scope data), a non-default scope `n` renders `"{label}.s{n}"`, so two
  same-label blanks from different scopes no longer conflate.
- **Directional literals round-trip through canonical Turtle.** `turtle_normalize.rs`
  threads `direction` into `literal()` and emits the RDF 1.2 `"text"@lang--ltr`/`--rtl`
  syntax; oxigraph 0.5's Turtle parser round-trips it, so the isomorphism gate holds
  (regression test added).
- **Hard-fail on conflicting language tags.** `validate/src/language_tags.rs` now
  returns an `Err` when two `gmeow:Language` individuals map one internal tag to
  different `bcp47Tag`s (identical duplicates remain harmless); the 206-entry parity
  output is unchanged.
- **Crossref malformed-date is a validation error, not a panic.** `crossref.rs`
  validates `release_date`'s `YYYY-MM-DD` shape once in `build_deposit_xml` (which
  already returns `Result`) and `build_date` is now total; valid-input output is
  byte-identical (parity preserved).
- **Move optimization completed + stale docstring fixed.** `ir/import_sink.rs` passes
  the just-`remove()`d `Term` BY VALUE into `intern_raw_term`/`literal_from_term` and
  MOVES its owned strings; the `literal_from_term` docstring now states that GTS
  *does* carry base direction (gmeow-gts 0.9.4 `Term.direction`).
- **Base direction requires a language tag.** `parse_gts_direction` (shared by all
  three decode paths) now takes the language and hard-fails
  (`gts-direction-without-language`) on a direction without a non-empty language,
  per RDF 1.2.
- **Subject-kind check tightened correctly.** `ir/validate.rs` splits the old
  "non-literal subject" check: an ASSERTED subject (quad subject / reifier resource /
  annotation reifier) must be IRI/blank — a quoted-triple subject there is rejected
  (`rdf-ir-triple-subject`) before it can reach an `unreachable!` in the owned /
  oxigraph conversions — while the subject position *inside* a quoted triple still
  legitimately accepts a NESTED triple term (`<< <<s p o>> p2 o2 >>`), which the
  oxigraph materializer already turns into a totally-handled `Err` rather than a panic.
- **Citation `gmeow:Selector` definition** de-garbled to one clear sentence.

Remaining deferrals (genuine heavy lifts, with reason):

- **Blob *reference* carry-forward on GTS export (`gts_write.rs::apply_lookaside`)** —
  blocked UPSTREAM: `gmeow_gts::model::BlobEntry` is byte-bearing by construction
  (`Bytes(..)`/`Lazy { raw, .. }`) and has **no reference-only variant** that could
  carry just the `RdfBlobRecord` digest + origin. We cannot emit a byte-less blob
  reference into `graph.blobs` without materializing payload bytes (which the IR
  deliberately never holds — blobs may be multi-terabyte). The reference therefore
  round-trips out of band (the `blob-bytes-absent` intentional-loss entry); carrying
  it inline is unblocked only by a new gmeow-gts reference-only blob-entry API.
- **Reifier/annotation tables in SHACL validation** — the direct-dataset
  gap was closed: `validate_dataset` now projects reifiers as `rdf:reifies` quads and annotations
  as plain triples before the Core/SPARQL engines read the data graph.
- **`datasets_isomorphic` completeness (`ir/compare.rs`)** — the hash-refinement
  oracle is sound on the POSITIVE side and conservative on collision: when two blanks
  share a refined signature (a symmetric blank graph the hash cannot split) it returns
  `false` rather than guessing, so it can FALSE-NEGATIVE on a genuinely-isomorphic
  symmetric graph. Real importer-equivalence graphs (OWL restrictions, RDF lists)
  carry distinguishing IRIs in their closure and do not collide, so no current gate
  false-negatives; a complete RDFC-1.0 backtracking pass is the deferred follow-up for
  pathological symmetric inputs.

SHACL-SPARQL store caching (CodeRabbit "re-materializes per call") WAS fixed in this
pass, not deferred: `CachedIrDataGraph` (`shacl/src/data.rs`) wraps `&RdfDataset` with
a `OnceLock<Store>` so `validate_dataset` materializes the SPARQL store at most once
per validation, shared across every `sh:sparql` target/constraint.

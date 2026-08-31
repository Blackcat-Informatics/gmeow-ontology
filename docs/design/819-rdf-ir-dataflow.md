<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# RDF dataset IR and carrier dataflow

This document records the implemented RDF dataflow contract. It describes
current ownership and invariants; it is not a delivery ledger or a list of
future implementation steps. The words **MUST**, **MUST NOT**, **SHOULD**, and
**MAY** are used as defined by RFC 2119.

## Ownership boundary

[`purrdf`](https://crates.io/crates/purrdf) owns the semantic RDF 1.2 dataset
model, parsing, serialization, RDFC-1.0 canonicalization, query interfaces, and
GTS transport primitives. GMEOW owns the repository-specific authoring profile,
pipeline policy, logical interpretation, and generated products:

- `purrdf::RdfDataset` is the frozen RDF dataset IR consumed throughout the
  Rust workspace.
- `purrdf::RdfDatasetBuilder` is the fallible construction boundary.
- `purrdf::RdfLookaside` carries transport and evidence material that is not an
  RDF quad.
- `crates/gts-profile` is the sole production door from a GMEOW dataset or
  stream to authored GTS bytes.
- `crates/pipeline` owns the repository DAG and the cumulative carrier that
  flows through it.

No GMEOW-local parallel RDF model is authoritative. A compatibility backend may
adapt the frozen dataset for a selected operation, but it MUST NOT redefine RDF
term identity or become a second canonical carrier.

## Canonical dataflow

```text
canonical authored sources
          │
          ▼
  RdfDatasetBuilder ── parse/build diagnostics
          │
          ▼
     RdfDataset + RdfLookaside
          │
          ▼
 cumulative pipeline carrier ── logic, validation, projection, metadata
          │
          ▼
       gmeow.gts
          │
          └── deterministic registered fanout to generated products
```

Canonical sources remain the only authoring surface under Principle 4. The
pipeline holds cumulative RDF state in memory and emits one `gmeow.gts` terminal
before registered fanout stages project the requested flat products. The full
producer and fanout contract is specified in
[`docs/PIPELINE_SPINE.md`](../PIPELINE_SPINE.md); lock, cache, and gate ownership
is specified in [`docs/GATE-AND-PIPELINE.md`](../GATE-AND-PIPELINE.md).

## Dataset invariants

The following properties are load-bearing:

- A term identifier is local to one frozen `RdfDataset`. It is neither stable
  across datasets nor a persistent external identity.
- Cross-dataset semantic equality uses purrdf's full RDFC-1.0 canonical form;
  local term ids, blank labels, and iteration order are never equality or digest
  substitutes.
- GTS identifiers are transport-local. Import resolves them to RDF values and
  does not expose them as dataset identity.
- Blank-node scope participates in identity. Independently parsed sources do
  not acquire accidental blank-node equality when combined.
- RDF 1.2 triple terms are structural terms. Reifier resources remain distinct,
  so multiple resources may reify the same triple term without collapsing.
- Literal lexical form, datatype, language, and base direction are preserved by
  the semantic IR. A compatibility backend may not silently erase one of those
  components.
- Quads have set semantics. Deterministic ordering is a serialization and
  product requirement, not an RDF semantic distinction.
- Invalid term references, illegal positional terms, malformed encodings, and
  structurally invalid triple terms fail at an ingestion or validation boundary
  before downstream consumers observe a partial dataset.

Consumers that need identifiers use dataset-local handles. Consumers that need
lexical values use borrowed resolved views. Neither path may treat a serialized
GTS integer as cross-segment RDF identity.

## Dataset and lookaside fidelity

The hot RDF graph and the transport envelope are related but not interchangeable:

```text
carrier
├── dataset:   terms, quads, graph names, triple terms, reifiers
└── lookaside: source locations, diagnostics, blobs, signatures, suppressions,
               segment evidence, and other transport metadata
```

An operation that promises dataset fidelity MUST preserve every RDF component.
An operation that promises bundle fidelity MUST additionally preserve the
selected lookaside and envelope material. Dropping unsupported material is a
hard error unless the operation explicitly selects a documented lossy
projection whose loss record is part of its result. A cache miss recomputes an
authorized producer action; it never licenses a weaker representation.

Large payload bytes are content-addressed and referenced from the envelope
rather than forced into the quad hot path. The exact payload identity and its
binding metadata are part of the action result, so a consumer cannot substitute
ambient bytes. Pipeline and cache commitments require the content store to be
closed over those references: an orphan payload, a missing payload, a malformed
digest, or a declared-length mismatch is a hard error.

## GTS authorship profile

Every payload-bearing frame authored by production GMEOW code uses
`zstd-rsyncable` at compression level 12. `crates/gts-profile` centralizes this
policy through:

- `emit_gmeow_gts` and `emit_gmeow_gts_with_medium` for snapshot bundles;
- `dataset_to_gmeow_gts` for the frozen-dataset conversion exit;
- `GmeowGtsWriter` for append-only segments; and
- `compact_gmeow_gts` for streamable repacking.

Each producer audits its bytes with `validate_mandated_frames`. Repository-static
seals prevent production code from bypassing this profile through lower-level
`purrdf` writers or byte-returning composition APIs. Header-only and
payload-free transport metadata are not compression frames.

## Pipeline and cache determinism

A pipeline action key covers all semantic inputs and selected policy: source
identity, dependency receipts, implementation identity, output profile, medium
plan, and other behavior-affecting configuration. The same key MUST produce the
same bytes and evidence. A policy change therefore changes the key rather than
mutating the meaning of a cached result.

`make check-sync` is the single repository producer. Its update mode writes only
byte-changed outputs; its check mode proves the fixed point without writing.
Independent DAG nodes may execute concurrently, but publication occurs only
after their declared inputs and output identities have been verified. Selected
outputs and capabilities are mandatory under the no-optionality doctrine.

## Native consumers

- `crates/logic` and `crates/logic-compile` consume typed RDF values and compiler
  IR. Executable rule text is not a production reasoning interchange.
- `crates/validate/src/store.rs` constructs native datasets with explicit
  per-source blank scopes. Structural validation and repository-static policy
  operate on authenticated inputs.
- `crates/pipeline/src/ingest.rs` enables purrdf source-span tracking at the
  ingestion boundary and carries the resulting line, column, and byte offset in
  the authenticated span index used by diagnostic projections.
- SHACL Core and native query paths consume dataset views. A selected SPARQL
  operation may use its explicit query backend; backend materialization is a
  compatibility boundary, not canonical storage.
- `crates/errors` owns structured diagnostics and deterministic renderers.
  `crates/validate/src/findings.rs` and `report_bridge.rs` preserve diagnostic
  identity and source locations, while
  `crates/pipeline/src/stages/diag_render.rs` emits requested diagnostic products.

Diagnostics stay structured until the rendering boundary. Text, JSON, HTML,
SARIF, and RDF renderings are projections of the same diagnostic records, so a
renderer cannot invent or discard semantic severity, locations, causes, or
stable rule identity.

## Streaming and bounded memory

Streaming is an explicit producer or transport capability, not a claim that
every consumer is incremental. `GmeowGtsWriter` owns append-only segment
authorship; the pipeline medium implementation under `crates/pipeline/src/medium`
owns action-store and envelope policy. A consumer that requires a complete
closure or global index declares that requirement and fails if the selected
profile cannot supply it.

Bounded-memory changes must preserve deterministic bytes, evidence, and failure
semantics. They are measured against the actual workload under
[`docs/RUST-OPTIMIZATION.md`](../RUST-OPTIMIZATION.md); compiler-flag churn or a
silent lower-fidelity fallback is not an optimization.

## Validation ownership

Tests consume an already-produced authenticated corpus selected by an exact
fixture-manifest digest. They MUST NOT build, regenerate, or repair corpus data.
Missing, stale, corrupt, or mismatched products fail closed. Corpus construction
belongs to the explicit producer stage before the test process begins.

The relevant repository proofs are reached through Make targets:

- `make check-sync` verifies deterministic registered generation;
- `make validate` checks RDF structure and authored annotations;
- `make reason-verify` checks native reasoning and reasoned-graph negatives;
- `make rust-test` exercises the authenticated Rust inventory; and
- `make check` runs the complete local gate DAG once over the synchronized fixed
  point.

These gates enforce the live architecture. Historic branch names, delivery
sequences, reviewer transcripts, and tracker references are intentionally not
part of this design contract.

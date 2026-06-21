<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# RFC: `gmeow-rdf` as the optimal IR & dataflow stream

Tracking epic: [#819](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/819).
Parent: [#672](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/672)
(META-EPIC — the reference RDF-1.2 stack). Builds on
[#630](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/630) (Rust
reasoning authority).

This is a design RFC. No engine code lands with this document; the architecture
is realized through the staged children C1–C7 below.

## Context

The `630` work finishes moving the logic compiler, reasoning, SHACL, and validation
paths off Python/rdflib into Rust (five PyO3 extensions fused into one `gmeow_native`
cdylib; Python logic duplicates deleted). The declared end-state — per the north-star
goals (rust-first, maximal information flow, overcome-not-inherit constraints,
SOTA/greenfield) and the logic-stack-retirement doctrine ("Rust is the authority") — is
that **all** Python is eventually replaced by Rust.

Designing for that end-state changes what "optimal" means. Today three RDF
representations fight inside the Rust code, and the Python boundary forces text
round-trips.

1. **`RdfQuad` / `RdfStore` (kernel model, `crates/rdf/src/model.rs`, `store.rs`)** —
   owned-`String` terms, **no interning**. `RdfStore::quads()` returns
   `Box<dyn Iterator<Item = Result<RdfQuad, _>>>` that **clones every quad per
   iteration** (`store.rs:88,92,95`) and clones the whole lookaside each call
   (`store.rs:104`). It is a lowest-common-denominator *conversion shim*, not a working
   IR.
2. **oxigraph `Store` (the de-facto working IR)** — `logic`, `shacl`, `validate`, and
   `slicetest` all materialize into oxigraph and SPARQL-query it. `RdfQuad` exists only
   to feed `store_from_rdf_store` (`crates/rdf/src/oxigraph.rs:67`).
3. **GTS folded `Graph` (the wire form) — already the ideal shape**: an interned term
   table `terms: Vec<Term>` plus id-tuple `quads` and id `reifiers`
   (`gmeow-gts model.rs:222`). `gmeow-rdf` **discards that interning**, reconstructing
   owned `RdfTerm`s on every iteration (`crates/rdf/src/gts.rs`, `term_from_id`).

Every stage pays a conversion tax: GTS → `RdfQuad` (clone) → oxigraph (clone again). The
Python FFI adds text round-trips — N-Triples/N-Quads serialization for logic
materialize, RL closure, and the test SHACL path (`native_rl.py`,
`native_rl_rdflib.py:47`, `logic_runner.py:1210`, `tests/_graph_nt.py:49`) — with the
zero-copy capsule-pointer (`py_store.rs:835`) used *only* for production SHACL. Logic
adds a further internal text round-trip: quads → Nemo ternary fact **strings** →
ChaseRow **strings** → decoded terms.

**Intended outcome:** `gmeow-rdf` exposes a single interned, id-addressed graph IR with
zero-copy/streaming iteration, through which every Rust consumer and (transitionally)
the FFI flows — collapsing the GTS↔`RdfQuad`↔oxigraph triple-conversion and, once
Python is gone, the text FFI, into one substrate.

## Target architecture

### 1. The IR — an interned, id-addressed columnar graph (`RdfGraph`)

Promote the already-interned shape (GTS `Graph`) into the canonical in-memory IR of
`gmeow-rdf`, owned by the kernel rather than borrowed from `gmeow-gts`:

- **Term table** interned once: `terms: Vec<Term>` with `Arc<str>`-backed values and a
  `HashMap<Term, TermId>` for dedup. Equality and joins become integer ops; duplicate
  IRIs collapse to one allocation.
- **Quads as id-tuples**: `Vec<QuadIds { s, p, o, g: Option<TermId> }>`.
- **RDF-1.2 first-class**: reifiers, annotations, and quoted-triple terms carried as
  id-tuples (`reifiers: Vec<(TermId, Triple3Ids)>`), never re-serialized — preserving
  the RDF-1.2-first invariant the kernel already guarantees.
- **Lookaside** travels by reference, not cloned per access (fixes `store.rs:104`).
- The GTS folded `Graph` maps 1:1, so GTS load/save is a near-zero-cost re-id, not a
  term-by-term rebuild.

`RdfGraph` becomes THE thing built once and shared by reference across stages.

### 2. Zero-copy / borrowed iteration surface

Replace the clone-per-item trait surface with id-level plus borrowed access:

- `quad_ids(&self) -> &[QuadIds]` and `term(&self, TermId) -> &Term` — integer-level
  walking, no allocation.
- `QuadRef<'a>` — a borrowed view (`&Term` subject/predicate/object) for ergonomic
  iteration that never owns `String`s.
- Keep an owned-`RdfQuad` adapter **only** at true external boundaries (for example,
  emitting Turtle), explicitly opt-in. The current
  `Box<dyn Iterator<Item = Result<RdfQuad, _>>>` (`store.rs:40`) is retired, not kept
  as a fallback.

### 3. id-graph displaces oxigraph on hot paths

Add native indexes over `TermId`s on `RdfGraph` (SPO / POS / OSP, a type/class index, a
subclass-closure cache) and serve the hot consumer access patterns directly:

- **logic EDB**: encode the chase input from `TermId`s, and — key win — **kill the Nemo
  fact-string round-trip** by feeding the engine integer-encoded facts and decoding
  results back to `TermId`s (`crates/logic/src/store.rs`, `reason/mod.rs` decode path).
- **SHACL target resolution**: `instances_of_class` / `subclass_closure`
  (`crates/shacl/src/engine.rs:64-110`) read the native index instead of SPARQL.
- **validate lints**: `sameAs` scan and structural lints walk the id-graph
  (`crates/validate/src/store.rs`).
- **oxigraph relegated** to a lazily-built SPARQL backend, constructed from the id-graph
  **only** where arbitrary SPARQL/CONSTRUCT is genuinely required (RDFS closure rules,
  free-form SELECT in slicetest/validate). It stops being the default working set.

### 4. Streaming dataflow contract (the "dataflow stream")

Define a push contract in `gmeow-rdf` so stages compose without materializing whole
graphs:

- `trait QuadSink { fn push(&mut self, q: QuadRef<'_>) -> Result<…>; … }` and a dual
  `QuadSource`. The materialized `RdfGraph` is one sink/source implementation; chase
  output, SHACL results, and projections emit through the same sink.
- Enables large-graph streaming and stage fusion (parse → reason → validate without an
  intermediate owned graph), which is the literal "IR + dataflow stream" the architecture
  targets.

### 5. The boundary in a Python-free world

Because the directive is to replace **all** Python with Rust, the FFI text round-trips
disappear *by construction* rather than via a separate optimization:

- Rust↔Rust stage calls pass `&RdfGraph` directly. The N-Triples/N-Quads serialization
  seams (`native_rl_rdflib.py:47`, `logic_runner.py:1210`, `tests/_graph_nt.py:49`) and
  the unsafe capsule-pointer hack (`py_store.rs:835`) are both **transition
  scaffolding**, retired as their Python callers are ported.
- The only remaining serialization boundaries are *process IO*: read GTS bytes, emit
  GTS/Turtle/diagnostics. GTS is the on-disk/wire form; `RdfGraph` is the in-memory
  form; same shape ⇒ cheap. PyO3 surfaces (`PyStore`, `parse`, capsule) are explicitly
  throwaway adapters.

**Priority axis.** In the all-Rust end-state the ideal first axis is **intra-Rust
clone/conversion elimination via the interned id-graph + borrowed iteration (§1–2)**,
immediately followed by the **streaming contract (§4)**. FFI text elimination (§5) is
achieved as a *consequence* of the cutover, not as a standalone effort.

## Epic decomposition

| Child | Scope |
| ----- | ----- |
| **C1** | Land `RdfGraph` interned IR (own the GTS-shaped graph in the kernel) + `QuadRef` borrowed view + `TermId` indexes; add id-level methods to the store surface. |
| **C2** | Borrowed/zero-copy iteration: migrate `VecRdfStore`/oxigraph/gts adapters to serve borrowed views; delete the clone-per-iteration path. |
| **C3** | Native indexes + SHACL target resolution off the id-graph (displace SPARQL on target resolution). |
| **C4** | Logic EDB from `TermId`s; eliminate the Nemo fact-string round-trip. |
| **C5** | `QuadSink`/`QuadSource` streaming contract; route chase/projection/diagnostics emit through it. |
| **C6** | Collapse the FFI: as Python callers are ported, replace N-Triples/N-Quads + capsule with `&RdfGraph`; retire `native_rl_rdflib`, the text SHACL path. |
| **C7** | oxigraph as a lazy SPARQL-only backend built from the id-graph. |

Ordering: C1 → C2 are foundational; C3/C4 are parallel hot-path wins; C5 enables
streaming; C6/C7 are end-state cleanups gated on the Python cutover progressing.

## Critical files / anchors

- Kernel IR & surface: `crates/rdf/src/model.rs`, `store.rs`, `oxigraph.rs`, `gts.rs`,
  `lookaside.rs`, `py_store.rs`, `lib.rs`.
- Interned wire shape to mirror: `gmeow-gts` `model.rs` (`Graph`/`Term`, ~lines 50, 222).
- Consumers to migrate: `crates/logic/src/store.rs` and `reason/mod.rs`;
  `crates/shacl/src/engine.rs` (`validate`, `validate_rdf_store`, `parse_shapes`, target
  helpers `:64-110`); `crates/validate/src/store.rs` and `validate_all.rs`;
  `crates/slicetest/src/stores.rs`.
- FFI scaffolding to retire: `src/gmeow_tools/native_rl.py`, `native_rl_rdflib.py`,
  `logic_runner.py`, `shacl_engine.py`, `tests/_graph_nt.py`.

## Verification

Every child PR must demonstrate behavior-preserving speedups against fixed gates:

1. **Baseline benchmark harness** (criterion) measuring quads-iterated/sec and
   allocations for the GTS→logic, GTS→shacl, and GTS→validate paths on the current
   branch — quantifying the clone tax the IR removes.
2. **Parity gates that stay green** through every child PR: `make check`, `make test`,
   and the logic **derivation-graph goldens** (content-addressed explanations) — the IR
   change must be behavior-preserving, only faster.
3. **Proptest extension**: extend `crates/rdf/tests/proptest_roundtrip.rs` to
   `RdfGraph` ↔ GTS ↔ oxigraph, asserting term-identity preservation and RDF-1.2
   (`<<>>`) fidelity. Use the pyoxigraph star comparator already used in the repo —
   `rdf_compare` cannot parse RDF-1.2 stars.
4. **Acceptance**: each child PR reports a measured allocation/throughput delta on its
   path with no golden or gate regression.

## Doctrines honored

Greenfield / no-backcompat (replace the clone shim; do not keep it as a fallback);
no-optionality / hard-fail (no degraded paths); one-PR-at-a-time (the epic is
decomposed; implementation lands slice by slice).

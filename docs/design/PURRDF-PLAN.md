<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# PURRDF-PLAN — a high-performance, RDF-1.2-first drop-in replacement for rdflib

> **Product name: `purrdf`** (the public PyPI distribution) — built from the AGPL `gmeow-rdf` crate +
> the permissive `gmeow-gts` I/O layer. "purr" (runs smooth/fast) · the Blackcat/GMEOW house · "pure RDF".
>
> **Status:** design / EPIC source-of-truth. This document is the canonical plan the purrdf EPIC points
> to. No implementation begins until the EPIC's child issues are approved and scheduled.

## Context

Can `gmeow-rdf` (the `crates/rdf/` kernel, surfaced to Python as `gmeow_rdf` inside the unified
`gmeow_native` cdylib) be published as a high-performance, RDF-1.2-first **drop-in replacement for
Python's `rdflib`** — and, beyond Python, the dominant RDF library in every major ecosystem?

This is consistent with `.goals` ("SUBSUME, EXTEND, ENHANCE"; "gmeow = the goto for KG/AI for RAW
POWER, EXPRESSIVE DEPTH, REASONING") and with native drop-ins already in-repo: the Rust RL engine
`gmeow_logic.rl_closure_nt` (replaces `owlrl`), `rdf_canonical.py` (replaces `rdflib.compare.isomorphic`),
`sparql.py` (replaces rdflib's
SPARQL engine, ~12× faster). Yet `pyproject.toml` still hard-depends on `rdflib>=7.6` and ~15 internal
modules import it — gmeow does not yet eat its own dog food here.

### Decisions (locked)

- **Compatibility model = Hybrid.** Keep the greenfield native API; ALSO ship a thin Python
  `gmeow_rdf.compat.rdflib` shim so `import rdflib` works for most code. The greenfield core stays clean;
  the compat layer absorbs rdflib's legacy warts at the edge.
- **Ecosystem reach = Full public.** Target credibility as a real PyPI rdflib replacement for arbitrary
  third-party code (implies rdflib plugin entry-point compat + near-total format/SPARQL/term parity).
- **oxigraph is ring-fenced behind traits as a single required backend** (NOT a degraded optional — see
  the no-optionality reconciliation under DIP).
- **Syntax/semantics split across two repos:** `gmeow-gts` (Apache/MIT) owns format I/O + container;
  `gmeow-rdf` (AGPL) owns the semantic IR, value space, canonicalization, SHACL/logic, SPARQL bridge.
- **GTS is lossless** — loss lives only at exit-gate projections, never in the container.
- **Multi-ecosystem reach builds on `libgts`'s existing C-ABI + engines**, not a new universal waist.
- **v1 scope = parcels P0–P9 inclusive** (P8 `rdf-capi` + P9 term-facade are IN). The rdflib plugin
  category-mapping is a deferred follow-up.

## Verdict

**Practically possible. A program of many independent PRs, not a single change.** The Rust core is the
right foundation (RDF-1.2-native, value-interned, zero-alloc, oxigraph-backed SPARQL). The work is
overwhelmingly at the **Python compatibility boundary**, not in Rust. Ranked by relative difficulty
(risk ordering, not duration):

1. **HARDEST — term semantics.** rdflib's `URIRef`/`Literal`/`BNode` are `str` subclasses; real code
   does `str(uri)`, slicing, `Literal(5).toPython()==5`, datatype-typed comparison/arithmetic. The
   compat layer must present `str`-subclass terms with `toPython()` and the XSD↔Python type map.
2. **HARD — the `Graph` facade.** A *mutable* `Graph` with `triples((s,p,o))` wildcard matching,
   `add/remove/set/value`, the accessor family, graph algebra (`+ - * ^`), transitive closures, RDF-list
   `Collection`. The native surface is an immutable IR + an oxigraph store.
3. **HARD — plugin ecosystem.** "Full public" means `rdflib.plugin` entry-points so arbitrary third-party
   code resolves our parsers/serializers/stores/SPARQL-functions.
4. **MODERATE — format breadth.** Net-new: RDF/XML, full JSON-LD; rounding out Turtle/N-Triples.
5. **MODERATE — SPARQL completeness** (UPDATE, `Result` serialization, custom functions) — carved out;
   served by oxigraph behind the seam for now.
6. **EASIER — namespaces & persistence** (pure-Python; GTS covers persistence differently/better).

## Current state (what exists — do NOT rebuild)

- **Rust kernel** `crates/rdf/src/model.rs`, `ir/` — RDF-1.2-native terms (triple terms, directional
  literals `RdfTextDirection`, reifiers, annotations), frozen interned `RdfDataset`,
  structural isomorphism `ir/compare.rs`, loss ledger `loss.rs`.
- **oxigraph adapter** `crates/rdf/src/oxigraph.rs` — materialize to oxigraph `Store`; full SPARQL.
- **Python surface** `crates/rdf/src/py_store.rs` + stub `crates/native/python/gmeow_rdf/__init__.pyi`:
  pyoxigraph-shaped `NamedNode/BlankNode/Literal/Triple/Quad/Variable`, `Store`, `Dataset`,
  `QuerySolutions/QueryTriples/QueryBoolean`, `parse()`/`serialize()`, `canonicalize_turtle`.
- **Existing rdflib drop-ins:** `src/gmeow_tools/sparql.py` (the `owlrl` replacement
  lives in the Rust RL engine `gmeow_logic.rl_closure_nt`; its former rdflib-graph adapter
  `native_rl_rdflib.py` has been retired — its last reasoning consumer moved to the native
  `crates/logic/tests/ontology_entailments.rs` harness).
- **gmeow-gts (separate Apache/MIT repo, crates.io 0.9.4):** six conformance-gated engines (Rust,
  Python, Go, TypeScript, Smalltalk/Pharo, Kotlin/JVM), a Rust-backed C-ABI `libgts` with C/C++/.NET/
  PHP/Lua/Swift/Ruby/R/Julia wrappers, full RDF-1.2 losslessness, and formats
  N-Quads / TriG / JSON-LD-star / YAML-LD-star + relational exports (SQLite/DuckDB/Parquet). It is
  **not** a query engine/reasoner/store by charter.

## Architecture

### Ring-fence oxigraph — by CRATE boundary, not directory convention

A **crate boundary is stronger than a `mod` convention** (it makes leaking an oxigraph type a
compile error, not a lint). Split the workspace:

```text
gmeow-rdf-core       IR, values, diagnostics, the DatasetView interfaces — NO backend, never names oxigraph
gmeow-rdf-oxigraph   the oxigraph parser/query/store adapter — the ONLY crate that names oxigraph
gmeow-rdf-events     the permissive ingestion protocol (zero deps; see the seam section)
purrdf-python        standalone PyO3 extension + the gmeow_rdf.compat.rdflib package
rdf-capi             LATER — only after the API stabilizes
```

The crate split dissolves today's feature leaks (the `gts`/`python` features transitively pulling
oxigraph; `py_store`/`py_gts` importing oxigraph types): `gmeow-rdf-core` simply has no oxigraph in its
dependency graph, full stop.

**Reconcile with the retired owned-quad abstraction — do NOT add a parallel trait family.** The
legacy boxed owned-quad iterator has now been replaced under a migration plan, not left beside the
IR. Define exactly two layers:

- **`DatasetView`** — a static, **allocation-free** read interface for Rust internals (borrowed
  `TermRef`/`QuadRef`, `quads_for_pattern`); the hot path. This is the replacement for the retired
  owned-quad reader.
- **An object-safe ERASED adapter** (`&mut dyn …`) for runtime backends + the parser/format registry.

Migration: port the old consumers (compat bridge, SHACL/validate/logic) to the concrete IR and
delete the legacy reader. The write side (`DatasetMut`) + `RdfParserBackend`/`SparqlEngine`/
`RdfSerializer` + `TermFactory` layer on top.

> **P2c migration reality.** `DatasetView` is NOT a drop-in
> superset of the retired reader: it is **quad-only** (no `reifiers()`/`annotations()`/`lookaside()`),
> **`RdfDataset`-only**, and **not object-safe** — while production consumers were previously fed
> adapter-backed stores, not an `RdfDataset`. So the migration was "**port consumers onto the concrete IR**
> (`&RdfDataset`, reading the RDF 1.2 statement layer via the IR's inherent id-based accessors), NOT
> extend the `DatasetView` trait." It is split:
>
> - **P2c part 1 (landed):** added the borrowed `RdfDataset::reifier_refs()`/`annotation_refs()`
>   read surface; ported the two genuinely IR-native seams — `gts_write::to_writer`/`to_gts` (now
>   `&RdfDataset` + `&RdfLookaside`) and the SHACL SPARQL materialization (`store_from_dataset`) — off
>   the owned-quad bridge.
> - **P2c part 2 (landed):** route production GTS/oxigraph ingress through the IR
>   (`import_gts_graph`/`import_gts_events`), migrate the remaining adapter-sourced consumers
>   (reasoning EL/DL/RL, metadata folds, `build_lpg`, `asserted_turtle`, SHACL, world loading,
>   `logic::verify`), swap materialization callers to `store_from_dataset`, then delete the legacy
>   trait, its backend impls, the compat bridge, and the owned fixture store.

**Graph position needs an explicit match type** — `Option<TermId>` cannot distinguish "any graph" from
"the default graph". Storage/quads keep `Option<TermId> g` (`None` = default graph), but *matching* uses:

```rust
enum GraphMatch { Any, Default, Named(TermId) }
```

**Specify BEFORE implementing P2** (the backend contract, not afterthoughts): query-result ownership +
cursor lifetime; SPARQL UPDATE + transaction semantics; cancellation; the backend error model;
capability negotiation (extend `RdfStoreCapabilities`); thread-safety per handle; and whether backend
selection is **compile-time** (features) or **runtime** (registry) — that answer decides whether the
erased layer is mandatory.

**No-optionality reconciliation:** the ring-fence is **DIP code isolation**; "builds without the
oxigraph crate" is a **CI hygiene check** (`gmeow-rdf-core` compiles + core tests pass with no oxigraph
in the graph) — NOT a shipped degraded mode. Exactly ONE backend is wired and **required**; missing it
is a HARD FAIL. oxigraph is **removed at parity**, not left as a parallel optional.

### Borrowed-conversion bridge to oxigraph (zero *intermediate* allocation — not end-to-end zero-copy)

The IR resolves `TermId -> TermRef<'_>` (borrows `&str` from the intern table) and oxigraph exposes
borrowed `QuadRef<'a>`/`*Ref`; the bridge maps IR refs → oxigraph `*Ref` over the **same backing
bytes**, so **no intermediate owned `RdfQuad`/`oxrdf::Quad` is allocated** on ingress. That replaces the
owned `rdf_quad_from_oxigraph`/`oxigraph_quad_from_rdf` conversions and kills the
`GTS → RdfQuad (clone) → oxigraph (clone again)` double-tax.

**It is NOT an end-to-end zero-copy store path:** oxigraph still copies/encodes terms into its own
internal indexes on `insert`, and scoped blank-node qualification (`BlankScope::qualify_label`) may
construct a new identifier string. Call it a **borrowed-conversion / zero-intermediate-allocation
bridge**, not a zero-copy store.

### Cross-repo split: gts owns syntax, gmeow-rdf owns semantics

- **gmeow-gts (Apache/MIT)** — a standalone permissive multi-format RDF I/O + container library: all
  format codecs (bytes ↔ event stream) + CBOR container + verify. Net-new vs. today: **Turtle / N-Triples
  / RDF-XML** (N-Quads, TriG-write, JSON-LD-star/YAML-LD-star already ship). Permissive parsers adopt
  where AGPL can't — directly serving the ecosystem-reach goal.
- **gmeow-rdf (AGPL)** — semantics: events → interned IR, the XSD value space, RDFC-1.0
  canonicalization, RDF-1.2 reification meaning, SHACL/logic, the SPARQL bridge.
- **Connecting seam = the `gmeow-rdf-events` ingestion protocol** — a tiny standalone permissive crate
  (MIT/Apache, zero deps), NOT coupled into the container product. gts, gmeow-rdf, and every parser
  depend on it. It is a *richer, fallible ingestion protocol* (scopes, errors, cancellation, parser
  hints, batching, forward references) — **distinct** from the existing infallible frozen-dataset output
  visitor (which is renamed `RdfDatasetVisitor`). Every parser emits these events; `RdfDatasetBuilder`
  consumes them, so a `parse-Turtle → serialize-NQ` path is pure-gts with no AGPL code. (Full protocol
  below.)
- **Doctrine refinement:** the narrow-waist "one producer, no re-parse" rule governs **export**, not
  **ingress** — parsers-as-ingress in gts do not violate it.

### GTS is lossless — *RDF-1.2 semantic* losslessness (precise)

"Lossless" means the **RDF-1.2 abstract graph** is preserved with full fidelity — terms, quads,
reifiers, annotations, base direction, datatypes, language tags. It does **NOT** promise concrete-syntax
round-trip fidelity: prefix bindings, `@base`, and lexical formatting (whitespace, literal spelling,
numeric formatting) are **droppable syntactic hints**, not semantic content — dropping them is not a
loss. *Semantic* loss occurs only at exit-gate projections to genuinely-lossy external formats
(Turtle-1.1 without triple terms, sheet music, CITATION.cff, …) and is ledgered there, never in the
container. Per the loss matrix the only remaining entries are `blob-bytes-absent` (recoverable
by-reference) and `bnode-scope-flatten` (flat-`read()` only; the streaming path is lossless); the
RDF-1.2 fidelity defects `direction-dropped`/`multi-reifier-collapsed` are closed
(losslessness closed in gmeow-gts).

### The ingestion event protocol (`gmeow-rdf-events`) — a protocol, not a trait rename

**Two distinct protocols; do not conflate them.** The existing `ir/event_sink.rs` `RdfEventSink`/`emit()`
is an **infallible output visitor** over an already-frozen dataset (term-before-reference by
construction, no scopes, no errors). **Rename it `RdfDatasetVisitor`** (a.k.a. `FrozenDatasetSink`). The
**ingestion** protocol is a different, richer, *fallible* thing and lives in its own **tiny permissive
crate `gmeow-rdf-events`** (MIT/Apache, zero deps) — owned by neither the container (gts) nor the engine
(gmeow-rdf); both depend on it, as does every parser.

Value types: `ScopeId`, `EventTermId`, `TextDirection`, `EventTerm<'a>` (Iri/Blank/Literal/Triple),
**`EventTriple { s, p, o }`** (a *reified statement* — a triple, NOT a quad), `EventQuad { s,p,o,g }`.

```rust
// gmeow-rdf-events (permissive, standalone): the ingestion protocol
pub struct EventTriple { pub s: EventTermId, pub p: EventTermId, pub o: EventTermId } // reified statement
pub struct EventQuad   { pub s: EventTermId, pub p: EventTermId, pub o: EventTermId, pub g: Option<EventTermId> }

// Object-SAFE (no generic methods; a CONCRETE error type) so `&mut dyn RdfEventSink` works for registries.
pub trait RdfEventSink {
    fn term      (&mut self, scope: ScopeId, id: EventTermId, t: EventTerm<'_>) -> Result<ControlFlow<()>, EventError>;
    fn quad      (&mut self, scope: ScopeId, q: EventQuad)    -> Result<ControlFlow<()>, EventError>;
    fn quads     (&mut self, scope: ScopeId, q: &[EventQuad]) -> Result<ControlFlow<()>, EventError> { /* default loop */ }
    fn reifier   (&mut self, scope: ScopeId, r: EventTermId, t: EventTriple) -> Result<ControlFlow<()>, EventError>;
    fn annotation(&mut self, scope: ScopeId, r: EventTermId, p: EventTermId, o: EventTermId) -> Result<ControlFlow<()>, EventError>;
    fn open_scope (&mut self, _: ScopeId) -> Result<ControlFlow<()>, EventError> { Ok(Continue(())) }
    fn close_scope(&mut self, _: ScopeId) -> Result<ControlFlow<()>, EventError> { Ok(Continue(())) }
    fn prefix  (&mut self, _p: &str, _iri: &str)              -> Result<ControlFlow<()>, EventError> { Ok(Continue(())) } // droppable hint
    fn base    (&mut self, _iri: &str)                        -> Result<ControlFlow<()>, EventError> { Ok(Continue(())) } // droppable hint
    fn location(&mut self, _id: EventTermId, _span: SourceSpan)-> Result<ControlFlow<()>, EventError> { Ok(Continue(())) } // droppable hint
    fn finish  (&mut self) -> Result<(), EventError>;   // resolve forward refs; error on any unresolved
}
pub trait RdfEventSource {
    fn drive<S: RdfEventSink>(self, sink: &mut S) -> Result<(), EventError> where Self: Sized; // zero-cost hot path
    fn drive_erased(&mut self, sink: &mut dyn RdfEventSink) -> Result<(), EventError>;          // for runtime registries
    fn declares_before_reference(&self) -> bool { false } // capability; bounded-memory sinks require true
}
```

**Forward references are ALLOWED (the key correction).** The current GTS event order can emit a triple
term *before* the reifier binding that resolves it, so a strict term-before-reference replacement would
need an adapter or a new versioned GTS contract. Instead the protocol **permits forward references**: a
`term`/`quad`/`reifier` MAY reference an `EventTermId` not yet declared, and the sink resolves them in
**`finish()`** — the existing two-phase `ir/import_sink.rs` resolution IS this step. A source MAY
advertise `declares_before_reference()`; bounded-memory sinks (no buffering) require it, and a
re-ordering adapter bridges the current GTS order to that guarantee.

**Specified semantics (no longer hand-waved):**

- *ID reuse:* an `EventTermId` is declared at most once per scope; redeclaration is a protocol error.
- *Scope lifecycle:* `open_scope`/`close_scope`; ids are scope-local; closing seals that scope's
  blank-node identity; referencing a closed scope's id is an error.
- *Forward references:* permitted; any id still unresolved at `finish()` is a diagnostic + error.
- *Cancellation / partial state:* `ControlFlow::Break` stops the source; the sink's accumulated state is
  **partial and MUST NOT be frozen** — no dataset is produced from a cancelled or errored run
  (no-degraded-fallback).
- *Nesting limit:* triple-term nesting depth is bounded (configurable; exceeding = diagnostic + error) —
  a DoS guard on adversarial input.
- *Diagnostic locations:* `location`/spans are droppable hints the sink MAY record (`RdfLocation`-style).
- *Object-safety:* generic `drive<S>` is the hot path; `drive_erased`/`&mut dyn RdfEventSink` is
  MANDATORY for the runtime format/parser **registry** — the generic form cannot be used behind `dyn`.

**Ill-typed literals are PRESERVED + FLAGGED, never auto-rejected.** RDFLib keeps a literal whose lexical
form is invalid for its datatype and records the ill-typed state; purrdf does the same — the gts XSD
lexical layer validates and emits a *diagnostic*, the literal is preserved verbatim
(lexical+datatype+lang), and the value space records "ill-typed". Rejecting the RDF on a bad lexical form
would itself be a fidelity loss.

Sources: GTS reader (via the re-ordering adapter or two-phase `finish`) · every format parser ·
frozen-IR replay. Sinks: `RdfDatasetBuilder` (freeze) · format serializers · the oxigraph bridge.

### Multi-ecosystem — build on gmeow-gts's existing reach, two ABIs not one

`libgts` already reaches 9+ languages but exposes a **narrow** surface (read/fold/verify, GTS↔N-Quads,
files-profile, capability discovery) and refuses query/reason by charter. So:

- **`libgts` (exists, permissive)** — transport + format I/O. Extend its exposed format set (below).
- **`rdf-capi` (new, AGPL)** — the semantic/query layer (rich-format parse→IR, SHACL, oxigraph-backed
  SPARQL, RDFC-1.0, namespaces). The genuinely net-new piece; gts refuses this by charter.
- **Per-ecosystem idiomatic shims** compose BOTH (gts wrapper for I/O + `rdf-capi` for query/semantics)
  into the incumbent's API. The Python PyO3 surface is the reference shim.

**Prime targets** (each already has a gmeow-gts engine/wrapper for the I/O half; RDF-1.2-first is the
universal wedge): JS/TS (N3.js / rdflib.js / RDF-JS, via WASM), Java/JVM (Jena / RDF4J, via Panama FFM
over the Kotlin engine), C/C++ (Redland/Serd), C#/.NET (dotNetRDF), Ruby (RDF.rb), PHP (EasyRdf), R
(`rdflib`, wraps Redland). Opportunistic: Go, Julia, Lua, Perl.

## Internal IR — optimization & enhancement

Ground: `crates/rdf/src/ir/{term,builder,dataset}.rs`. Today — AoS `InternedTerm` enum, per-term
`Box<str>`, an intern `HashMap<InternedTerm, TermId>` that **double-stores** every term (`builder.rs:55`),
`u32` `TermId`, a `Box<[QuadRow]>` sorted+deduped at freeze — **and the builder also holds each quad
twice** (a `Vec<QuadRow>` plus a parallel `HashSet<QuadRow>` for dedup, `builder.rs`; an unacknowledged
build-time cost), **no permutation indexes** (iterate-only), sparse handle-keyed locations. Every change
is **criterion-gated** (`benches/ir_layout.rs`, criterion baselines) and keeps the conformance corpus green
(LSP). Benchmarks must include the builder's quad Vec+HashSet duplication, not just the term double-store.

### Optimizations

1. **`NonZeroU32` `TermId`** — niche → `Option<TermId>` 8→4 bytes; `QuadRow` 20→16 (~20% off the quad
   table). Reserve id 0 sentinel. Low risk.
2. **String-arena, store-once interner** — one contiguous byte arena + `(offset,len)` ranges; a
   `hashbrown` raw `HashTable<TermId>` whose hash/eq look *into* the term table (sole owner). Kills the
   double-store, collapses N allocations → ~1, improves cache locality. `resolve()` still borrows `&str`.
   **The biggest ingestion + memory win.**
3. **SoA term columns** — split the enum into a `kind: u8` + typed columns; removes max-variant bloat.
   Decided by `benches/ir_layout.rs`.
4. **Lazy permutation indexes (access-pattern-driven)** — `OnceLock<Box<[u32]>>` per permutation (ordinal
   indirection, 4 B/quad). SPOG is free (quads already sorted); build POS/OSP/graph-orders **on observed
   access, NOT all at freeze** — six full ordinal arrays cost ~**24 B/quad if all warm**. Yields
   `quads_for_pattern`. Benchmark **cold-index construction, warm queries, peak memory, and concurrent
   first access**. **Term-lookup hazard:** `term_id_by_value(TermRef)` is conceptually UNSAFE — a
   `TermRef`'s literal-datatype and triple-component ids are local to *another* dataset. Accept a
   **dataset-independent `TermValueRef`** (or a resolver-bound key) instead, so the lookup can't smuggle
   foreign ids.
5. **Sparse location remap** — remap only located quads at freeze (today `push_to_frozen` covers all).

### Enhancements

- **Pattern-query API** (`quads_for_pattern`, `terms_by_role`) over the indexes — lets the native store /
  SPARQL / rdflib `Graph` stop linear-scanning.
- **COW suppression-delta** (below) — mutability without abandoning the frozen-share model; dogfoods
  GTS's append+suppression semantics.
- **Content hash at freeze — TWO distinct hashes, do not conflate.** A BLAKE3 over the frozen tables is
  a *representation* hash (cheap, but term order is ingestion-dependent and blank-node labels are NOT
  isomorphism-invariant — so it is not canonical); a *semantic* content hash requires RDFC-1.0
  canonicalization first. Offer both, labelled: representation hash for `.cache/validate`/identical-build
  dedup; RDFC-canonical hash for isomorphism/semantic identity + GTS content-addressing.
- **Lazy literal value-space cache** — parse lexical→typed once; serves FILTER + `Literal.toPython()`.
- **Sparse term metadata side-tables** — prefix/namespace hints, `@x-gmeow-*` lang tags, provenance.

### Lazy quad-index design (spec)

`indexes: QuadIndexes` on `RdfDataset`, one `OnceLock<Box<[u32]>>` per permutation, ordinal-indirection
arrays (4 B/quad each). SPOG free (the table is sorted); build POS/OSP and GSPO/GPOS/GOSP **on observed
access**, not eagerly — all six warm ≈ 24 B/quad. `quads_for_pattern` takes `GraphMatch` (Any vs Default
vs Named — `Option<TermId>` can't express that) and uses a static bound-set→permutation→`partition_point`
table, residual filter, zero-alloc. The value→id primitive the COW delta needs is
**`term_id_by_value(TermValueRef) -> Option<TermId>`** — keyed on a **dataset-independent `TermValueRef`**
(NOT `TermRef`, whose datatype/triple ids are foreign-dataset-local). `OnceLock` keeps the dataset
`Send+Sync`. Benchmark cold construction, warm queries, peak memory, concurrent first access. Defer
per-predicate cardinalities to the SPARQL round.

### COW suppression-delta design (spec)

**A measured hypothesis, not an assumed win** — benchmark it against BOTH the current oxigraph store AND
a simpler hash-indexed mutable store before committing; COW is not inherently optimal for high-churn
RDFLib workloads.

`MutableDataset { base: Arc<RdfDataset>, added: DeltaBuilder, suppressed: HashSet<QuadKey> }`. Effective
quads = `(base ∪ added) − suppressed` — GTS's append+suppression model in memory; `freeze()` =
compaction.

**Term identity uses TAGGED handles, not a numeric threshold** — a two-tier numeric `TermId` would break
the invariant that a `TermId` belongs to one frozen dataset. Use `enum MutTermId { Base(TermId),
Delta(DeltaTermId) }`; existing terms resolve via `term_id_by_value(TermValueRef)` (a miss mints a
`Delta` id).

**Mutation rules (specify explicitly):** insert of a *suppressed base* quad **un-suppresses** it; remove
of a *delta-added* quad removes it from `added`; remove of a *base* quad creates a suppression;
reinsert-after-removal is consistent with both. **`freeze()` remaps everything** — terms, reifiers,
annotations, graph names, locations, and metadata — not only quads. Branching and compaction **invalidate
no externally-visible handles**.

`DatasetMut` impl: `insert`/`remove`/`contains`/`quads_for_pattern`. Many deltas branch cheaply off one
shared base `Arc`; `should_compact()` signals re-freeze. The rdflib `Graph` facade AND the native
`MutableStore` both bind to `DatasetMut` — the substrate that *could* let oxigraph's store be dropped, if
the benchmarks bear it out.

## Rust-idiomatic surfaces (committed)

- **Abstraction traits:** `DatasetView` (read) + `DatasetMut` (write) + `TermFactory`/DataFactory —
  one read/write interface across frozen IR, COW delta, and backend adapters; the RDF/JS shim maps
  onto `TermFactory` 1:1. `DatasetMut` is already delivered by P5; P2d adds the
  remaining term/parser/SPARQL/serializer seams.
- **std traits:** `Display`/`FromStr` (canonical N-Triples term form) on `TermRef`/`QuadRef`;
  `IntoIterator for &RdfDataset`; `FromIterator`/`Extend` on the builder; `std::error::Error` on
  `RdfDiagnostic`; documented **`Send+Sync`** on the frozen dataset (why the lazy index uses `OnceLock`).
- **FFI/forward-compat:** `#[repr(transparent)]` `NonZeroU32` `TermId`; `#[non_exhaustive]` enums.
- **I/O idioms:** parsers take `impl BufRead`, serializers `impl Write`; both push (`RdfEventSink`) and
  pull (`QuadSource: Iterator`).
- **Feature-gated:** `serde` on the owned value model only (never `TermId`, preserving C0.8); `rayon`
  `par_quads()`; a `no_std`+alloc core (`hashbrown`, no file-IO in the IR) for wasm/embedded.
- **Ergonomics:** a `prelude`; `RdfStoreCapabilities` as capability introspection.

## Internal execution plan (PR-sized parcels)

Dependency-ordered; each independently shippable. `∥` = parallelizable in its own worktree. No
durations — order is dependency, not calendar. Every parcel: conformance corpus green; perf parcels add
a criterion baseline before/after.

- **P0 — Self-host: kill the internal rdflib dep [∥].** Port `src/gmeow_tools/**` (~15 modules) off
  rdflib onto the native surface + the native drop-ins. Drop `rdflib>=7.6` (the former
  `owlrl`-backed reasoning-oracle lane that once retained it is gone; the live in-process
  `purrdf::entail` cross-check that briefly replaced it has since been retired too — the
  native `logic:` reasoner is the sole reasoning authority). Gate: `rg "import rdflib" src/` empty; `make check`+
  `make test` green without rdflib. The honest proving gate.
- **P1 — SRP-decompose `py_store.rs` [no deps].** Split the 1,239-line god-module into
  `term`/`store`/`query`/`io`/`canon`. Behavior-identical; corpus green.
- **P2 — Backend traits + oxigraph ring-fence [needs P1].** `DatasetView` + `GraphMatch` (P2a),
  the `gmeow-rdf-core` / `gmeow-rdf` crate boundary (P2b), the concrete-IR consumer migration (P2c),
  and the remaining `TermFactory` + `RdfParserBackend` + `SparqlEngine` + `RdfSerializer` seams (P2d).
  `DatasetMut` is P5, not repeated here. The sole `use oxigraph` sites stay in the adapter
  crate; `gmeow-rdf-core` remains oxigraph-free under `make rdf-core-hygiene`.
- **P3 — IR perf [∥; SPLIT into measure-gated steps]:** **P3a** `NonZeroU32` `TermId` + compile-time
  `size_of!` assertions; **P3b** arena / string-range term representation; **P3c** store-once hash table
  (drops the term double-store *and* the builder's quad `Vec`+`HashSet` dedup duplication); **P3d** SoA
  columns — **only if the bench justifies it**. Each step independently criterion-gated.
- **P4 — Lazy quad indexes + `term_id_by_value(TermValueRef)` + `GraphMatch` [needs P3a].**
  Access-pattern-driven build; the value lookup keys on a dataset-independent `TermValueRef`.
- **P5 — COW suppression-delta + `DatasetMut` [needs P2, P4].** Tagged `Base`/`Delta` handles; explicit
  mutation rules; `freeze` remaps all tables. **Delivered.** A measured hypothesis — benchmark
  vs the oxigraph store AND a simple hash-indexed mutable store.
- **P6 — `gmeow-rdf-events` ingestion protocol crate [needs P2].** New standalone permissive crate (NOT
  a `StreamingSink` rename); rename the existing frozen-dataset visitor `RdfDatasetVisitor`. Object-safe
  sink + generic & erased source; forward-reference + `finish()` resolution; specified scope/ID/cancel/
  nesting/diagnostic semantics.
- **P7 — rust-isms surface + `no_std` readiness [folds into P2–P6].**
- **P9 — M1 term-facade rdflib compat shim [needs P5 + XSD value space + P2].** (spec below) — ships
  BEFORE the C-ABI: a stable Python beta is the prerequisite for stabilizing the C-ABI surface.
- **P8 — `rdf-capi` semantic C-ABI [needs a STABLE Python beta — i.e. after P9].** (spec below)
- **P10 — WASM build [needs P2/P4/P5; SEPARATE from P8].** Different ownership/packaging/async-I/O/JS-API
  concerns; needs neither the C-ABI nor `no_std`. Its own parcel.

**Parallelism:** P0/P1/P3a are concurrent worktrees; P2 after P1; P4 after P3a; P5 needs P2+P4; P6 after
P2; P9 after P5; **P8 (C-ABI) only after a stable Python beta (P9)**; P10 (WASM) after P2/4/5, parallel
to P8/P9. P7 rides along.

### P8 spec — `rdf-capi`/`libpurrdf` (the gmeow-rdf semantic C-ABI) — AFTER a stable Python beta

`crates/rdf-capi`, `cdylib`+`staticlib`, `extern "C"`, `cbindgen` header `purrdf.h`, `cargo-c`. Opaque
handles `PurrdfDataset*` (frozen, `Send+Sync`), `PurrdfGraph*` (COW delta, single-threaded mutable),
`PurrdfCursor*` (holds an `Arc` clone so it can't dangle), `PurrdfError*`.

**One shared library, not two.** A language shim must NOT have to coordinate two native `.so`s. The
high-level **`libpurrdf` internally reuses the permissive `gmeow-gts` Rust crate** (statically), while
**`libgts` remains independently usable** for gts-only consumers. Shims link `libpurrdf` alone.

**Term crossing — offer several representations, NOT only N-Triples bytes** (re-parsing N-Triples on
every cursor row is not the cheapest path):

- opaque, **cursor-scoped term handles** or **structured term views** (kind tag + parts) for hot iteration;
- **borrowed UTF-8 slices with documented lifetimes** (valid until the next `cursor_next`/free);
- a **row cursor** for SELECT results (column-addressed), not row re-serialization;
- N-Triples and SPARQL-JSON **convenience** functions for the simple/robust path.

Surface: `purrdf_abi_version`/`purrdf_capabilities`; `purrdf_parse`/`purrdf_serialize` (format_id = the
media-type registry); `purrdf_graph_from_dataset`/`_insert`/`_remove`/`_freeze`; `purrdf_quads_for_pattern`
(takes `GraphMatch`) + `purrdf_cursor_next`; `purrdf_query` (row cursor + SPARQL-JSON convenience). FFI
contracts: `catch_unwind` everywhere, `int32` status + out-params, `*_free` for every buffer/error/cursor,
documented thread-safety per handle, **SemVer-frozen ABI** (the one sanctioned no-backwards-compat
exception). Gated on a stable Python beta so the surface is proven before it is frozen.

### P10 spec — WASM build (separate parcel) — **DELIVERED**

`wasm32`, in-memory only (oxigraph RocksDB and `crates/logic` don't compile to wasm). **Not** a C-ABI
consumer and **not** dependent on `no_std`: WASM has its own ownership model, packaging (npm/ESM), async
I/O, and idiomatic JS API (RDF/JS `DataFactory`/Stream). Its own parcel, parallel to the C-ABI.

**Delivered** as `crates/rdf-wasm` (the `gmeow-rdf-wasm` cdylib) + the `purrdf` npm/ESM package at
`crates/rdf-wasm/js/`. It compiles the oxigraph-free / PyO3-free `gmeow-rdf` kernel
(`--no-default-features --features gts`) to `wasm32-unknown-unknown` — no engine cfg-gating was needed
(the probed `ed25519`/`getrandom` blocker did not materialize: Ed25519 is deterministic and unreachable
from the RDF/JS surface). The shipped surface:

- **`DataFactory`** mapped 1:1 onto the owned term model, extended with the RDF-1.2 wedge —
  `quotedTriple` (a triple term usable as a subject/object) and `directionalLiteral` (base direction) —
  that no incumbent RDF/JS library carries. The polymorphic `literal(value, languageOrDatatype)` is
  presented by the TS wrapper (a `#[wasm_bindgen]`-exported type can't be recovered from an untyped
  `JsValue` in Rust).
- **`Dataset`** (RDF/JS `DatasetCore`) over the COW `MutableDataset`: `parse`/`serialize`
  (turtle/ntriples/nquads/trig/rdfxml via the native codecs), `add`/`delete`/`has`/`match`/`quads`/`size`.
- **`Sink`** streams quads through the `gmeow-rdf-events` P6 ingestion protocol (with `finish()`
  resolution); the async `EventEmitter`-based RDF/JS `Stream`/`Sink.import` is presented by the TS wrapper
  (`datasetToStream`/`streamToDataset`).
- **Gates:** `make wasm` (engine + bindings build for wasm32, hard-fail in CI) and `make wasm-pkg-test`
  (the Node real-execution lane: the actual wasm round-trips the RDF-1.2 wedge through N-Quads). A
  **dormant** `npm-publish-purrdf` workflow mirrors `pypi-publish-gmeow`.

**Deferred (out of P10):** the JS-ecosystem conformance suites (N3.js / rdflib.js / RDF-JS), SPARQL over
wasm (oxigraph-backed, native-only by charter), `wasm-opt -Oz` size optimization, and the actual npm
publish — deferred to the post-v1 spin-up.

### P9 spec — the rdflib compat shim (`gmeow_rdf.compat.rdflib`), LSP-critical

Pure-Python facade over the native ext; absorbs rdflib's idioms so the greenfield core stays clean.

- **Terms as `str` subclasses:** `URIRef(str)`, `BNode(str)`, `Variable(str)`; **`Literal(str)`** with
  `.datatype`/`.language` and `.value`/`.toPython()` (the rdf-side XSD value space).
- **Match RDFLib's equality model EXACTLY — and put it in the SHIM, not the IR.** In RDFLib 7.6:
  `Literal.__eq__` is **RDF term equality** over `(lexical, datatype, language)` — and **`__hash__`
  follows `__eq__`**, so it hashes over that same triple; `Literal.eq()` is the separate
  **value-space** (interpreted) equality; ordering is yet another combination (value comparison →
  datatype ordering → language ordering → lexical fallback). `__eq__` is therefore **NOT value-based** —
  getting it wrong silently corrupts every dict/set in downstream code. The deepest LSP risk; gated by
  RDFLib's own `Literal` tests.
- **The `xsd:string`-expansion conflict (must resolve):** the native IR expands a plain literal to
  `xsd:string` (C0.1), but RDFLib keeps `datatype=None` distinct from an explicit `xsd:string` under
  `==` (a documented open RDFLib issue). **Do NOT distort the native RDF-1.2 model to match.** The shim
  resolves it by ONE of: (a) **preserve RDFLib constructor/syntax provenance** (was-explicit-datatype,
  original lexical/lang casing) as **shim-side metadata outside the IR** (the sparse term-metadata
  side-table pattern) so it reproduces RDFLib `==`/hash exactly while the IR stays value-interned and
  correct — *recommended*; (b) deliberately diverge and document the incompatibility; (c) keep separate
  native-correct vs RDFLib-compatible term representations at the boundary. All RDFLib historical
  behaviours live in the shim, never the core.
- **`Graph` (mutable)** backed by the native COW delta via PyO3: `add/remove/set/value`,
  `triples((s,p,o))` with `None` → `quads_for_pattern`, the accessor family, `__contains__/__len__/
  __iter__`, algebra (`+ - * ^`, `+=`/`-=`), `transitive_*`, `Collection`. `parse`/`serialize` route
  through the codecs; `query`/`update` through the `SparqlEngine`; `namespace_manager`/`bind` capture
  `prefix`/`base` hints.
- **`Dataset`/`ConjunctiveGraph`** facades. **Interning boundary:** a Python str-subclass term resolves
  to a native `TermId` by value via `term_id_by_value`. Batch bulk `add` to amortize PyO3 crossings.
- **LSP gate:** rdflib's own test suite + W3C suites; not "done" until green.

## SOLID posture (internal plan)

- **S:** strong at the seams; **decompose `py_store.rs` first** (god-module) along the traits.
- **O:** backend traits + `RdfEventSink` are additive; drive parse/serialize through a **media-type
  registry**, not a closed `RdfFormat` enum.
- **L:** the whole "drop-in" claim — gated on BOTH external rdflib parity AND internal native-vs-oxigraph
  parity by the **conformance corpus**.
- **I:** four narrow backend traits, not one fat `Backend`. Watch the wide `RdfEventSink` (split later if
  default-noise grows).
- **D:** the spine — consumers depend on backend abstractions; one **required** backend wired (DIP +
  no-optionality reconciled).

## gmeow-gts dependencies (tracked in the gmeow-gts repo)

To bring gmeow-gts "up to snuff" for purrdf:

1. **Depend on the standalone `gmeow-rdf-events` protocol crate** (defined above; NOT inside the gts
   container product), and implement an `RdfEventSource` over the GTS reader against it — including a
   **re-ordering adapter** (or a `declares_before_reference` mode) so the current forward-reference GTS
   order can feed bounded-memory sinks. Rename the existing frozen-dataset visitor `RdfDatasetVisitor`.
2. **Turtle + TriG codec** — parse + serialize, RDF-1.2-first, emitting the event protocol (TriG-write
   exists; add Turtle + the read side).
3. **N-Triples codec** — parse + serialize via the protocol (the triple subset of N-Quads).
4. **RDF/XML codec** — parse + serialize; net-new (XML namespaces, `rdf:parseType`, collections,
   reification). The largest format gap.
5. **XSD lexical-form validation/canonicalization layer** — the syntax-side datatype lexical checks the
   parsers need (the lexical half of the split; value space stays in gmeow-rdf). **Validate + FLAG, never
   reject:** ill-typed literals are preserved verbatim and surfaced as diagnostics (RDFLib parity).
6. **Expose the format codec set through the `libgts` C-ABI** — format-parametric parse/serialize + a
   media-type/format registry, beyond today's GTS↔N-Quads.

Losslessness is already done in gmeow-gts.

## Risks / open questions

- **str-subclass terms vs the interned fast path** — the central design tension; prototype before P9.
- **RDF/XML + full JSON-LD-star** — real net-new parsers; the largest gts-side effort.
- **Plugin entry-points** — "full public" parity may be asymptotic; define a concrete downstream
  acceptance list (deferred follow-up).
- **Performance claims** (~10× parse / ~12× SPARQL) come from comments; publish criterion-backed
  benchmarks (the criterion infrastructure exists) rather than restate them.

## Verification (per-parcel acceptance criteria)

- **P0 gate (the honest one):** `rg "import rdflib" src/` empty; `rdflib` removed from `pyproject.toml`;
  `make check` + `make test` green without rdflib installed.
- **oxigraph-optional gate (P2):** `cargo build -p gmeow-rdf --no-default-features --features <core>`
  compiles + core tests pass; proves no oxigraph type leaks outside `backend/oxigraph/`.
- **Per-parcel:** unit tests mirroring the changed behavior; native ext rebuilt
  (`maturin develop --manifest-path crates/native/Cargo.toml`) before Python tests; criterion baseline
  before/after for perf parcels.
- **LSP acceptance (P9 / v1):** rdflib's own test suite + W3C RDF 1.1/1.2 + SPARQL 1.1 suites green
  against `gmeow_rdf.compat.rdflib`; published benchmarks vs rdflib.

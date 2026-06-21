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
POWER, EXPRESSIVE DEPTH, REASONING") and with three drop-ins already in-repo: `native_rl.py` (replaces
`owlrl`), `rdf_canonical.py` (replaces `rdflib.compare.isomorphic`), `sparql.py` (replaces rdflib's
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
  literals `RdfTextDirection`, reifiers, annotations), frozen interned `RdfDataset`, `RdfStore` trait,
  structural isomorphism `ir/compare.rs`, loss ledger `loss.rs`.
- **oxigraph adapter** `crates/rdf/src/oxigraph.rs` — materialize to oxigraph `Store`; full SPARQL.
- **Python surface** `crates/rdf/src/py_store.rs` + stub `crates/native/python/gmeow_rdf/__init__.pyi`:
  pyoxigraph-shaped `NamedNode/BlankNode/Literal/Triple/Quad/Variable`, `Store`, `Dataset`,
  `QuerySolutions/QueryTriples/QueryBoolean`, `parse()`/`serialize()`, `canonicalize_turtle`.
- **Existing rdflib drop-ins:** `src/gmeow_tools/native_rl.py`, `rdf_canonical.py`, `sparql.py`.
- **gmeow-gts (separate Apache/MIT repo, crates.io 0.9.4):** six conformance-gated engines (Rust,
  Python, Go, TypeScript, Smalltalk/Pharo, Kotlin/JVM), a Rust-backed C-ABI `libgts` with C/C++/.NET/
  PHP/Lua/Swift/Ruby/R/Julia wrappers, RDF-1.2 losslessness (#212/#213/#214 closed), and formats
  N-Quads / TriG / JSON-LD-star / YAML-LD-star + relational exports (SQLite/DuckDB/Parquet). It is
  **not** a query engine/reasoner/store by charter.

## Architecture

### Ring-fence oxigraph into one backend module, single required backend

Collapse every `use oxigraph` into one `crates/rdf/src/backend/oxigraph/` module; the rest of the crate
talks to it only through traits. oxigraph becomes a swappable backend, and the trait seam IS what native
parsers/SPARQL and the C-ABI plug into.

- **Already partly there:** oxigraph is `optional`; `oxigraph.rs`/`statements.rs`/`turtle_normalize.rs`/
  the `gts.rs` flatten bridge are gated. Gaps: `gts`/`python` features transitively pull oxigraph; the
  `py_store.rs`/`py_gts.rs`/`py_gts_dataset.rs` leaks import oxigraph types while gated only on `python`;
  there is no parser-backend or query-backend trait yet.
- **Trait seam:** `Dataset` (read) + `DatasetMut` (write) + `TermFactory`, and the backends
  `RdfParserBackend` / `SparqlEngine` / `MutableStore` / `RdfSerializer`. oxigraph implements them;
  native impls implement the SAME traits and replace oxigraph at conformance parity.
- **No-optionality reconciliation:** the ring-fence is **DIP code isolation**; "builds without oxigraph"
  is a **CI hygiene check** proving no oxigraph type leaks past the traits — NOT a shipped degraded mode.
  Exactly ONE backend is wired and **required** at any time; missing it is a HARD FAIL, never a silent
  no-SPARQL mode. oxigraph is **removed at parity**, not left as a parallel optional.

### Zero-copy oxigraph bridge

Both sides speak borrowed references: the IR resolves `TermId -> TermRef<'_>` (borrows `&str` from the
intern table); oxigraph exposes `QuadRef<'a>`/`TermRef<'a>`/`*Ref` borrowing `&str`. The bridge maps IR
refs → oxigraph `*Ref` over the **same backing bytes** — no intermediate owned `RdfQuad`/`oxrdf::Quad`
on ingress. Replaces the owned `rdf_quad_from_oxigraph`/`oxigraph_quad_from_rdf` conversions and kills
the `GTS → RdfQuad (clone) → oxigraph (clone again)` double-tax flagged in #819.

### Cross-repo split: gts owns syntax, gmeow-rdf owns semantics

- **gmeow-gts (Apache/MIT)** — a standalone permissive multi-format RDF I/O + container library: all
  format codecs (bytes ↔ event stream) + CBOR container + verify. Net-new vs. today: **Turtle / N-Triples
  / RDF-XML** (N-Quads, TriG-write, JSON-LD-star/YAML-LD-star already ship). Permissive parsers adopt
  where AGPL can't — directly serving the ecosystem-reach goal.
- **gmeow-rdf (AGPL)** — semantics: events → interned IR, the XSD value space, RDFC-1.0
  canonicalization, RDF-1.2 reification meaning, SHACL/logic, the SPARQL bridge.
- **Connecting seam = `RdfEventSource`/`RdfEventSink`** — the #819-blessed generalization of gts's
  existing `StreamingSink` (format-neutral: it already carries term/quad/reifier/annotation *values*, not
  CBOR framing). Every gts parser emits the same events; `RdfDatasetBuilder` (which *is* the sink)
  ingests parsed-Turtle identically to parsed-GTS-binary. The seam vocabulary + traits live in the
  **permissive** crate (AGPL may depend on Apache, not vice-versa), so a `parse-Turtle → serialize-NQ`
  path is pure-gts with no AGPL code.
- **Doctrine refinement:** the narrow-waist "one producer, no re-parse" rule governs **export**, not
  **ingress** — parsers-as-ingress in gts do not violate it.

### GTS is lossless — a hard invariant

Per `.goals` maximal-information-flow, GTS carries full fidelity; information is trimmed ONLY at
exit-gate projections to genuinely-lossy external formats (Turtle-1.1 without triple terms, sheet music,
CITATION.cff, …) — only the *projection* is lossy and ledgered, never GTS itself. Current status: met —
the loss matrix lists only `blob-bytes-absent` (recoverable by-reference) and `bnode-scope-flatten`
(flat-`read()` only; the streaming path is lossless); `direction-dropped`/`multi-reifier-collapsed` were
flipped (gmeow-gts #212/#213/#214 closed). The loss ledger accounts for **exit-gate projections**, not
the container.

### `RdfEventSource` / `RdfEventSink` contract (the gts↔rdf seam)

Defined in gmeow-gts (permissive). ID-addressed, term-before-reference. Sketch:

```rust
// gmeow-gts (permissive): vocabulary + traits
pub struct ScopeId(u32);        // a document/segment boundary for blank-node identity
pub struct EventTermId(u32);    // SCOPE-LOCAL handle, NOT identity — the sink re-interns by value
pub enum TextDirection { Ltr, Rtl }
pub enum EventTerm<'a> {
    Iri(&'a str),
    Blank { label: &'a str },                       // identity = (label, current scope)
    Literal { lexical: &'a str, datatype: EventTermId,
              language: Option<&'a str>, direction: Option<TextDirection> },  // RDF-1.2 base dir
    Triple { s: EventTermId, p: EventTermId, o: EventTermId },                // RDF-1.2 triple term
}
pub struct EventQuad { s: EventTermId, p: EventTermId, o: EventTermId, g: Option<EventTermId> }
pub trait RdfEventSink {
    type Error;
    fn term(&mut self, scope: ScopeId, id: EventTermId, t: EventTerm<'_>) -> Result<ControlFlow<()>, Self::Error>;
    fn quad (&mut self, scope: ScopeId, q: EventQuad) -> Result<ControlFlow<()>, Self::Error>;
    fn quads(&mut self, scope: ScopeId, q: &[EventQuad]) -> Result<ControlFlow<()>, Self::Error> { /* default loop */ }
    fn reifier   (&mut self, scope: ScopeId, r: EventTermId, t: EventQuad) -> Result<ControlFlow<()>, Self::Error>;
    fn annotation(&mut self, scope: ScopeId, r: EventTermId, p: EventTermId, o: EventTermId) -> Result<ControlFlow<()>, Self::Error>;
    // lifecycle + lossy-droppable syntactic hints + diagnostics (default no-op)
    fn open_scope (&mut self, _: ScopeId) -> Result<ControlFlow<()>, Self::Error> { Ok(Continue(())) }
    fn close_scope(&mut self, _: ScopeId) -> Result<ControlFlow<()>, Self::Error> { Ok(Continue(())) }
    fn prefix(&mut self, _p: &str, _iri: &str) -> Result<ControlFlow<()>, Self::Error> { Ok(Continue(())) }
    fn base  (&mut self, _iri: &str)            -> Result<ControlFlow<()>, Self::Error> { Ok(Continue(())) }
}
pub trait RdfEventSource { type Error; fn drive<S: RdfEventSink>(self, s: &mut S) -> Result<(), /* … */>; }
```

Rules: IDs are compression handles, not identity (sink re-interns by value, as `ir/import_sink.rs` does
today); blank-node identity = `(label, ScopeId)`; triple terms recurse; base `direction` is carried;
the literal event carries `lexical: &str` + a `datatype` term-id (gts validates the lexical form — the
*value space* is the sink's job, gmeow-rdf); `prefix`/`base`/`location` are lossy-droppable hints
(serializers + the rdflib `NamespaceManager` capture them); `ControlFlow` = cancel, `Err` = malformed.
Sources: gts-binary reader · every format parser · frozen-IR replay. Sinks: `RdfDatasetBuilder` (freeze)
· format serializers · the oxigraph bridge.

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
`u32` `TermId`, a `Box<[QuadRow]>` sorted+deduped at freeze, **no permutation indexes** (iterate-only),
sparse handle-keyed locations. Every change is **criterion-gated** (`benches/ir_layout.rs`, #630
baselines) and keeps the conformance corpus green (LSP).

### Optimizations

1. **`NonZeroU32` `TermId`** — niche → `Option<TermId>` 8→4 bytes; `QuadRow` 20→16 (~20% off the quad
   table). Reserve id 0 sentinel. Low risk.
2. **String-arena, store-once interner** — one contiguous byte arena + `(offset,len)` ranges; a
   `hashbrown` raw `HashTable<TermId>` whose hash/eq look *into* the term table (sole owner). Kills the
   double-store, collapses N allocations → ~1, improves cache locality. `resolve()` still borrows `&str`.
   **The biggest ingestion + memory win.**
3. **SoA term columns** — split the enum into a `kind: u8` + typed columns; removes max-variant bloat.
   Decided by `benches/ir_layout.rs`.
4. **Lazy permutation indexes at freeze** — `OnceLock<Box<[u32]>>` per non-free permutation (ordinal
   indirection, 4 B/quad). SPOG is free (quads already sorted). Build POS/OSP/graph-orders on first use.
   Yields `quads_for_pattern` and `term_id_by_value`. Turns the iterate-only IR queryable.
5. **Sparse location remap** — remap only located quads at freeze (today `push_to_frozen` covers all).

### Enhancements

- **Pattern-query API** (`quads_for_pattern`, `terms_by_role`) over the indexes — lets the native store /
  SPARQL / rdflib `Graph` stop linear-scanning.
- **COW suppression-delta** (below) — mutability without abandoning the frozen-share model; dogfoods
  GTS's append+suppression semantics.
- **BLAKE3 content hash at freeze** — `.cache/validate` key, dedup, GTS content-addressing.
- **Lazy literal value-space cache** — parse lexical→typed once; serves FILTER + `Literal.toPython()`.
- **Sparse term metadata side-tables** — prefix/namespace hints, `@x-gmeow-*` lang tags, provenance.

### Lazy quad-index design (spec)

`indexes: QuadIndexes` on `RdfDataset`, one `OnceLock<Box<[u32]>>` per permutation, ordinal-indirection
arrays. SPOG free (the table is sorted); lazily build POS/OSP and GSPO/GPOS/GOSP when `caps.named_graphs`.
`quads_for_pattern(s?,p?,o?,g?)` uses a static bound-set→permutation→`partition_point` table, residual
filter, zero-alloc. The same machinery yields `term_id_by_value(TermRef) -> Option<TermId>` — the
primitive the COW delta needs. `OnceLock` keeps the dataset `Send+Sync`. Defer per-predicate
cardinalities to the SPARQL round.

### COW suppression-delta design (spec)

`MutableDataset { base: Arc<RdfDataset>, added: DeltaBuilder, suppressed: HashSet<QuadKey> }`. Effective
quads = `(base ∪ added) − suppressed` — GTS's append+suppression model in memory; `freeze()` =
compaction. Two-tier term ids (`< base.term_count()` resolve in base; `≥` resolve in the delta's
interner; existing terms resolved via `term_id_by_value`, a miss mints a delta id). `DatasetMut` impl:
`insert`/`remove`/`contains`/`quads_for_pattern`. Many deltas branch cheaply off one shared base `Arc`;
`should_compact()` signals re-freeze. The rdflib `Graph` facade AND the native `MutableStore` both bind
to `DatasetMut` — so this one delta is the substrate that finally lets oxigraph's store be dropped.

## Rust-idiomatic surfaces (committed)

- **Abstraction traits:** `Dataset` (read) + `DatasetMut` (write) + `TermFactory`/DataFactory — one read
  interface across frozen IR, COW delta, oxigraph; the RDF/JS shim maps onto `TermFactory` 1:1.
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
  rdflib onto the native surface + the native drop-ins. Drop `rdflib>=7.6` (retain only in the
  `classic_cross_check` lane while owlrl needs it). Gate: `rg "import rdflib" src/` empty; `make check`+
  `make test` green without rdflib. The honest proving gate.
- **P1 — SRP-decompose `py_store.rs` [no deps].** Split the 1,239-line god-module into
  `term`/`store`/`query`/`io`/`canon`. Behavior-identical; corpus green.
- **P2 — Backend traits + oxigraph ring-fence [needs P1].** `Dataset`/`DatasetMut`/`TermFactory` +
  `RdfParserBackend`/`SparqlEngine`/`MutableStore`/`RdfSerializer`; sole `use oxigraph` in
  `backend/oxigraph/`; fix leaks; decouple `gts`/`python` from oxigraph; `--no-default-features` hygiene
  build.
- **P3 — IR perf: `NonZeroU32` + arena/store-once interner [∥].** Measure-gated.
- **P4 — Lazy quad indexes + `term_id_by_value` [needs P3].** Queryable IR.
- **P5 — COW suppression-delta + `DatasetMut` [needs P2, P4].** The mutability substrate.
- **P6 — `RdfEventSource`/`RdfEventSink` seam [needs P2].** Generalize `StreamingSink`; builder = sink;
  GTS reader + frozen-IR replay as sources.
- **P7 — rust-isms surface + `no_std` readiness [folds into P2–P6].**
- **P8 — `rdf-capi` semantic C-ABI + wasm [needs P2,P4,P5].** (spec below)
- **P9 — M1 term-facade rdflib compat shim [needs P5 + XSD value space + P2].** (spec below)

**Parallelism:** P0/P1/P3 are three concurrent worktrees; P2 after P1; P4 after P3; P5 needs P2+P4; P6
after P2; P8 after P2/4/5; P9 after P5. P7 rides along.

### P8 spec — `rdf-capi` (the gmeow-rdf semantic C-ABI, distinct from `libgts`)

`crates/rdf-capi`, `cdylib`+`staticlib`, `extern "C"`, `cbindgen` header `purrdf.h`, `cargo-c`; parallel
wasm32 (in-memory only). Opaque handles `PurrdfDataset*` (frozen, `Send+Sync`), `PurrdfGraph*` (COW
delta, single-threaded mutable), `PurrdfCursor*` (holds an `Arc` clone so it can't dangle), `PurrdfError*`.
**Terms cross as N-Triples byte slices** (cheapest lossless cross-ABI form; reuses `Display`/`FromStr`).
Surface: `purrdf_abi_version`/`purrdf_capabilities`; `purrdf_parse`/`purrdf_serialize` (format_id = OCP
codec registry); `purrdf_graph_from_dataset`/`_insert`/`_remove`/`_freeze`; `purrdf_quads_for_pattern` +
`purrdf_cursor_next`; `purrdf_query` returning SPARQL-JSON bytes (oxigraph-backed behind the seam). FFI
contracts: `catch_unwind` everywhere, `int32` status + out-params, `*_free` for every buffer/error,
documented thread-safety, SemVer-frozen ABI (the one sanctioned no-backwards-compat exception). Composes
`libgts` for transport rather than duplicating it.

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

1. **`RdfEventSource`/`RdfEventSink`** — generalize `StreamingSink` into the public, format-neutral,
   RDF-1.2-faithful event contract (the seam every codec emits into).
2. **Turtle + TriG codec** — parse + serialize, RDF-1.2-first, via the event contract (TriG-write exists;
   add Turtle + the read side).
3. **N-Triples codec** — parse + serialize via the event contract (the triple subset of N-Quads).
4. **RDF/XML codec** — parse + serialize; net-new (XML namespaces, `rdf:parseType`, collections,
   reification). The largest format gap.
5. **XSD lexical-form validation/canonicalization layer** — the syntax-side datatype lexical checks the
   parsers need (the lexical half of the split; the value space stays in gmeow-rdf).
6. **Expose the format codec set through the `libgts` C-ABI** — format-parametric parse/serialize + a
   media-type/format registry, beyond today's GTS↔N-Quads.

Losslessness is already done in gmeow-gts (#212/#213/#214 closed).

## Risks / open questions

- **str-subclass terms vs the interned fast path** — the central design tension; prototype before P9.
- **RDF/XML + full JSON-LD-star** — real net-new parsers; the largest gts-side effort.
- **Plugin entry-points** — "full public" parity may be asymptotic; define a concrete downstream
  acceptance list (deferred follow-up).
- **Performance claims** (~10× parse / ~12× SPARQL) come from comments; publish criterion-backed
  benchmarks (the #630 infra exists) rather than restate them.

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

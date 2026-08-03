<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# purrdf backend contract (P2)

Companion to [`PURRDF-PLAN.md`](./PURRDF-PLAN.md). PurRDF owns the physical RDF
dataset, paging, pack, parser/serializer, and operation-evidence contracts; this
document records the boundary GMEOW consumes. GMEOW owns the RDF 1.2 semantic
language, calculus, and conformance layered over that boundary.

This document is normative for the purrdf backend traits (`DatasetView`,
`DatasetMut`, `TermFactory`, `RdfParserBackend`, `SparqlEngine`, and
`RdfSerializer`). `DatasetView` landed in `rdf-core/src/dataset_view.rs` after the
P2b crate split; `DatasetMut` landed as the P5 write substrate; the remaining four
narrow seams land in P2d.

> **Historical record.** The kernel crates named here were subsequently extracted into the sibling
> **`purrdf`** package, so `crates/rdf-core` and its siblings no longer exist in this repository —
> these traits are now reached through the single `purrdf` dependency. The boundary this document
> defines is unchanged; only its location is.

## C1 — Backend-selection: compile-time, single, required

GMEOW is **no-optionality / hard-fail** (`.goals`). Therefore:

- Exactly **one** backend is wired and **required**. A missing backend is a hard
  build/link failure, never a degraded runtime fallback. There is no "no backend"
  mode and no parallel optional backend.
- Backend selection is **compile-time**, not runtime. The kernel names a single
  concrete backend through a crate-boundary (P2b); it does not dispatch over a
  registry of backends at runtime.

**Consequence (load-bearing):** because selection is compile-time, the **static**
trait layer (`DatasetView`, generic `impl Trait`, RPITIT, allocation-free) is the
*only* mandatory layer. The **erased** layer (`&mut dyn DatasetView`-style object
safety, a runtime parser/format registry) is **deferred** — it is added only if and
when a genuine runtime-plugin need appears (e.g. a C-ABI host loading formats at
runtime). P2a therefore ships the static trait and does **not** force object safety.

## C2 — Read view: ownership, lifetimes, cursor model

- The read view borrows; it never owns. `DatasetView` yields `QuadIds` (`Copy`,
  id-only) and `QuadRef<'a>` (borrowed term bytes from the backing term table). No
  per-quad heap allocation, no term-string clones (this kills the owned
  `rdf_quad_from_oxigraph` double-tax).
- A cursor (the returned iterator) borrows the view for `'a`; it cannot outlive the
  view and does not pin a snapshot beyond the borrow. Concurrent mutation during
  iteration is impossible by construction (the read view is `&self`; mutation needs
  `&mut self` via `DatasetMut`, P2c/P5).
- Identity is dataset-local (`TermId`, C0.8): ids and `GraphMatch::Named(TermId)`
  are only meaningful against the *same* view that minted them. Cross-view queries
  resolve through values (`term_id_by_value`, P4) first.

## C3 — Pattern matching: id-based, `GraphMatch` for the graph slot

`quads_for_pattern(s, p, o: Option<TermId>, g: GraphMatch)` filters by **id
equality** (zero string resolution on the hot path). The graph slot needs its own
type because storage keeps `g: Option<TermId>` where `None` = the default graph, and
`Option<TermId>` cannot distinguish *"any graph"* from *"the default graph"*. Hence
`enum GraphMatch { Any, Default, Named(TermId) }`. The P2a default impl is a linear
scan; P4 overrides it with the lazy access-pattern indexes.

## C4 — Write side (DatasetMut): deferred to P2c/P5

`DatasetMut` (insert/remove, `Base`/`Delta` COW handles) is delivered as the P5
substrate. SPARQL **UPDATE** and **transactions** are a backend concern
(oxigraph provides them); the contract: UPDATE APIs live on the SPARQL backend
trait (P2d), never on `DatasetView` (read-only), and an UPDATE is atomic per the
backend's transaction semantics (oxigraph: serializable in-memory).

## C5 — Cancellation and bounded lazy reads

Resident iterators remain pull-based: dropping a cursor stops further work. A lazy
paged operation additionally exposes provider, page/byte-budget, cancellation,
deadline, and generation failures through its operation-scoped fallible view. The
first failure is sticky; iterators stop, and an engine must sample the operation
status after all result materialization before it can certify completeness.

## C6 — Error model

- The resident read view is **infallible** for a frozen, validated `RdfDataset`:
  `quads`, `quad_refs`, `resolve`, `quads_for_pattern` cannot fail (validation
  happened at freeze). They therefore return values/iterators, not `Result`.
- `FallibleDatasetView` preserves that iterator shape for lazy providers while making
  the operational result explicit through preflight/final `operation_status`
  checkpoints. A provider failure or exhausted resource budget means computational
  incompleteness, never an empty RDF graph.
- Fallible backend operations (parse, load, SPARQL eval, serialize) return
  `Result<_, RdfDiagnostic>` — the kernel's structured, SARIF-free diagnostic type
  (the single error currency; backends map their native errors into it). This keeps
  the error type concrete (object-safe-friendly) for the future erased layer.

## C7 — Capability negotiation

Views advertise representational support via `RdfStoreCapabilities` (named graphs,
triple terms, reifiers, annotations, source locations, loss records, lookaside).
The backend never silently drops data. A GMEOW operation that requires a missing
capability refuses explicitly; an operation whose declared DAG path does not use that
capability continues normally. This is operation selection, not an optional-dependency
fallback or a half-capable alternate implementation.

## C8 — Thread-safety

A frozen `RdfDataset` (and `TermId`/`QuadIds`) is `Send + Sync` (asserted at compile
time) so a read view can be shared across threads for parallel reasoning.
Per-handle: a `DatasetView` is `Sync` when its backing data is; mutable backends
(`DatasetMut`) are `Send` but a single handle is not concurrently mutable (`&mut self`).
Rayon-style fan-out over a shared `&RdfDataset` is sound today.

## Trait summary (SOLID-I: narrow traits, not one fat `Backend`)

| Trait | Layer | Parcel | Object-safe? |
| --- | --- | --- | --- |
| `DatasetView` | static read | **P2a** | no (RPITIT) — fine, selection is compile-time |
| `FallibleDatasetView` | operation-scoped lazy read | PurRDF paging | no; static view plus typed status/evidence |
| `DatasetMut` | static write | **P5** | no |
| `TermFactory` | interning | **P2d** | no object-safety requirement |
| `RdfParserBackend` | ingress | **P2d** over P6 events | erased only if runtime registry needed |
| `SparqlEngine` | query/update | **P2d** | no object-safety requirement |
| `RdfSerializer` | egress | **P2d** | no object-safety requirement |

The first concrete P2d adapter is `OxigraphBackend` in
[`crates/rdf/src/oxigraph/backend.rs`](../../crates/rdf/src/oxigraph/backend.rs).
It implements parser ingress over `gmeow-rdf-events`, SPARQL query/update, and
serializer egress without exposing oxigraph types through `gmeow-rdf-core`.

<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# purrdf backend contract (P2)

Companion to [`PURRDF-PLAN.md`](./PURRDF-PLAN.md). The PLAN requires this contract to
be **specified before implementing P2** — the answers here decide the trait shapes,
in particular whether the erased (`&mut dyn`) layer is mandatory.

This document is normative for the purrdf backend traits (`DatasetView`, and the
later `DatasetMut` / `RdfParserBackend` / `SparqlEngine` / `RdfSerializer` /
`TermFactory`). The first trait it governs — [`DatasetView`](../../crates/rdf/src/dataset_view.rs) —
lands in P2a (#836); the rest land with P2b–P2d and P5/P6.

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
  `rdf_quad_from_oxigraph` double-tax of #819).
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
scan; P4 (#838) overrides it with the lazy access-pattern indexes.

## C4 — Write side (DatasetMut): deferred to P2c/P5

`DatasetMut` (insert/remove, `Base`/`Delta` COW handles) is P5 substrate (#839, needs
P2 + P4). SPARQL **UPDATE** and **transactions** are a backend concern (oxigraph
provides them); the contract: UPDATE/transaction APIs live on the mutable/SPARQL
backend traits (P2d/P5), never on `DatasetView` (read-only), and an UPDATE is atomic
per the backend's transaction semantics (oxigraph: serializable in-memory).

## C5 — Cancellation

Long-running backend operations (a SPARQL query, a bulk load) accept cooperative
cancellation through the iterator-drop / early-`break` model: dropping the cursor
stops the work. No separate cancellation token is introduced in P2a (the read view's
iterators are pull-based and stop when dropped). A token-based API is added only if a
backend needs to cancel work that is already in flight off-thread (not the case for
the in-memory oxigraph backend).

## C6 — Error model

- The read view is **infallible** for a frozen, validated `RdfDataset`: `quads`,
  `quad_refs`, `resolve`, `quads_for_pattern` cannot fail (validation happened at
  freeze, #819). They therefore return values/iterators, not `Result`.
- Fallible backend operations (parse, load, SPARQL eval, serialize) return
  `Result<_, RdfDiagnostic>` — the kernel's structured, SARIF-free diagnostic type
  (the single error currency; backends map their native errors into it). This keeps
  the error type concrete (object-safe-friendly) for the future erased layer.

## C7 — Capability negotiation

Backends advertise support via `RdfStoreCapabilities` (named graphs, quoted triples,
reifiers, annotations, source locations, loss records, lookaside). Consumers query
`capabilities()` and degrade *their own* behavior (e.g. skip reifier emission) rather
than the backend silently dropping data. New capabilities are added to the struct
additively (it is `#[non_exhaustive]`-eligible if external backends appear; today it
is kernel-internal).

## C8 — Thread-safety

A frozen `RdfDataset` (and `TermId`/`QuadIds`) is `Send + Sync` (asserted at compile
time, #841) so a read view can be shared across threads for parallel reasoning.
Per-handle: a `DatasetView` is `Sync` when its backing data is; mutable backends
(`DatasetMut`) are `Send` but a single handle is not concurrently mutable (`&mut self`).
Rayon-style fan-out over a shared `&RdfDataset` is sound today.

## Trait summary (SOLID-I: narrow traits, not one fat `Backend`)

| Trait | Layer | Parcel | Object-safe? |
|---|---|---|---|
| `DatasetView` | static read | **P2a (#836)** | no (RPITIT) — fine, selection is compile-time |
| `DatasetMut` | static write | P5 (#839) | no |
| `TermFactory` | interning | P2d | tbd |
| `RdfParserBackend` | ingress | P2d/P6 | erased only if runtime registry needed |
| `SparqlEngine` | query | P2d | — |
| `RdfSerializer` | egress | P2d | — |

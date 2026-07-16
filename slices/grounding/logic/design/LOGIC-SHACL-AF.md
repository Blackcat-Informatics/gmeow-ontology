<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Computation as a Projection: the SHACL-AF map/reduce surface

> The **computation-surface** chapter of the GMEOW Logic design set: how derivation
> and aggregation — the "computation layer" others propose bolting *onto* SHACL — are
> authored once as canonical `logic:` rules and **projected** to a SHACL Advanced
> Features (SHACL-AF) rule surface, never added to the constraint language directly.
> It is the inward case of the doctrine stated once in [`LOGIC.md`](LOGIC.md) and
> generalized in [`LOGIC-META-SEMANTICS.md`](LOGIC-META-SEMANTICS.md): every prior
> formalism — OWL, Datalog, N3, SPARQL, **and SHACL** — is a generated lossy
> projection of the Turing-complete canon, so power is added to the canon and emitted
> to the surface. It is the computation peer of the traversal projection in
> [`LOGIC-PATHS.md`](LOGIC-PATHS.md) and the alignment projection in
> [`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md).
>
> **Reading this document.** The declarative present tense is normative: "X is" means a
> conforming realization implements X, established by the loss ledger and the
> conformance corpus ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md)). It is not a claim
> that any implementation realizes more than the corpus demonstrates.

## The proposal this inverts

A recurring proposal — most recently the "RDF needs a computation layer" argument —
is to make RDF compute by **extending SHACL** with a map/reduce pipeline: push
aggregation operators into SHACL-AF, treat the constraint language as the place where
derivation lives, and let a SHACL engine be the executor of record. The same move is
proposed for query (see [`LOGIC-RDFQUERY.md`](LOGIC-RDFQUERY.md)).

GMEOW's position is the exact inverse, and it is not a matter of taste — it follows
from Constitution **Principle 17** (the logic is canonical; OWL/Datalog/SHACL/SPARQL
are projections of it), **Principle 4** (one canonical source; everything else a
generated lossy projection), and **Principle 12** (compute outside the solver, but the
*expressivity ceiling* is set by the canon, not by a downstream surface). Bolting
computation onto SHACL would:

- make a **constraint** language the home of **derivation**, conflating two notions the
  logic set keeps distinct (a `constraint` is not a `derivation-rule`);
- cap expressivity at what SHACL-AF's SPARQL fragment can say, rather than at what the
  Turing-complete canon can say, and then have no honest account of what was lost;
- create a **second source of truth** — computation authored on the SHACL surface that
  no reasoner backs and no loss ledger governs.

So GMEOW does not add map/reduce to SHACL. It authors derivation in `logic:` — which
already has rules, recursion, stratified negation, and a reasoner behind them — and
**projects** a SHACL-AF rule surface for any consumer that wants the computation
expressed in standard SHACL. The projection is generated, drift-gated, and carries a
preservation judgment at the boundary.

## What "map" and "reduce" are in the canon

The canon does not gain a new "map/reduce" construct; it already has the two halves.

- **Map** is a `logic:` **derivation rule**: for every binding of its body it derives
  its head. A least-model evaluation of a rule over a graph *is* a map over the matched
  subgraphs — this is exactly what the native materializer and the Datalog/N3
  projections already do with `program.rules`. No new vocabulary is minted for it.
- **Reduce** is **aggregation**: a derivation whose head is a function (count, sum,
  extremum) of a *group* of body bindings. The relational-core dialect's declared
  shape is `Datalog± + stratified ¬ + aggregation + existentials`
  ([`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md)); aggregation is a body form, not a separate
  paradigm, and it stays in the canon where a reasoner can evaluate it.

The "pipeline" in the external proposal — a staged transform of one dataset into
another — is, in GMEOW, the typed build DAG (`crates/pipeline`) whose stages each
transform the in-memory carrier; that pipeline is itself governed by the canonical
process model. The SHACL-AF surface is not where a pipeline *runs*; it is one of the
forms a pipeline *step* can be **projected to** when a SHACL consumer needs it.

## The SHACL-AF projection

A `logic:` derivation rule projects to a SHACL-AF **rule shape**, the standard SHACL
construct for "derive new triples," distinct from the `sh:sparql` **constraint**
shapes the result/frame projections emit (those validate; these derive):

- The rule's matched class (or a `sh:SPARQLTarget` when the trigger is value-keyed)
  becomes the shape's target.
- The rule body + head become an `sh:rule` of kind `sh:SPARQLRule` carrying an
  `sh:construct`: the SPARQL `CONSTRUCT { head } WHERE { body }` lowering of the rule.
  The body/head lowering is the **same** rule-to-relational lowering the Datalog and
  N3 targets use ([`LOGIC-IR.md`](LOGIC-IR.md)); there is one lowering, reused, so the
  surfaces cannot drift from each other.
- A **reduce** rule lowers to an aggregating `CONSTRUCT` — `GROUP BY` over the group
  key, the aggregate function in the projection — inside the same `sh:SPARQLRule`. The
  map case is the non-aggregating special case of the same projector.

The projection is **not** the inverse of constraint validation and never re-expresses a
`logic:` constraint as a rule: derivation and constraint are kept apart on the SHACL
surface exactly as they are in the canon. The constraint half — the closed-world
data-shape validation the result/frame projections emit as `sh:sparql`, and the
`logic:` **validation shape** node kind it generalizes — is the subject of its peer
document, [`LOGIC-VALIDATION.md`](LOGIC-VALIDATION.md), which projects that node kind to a
SHACL Core surface and a ShEx surface under the same emit-only, ledgered, purity-gated
doctrine this document applies to rules.

### Where the loss is

The SHACL-AF surface is an **execution dialect of a fixed fragment**, so the projection
declares its preservation in the loss ledger
([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md), `target_meta`), never silently:

- The faithfully projectable fragment is the **stratified Horn-with-aggregation**
  fragment a SHACL-AF SPARQL rule can carry. Within it the projection is sound: every
  triple the SHACL-AF rule derives is a triple the canonical rule derives.
- **Full first-order** rule bodies (genuine quantifier/connective trees), **default
  negation** beyond a stratified guard, **existential** (value-inventing) heads, and
  the **modal / world / standpoint** context of a contextualized rule have no faithful
  SHACL-AF rule form. They are **carried and flagged** in the canonical layer (lossless
  under the canonical RDF-1.2 projection, reachable for execution through the
  relational-core lowering), and recorded as ledgered drops on the SHACL-AF target —
  never dropped in silence, exactly as the OWL/Datalog targets record their drops.
- The preservation kind is therefore `SoundUnderApproximation`: the SHACL-AF rule
  surface computes a sound subset, and the residue is disclosed, not lost.

The asymmetry is stated, not hidden, in the LOGIC-PATHS sense: the canon authors the
rule; the SHACL-AF surface is **emit-only** today — there is no parse-back from
`sh:SPARQLRule` into a `logic:` rule, because the canon, not SHACL, is the authoring
ground (Principle 4). A reader must not infer a round-trip the doctrine deliberately
does not provide.

## Verified by construction, and the placement rule

The emitted SHACL-AF surface is a generated artifact under `generated/`, regenerated by
the single-pass pipeline and proven drift-free by strict `sync` (Principle 7): the
committed bytes equal the bytes the projector produces, or the build is red. It rides
the in-memory carrier into `gmeow.gts`, so a repo-free consumer reads the computation
surface without re-running the compiler (maximal information flow).

The placement rule is the **Hybrid convention** the foundation already uses: computation
is authored as `logic:` rules; a SHACL-AF computational construct (`sh:rule` /
`sh:SPARQLRule`) that appears anywhere in the authored sources (`slices/`, `dsl/`)
rather than under `generated/` is either a hand-authored second source of truth — which
is forbidden — or it carries a `logic:formalizes` back-reference naming the `logic:`
source it is the projection of. The static seal that enforces this is part of the
computation-projection-purity gate in [`governance/constitution.ttl`](../../../../governance/constitution.ttl).

## A named consumer (P15)

A projection earns its existence by a consumer (Principle 15). The SHACL-AF rule surface
serves a consumer that already runs a SHACL-AF engine — a validation/derivation
toolchain that wants GMEOW's derivations expressed as standard SHACL rules it can
execute in place, rather than adopting the `logic:` runtime. For that consumer the
surface is the point of contact; for GMEOW it remains a projection, peer to the
OWL/Datalog/gUFO projections, never a second source of truth.

## Where this sits

| Concern | Document |
|---|---|
| The projection doctrine these surfaces obey | [`LOGIC.md`](LOGIC.md), [`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md), [`LOGIC-META-SEMANTICS.md`](LOGIC-META-SEMANTICS.md) |
| The typed IR a rule compiles through, and the one rule lowering reused here | [`LOGIC-IR.md`](LOGIC-IR.md) |
| The loss ledger and preservation kinds | [`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md) |
| The relational-core dialect (Datalog± + stratified ¬ + aggregation) and the runtime | [`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md) |
| The query-surface sibling of this computation surface | [`LOGIC-RDFQUERY.md`](LOGIC-RDFQUERY.md) |
| The traversal and alignment projections that obey the same rule | [`LOGIC-PATHS.md`](LOGIC-PATHS.md), [`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md) |

The vocabulary the rules are authored in is in [`../module.ttl`](../module.ttl); the
generated SHACL-AF rule surface is a projection of those rules, written by the pipeline
and proven drift-free by the generated-artifact gate.

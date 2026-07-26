<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — The Projection and Alignment Contract

> The **projection charter** of the GMEOW Mathematics design set: the generated lossy lowerings of
> every mathematical, probabilistic, and statistical artifact, and the loss ledger that makes their
> preservation checkable. It makes precise the manifesto's thesis ([`MATHEMATICS.md`](MATHEMATICS.md))
> that external vocabularies are targets and reference surfaces, never canonical sources. It closes
> the design set opened by the mathematical core
> ([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md)), the probability layer
> ([`MATHEMATICS-PROBABILITY.md`](MATHEMATICS-PROBABILITY.md)), and the statistics layer
> ([`MATHEMATICS-STATISTICS.md`](MATHEMATICS-STATISTICS.md)).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's canonical `module.ttl` axioms and `logic:Constraint` records, competency queries, and the
> projection loss ledger.

## Purpose

The mathematics slice preserves a hard distinction between *source fidelity* and *consumer
projections*. GMEOW is the richer local source of truth; every external vocabulary is a generated,
lossy view of it. This charter names each consumer, states what the GMEOW canonical source is for
that consumer, states what the projection loses, and binds the whole to the same loss-ledger
discipline `logic:` applies to OWL, Datalog, SHACL, and the correspondence lowerings
(`slices/grounding/logic/design/LOGIC-CORRESPONDENCE.md`).

## Shipped grounding laws versus projections

External term grounding is ontology content, not a disposable output view. The
canonical `mappings/equivalences.ttl` catalog (including symbol-level OpenMath
correspondences), the logic-owned SUMO catalog's broad Quantity bridge, the six-row
`mappings/quantity-bridges.ttl` catalog, and the 13-row
`mappings/statistical-bridges.ttl` catalog compile to content-addressed
`logic:GroundingCorrespondence` records in the shipped
`graph/correspondence-laws` graph. The quantity catalog owns the direct SOSA,
OM 1.8, IVOA ObsCore, LOINC, and QUDT bridges for `math:Quantity` and
`math:quantityValue`; downstream observation slices retain their domain roles
and qualifiers but do not re-author those terms. The observation-facing rows
still fan out into `gmeow-observations.sssom.tsv` as a generated consumer-output
grouping; that derived grouping does not transfer canonical ownership out of
`math:`. The statistical catalog covers
RDF Data Cube, STATO, OBCS, SIO, and OBI terms and is validation-only; no target
TBox enters the mathematical closure. The OBI data-transformation row is deliberately only
`skos:relatedMatch`: OBI_0200000 denotes an executed planned process, whereas
`math:DataTransformation` may denote a mathematical transformation specification.

By contrast, a cube document, MathML tree, OpenMath payload, D-SI certificate,
or other consumer serialization is a generated codec/projection with an
explicit loss judgment. A target may participate in both roles—one narrow
term bridge and one lossy document projection—but the two records are never
conflated.

## The canonical/consumer table

| Consumer | GMEOW canonical source | Projection loss |
|---|---|---|
| MathML | Expression AST + notation/rendering context | provenance, theory context, binding metadata, semantic type constraints |
| OpenMath-like content | Expression AST + symbol references | GMEOW claim/provenance/standpoint frame |
| RDF Data Cube | Statistical result/model/data structures | model assumptions, proof/provenance, inference method, parameterization detail |
| STATO-style export | Statistical method/test/model references | GMEOW expression/probability/dependency detail |
| QUDT | Quantity/unit/dimension references | claim/provenance/result context |
| JSON / Pydantic / Python surface | Typed GMEOW DTOs | must expose the loss ledger when simplified |

The rule that governs every row:

> **Any projection that drops assumptions, parameterization, dependency structure, expression
> binding, or provenance emits a machine-readable preservation/loss record.**

## Alignment is by reference, not import

External vocabularies are aligned by reference and projected to, never copied into the canon. The
mechanism is the repository's established one, not a bespoke predicate: an external link is a
native RDF-1.2 alignment cell — a reified `S skos:*Match O {| gmeow:sssomFile …; gmeow:justification
…; gmeow:confidence … |}` statement — in a canonical file under the slice's `mappings/`,
marked `logic:GroundingCorrespondence`, and lowered as a shipped
`logic:Correspondence` — the ninth `logic:` IR node kind. There is **no**
free-standing `authorityLink` property in the mathematics slice; a Wikidata QID, a QUDT unit IRI, or
an OpenMath symbol is a `skos:exactMatch`/`skos:closeMatch` alignment carrying its preservation
judgment in the loss ledger. This keeps the mathematics slice consistent with how every other GMEOW
slice records external identity, and makes "GMEOW subsumes vocabulary V" a checkable
section/retraction law rather than a claim.

OpenMath targets use the official content-dictionary symbol IRIs (for example
`http://www.openmath.org/cd/arith1#plus` and
`http://www.openmath.org/cd/limit1#limit`), not an HTML dictionary page standing
in for a symbol. A page-level `skos:relatedMatch` remains only where the GMEOW
class spans several symbols and no unique symbol target is honest (`math:Interval`
and `math:SpecialFunction`).

## Per-target contracts

### QUDT — units and quantity kinds

QUDT is the reference for units, quantity kinds, dimensions, dimensional analysis, and unit
conversion. GMEOW references QUDT IRIs from the quantities held in the observations spine and
projects where useful; it does **not** import QUDT axioms unless the project's import policy
explicitly permits it. The projection to a QUDT-shaped quantity loses the claim, provenance, and
result context that the GMEOW observation carries.

### STATO / OBCS — statistical methods

STATO and adjacent OBO-family resources are the reference for statistical tests, methods, variables,
model terms, and assumptions where mappings are clear. STATO is an alignment/reference target, not
the canonical schema, and its expressivity ceiling is not GMEOW's: the export to a STATO-style
method reference loses the GMEOW expression, probability, and dependency detail that STATO cannot
carry.

### RDF Data Cube / SDMX / DDI — statistical cubes

RDF Data Cube and SDMX-like structures are projection targets for multidimensional statistical
datasets and results. GMEOW retains the richer model, provenance, assumption, and probability
structure and exports a declared-loss cube when a consumer needs one. The cube loses model
assumptions, proof and provenance, inference method, and distribution/parameterization detail; the
loss record names each dropped construct.

### MathML — notation

MathML (presentation and content) is a projection surface for notation. A MathML string or tree is
**not** canonical identity unless a formula was ingested only at that fidelity and marked as such;
canonical computable content is the GMEOW expression AST
([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md)). The projection loses provenance, theory
context, binding metadata, and semantic type constraints.

### OpenMath / OMDoc / MMT — symbols and theories

OpenMath symbol/content dictionaries and OMDoc/MMT theory and module references are alignment and
projection candidates. Their role is symbol-identity reference, theory/context reference, projection
surface, and — only after licensing and semantic review — possible import source. The content
export loses the GMEOW claim, provenance, and standpoint frame.

### ProbOnto / Distributome — distribution catalogs

Distribution catalogs are references for distribution families, parameters, relationships, and
reparameterizations, adopted only after license and maintenance review. GMEOW stores its own local
distribution-family individuals for high-value families with explicit external links; it does not
let a catalog's parameter conventions become implicit defaults (the mandatory-parameterization gate
in [`MATHEMATICS-PROBABILITY.md`](MATHEMATICS-PROBABILITY.md) forbids exactly that).

### Wikidata — named-concept identity

Wikidata QIDs are authority links for named mathematical concepts, theorems, distributions,
constants, and structures where identifiers exist. Wikidata is an authority link, not a definition
source; the GMEOW term remains the definition and the local source of truth.

## The loss ledger

Every lossy transformation produces a machine-readable preservation/loss record. The ledger is the
mathematical instance of the project-wide preservation contract, and it **reuses the existing
`logic:` preservation vocabulary verbatim** rather than minting near-synonyms — so mathematics loss
is queryable in the same ledger as the OWL, Datalog, SHACL, and correspondence lowerings. Each
projection declares its unsupported constructs and a `logic:preservationKind`:

| Design-set prose | Canonical `logic:` term |
|---|---|
| "exact" | `logic:ExactPreservation` |
| "sound but incomplete" | `logic:SoundUnderApproximation` |
| "complete but unsound" | `logic:CompleteOverApproximation` |
| "lossy with named drops" | a non-exact preservation with enumerated `unsupportedConstruct` entries — **not** a distinct polarity |

A mathematics projection is therefore a `logic:Correspondence` lowering carrying a
`logic:preservationKind` and, where lossy, its unsupported constructs, exactly as the correspondence
calculus specifies (`slices/grounding/logic/design/LOGIC-CORRESPONDENCE.md`). The full rule→gate→failure
mapping is in [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md).

## Inbound contracts — lifting external artifacts

Projection is not only outbound. `gmeow transpile` and the ingest lifters
([`MATHEMATICS-RUNTIME.md`](MATHEMATICS-RUNTIME.md)) move external artifacts *into* the canon, and
each target has an inbound contract that is deliberately conservative — a lift never pretends an
external artifact carries more than it does, and it **refuses** rather than fabricating structure.

| Target | Ingest result | Refusal condition |
|---|---|---|
| MathML presentation | `math:ExpressionRendering` only, unless the presentation is unambiguously parseable | no AST identity claim when content semantics are missing |
| MathML content | AST draft with `math:parseSource` provenance | unsupported binders/operators |
| OpenMath | symbol-aligned AST draft | unresolved content-dictionary symbol |
| RDF Data Cube | statistical-dataset draft | lost model/provenance marked, not silently filled |
| STATO | method/test alignment draft | insufficient expression semantics to anchor the method |
| QUDT | unit/dimension links | unclassified unit or incompatible dimension |

The asymmetry is intentional: an outbound projection *may* drop structure (recording the loss); an
inbound lift may **not** invent structure. A MathML presentation string that cannot be
unambiguously parsed becomes a rendering, never a fabricated AST — the same no-silent-fallback
posture the runtime charter enforces on ingestion.

## Projection gates

The projection gates, verbatim to the manifesto's doctrine:

- Every projection declares its unsupported constructs.
- Every projection declares its preservation polarity.
- No projection silently converts confidence to probability.
- No projection silently drops distribution parameterization.
- No projection silently flattens an expression AST to a string without recording the loss.

## Competency questions

The projection layer is accepted only when it can answer these structurally:

1. What projection loss occurs when a given statistical result is exported to RDF Data Cube?
2. Which constructs of a given expression are unsupported by its MathML projection?
3. What is the preservation polarity of a given external alignment, and by which
   `logic:Correspondence` is it carried?
4. Which projections, if any, dropped distribution parameterization or converted confidence to
   probability — and are those drops recorded in the loss ledger?

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Citations — who cites what, why, and exactly where

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/citations` · **tier: core**
> The universal citation, credit, and attribution facility: every "see source X" in the model is one of these.

Citation, credit, and attribution — the universal CitationAct relator, Contribution degree, and selector vocabulary. Replaces the old sources.ttl evidence link with a typed, reified citation relator and an anti-rigid SourceRole. Bridges to CiTO, CRediT, PROV-O, and PAV by reference (Principle 5).

A citation is not a bare edge — it is an *act*, with an author, an intent, and a pinpoint.
The slice is flat-first: `gmeow:cites` covers the 80 % case, and the machine-readable
`gmeow:pairsWith` assertion (`cites` ↔ `CitationAct`) tells tooling exactly which relator
to promote to when intent, selector, provenance, or standpoint must be recorded. The
*why* of a citation is a value from the open, CiTO-shaped `gmeow:CitationIntent`
vocabulary (Principle 9 — individuals, never subclasses), and the *where* is a
`gmeow:Selector` — a pinpoint that specializes the evidence-span mechanism rather than
reinventing it. This is also the most aggressively dogfooded slice in GMEOW: the
project's own credit model and CITATION.cff projection ride this machinery (source/claim refactor).

The rubrics facility (core/norms) extends the relator rather than forking it:
`gmeow:Exemplar` is a SubKind of `gmeow:CitationAct` — a citation with polarity that
holds the cited span up as a positive, negative, or cautionary example — so the CiTO
alignment arrives there for free.

## The citation relator

### gmeow:CitationAct

The reified citation: a citing entity cites a cited creative work with a typed intent,
optionally via one or more selectors, carrying provenance, confidence, and standpoint.
The promoted form of `gmeow:cites` (`gmeow:pairsWith`). Open-world EL restrictions
require some citing entity, cited entity, and intent; closed-world cardinality is
SHACL's. Genealogical evidence is a CitationAct with `gmeow:intentCitesAsDataSource`
toward a work, via a selector — the typed replacement for the old hasSource link.

### gmeow:cites

The flat 80 %-case shortcut from any `gmeow:Entity` to a `gmeow:CreativeWork`.
Non-functional. Promote to a `gmeow:CitationAct` the moment intent, pinpoint,
provenance, or standpoint matters — never bolt those onto the flat triple.

### gmeow:citingEntity · gmeow:citedEntity

The two functional posts of the relator: one citing entity (any Entity — a claim, a
module, a dataset) and one cited entity (a `gmeow:CreativeWork` — any WEMI tier) per
CitationAct. Several citations to the same work are several acts, each with its own
intent and selector.

### gmeow:citationIntent · gmeow:CitationIntent

The typed *why* — a functional pointer into an open value vocabulary bridged by
reference to CiTO sub-properties (`cito:citesAsDataSource` and kin, Principle 5). Seeds
run from `gmeow:intentCitesAsDataSource` through `gmeow:intentDisagreesWith` to
`gmeow:intentBridgedByReference` — the last being the intent GMEOW itself uses to record
its own Principle-5 alignments. A new intent is a fresh individual, never a subclass.

### gmeow:viaSelector · gmeow:Selector

The pinpoint into the cited work: page (`gmeow:selectorPage`), character position,
verbatim quote, or generic locator. Non-functional — one citation may span several
pages. `Selector` is a specialization of `gmeow:EvidenceSpan` (EvidenceSpan audit machinery) and is reused
by the annotation target span (annotation target span); it replaces the retired `gmeow:Citation`
class that collided with scholarly citation.

## Credit and source-hood

### gmeow:ContributionDegree

The weight of a contribution — an open value vocabulary (Principle 9) characterizing
the universal `gmeow:Contribution` relator: `gmeow:degreeLead`, `gmeow:degreeEqual`,
`gmeow:degreeSupporting` as seeds, CRediT-shaped, projected to CITATION.cff and
CrossRef contributor metadata (source/claim refactor).

### gmeow:SourceRole

The anti-rigid RoleMixin of *being a source*: a CreativeWork is only contingently a
source — evidence for one claim, subject of another, a work in its own right
throughout. Source-hood is primarily relator-mediated via CitationAct; SourceRole is
the named handle for the rare case that needs one. Nothing is a source by kind.

### gmeow:references · gmeow:isReferencedBy

The generic flat Entity→Entity reference pair, gathered here by the dependency surgery
of slice-dependency doctrine: one definition serves RFC 5322 message threading and bibliographic
referencing alike. The flat-first companion of the typed machinery above — promote when
kind, locus, or standpoint matters.

## Solver layer & deferred alignment

Citation *analytics* — counting, ranking, co-citation clustering, credit rollups — are
solver-layer computations (Principle 12): the slice records the acts; it never asserts
derived metrics as facts. CiTO, CRediT, PROV-O, and PAV remain bridges by reference,
never axiom imports (Principle 5); the rubrics-facility projections (EARL, DQV,
schema.org Rating) arrive free through `Exemplar ⊑ CitationAct` and are deferred to the
compiler-arc window.

## Dependencies

Depends on `kernel`, `documents` (the CreativeWork range), and `evidence` (the
EvidenceSpan that Selector specializes). Consumed by the citation/credit dogfood
(source/claim refactor), slice manifests, and the norms slice's Exemplar.

## Shared page and generic locator properties

`selectorPage` and `selectorLocator` belong to the shared `EvidenceSpan` layer.
Their canonical `logic:domain` is `EvidenceSpan`, with the existing literal
ranges preserved. A generic documentary page span or structural locator does
not become a citation Selector simply because it uses one of these properties.
An explicitly typed `Selector` remains an EvidenceSpan and keeps its citation
meaning through the unchanged `viaSelector` relation from an actual `CitationAct`
(Principles 4, 9 and 17).

Keep the complete original RDF literal on the span. A page number, label, index
or range remains a source-control descriptor without index-base conversion,
counting, range decoding, normalization or resolved page-resource identity.
A generic locator retains its actual source wording or typed descriptor without
an inferred immutable revision or source authentication. Neither shared property
acquires global functionality, a maximum, requiredness or a closed-world
provided-type policy from this repair.

Documents’ [source-locus vocabulary](../documents/docs.md) reuses `selectorPage`
directly. Its exact href role specializes `selectorLocator`, so the original
literal object follows the generic relation by ordinary subproperty entailment.
It does not need a second authored locator value, href resolution, invented
base URI or datatype cast. Known source-region geometry uses framed Math
quantities on the actual EvidenceSpan, rather than being hidden in a locator
string.

This repair changes only these two shared domains and their complete localized
coats. Selector, CitationAct, viaSelector, selectorTextQuote and
selectorTextPosition retain their existing contracts. The generic page/href
path makes no claim that other selector-field authoring has been migrated.
Existing explicit citation selectors and source-reference literals remain valid.

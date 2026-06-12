<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Evidence — warrant and notability, two axes that never bridge

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/evidence` · **tier: core**
> The evidential substrate of the citation link: how strong the evidence is, and — separately — whether it makes anything notable.

No SOTA vocabulary unifies evidential warrant with source-independence typing
(Principle 1), so GMEOW does, as **two orthogonal axes that travel with the evidence
link** (`CitationAct` / `EvidenceSpan`), never with the asserted fact. **Axis A —
evidential warrant**: the kind and strength of evidence for a claim, an open value
vocabulary (`EvidenceClass`) reached non-functionally — a multi-source claim legitimately
carries several evidence classes at once (Principle 9). **Axis B — source independence
and coverage typing**: four facets on the citation that feed *notability* assessment, not
truth. The axes are explicitly orthogonal: a primary legal filing may verify a fact
beyond doubt (high warrant) while establishing no notability at all
(`supportsNotability false`). Bridged by reference to CRMinf, PROV-O, schema.org, C2PA,
DataCite, nanopublications, and WP:GNG (Principle 5).

On the claim spine (Source → Chunk → EvidenceSpan → Claim, Principle 14) this slice owns
the **EvidenceSpan** anchor — the issue #55 link from a claim back into the exact span of
its source — and the warrant facets that ride on each citation edge. Weak evidence is
recorded, never deleted: rumour-tier claims are suppressed by projection (Principle 10),
and every eligibility decision is the solver's, not the reasoner's (Principle 12).

## The spine anchor

### gmeow:EvidenceSpan

An anchored target span within a resource — text quote, character position, fragment
identifier, page, or generic locator. Generalised from the citation-selector model to
serve both evidentiary claims (issues #54/#55) and annotation targets (issue #63): the
citations slice's `Selector` specialises it, the notes slice's annotation targets reuse
it, and no second selector model is ever minted (Principle 4). Re-homed to core in the
issue #287 dependency surgery because the spine anchor *is* core evidence machinery.

## Axis A — evidential warrant

### gmeow:EvidenceClass

The kind and strength of evidence supporting a claim — a value vocabulary (individuals,
never subclasses), and open: a new evidence kind is data, not a schema change. Coarse
tiers (`evidenceVERIFIED`, `evidenceSELF`, `evidenceANECDOTAL`, `evidenceRUMOR`) coexist
with fine refinements (legal filing, public registry, independent trade press, OCR
extract, family narrative, source-code archive, private correspondence…). Note the
Principle 9 inflection in `evidenceSELF`: self-assertion is *top* authority for the
subject's own standpoint while remaining *low* warrant for third-party verification —
two different questions, both answered honestly.

### gmeow:hasEvidenceClass

The warrant facet on a `CitationAct`. Non-functional by doctrine: one citation may be
both a legal filing and the trade-press coverage of that filing, and competing
classifications coexist rather than collapse (Principle 9).

## Axis B — independence & coverage (the notability axis)

### gmeow:sourceIndependence

Whether the cited source is editorially and financially independent of its subject, or
self/issuer-originated (press releases, self-published sites, the subject's own
filings). Range is the `SourceIndependence` value vocabulary. About notability
eligibility, never factual truth — competing assessments from different standpoints
coexist (Principle 9).

### gmeow:sourceTier

The standard bibliographic tier — `sourceTierPrimary` / `sourceTierSecondary` /
`sourceTierTertiary` (`SourceTier` vocabulary, cf. WP:GNG). A value vocabulary naming an
evidentiary reality, not a selector privileging one co-equal claim. Non-functional:
tier assessments are themselves contestable.

### gmeow:coverageDepth

How deeply the source treats the subject — `coverageDepthSignificantCoverage`,
`coverageDepthPassingMention`, or `coverageDepthRoutineFiling` (`CoverageDepth`
vocabulary). Orthogonal to tier and independence: a secondary independent source can
still mention the subject only in a list.

### gmeow:supportsNotability

The explicit boolean assertion that *this* citation is offered as notability support, as
distinct from factual verification. The keystone of the two-axis doctrine: warrant and
notability never bridge automatically — a citation must be *claimed* as notability
evidence, and competing notability assessments coexist (Principle 9).

## Open-world policy & solver boundary

No existential restrictions are asserted on `CitationAct`: every facet here is an
optional annotation on the evidence link, with closed-world enforcement left to SHACL at
instance-validation time (Principles 7–8). And the question the axes exist to answer —
"is this subject notable?", "is this claim adequately evidenced?" — is **never** answered
in OWL: eligibility folds over independence × tier × depth × warrant are projection-time
solver policy (Principle 12). The graph records the facets; the consumer's policy decides.

## Dependencies

Depends on `citations` (the `CitationAct` the facets attach to) and `kernel`. Consumed by
the claim spine's EvidenceSpan (issue #55), citation warrant in every sourced slice, and
the deception analyses' evidence grading.

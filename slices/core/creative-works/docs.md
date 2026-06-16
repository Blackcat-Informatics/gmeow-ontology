<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Creative-Works Mapping Guide

This document describes the WEMI (Work / Expression / Manifestation / Item) spine
introduced in WEMI spine, its design rationale, and its mappings to external
vocabularies.

## The WEMI spine in GMEOW

GMEOW reconstructs the four-tier FRBR/LRMoo distinction using native primitives:

| Tier | Class | Identity criterion | gUFO meta-class |
|---|---|---|---|
| **Work** | `gmeow:Work` | Abstract intellectual content | `gufo:Kind` |
| **Expression** | `gmeow:Expression` | Language/notation/arrangement | `gufo:Kind` |
| **Manifestation** | `gmeow:Manifestation` | Edition/format/release | `gufo:Kind` |
| **Item** | `gmeow:Item` | Single exemplar | `gufo:Kind` |

`gmeow:CreativeWork` is the `gufo:Category` umbrella over all four tiers. It
replaces the former flat `gufo:Kind` (pre-WEMI spine) so that surface-vocabulary
alignments become projections from the umbrella rather than core equivalences.

### Key design decisions

- **Expression variance by reference frame** (`gmeow:hasReferenceFrame`): A
translation is the same Work realized in another language frame; a musical
arrangement is the Work in another notation frame. No subclass taxonomy
explosion (Principle 9).
- **Creation as reified Events** (`gmeow:eventType` values): Work conception,
Expression creation, and Manifestation production are event types, not Event
subclasses (Principle 12).
- **Contribution relator + flat shortcuts**: `gmeow:Contribution` binds
contributor × target × role with provenance/period/confidence; flat shortcuts
(`hasAuthor`, `hasTranslator`, …) cover the 80% case.
- **Open value vocabularies**: `CreativeWorkType`, `ContributionRole`,
`ManifestationFormat`, and `CarrierMedium` are individuals, never subclasses
(Principle 9).
- **No primary/privileged claim**: Co-equal multilingual titles, no canonical
edition. Superseded values use `gmeow:displayable false` (Principles 9–10).

## External mappings

### FRBRcore (equivalence by reference)

`gmeow:Work/Expression/Manifestation/Item` are `owl:equivalentClass` to their
FRBRcore counterparts. The spine relations (`realizes`, `embodies`, `exemplifies`)
are `owl:equivalentProperty` to FRBR R-relations.

### FaBiO / SPAR ontologies (closeMatch)

FaBiO's WEMI classes are scholarly-publication-scoped; GMEOW's are universal.
Mapped as `skos:closeMatch` with confidence 0.95.

### LRMoo (equivalence by reference)

`gmeow:Work` ↔ `lrmoo:F1_Work`, `Expression` ↔ `F2`, `Manifestation` ↔ `F3`,
`Item` ↔ `F5`. Creation event types map to `F27` and `F28` as `skos:closeMatch`.

### BIBFRAME 2.0

`Manifestation` ↔ `bf:Instance` and `Item` ↔ `bf:Item` are `owl:equivalentClass`.
`Work` ↔ `bf:Work` is `skos:closeMatch` (lossy: BIBFRAME conflates Work+Expression).

### schema.org (lossy projection)

`CreativeWork` → `schema:CreativeWork` (`skos:closeMatch`, confidence 0.8).
The WEMI spine collapses to one flat node in schema.org; relations flatten to
`workExample` / `exampleOfWork`. `BookRelease` → `schema:Book`;
`MediaObject` → `schema:MediaObject`; `WebPage` → `schema:WebPage`.

### Wikidata

`Work` → `wd:Q386724` (*work*). `Manifestation` → `wd:Q3331189`
(*version, edition or translation*) as `skos:broadMatch` (lossy: muddles
edition and translation). CreativeWorkType seeds map to specific Wikidata items
(Q571 *book*, Q7725634 *literary work*, Q47461344 *written work*, etc.).

## Refactor impact on existing classes

The former flat `documents.ttl` classes were re-homed:

- **Work specializations**: `Document`, `Article`, `Patent`, `Dataset`,
  `SoftwareProject`
- **Manifestation specializations**: `BookRelease`, `SerialInstallment`,
  `MediaObject`, `WebPage`

`CreativeWorkTitle`, `hasTitle`, `title`, `identifier`, and `datePublished`
retain their domains (now the `CreativeWork` umbrella) and continue to function
for all WEMI tiers.

## Terms

The WEMI spine, its relations, the contribution relator, the derivation relator,
and the open value vocabularies this slice declares, anchored to the design above.

### gmeow:Work · gmeow:Expression · gmeow:Manifestation · gmeow:Item

The four-tier WEMI spine as native `gufo:Kind` classes: a `gmeow:Work` is abstract
intellectual content, a `gmeow:Expression` its realization in a language /
notation / arrangement frame, a `gmeow:Manifestation` an edition / format /
release, and a `gmeow:Item` a single exemplar. `gmeow:CreativeWork` is the umbrella
over all four.

### gmeow:realizes · gmeow:embodies · gmeow:exemplifies · gmeow:realizedThrough · gmeow:embodiedIn · gmeow:exemplifiedBy

The tier-binding relations (FRBR R-relations by equivalence): `gmeow:realizes`
(Expression → Work), `gmeow:embodies` (Manifestation → Expression), and
`gmeow:exemplifies` (Item → Manifestation), with `gmeow:realizedThrough`,
`gmeow:embodiedIn`, and `gmeow:exemplifiedBy` their inverses descending the spine.

### gmeow:CreativeWorkType · gmeow:RealizationMode · gmeow:realizationMode

The work's kind and mode: `gmeow:CreativeWorkType` is the open vocabulary of work
types (literary, musical, software, film, dataset…); `gmeow:realizationMode` over
`gmeow:RealizationMode` records how an expression is realized (notated, performed,
oral, improvised, machine-generated…) — individuals, never subclasses (Principle
9).

### gmeow:Contribution · gmeow:contributor · gmeow:contributionTarget · gmeow:contributionRole · gmeow:ContributionRole · gmeow:contributionDegree

The contribution relator: `gmeow:Contribution` binds `gmeow:contributor` ×
`gmeow:contributionTarget` × `gmeow:contributionRole` (over the open
`gmeow:ContributionRole` vocabulary) with provenance, period, and
`gmeow:contributionDegree`. The reified form behind the flat 80%-case shortcuts.

### gmeow:hasContributor · gmeow:hasAuthor · gmeow:hasEditor · gmeow:hasTranslator · gmeow:hasIllustrator · gmeow:hasComposer · gmeow:hasLyricist · gmeow:hasArranger · gmeow:hasConductor · gmeow:hasPerformer · gmeow:hasNarrator · gmeow:hasProducer

The flat contribution shortcuts: `gmeow:hasContributor` and its role-specialized
siblings (`gmeow:hasAuthor`, `gmeow:hasEditor`, `gmeow:hasTranslator`,
`gmeow:hasIllustrator`, `gmeow:hasComposer`, `gmeow:hasLyricist`,
`gmeow:hasArranger`, `gmeow:hasConductor`, `gmeow:hasPerformer`,
`gmeow:hasNarrator`, `gmeow:hasProducer`) cover the common case; promote to a
`gmeow:Contribution` when period, confidence, or degree must be recorded.

### gmeow:CreativeDerivation · gmeow:derivationSource · gmeow:derivationProduct · gmeow:derivationType · gmeow:DerivationType

The derivation relator: a `gmeow:CreativeDerivation` binds a
`gmeow:derivationSource` work to its `gmeow:derivationProduct`, classified by
`gmeow:derivationType` over the open `gmeow:DerivationType` vocabulary (arrangement,
cover, remix, sample, parody, transcription, translation…).

### gmeow:arrangementOf · gmeow:coverOf · gmeow:remixOf · gmeow:samples · gmeow:transcriptionOf · gmeow:quotesWork

The flat derivation shortcuts: `gmeow:arrangementOf`, `gmeow:coverOf`,
`gmeow:remixOf`, `gmeow:samples`, `gmeow:transcriptionOf`, and `gmeow:quotesWork`
name the common work-to-work derivations directly, with the reified
`gmeow:CreativeDerivation` available when provenance must be carried.

### gmeow:ManifestationFormat · gmeow:hasManifestationFormat · gmeow:CarrierMedium · gmeow:hasCarrier · gmeow:medium · gmeow:Genre · gmeow:hasGenre · gmeow:audience

The manifestation and classification facets: `gmeow:hasManifestationFormat` over
the open `gmeow:ManifestationFormat` vocabulary, `gmeow:hasCarrier` /
`gmeow:medium` over `gmeow:CarrierMedium`, and the descriptive `gmeow:hasGenre`
(over `gmeow:Genre`) and `gmeow:audience` — all individuals, never subclasses.

### gmeow:MusicalWork · gmeow:Recording · gmeow:ScoreEdition · gmeow:hasVersion · gmeow:requires · gmeow:isRequiredBy · gmeow:conformsTo

The music-spine specializations and structural relations: `gmeow:MusicalWork` (a
Work), `gmeow:Recording` and `gmeow:ScoreEdition` (manifestation-tier
embodiments), plus `gmeow:hasVersion`, the `gmeow:requires`/`gmeow:isRequiredBy`
dependency pair, and `gmeow:conformsTo` for standard conformance.

## References

1. IFLA Library Reference Model (LRM), 2017.
2. FRBR — Functional Requirements for Bibliographic Records, 1998/2009.
3. FaBiO and the SPAR ontologies (Peroni & Shotton).
4. CIDOC-CRM Conceptual Reference Model.
5. LRMoo (CIDOC-CRM Issue ID-360).
6. BIBFRAME 2.0 Model — Library of Congress.
7. schema.org `CreativeWork`.
8. OpenWEMI — Code4Lib Journal.

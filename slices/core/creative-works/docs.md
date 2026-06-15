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

## References

1. IFLA Library Reference Model (LRM), 2017.
2. FRBR — Functional Requirements for Bibliographic Records, 1998/2009.
3. FaBiO and the SPAR ontologies (Peroni & Shotton).
4. CIDOC-CRM Conceptual Reference Model.
5. LRMoo (CIDOC-CRM Issue ID-360).
6. BIBFRAME 2.0 Model — Library of Congress.
7. schema.org `CreativeWork`.
8. OpenWEMI — Code4Lib Journal.

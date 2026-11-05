<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Coreference — identity by reference, never by merger

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/coreference` · **tier: core**
> The slice that lets GMEOW point at the rest of the world's records without ever marrying them.

Universal identity and coreference links: stable GMEOW entities link to external records and cross-realm counterparts by reference, never by owl:sameAs merger. Wikidata is the recommended hub, but all authority records remain external claims that may be exact, close, contested, or standpoint-indexed.

The doctrine is Principle 5 applied to identity. `owl:sameAs` is a logical sledgehammer:
it merges every property of both nodes, propagates upstream errors as entailments, and
collapses standpoints that were deliberately kept apart. GMEOW therefore never asserts
it across system boundaries. The stable entity is the GMEOW node itself; an external
record is an *authority reference* attached with `gmeow:authorityLink`, and the
*strength* of the coreference is a separate, explicit claim (`skos:exactMatch` /
`skos:closeMatch` in instance data). A contested identification is several
standpoint-indexed claims that coexist (Principle 9); a withdrawn one is suppressed
with `gmeow:displayable` false, never deleted (Principle 10).

The same by-reference stance covers identity *within* the graph: counterparts across
realms and frames stay linked but unmerged, and version/edition/supersession lineage
keeps every member of the lineage first-class.

## Authority and counterpart links

### gmeow:authorityLink

The universal pointer from a GMEOW entity to a record in an external authority,
registry, database, gazetteer, or catalogue — by IRI, range intentionally open. It is a
see-also authority pointer, **not** an OWL identity merge; assert match strength
separately with `skos:exactMatch` or `skos:closeMatch`. Wikidata is the recommended hub
because its cross-references reach most other authorities — one well-chosen QID buys
VIAF, GeoNames, and ORCID transitively (the maximal-linkage convention curl-validates
QIDs at gate time).

### gmeow:counterpartOf

Symmetric — and deliberately *not* transitive — linkage between two GMEOW entities that
are recognisable counterparts across realms, datasets, editions, or modelling contexts
without being safely mergeable: the historical person and their fictionalized portrayal,
the same place in two gazetteers' worldviews. Non-transitivity is the point: counterpart
chains must not silently weld A to C through B.

## Version, edition, and supersession lineage

### gmeow:versionOf

Relates a concrete version entity to the stable lineage entity it versions — a language
version to its language, a software release to its project, a data release to its
dataset. Functional (a version belongs to one lineage); the lineage has many versions.

### gmeow:editionOf

The creative-work specialization: a concrete edition, issue, or manifestation pointing
to the stable work it editions. Functional per edition, non-merge semantics — editions
remain first-class CreativeWorks with their own identifiers, dates, rights, and
provenance. Bridges directly to the WEMI spine in `documents`/`creative-works`.

### gmeow:supersedes

Relates a newer entity, version, record, or claim-bearing artifact to a prior one it
replaces. Non-functional: one successor may consolidate several predecessors. The
superseded entity is retained and usable (Principle 10); suppression from display is a
separate decision (`gmeow:displayable` false). The lifecycle slice supplies the inverse
(`gmeow:supersededBy`) and the event-shaped view (`gmeow:eventTypeSupersession`) when
the replacement itself needs a date, location, or participants.

## Reference resolution — coreference by shared referent (lang: graft)

The links above are identity **lineage**: they relate one entity or record to another
across realms or over a version/edition/succession history. They are *not* NLP-style
reference resolution. That seam — the point where a link asserts "these forms denote the
**same** referent" — is grounded in the `lang:` layer (Principle 19) as a new term,
`gmeow:corefDenotation`, and none of the lineage links above is superseded by it (each
carries a negative-supersession cell in `mappings/equivalences.ttl`: `lang:Denotation` is
form→referent meaning and cannot express an entity→entity lineage).

### gmeow:corefDenotation

Relates a `gmeow:Entity` to a reified **`lang:Denotation`** that resolves a form or
appellation to it as *referent* (`lang:denotationKind lang:denotesEntity`,
`lang:denotationTarget` the entity, `lang:denotedForm` the resolved non-surface
`lang:Form`). Coreference is decided **structurally**: two forms co-refer exactly when
their denotations share a `lang:denotationTarget` — "the morning star" and "the evening
star" both resolve to Venus — never an `owl:sameAs` merge and never a same-string guess.
Non-functional: many context-scoped denotations may resolve to one entity. See
`examples/authority-links.ttl` for the worked Venus resolution.

## Solver layer & deferred alignment

Entity *resolution* — deciding that two records corefer, scoring the match, clustering
candidates — is solver-layer computation (Principle 12): the slice records asserted
links and their declared strengths; it never derives identity. The SKOS mapping
vocabulary is consumed by reference in instance data (Principle 5), and the
maximal-linkage doctrine expects every slice to exercise these links — coreference is
the one slice whose job is to be everyone else's out-edge.

## Per-mention process readings

For the operational question “which target did this refiner read here?”, use one
[`gmeow:Refinement`](../observations/docs.md#refinement-readings) per occurrence.
Its `observedFeature` is the exact immutable source version. The existing,
domain-free `spanStart` and `spanEnd` locate the occurrence in that source using
zero-based Unicode code points and an exclusive end. A `description` can repeat
the mention as a language-tagged display note; the source and offsets, not that
note or string equality, identify the occurrence. Preserve the source digest,
encoding/normalization identity and versioned process vantage. A changed source
cannot inherit old offsets without a new reading.

Put a selected referent in `observationResult`, retain the qualitative
`refinementOutcome`, and use `refinementCandidate` for unselected alternatives.
This pattern adds one reading record per ordinary mention and reuses the
existing source and target; a Form/Denotation pair is not required merely to
record that process output. Ambiguous or disputed readings may have additional
candidate records. It does not merge referents or assert that a source statement
is true.

This is an observational reading operation, not a lightweight replacement for
formal linguistic denotation. When the requested operation asserts analyzed
form/reference semantics, retain the full `lang:Denotation` contract: form,
sign system, denotation kind, target and context. Never synthesize a fake Form,
hang denotation on a raw SurfaceForm, or silently substitute the process-reading
pattern for that operation. The [worked record](../observations/examples/refinement-readings.ttl)
shows the one-record pattern without inventing formal linguistic analysis.

## Dependencies

Depends on `kernel`, `documents` (the CreativeWork range of `editionOf`), and `lang`
(the `lang:Denotation` reference-resolution substrate of `gmeow:corefDenotation`). Consumed
by every slice that links to Wikidata and external registries — which, under the
maximal-linkage convention, is all of them.

## Source-supplied authority-reference records

`authorityReference` associates an actual InformationObject with a distinct
`AuthorityReferenceRecord`. It retains one coherent curation reference rather
than merging the subject with an external record. The external record may
concern an author or another related resource. The association alone does not
say that the external target describes the subject itself.

Each record requires one `authorityReferenceTarget` IRI and one
`authorityReferenceSource` word literal. The latter retains the supplied
authority, registry, database or catalogue name. It is not the evidence source
or asserting standpoint of the reference assertion. Optional
`authorityReferenceResourceType`, `authorityReferenceRelation`,
`authorityReferenceAuthorName`, `authorityReferenceLabel` and
`authorityReferenceNote` retain their five distinct reported wording roles.

All six wording roles specialize `lang:literalValue` and retain complete
`xsd:string`, `rdf:langString` or `rdf:dirLangString` literals. Preserve actual
language and direction; do not add tags to untagged source text. Blank supplied
wording is valid. A missing required source is a failure, while unavailable
optional wording remains absent. Source words do not establish an Agent,
Appellation, actual authorship, formal resource type or interpreted relation.

The selected locator-to-IRI mapping must validate the supplied locator and
retain its spelling. Missing or invalid input fails that link operation.
The target field cannot fall back to literal locator text. The ordinary
`AuthorityReferenceTargetIriConstraint` checks every supplied target's term
kind; the separate closed paths and local maxima require exactly one target
and source on each coherent record. There is no URI resolver, normalization,
network lookup, target-type discovery or identity-matching operation here.

Keep separate supplied references separate even when their targets agree.
The subject relation stays nonfunctional; maxima apply within each coherent
record. No property chain or subproperty assertion derives `authorityLink`,
`skos:exactMatch`, `skos:closeMatch` or identity from these records. The existing
authorityLink contract and its match-strength warning remain unchanged. A
reported relation label is not a substitute for separately justified matching
semantics (Principles 4, 5, 9, 17 and 19).

Preserve attribution on each actual statement. Provenance on a record does
not propagate to its outgoing field assertions, its target, or assertions
about its target. Reporting a source's words records those words; it does
not endorse their content. Explicitly supplied interpretation can be modeled
separately through its actual naming, authorship and identity contracts.

The fictional scene below explicitly supplies the language tags and two
independent curation sources. Both references point to the same fictional
external record, while retaining different notes and their own exact-statement
source attribution. The target is author-related, and the subject remains an
InformationObject. None is asserted to be a Person, Agent, matched identity
or resolved author.

```turtle
@prefix ex: <https://example.org/authority-reference-scene/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

ex:subject a gmeow:InformationObject ;
    gmeow:authorityReference ex:first , ex:second .

ex:firstCuration a gmeow:InformationObject ;
    rdfs:label "Fictional first curation source"@en .
ex:secondCuration a gmeow:InformationObject ;
    rdfs:label "Fictional second curation source"@en .

ex:first a gmeow:AuthorityReferenceRecord ;
    gmeow:authorityReferenceTarget <https://example.org/authority/amber> ;
    gmeow:authorityReferenceSource "Amber catalogue"@en ;
    gmeow:authorityReferenceResourceType "person entry"@en ;
    gmeow:authorityReferenceRelation "author reference"@en ;
    gmeow:authorityReferenceAuthorName "ソラ"@ja ;
    gmeow:authorityReferenceLabel "Amber entry"@en ;
    gmeow:authorityReferenceNote "Check the author heading."@en .

ex:second a gmeow:AuthorityReferenceRecord ;
    gmeow:authorityReferenceTarget <https://example.org/authority/amber> ;
    gmeow:authorityReferenceSource "Amber catalogue"@en ;
    gmeow:authorityReferenceRelation "author reference"@en ;
    gmeow:authorityReferenceNote "Retain the earlier catalogue wording."@en .

ex:firstAssociation rdf:reifies
    <<( ex:subject gmeow:authorityReference ex:first )>> ;
    logic:provenance ex:firstCuration .
ex:firstTarget rdf:reifies
    <<( ex:first gmeow:authorityReferenceTarget <https://example.org/authority/amber> )>> ;
    logic:provenance ex:firstCuration .
ex:firstSource rdf:reifies
    <<( ex:first gmeow:authorityReferenceSource "Amber catalogue"@en )>> ;
    logic:provenance ex:firstCuration .
ex:firstType rdf:reifies
    <<( ex:first gmeow:authorityReferenceResourceType "person entry"@en )>> ;
    logic:provenance ex:firstCuration .
ex:firstRelation rdf:reifies
    <<( ex:first gmeow:authorityReferenceRelation "author reference"@en )>> ;
    logic:provenance ex:firstCuration .
ex:firstAuthor rdf:reifies
    <<( ex:first gmeow:authorityReferenceAuthorName "ソラ"@ja )>> ;
    logic:provenance ex:firstCuration .
ex:firstLabel rdf:reifies
    <<( ex:first gmeow:authorityReferenceLabel "Amber entry"@en )>> ;
    logic:provenance ex:firstCuration .
ex:firstNote rdf:reifies
    <<( ex:first gmeow:authorityReferenceNote "Check the author heading."@en )>> ;
    logic:provenance ex:firstCuration .

ex:secondAssociation rdf:reifies
    <<( ex:subject gmeow:authorityReference ex:second )>> ;
    logic:provenance ex:secondCuration .
ex:secondTarget rdf:reifies
    <<( ex:second gmeow:authorityReferenceTarget <https://example.org/authority/amber> )>> ;
    logic:provenance ex:secondCuration .
ex:secondSource rdf:reifies
    <<( ex:second gmeow:authorityReferenceSource "Amber catalogue"@en )>> ;
    logic:provenance ex:secondCuration .
ex:secondRelation rdf:reifies
    <<( ex:second gmeow:authorityReferenceRelation "author reference"@en )>> ;
    logic:provenance ex:secondCuration .
ex:secondNote rdf:reifies
    <<( ex:second gmeow:authorityReferenceNote "Retain the earlier catalogue wording."@en )>> ;
    logic:provenance ex:secondCuration .
```

Each underlying statement is explicitly asserted as well as reified. The
reifiers name the supplied curation sources directly; this does not claim a
separately supplied asserting agent or standpoint. A matching target IRI does
not merge the two reference records, turn their labels into target labels or
make their associated subject the target's referent. No source truth,
retrieval, language analysis or runtime acceptance is certified by this scene.

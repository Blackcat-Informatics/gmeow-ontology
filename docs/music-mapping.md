<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Music — oral tradition, tune families, and performance lineage

GMEOW treats **sheet music as one lossy projection among many** (Principle 4).
Consequently, a `gmeow:MusicalWork` whose only `Expression`s are oral,
improvised, or performed is first-class — never a deficient or incomplete
object. This guide explains how three existing GMEOW facilities compose to give
identity to score-less works and tune families without inventing new identity
machinery (Principle 16).

## The oral-tradition guarantee

A `gmeow:MusicalWork` may be realized by `Expression`s carrying any of the
`gmeow:realizationMode` values:

- `gmeow:realizationModeNotated`
- `gmeow:realizationModePerformed`
- `gmeow:realizationModeImprovised`
- `gmeow:realizationModeOral`
- `gmeow:realizationModeMachineGenerated`

No SHACL shape may require a `MusicalWork` to have a notated `Expression`. This
is the exact move the Languages slice made for code-less conlangs
(`tests/test_languages.py::test_registry_independence_no_required_code`): the
absence of a score is a feature of the work, not a validity error
(Principle 9).

When a work IRI is minted retrospectively for an oral tradition, record the
ontic uncertainty explicitly with `gmeow:hasDeterminacy gmeow:determinacyVague`
(the unified observation stance, Principle 9).

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/music-mapping/> .

ex:bodyAndSoulTuneFamily a gmeow:MusicalWork ;
    rdfs:label "Body and Soul tune family"@x-gmeow-english ;
    skos:definition "The constellation of performances, oral renditions, and jazz heads recognised as 'Body and Soul'."@x-gmeow-english ;
    gmeow:hasDeterminacy gmeow:determinacyVague ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/music> .

ex:bodyAndSoulOral1940 a gmeow:Expression ;
    rdfs:label "Body and Soul — oral transmission c. 1940"@x-gmeow-english ;
    gmeow:realizes ex:bodyAndSoulTuneFamily ;
    gmeow:realizationMode gmeow:realizationModeOral ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/music> .
```

## Tune family = `VersionSet`

A tune family ("Body and Soul", "Raga Yaman as performed in the Kirana
gharana") is a `gmeow:VersionSet` reused verbatim from
`slices/core/versions/module.ttl`. Each performance-Expression joins the set
through a `gmeow:VersionMembership` relator:

- `gmeow:versionMember` → the performed `Expression`.
- `gmeow:versionSet` → the `VersionSet` representing the family.
- `gmeow:membershipAuthority` → the tradition, gharana, or scholar standpoint
  asserting the membership.
- `gmeow:confidence` → the authority's confidence.
- `gmeow:displayable` → whether the membership should be shown.

Competing family-membership claims coexist as distinct relators; none is
privileged (Principle 9).

```turtle
ex:ragaYamanKiranaSet a gmeow:VersionSet ;
    rdfs:label "Raga Yaman as performed in the Kirana gharana"@x-gmeow-english ;
    skos:definition "The tune family / performance lineage of Raga Yaman transmitted in the Kirana gharana."@x-gmeow-english ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/music> .

ex:yamanKiranaMembership1960 a gmeow:VersionMembership ;
    rdfs:label "Raga Yaman performance in Kirana gharana lineage"@x-gmeow-english ;
    gmeow:versionMember ex:yamanPerformance1960 ;
    gmeow:versionSet ex:ragaYamanKiranaSet ;
    gmeow:membershipAuthority ex:kiranaGharanaStandpoint ;
    gmeow:confidence "0.92"^^xsd:decimal ;
    gmeow:displayable true ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/music> .
```

## Suppression, never erasure

A retracted or contested membership is suppressed, not deleted
(Principle 10):

```turtle
ex:yamanContestedMembership a gmeow:VersionMembership ;
    rdfs:label "Contested Raga Yaman membership (suppressed)"@x-gmeow-english ;
    gmeow:versionMember ex:yamanPerformance1975 ;
    gmeow:versionSet ex:ragaYamanKiranaSet ;
    gmeow:membershipAuthority ex:rivalScholarStandpoint ;
    gmeow:confidence "0.45"^^xsd:decimal ;
    gmeow:displayable false ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/music> .
```

Display projections filter with `gmeow:displayable true`; the data is retained
for audit and standpoint recovery.

## Transmission lineage

Oral teaching is an ordinary `gmeow:Event` typed `gmeow:eventTypeTransmission`.
The teacher and student participate with `gmeow:roleTransmitter` and
`gmeow:roleLearner` (issue #313). Performance-to-performance descent is
recorded with `gmeow:wasDerivedFrom` between performed `Expression`s, optionally
reified as `gmeow:CreativeDerivation` when provenance detail matters.

```turtle
ex:kiranaTeachingEvent a gmeow:Event ;
    rdfs:label "Kirana gharana teaching transmission"@x-gmeow-english ;
    gmeow:eventType gmeow:eventTypeTransmission ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/music> .

ex:teacherParticipation a gmeow:Participation ;
    gmeow:participationEvent ex:kiranaTeachingEvent ;
    gmeow:participationParticipant ex:panditBhimsenJoshi ;
    gmeow:participationRole gmeow:roleTransmitter ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/music> .

ex:studentParticipation a gmeow:Participation ;
    gmeow:participationEvent ex:kiranaTeachingEvent ;
    gmeow:participationParticipant ex:studentMusician ;
    gmeow:participationRole gmeow:roleLearner ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/music> .

ex:yamanPerformance1975 a gmeow:Expression ;
    rdfs:label "Raga Yaman performance, 1975"@x-gmeow-english ;
    gmeow:realizes ex:ragaYamanWork ;
    gmeow:realizationMode gmeow:realizationModePerformed ;
    gmeow:wasDerivedFrom ex:yamanPerformance1960 ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/music> .
```

## Why no new identity primitive?

- A tune family is a set of versions of a stable idea → `VersionSet`.
- Membership is a standpointed, confidence-weighted claim → `VersionMembership`.
- Teaching is an event with roles → `Event` + `Participation`.
- Descent is derivation → `wasDerivedFrom` / `CreativeDerivation`.

Reusing these keeps the core small and puts the music-specific doctrine in the
extension slice where it belongs (Principle 16).

## Timbre and sensory bridge

Timbre is modelled as an attributed, standpoint-indexed observation, not a
single ground-truth label. A `gmeow:ToneEvent` may carry a flat shortcut
(`gmeow:toneEventTimbre`) for the simple case; the worked form uses the core
observation stack.

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/music-timbre/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .

ex:segment a gmeow:ToneEvent ;
    gmeow:toneEventPitchValue gmeow:pitchValueC4Fixture .

ex:humanListener a gmeow:Agent .
ex:mirExtractor a gmeow:Agent .

ex:humanTimbre a gmeow:Observation ;
    gmeow:observedFeature ex:segment ;
    gmeow:vantage ex:humanListener ;
    gmeow:observationMethod gmeow:methodDirectObservation ;
    gmeow:observationType gmeow:observationTypeSensory ;
    gmeow:timbreObservationResult gmeow:timbreDescriptorBright ;
    gmeow:confidence "0.85"^^xsd:decimal .

ex:mirTimbre a gmeow:Observation ;
    gmeow:observedFeature ex:segment ;
    gmeow:vantage ex:mirExtractor ;
    gmeow:observationMethod gmeow:methodComputationalModel ;
    gmeow:observationType gmeow:observationTypeDerived ;
    gmeow:timbreObservationResult gmeow:timbreDescriptorGritty ;
    gmeow:confidence "0.72"^^xsd:decimal .
```

The two observations coexist without privilege (Principle 9). The actual
spectral feature vectors are referenced by identifier, never materialised as
triples (Principle 12). The AFO 1.1 alignment row in
`slices/extensions/music/mappings/equivalences.ttl` links `TimbreDescriptor` to
`afo:AudioFeature`. The auditory `ObservableProperty` seeds and their `afv:*`
alignments live in `slices/extensions/sensory/mappings/equivalences.ttl`.

## Competency queries

- `queries/competency/music-oral-works.rq` — MusicalWorks with no notated
  Expression but at least three performances.
- `queries/competency/music-gharana-memberships.rq` — Kirana-gharana memberships
  of Raga Yaman performances, excluding suppressed claims.

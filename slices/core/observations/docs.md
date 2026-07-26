<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Observations — the universal claim relator

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/observations` · **tier: core**
> One reified structure for every measured, sensed, claimed, or asserted value in the graph.

Most vocabularies scatter their claim machinery: SOSA for sensor readings, CIDOC E13 for
attribute assignments, bespoke relators for names, rights, and identities. GMEOW collapses
them into **one** structure — `Observation ≡ Measurement ≡ Standpoint ≡ IdentityFacet ≡
NameUsage ≡ RightsStatement ≡ KinRelationship` (Principle 9: every vantage is a co-equal
facet; no frame is privileged). An observation is a reified `gufo:Relator` mediating a
**vantage** (who holds it), an **observedFeature** (what it is about), and an
**observationResult** (what was found). SOSA/SSN, PROV-O, and CIDOC E13 are aligned by
reference (Principle 5).

This slice is the **Claim** end of the claim spine (Source → Chunk → EvidenceSpan → Claim,
Principle 14): an LLM output, a sensor reading, and a census entry are all observations —
stored with a vantage, never adjudicated by rank. Dimensioned values use the grounding slice's
single `math:Quantity` authority. Observations qualifies that object with unit/frame,
determinacy, uncertainty, and provenance; it does not mint quantity aliases or a second value
property.

## The core relator

### gmeow:Observation

The universal claim construct: a `gufo:Kind` under `gufo:Relator`. EL-axiomatised to
mediate at least one vantage and one feature (open-world); the closed-world "exactly one"
cardinality is SHACL's concern (Principle 7). Competing observations of the same feature
coexist rather than collapse — disagreement is data.

### gmeow:vantage

The agent or standpoint the observation is made from — the reified counterpart of the
`gmeow:accordingTo` annotation axis. The flat↔reified pairing is documented, not
axiomatised: `accordingTo` is an annotation property (keeping the OWL 2 DL downcast clean,
Principles 2–3), so vantage ⊑ accordingTo is realised in the projection layer. Range is
`gmeow:Entity`: a vantage may be a bare agent or a first-class Standpoint. Non-functional —
joint observations are valid.

### gmeow:observedFeature

The feature of interest — deliberately `owl:Thing`-ranged, because anything can be
observed: a place, a person, a proposition, a quality value. Per-kind narrowing is SHACL's
job, not the open ontology's.

### gmeow:observationResult · gmeow:isResultOf

The entity-valued result (a coordinate set, an instant, a quantity) and its inverse — the
explicit provenance leg of the measurement bundle. Uniformly entity-valued: scalar literals
live on a `math:Quantity`, never directly on the observation. Non-functional: one
observation may yield results in several frames; one consensus result may flow from many
observations. A property chain (`isResultOf ∘ hasReferenceFrame ⊑ hasReferenceFrame`)
lets results inherit the observation's reference frame (Principle 11).

### gmeow:observationMethod · gmeow:observationType

*How* it was done versus *what kind* of act it was — both open value vocabularies
(individuals, never subclasses). Method is functional and constitutive: a different method
is a different observation. Type is non-functional: a calibrated reading can be both
measurement and derived inference. Seeds cover measurement, sensory, standpoint, derived,
simulation, identity, naming, rights, kinship, and streaming.

### gmeow:Measurement · gmeow:SensoryObservation · gmeow:StandpointClaim

The shared specialisation spine, declared here so every consumer slice (temporal, places,
sensory, standpoint) deepens the *same* parent. `StandpointClaim` is the assertion form:
vantage = the standpoint, observedFeature = the proposition, observationResult = the
modality (unequivocal, probable, conceivable, refuted) — the structure the deception and
falsehood doctrine builds on.

### gmeow:facetSubject · gmeow:facetVantage

The observation-spine bridge bridge idiom: domain relators (IdentityFacet, NameUsage, RightsStatement,
VersionMembership) plug their role properties into the core roles via `rdfs:subPropertyOf`
(`facetSubject ⊑ observedFeature`, `facetVantage ⊑ vantage`, likewise `usageNamer`,
`usageAppellation`, `statementAbout`, `membershipAuthority`). A generic consumer asks "all
observations about Alice" and gets her names, rights, identities, and coordinates without
knowing any domain vocabulary. When `gmeow:selfAsserted` is true, the facetVantage is the
person themselves — self-assertion is top authority (Principle 9).

## Streams and scalar results

### gmeow:Stream

A time-ordered observation sequence (stream-to-trajectory design) — `streamOf` exactly one tracked entity,
composed via `streamSample`, hosted on a `streamPlatform`, produced by a `streamSensor`
over a `streamInterval`. Ordering is implicit in sample timestamps, never an asserted list
(Principle 12); deriving a continuous trajectory from the stream is the solver's job.

### math:Quantity

The one canonical dimensioned-quantity construct. `math:hasDimension` situates the quantity and
`math:quantityValue` carries a concrete magnitude where measured. Observations adds
`gmeow:quantityUncertainty`, `gmeow:unit` / `gmeow:hasReferenceFrame`,
`gmeow:hasDeterminacy`, and `gmeow:isResultOf`. Temperatures, counts, probabilities, and masses
share that authority while retaining their domain-specific subclasses and qualifiers.

### YAMATO quality stratification — persistent Quality, generic→role ladder, true quantity

A measured value is the *frame-relative reading* of a *frame-independent quantity* of a
*persistent quality* (Principle 11). `gmeow:Quality` (⊑ `logic:Quality`) is the enduring
quality of a `gmeow:bearer` — a patient's systolic blood pressure — that persists while its
values change; dated `gmeow:Observation`s attach to it via `gmeow:observationOf` +
`gmeow:observedAt`, so its value-history is a first-class series rather than disconnected
results. The quality instantiates a generic quality (`logic:genericQuality` → a
`gmeow:GenericQuality` such as `gmeow:pressure`) and plays an anti-rigid quality-role
(`logic:qualityRole` over `logic:Role` — `height` is `length` in a body-context). The
reading carries `gmeow:trueQuantity` → a `gmeow:Magnitude` (the unit-independent magnitude,
dimension only) plus the frame-relative `math:quantityValue` + `gmeow:unit` +
`gmeow:hasReferenceFrame`. These ground onto the `logic:` foundation by sub-property, so a
value expressed in a unit with no frame is the native `logic:MeasurementFrameMissing`
violation and a role with no generic is `logic:QualityRoleWithoutGeneric`. Worked end-to-end
in `examples/blood-pressure.ttl`.

### gmeow:MonetaryAmount

Promoted to core in the dependency refactor: money is frame-relative quantity
machinery, not finance-domain. `gmeow:monetaryValue` carries the decimal;
`gmeow:currency` (⊑ `hasReferenceFrame`, functional) names the currency frame — a value
without its currency is ill-formed (Principle 11). Canonical superset of
schema:MonetaryAmount and the FIBO monetary amount.

## The evaluative-primitive vocabulary

### gmeow:Assessment · gmeow:assessmentCriterion · gmeow:assessmentRubric · gmeow:assessmentTarget · gmeow:assessmentScoreValue

Promoted to core from the norms extension (Principle 16): scoring a target against a
criterion or rubric is a genuinely cross-domain need (the preference extension reuses it
too), not norms-domain-specific. `Assessment ⊑ Observation` — vantage = the judge, human or
model; two disagreeing judges are coexisting cells, no winner (Principle 9).
`assessmentCriterion` / `assessmentRubric` play the `observationMethod` ROLE but are
deliberately not its subproperties (observationMethod is functional with a QualityValue
range; Criterion and Rubric are Entities — the claimModality axiom pattern).

`assessmentTarget` (⊑ `observedFeature` — the observation-spine bridge idiom) and
`assessmentScoreValue` (the datatype twin of `observationResult`) are core-owned here too.
They were briefly left behind in norms on the premise that they were "not reused outside
it"; that premise was false — the preference extension's hard-versus-soft competency query
reads `assessmentTarget`, and an extension may not reach into a sibling extension
(Principle 16). Their domain is this slice's `Assessment`, their superproperty is this
slice's `observedFeature`, and `Assessment`'s own `howToUse` mandates them, so the whole
property cluster belongs together in core. Their schema.org review-cluster alignments moved
with them into `mappings/equivalences.ttl`.

### gmeow:Criterion · gmeow:Condition · gmeow:EvaluationVerdict · gmeow:Rubric

The remaining evaluative primitives, likewise promoted from norms: `Criterion` (one
evaluative axis of a rubric, with named poles), `Condition` (a describable circumstance
whose canonical form is prose, never executed — Principle 12), `EvaluationVerdict` (the
closed held/not-held/undetermined trichotomy, seeded by `verdictHeld` /
`verdictNotHeld` / `verdictUndetermined`), and `Rubric` (a reified evaluation framework,
`⊑ SocialObject`). The norms extension additionally declares `Rubric rdfs:subClassOf Norm`
— a norms→observations bridge axiom (extension depending on core is legal; the reverse is
not, which is exactly why these primitives live here rather than in norms). Domain-specific
machinery that reuses these primitives (`ConditionGroup`, `ConditionExpression`,
`CriterionPole`, `ScoreAnchor`, `ScoreScale`, `ComplianceAssessment`, …) stays in norms.

## Solver boundary & alignment

Aggregation, calibration, consensus derivation, trajectory reconstruction, and
inter-frame conversion are computations, never assertions (Principle 12): the slice
records co-existing observations; the solver layer ranks, fuses, or projects them per
consumer policy. Suppressing a contested observation is projection-time filtering, never
erasure (Principle 10).

## Dependencies

Depends on `kernel`, `entities`, `events`, `names`, `places`, and `temporal`. Consumed by
every slice that measures, claims, or senses anything — which is to say, by every slice;
the AI claim layer (GraphRAG provenance design) is its flagship consumer.

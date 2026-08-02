<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# affect

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/affect` · **tier: core**

Emotions and appraisals — a **core** slice. An agent's felt mental life is part of
the grounded-agent-memory flagship (Principle 14), so affect joins the kernel
`gmeow:MentalMoment` family alongside cognition, epistemics, and teleology.

## The model

Core is comprehensive by design. This slice is the kernel of the affect model;
the fuller high-dimensional landscape, the affective-experience and evidence
layers, and the external bridges are specified canonically in
[`design/AFFECT-DESIGN.md`](./design/AFFECT-DESIGN.md). The current vocabulary
is a kernel, not a ceiling: `gmeow:Emotion` (an intrinsic affective
mode inhering in one agent, grafted under `gmeow:AffectiveMoment ⊑ gmeow:MentalMoment`),
an open Plutchik-seeded `EmotionType`, `Appraisal ⊑ Observation` with the PAD
dimensions and an open `AestheticQuality` vocabulary, and the emotion's
`affectiveTarget` (aboutness) separated from its `affectiveElicitor` (cause).

The **occurrent branch** now completes the mode/experience cut:
`gmeow:AffectiveExperience` (⊑ mentation's `gmeow:Experience`) is the phenomenal
*felt episode* — a perdurant `logic:Event` whose kind is pinned to
`gmeow:processAffectiveExperience` — realizing the enduring `gmeow:Emotion` mode
via `gmeow:feltAffect` (⊑ `gmeow:realizesMentalMoment`). A felt episode is never a
`gmeow:MentalMoment`: modes are endurants (`logic:Mode`), experiences are
occurrents (`logic:Event`).

## The high-dimensional landscape

The canonical *description* of an affective state is **vectorial and relational**,
not a token from a list. The affect itself stays an intrinsic `gmeow:Emotion` mode
or an occurrent `gmeow:AffectiveExperience`; what is vectorial is the frame-relative
description over an **open two-family axis basis**:

- **Core-affect axes** (`gmeow:CoreAffectDimension ⊑ gmeow:AppraisalDimension`) —
  the felt quality itself: valence, arousal, dominance, unpredictability.
- **Appraisal axes** — what the mind computed about the eliciting situation:
  novelty, goal-relevance, goal-congruence, agency, certainty, coping,
  norm-compatibility, temporal-orientation, object-focus. These generate and
  differentiate emotions that share core affect (fear vs anger differ on agency,
  certainty, coping). The goal/norm axes read *into* teleology
  (`gmeow:Goal`/`gmeow:Desire`/`gmeow:counterGoal`) — connected, never identified.

Both families are also tagged with `gmeow:dimensionFamily`
(`gmeow:familyCoreAffect` / `gmeow:familyAppraisal`); the distinct type and the tag
are complementary, and the axis vocabulary stays open and contested (P9) — PAD,
OCC, Scherer, and Plutchik are different bases of one landscape.

Each axis magnitude is read against a declared **`gmeow:AffectScaleProfile`**
(range, midpoint, polarity, transform). Cross-scale conversion and the intensity
norm are solver work, never asserted in triples (P12).

## The `appraisalValue` reshape (greenfield, no shim)

`gmeow:appraisalValue` **removed** its bare-decimal floor: a dimensional reading now
MUST carry a `gmeow:appraisalScaleProfile` naming the `gmeow:AffectScaleProfile` it
is read against (SHACL hard-fail). An unframed magnitude is ill-formed, not a
permitted default — this is Principle 6 (reshape, don't shim) enforcing Principle 11
(a value asserted without its frame is ill-formed).

## The evidence spine

Model outputs and signals are **attributed evidence, never inner-state truth**. A
classifier output is "this model, at this revision, over this span, emitted this
label with this score under this label set" — never "the user is joyful".

- `gmeow:ModelInferenceRun` — one classifier execution (PROV-O-aligned) with full
  run provenance and a **mandatory pinned** `modelRevision`.
- `gmeow:AffectClassifierOutput` — one emitted output (label + raw score +
  `scoreSemantics` + threshold), `producedBy` a run, over a `classifiedTarget`. It
  `supportsAffectiveClaim` — evidence, never entailment.
- `gmeow:AffectClassifierLabel` / `gmeow:AffectLabelSet` — the exact external label
  identities, registered under their own authority path (`gmeow-goemotions:`,
  `gmeow-hf:`, `gmeow-labelset:`), **never** the canonical `gmeow:` emotion
  namespace. GoEmotions (28), Ekman-7, SST-2, and CardiffNLP TweetEval are seeded.
- `gmeow:AffectiveClaim` — the richer human-level claim evidence supports; the
  evidence/claim boundary made explicit (the *expresses / reports / felt / appraised*
  readings stay separate claims).
- `gmeow:AffectiveExpression` — an observed expression (facial, vocal, textual) that
  evidences a claim but never entails the state.
- `gmeow:AffectTelemetryStream` — high-frequency evidence held by-reference
  (`telemetryBlob`, a digest + origin), never a per-frame triple storm.

A raw sigmoid/softmax `classifierScore` is `logic:evidenceStrength`/probability, and
only a **calibrated** probability (declared `scoreCalibration`) may be read as
confidence — `logic:confidence ≠ probability` without a declared mapping.

The label prefixes are registered in **both** the Rust (`prefixes.rs`) and Python
(`config.py`) prefix registries, kept byte-parallel.

## Scope of the current module

Still absent (see `design/AFFECT-DESIGN.md`), landing in later work:

- **Mood/tenure** has no named surface; a diffuse, long-lived `gmeow:Mood` and its
  tenure are described in `design/AFFECT-DESIGN.md`.

Permanent stances (true regardless of how the model grows): **no emotion or
aesthetic hierarchies** — open value vocabularies, contested by design (P9);
and **attributed-vs-self-report is the vantage axis** (self-report is top
authority for the subject's own standpoint, the `facetVantage` precedent), not
new machinery.

## Alignments

Every external link is a `logic:Correspondence` lowering in the shared
`projection-report.ttl` loss ledger — `closeMatch` by default, `exactMatch` only
after review (the overclaim gate reds an unearned equivalence). Authored in
`mappings/equivalences.ttl`:

- **Classifier registries → canonical** (the first bridge layer): GoEmotions and
  Ekman-7 emotion labels `closeMatch` their `gmeow:EmotionType`; `desire`
  bridges to teleology's `gmeow:Desire`; SST-2 / CardiffNLP sentiment labels
  `relatedMatch` the valence axis (positive sentiment is not joy); `neutral` has
  no canonical mapping.
- **PROV-O** — `ModelInferenceRun`→`prov:Activity`, output→`prov:Entity`,
  `producedBy`→`prov:wasGeneratedBy`, `usedInput`→`prov:used`.
- **Web Annotation** — evidence spans (`classifiedTarget`→`oa:hasTarget`,
  `AffectiveExpression`→`oa:Annotation`, `relatedMatch`).
- **MFOEM** (Emotion Ontology, BFO lineage, linkage-only) — `Emotion` and the
  Plutchik emotion types `closeMatch` their MFOEM terms.
- **Wikidata** — curl-verified authority links for `Emotion`, anger, disgust.
- **W3C EmotionML** — the affect vocabulary is *emitted* as a lossy EmotionML XML
  projection (category + dimension `<vocabulary>` blocks, `emotionml` in the projection
  loss ledger — many-to-one: `Emotion`/`AffectiveExperience`/`Appraisal`/
  `AffectClassifierOutput` collapse into one `<emotion>` envelope), and *bridged* at the
  vocabulary-set level (`EmotionType`/`CoreAffectDimension`/`AppraisalDimension`
  `relatedMatch` the EmotionML everyday-category / PAD-dimension / Scherer-appraisal sets).
  Set-level only, because EmotionML category items are XML `name` attributes with no
  per-term IRI.

The WordNet lexical layer **is** bridged, by reference to **Open English WordNet** —
its per-synset IRIs content-negotiate to OntoLex-Lemon RDF, so each canonical emotion
type `closeMatch`es its noun.feeling synset. That is the live successor to the defunct
Princeton WordNet-Affect *affective-label* export. The WordNet-Affect affective labels
and NRC lexicons, the Emotion Frame Ontology, and Ithkuil carry **no resolvable per-term
RDF surface** (the affective-label / NRC data ship without an RDF namespace, the Emotion
Frame Ontology's term IRIs do not dereference, and Ithkuil is a reference inventory, not
a namespace authority). Authoring a correspondence against them would fabricate a link to
a dead IRI, so rather than an implicit comment each is a machine-reviewable
`gmeow:DeclinedCorrespondence` in `mappings/declined-bridges.ttl`, carrying its rationale,
a revisit condition, and `logic:preservationKind logic:Unsupported` — their axis/category
content already carried, modeled up, in-slice, and a bridge authored only if a verifiable
namespace appears.

## Terms

### gmeow:Emotion · gmeow:emotionBearer

An emotion is an intrinsic mode inhering in one agent (the Desire/Intention
grounding) — episodic scope rides `validFrom`/`validUntil` on the statement, no
tenure class. `emotionBearer` is functional and mandatory: an intrinsic mode has
exactly one bearer.

### gmeow:EmotionType · gmeow:emotionType

The kind of an emotion as an OPEN vocabulary seeded with Plutchik's primary eight
(the EmotionML standard set) — registry-independent and contestable, never a tree
(P9). `emotionType` is non-functional: blended emotions carry several types and
classifications from different traditions coexist; at least one (SHACL).

### gmeow:Appraisal · gmeow:appraisalOf

An affective or aesthetic reading of something, as an observation whose vantage is
the appraiser — dimensional or qualitative (at least one of the two forms). Two
critics disagreeing are two coexisting cells. `appraisalOf` (⊑ `observedFeature`)
is functional: one appraisal, one subject.

### gmeow:AppraisalDimension · gmeow:CoreAffectDimension · gmeow:dimensionFamily

The dimensional form: an OPEN axis vocabulary. `gmeow:CoreAffectDimension`
(⊑ `AppraisalDimension`) distinguishes the four felt-quality axes; the nine
cognitive appraisal axes stay plain `AppraisalDimension`; `gmeow:dimensionFamily`
tags each with `familyCoreAffect` / `familyAppraisal`. The basis is seeded, never
closed (P9).

### gmeow:appraisalDimension · gmeow:appraisalValue · gmeow:appraisalScaleProfile

`appraisalDimension` reads at most one axis per appraisal (a PAD triple is three
Appraisals sharing a vantage). `appraisalValue` carries the magnitude — and now
MUST be framed: `appraisalScaleProfile` (functional) names the
`gmeow:AffectScaleProfile` the number is read against. The old plain-decimal floor
is gone (the reshape); an unframed reading is a SHACL hard-fail.

### gmeow:AffectScaleProfile · gmeow:profileRangeMin/Max · gmeow:profileMidpoint · gmeow:profilePolarity · gmeow:profileTransform

The declared scale/frame for numeric affect readings — range (min/max, mandatory),
midpoint, polarity (`gmeow:ScalePolarity`: bipolar/unipolar, open vocab), and a
declared normalization transform (a string spec). The affect analogue of the
rubrics facility's `gmeow:ScoreScale`, minted in core because affect cannot depend
on the norms slice. Scale arithmetic is solver work (P12).

### gmeow:AffectVectorObservation · gmeow:vectorComponent · gmeow:vectorProfile

The stable, queryable identity for "the vector reading" — a reified multidimensional
assessment (the `logic:GoalEvaluation`/`logic:AgencyAssessment` idiom, ⊑
`gmeow:Observation`) grouping the per-axis `gmeow:Appraisal` cells that share
vantage/target/elicitor/time (`vectorComponent`) and naming its metric/basis
(`vectorProfile`, functional). One cell per axis is preserved; the grouping is the
handle that makes the vector citable, signable, and suppressible as a unit.

### gmeow:AffectComposite · gmeow:affectiveConstituent

Model up, don't mint. A `gmeow:AffectComposite` (⊑ `gmeow:Emotion`) is a named
emotion whose meaning is a *declared composition* — a core-affect vector bound by
relations to elicitor, target, agency, norm, and other emotions
(`affectiveConstituent`, mandatory). *Schadenfreude* = positive core affect whose
elicitor is another agent's goal-incongruent outcome + other-directed agency +
deservingness; *saudade* = bittersweet mixed valence + a past/absent target +
prospective longing (schadenfreude ships as the `gmeow:schadenfreudeComposite`
worked instance in `module.ttl`; saudade in `examples/saudade.ttl`). A
compound that cannot be decomposed is evidence the axis basis is incomplete (add an
axis), never a licence for an opaque primitive. Named EmotionType prototypes
(`gmeow:emotionSchadenfreude`, `gmeow:emotionSaudade`) are minted for usability and
mapping *only because* the instances carry the decomposition.

### gmeow:DerivedAffectIntensityObservation · gmeow:fnAffectiveIntensity · gmeow:intensityBasis · gmeow:metricProfile · gmeow:weightingPolicy · gmeow:normFunction

Overall intensity is a **derived view**, never a stored fact. The norm of an
`AffectVectorObservation` is computed *outside the logic* (P12) under a declared
metric — and because the basis is non-orthogonal (valence correlates with
goal-congruence, dominance overlaps coping), a raw L² norm is **not** the default.
An intensity record MUST declare its basis, scale (`metricProfile`),
`weightingPolicy`, and `normFunction` (SHACL hard-fail rule 8); `fnAffectiveIntensity`
is the CLI-exposed function handle.

### gmeow:AffectEvaluationConcluded

"Checked and found flat" ≠ "never checked". A positive, queryable `Observation`
recording that an affect evaluation ran over a target and found zero active
magnitudes — so downstream logic can tell a concluded-flat baseline from the
absence of any evaluation, without ever modelling a forbidden `neutral` EmotionType.

### gmeow:AestheticQuality · gmeow:appraisalQuality

The qualitative form: an OPEN vocabulary seeded with elegance, sublimity, kitsch —
no hierarchy exists or ever will (P9). `appraisalQuality` is non-functional: one
cell may attribute several coexisting qualities.

### gmeow:AffectiveExperience · gmeow:feltAffect · gmeow:processAffectiveExperience

The **occurrent** branch (the feeling bridge). `gmeow:AffectiveExperience` is the
phenomenal felt episode — a perdurant `logic:Event` subclassing mentation's
`gmeow:Experience`, with its `gmeow:mentalProcessType` pinned to
`gmeow:processAffectiveExperience` by an `owl:hasValue` restriction (the
`gmeow:DreamReport` idiom — a queryable handle, not over-typing; the kind still
lives in the value vocabulary, P9). It inherits `gmeow:experiencer` from
`gmeow:MentalProcess` (reused, never re-minted — P4). `gmeow:feltAffect`
(⊑ `gmeow:realizesMentalMoment`) links the episode to the enduring
`gmeow:AffectiveMoment` mode it manifests; it is non-functional (one episode may
manifest several coexisting modes). The mode/experience cut is structural: a
`gmeow:MentalMoment` is an endurant (`logic:Mode`) and can never be an occurrent
(`logic:Event`), backed by the foundation's `logic:Endurant`/`logic:Perdurant`
disjointness.

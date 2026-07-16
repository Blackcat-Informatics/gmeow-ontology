<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Affect — emotions, feelings, moods, appraisal, and affective evidence

> Design-only RFC for `slices/core/affect`. The slice was promoted from the
> extension tier to **core**: affect is a first-class interoperability hub for
> grounded agent memory, not an optional add-on. The slice name is **affect**:
> "emotions and feelings" is the user-facing gloss, not the technical boundary.
> This document names the future architecture; ontology terms, shapes, mappings,
> queries, and generated artifacts still flow from slice source files into
> `gmeow.gts` and must not be hand-edited downstream.

## Why this slice exists

Affect is first-class because agent memory without affect is a degraded memory.
A record of what an agent believed, perceived, wanted, or did is incomplete if it
cannot also say what was felt, reported, inferred, expressed, appraised, revised,
suppressed, or contested about the affective state around that episode.

The slice must support four flagship consumers:

1. **Grounded agent memory** — recall and revise affective claims with provenance,
   confidence, standpoint, time, and suppression semantics.
2. **Narrative and interaction analysis** — model emotional trajectory, tension,
   release, salience, surprise, frustration, rapport, aesthetic response, and
   narrative arc without collapsing them into bare sentiment scores.
3. **Affective evidence pipelines** — ingest self-reports, third-party reports,
   expression observations, physiological measurements, model classifications,
   and text/audio/image cues as attributed evidence, not truth.
4. **Projection and bridging** — emit weaker surfaces such as EmotionML-like
   categories, sentiment labels, or external affect lexicons as explicit lossy
   projections, never as the canonical model.

Principles in force: Principle 1 (SOTA), Principle 4 (one canonical source),
Principle 5 (maximal bridging by reference), Principle 6 (greenfield), Principle
9 (self-assertion and co-equal contested claims), Principle 10 (suppression,
never erasure), Principle 11 (frame-relative values), Principle 12 (compute
outside the logic), Principle 13 (tool-first adoption), Principle 14 (agent
memory), Principle 15 (module earns consumer), Principle 16 (extension bundle),
and Principle 17 (`logic:` is canonical; OWL is a projection).

**On the core promotion.** Affect is promoted to core because Principle 14 makes
affective memory load-bearing for the flagship agent-memory product. GMEOW core is
**comprehensive by design — not a minimal kernel** — so affect owns its *full*
surface here: the canonical model, the dimensional landscape, the classifier label
registries (GoEmotions, Ekman-7, SST-2, TweetEval, …), and the mapping/projection
bundles are all authored in `slices/core/affect`. Only the external *artifacts* —
model runs, telemetry blobs, live services, and third-party ontologies — are
referenced by identity (Principle 5), never imported.

**Migration note.** Promotion from `slices/extensions/affect` to
`slices/core/affect` is a source-layout change, *not* a slice-IRI change. The
slice IRI remains `https://blackcatinformatics.ca/gmeow/slices/affect` unless a
separate, explicit breaking-IRI decision is made — do not mint a second affect
slice.

## Naming doctrine

Use **affect** as the slice and design name. It is broad enough to include:

- **Emotion** — an affective mode inhering in a bearer: anger, joy, fear, grief,
  anticipation, shame, awe, relief, etc.
- **Feeling** — the phenomenal, first-person occurrent experience of an affective
  state. A feeling is not the same thing as the enduring emotion mode it may
  manifest.
- **Mood** — a diffuse affective mode or tenure, usually lower in object-focus and
  longer-lived than an emotion episode.
- **Appraisal** — a vantage-relative observation/evaluation assigning affective,
  aesthetic, or dimensional value to something.
- **Expression** — observable behaviour, language, posture, signal, style, or
  physiological evidence that may support an affective claim but never entails it
  by itself.
- **Aesthetic response** — an appraisal of a work, event, environment, design,
  performance, or experience as elegant, sublime, kitsch, tense, comforting,
  uncanny, alienating, beautiful, and so on.

Do not call the slice `emotions`, `feelings`, `sentiment`, or `emotion-state`.
Those are narrower surfaces. The super-domain is affect.

## Current shipped kernel and target direction

The existing slice is deliberately small: `gmeow:Emotion`, `gmeow:EmotionType`,
`gmeow:emotionBearer`, `gmeow:emotionType`, `gmeow:Appraisal`, PAD-style
`gmeow:AppraisalDimension`, `gmeow:appraisalValue`, and an open
`gmeow:AestheticQuality` vocabulary. Treat that as the **minimum shippable
kernel**, not the final ceiling.

Greenfield-first means the future target should not preserve a weak initial
shape for compatibility. When the consumer demand arrives, the correct move is to
reparent, split, rename, or project rather than carry shims. The canonical affect
model should be allowed to become richer than the current thin module, while all
compatibility remains at projection boundaries.

## Foundational cut

Affect has three different ontological families. Keeping them separate is the
whole point.

| Family | What it models | Foundational category | Existing / target GMEOW shape |
| --- | --- | --- | --- |
| affective mode | the state or disposition inhering in an agent | intrinsic mode / mental moment | existing `gmeow:Emotion`; target `gmeow:AffectiveMoment`, `gmeow:Mood`, `gmeow:AffectiveDisposition` if consumers require them |
| affective episode | the occurrent experience of feeling, expression, regulation, or affective shift | mental process / experience / event | target `gmeow:AffectiveExperience` under `gmeow:Experience`, typed by a mental-process value rather than subclass explosion |
| affective appraisal | the vantage-relative reading of something as pleasant, threatening, beautiful, tense, uncanny, etc. | observation / claim | existing `gmeow:Appraisal` under `gmeow:Observation` |

A person can have an emotion without currently feeling it, feel something without
being able to classify the emotion, appraise a scene as threatening without the
scene bearing any emotion, and express anger without actually being angry. The
model must preserve those distinctions even when external systems collapse them.

**Design stress test — Ithkuil affect distinctions.** Ithkuil is a *constructed*
philosophical language engineered for semantic precision — a stress test of
expressive coverage, not an empirical affect-science authority (it appears as a
reference inventory, not a bridge authority, in *External bridges*). Still, it is a
telling check: from a single affect root (`-ÇM-`, "affective state") its
Specification system derives four distinct notions — the **state** itself
(Basic ≈ our mode), the internal **experience** of being in it (Contential ≈ our
affective experience), its outward **manifestation/"look"** (Constitutive ≈
expression — our "evidence, not the state"), and the **event that triggers** it
(Objective ≈ the elicitor, below). A precision-oriented independent design making
the same four-way distinction is a useful independent check: the GMEOW cut can
express distinctions this fine without primitive proliferation. Ithkuil also groups
its 100+ affect roots *pragmatically* (desirable / relational / ambivalent /
undesirable), never as a taxonomic tree, and includes untranslatable single-concept
feelings (*saudade*, *duende*, hygge, German *Fernweh*) and explicit blends — the
same open, contested, composition-first shape this design mandates.

**The elicitor is first-class.** Ithkuil's Objective specification flags a
relation this design under-specified: *what triggered* an affective state is
distinct from both its bearer and what it is *about*. "Afraid of the dog"
(target = the dog) and "the bark frightened me" (elicitor = the bark) are
different links on the same emotion. The redesign adds `gmeow:affectiveElicitor` /
`gmeow:elicitedBy` alongside `emotionBearer` and `affectiveTarget`; conflating
aboutness with cause is a modeling error.

## Where affect sits: the logic core, the correspondence calculus, and teleology

This is a **domain slice grafted onto the `logic:` foundation**, not a free-standing
model. Three load-bearing relationships fix its place; each means affect *reuses*
existing machinery rather than minting a parallel copy.

### On the `logic:` core (foundation + IR)

- **Emotion is an intrinsic mode on the aspect/mode spine.** `gmeow:Emotion ⊑
  logic:Mode` is the standard grounding lift: a domain term sub-classes/sub-
  properties a `logic:` sort, and the foundation's rules fire on the lifted facts
  without mentioning any `gmeow:` term. Every affect term carries its
  `gmeow:graphBoxRole` (it does), and any affect *axiom* reaches the reasoned core
  only through the FormalizationCandidate gate — never by direct assertion.
- **Appraisal is a reified, factored assessment** — the same idiom as the
  foundation's `CognitiveAssessment` / `AgencyAssessment` (reified multidimensional
  role-property constructs that project to a coarse verdict). The two-family axis
  landscape (below) *is* this idiom: an `AffectVectorObservation` is a reified
  multidimensional assessment whose per-axis components `decomposesToAxis` the
  canonical factored axes, and whose coarse emotion labels are generated
  projections.
- **The vector math lives outside the logic (Principle 12).** The foundation
  classifies n-dimensional vector operations as heavy domain computation that stays
  external by reference — which is exactly why intensity is a *derived* observation
  under a declared metric profile, never a stored or reasoned-core fact. The
  `logic:` core holds the reified per-axis cells; the norm/geometry is a
  solver-boundary computation.
- **The affect axes ARE the foundation's factored quantitative axes.**
  `logic:confidence ≠ logic:probability ≠ logic:weight ≠ logic:evidenceStrength`,
  plus `logic:Determinacy` and credence, are already first-class and kept apart. A
  classifier score is `evidenceStrength`/`probability` (under declared
  calibration), *never* `logic:confidence`; "the relationship is vague" is
  `Determinacy = vague`, not low confidence. Every quantitative hard-fail here is an
  instance of that existing separation.
- **Vantage is `gmeow:accordingTo` / `logic:Standpoint`.** Self-report-outranks-
  attribution, two-critics-coexist, and unspecified-≠-universal are the foundation's
  standpoint indexing (Principle 9), not new machinery.

### On the correspondence calculus (`take1`)

Every external link in this design is a `logic:Correspondence` (the ninth IR node
kind), and every external surface is a **lowering** in the *same*
`projection-report.ttl` loss ledger that governs OWL/gUFO — the affect slice adds no
parallel bridging machinery:

- **Classifier registries, MFOEM, EmotionML, Open English WordNet** are live
  correspondence *sources*; **EmotionML / SSSOM / sentiment-label** emission are
  *lowerings* with declared preservation. Where a source carries no resolvable
  per-term RDF surface (the WordNet-Affect affective labels, the Emotion Frame
  Ontology, Ithkuil), it is a recorded `gmeow:DeclinedCorrespondence` rather than
  a live source — carried and flagged, never fabricated against a dead IRI.
- **`closeMatch`-by-default, `exactMatch`-only-after-review** *is* the relation
  lattice (`overlaps` / `relatedMatch` vs `equiv`) plus the `Determinacy` axis, and
  the **overclaim gate** enforces it: emitting `exactMatch` for a caveated label, or
  collapsing mode/experience/expression/output into one EmotionML envelope without a
  loss record, is a *build failure* — the same gate, not an affect-specific check
  (this is the real home of hard-fail rules 2 and 9).
- **Cross-basis dimensional mapping** (PAD ↔ Plutchik angle; 7-point ↔ `[-1,1]`) is
  a correspondence between `AffectScaleProfile`s — a lens with declared loss whose
  retained original reading is the mnemomorphic **witness**.
- **A `ModelInferenceRun` + `supportsAffectiveClaim`** is provenance-as-witness: the
  classifier output is evidence carrying its source, exactly the
  provenance/mnemomorphism the calculus makes law-bearing.

### On teleology (a sibling slice, heavily reused)

Affect and teleology are **siblings under the same mental-moment foundation**, and
affect is a member of the **normative stack** (teleology · norms · rubrics · risk ·
registers · affect — "no global ought, only ought-according-to"):

- **Reuse the mental-moment spine — it is kernel-level and shared.** The umbrella
  `gmeow:MentalMoment` is defined in the **kernel** (`⊑ logic:Mode`) as the family
  every agent-state *mode* subclasses: cognition's `gmeow:CognitiveState`,
  epistemics' doxastic states, teleology's `gmeow:IntentionalMode`. `gmeow:Emotion`
  (an affective mode) is a **new member of that family** — it should subclass
  `gmeow:MentalMoment` (a refinement of its current bare `⊑ logic:Mode`), so one
  agent-memory query over `MentalMoment` returns emotions alongside beliefs,
  knowings, and intentions. A slice-local `gmeow:AffectiveMoment` groups
  emotions / moods / dispositions beneath it. Teleology contributes the reusable
  *idioms*, not the parent: `gmeow:intentBearer` (which `emotionBearer` already
  follows), `gmeow:IntentionTenure` / `gmeow:TimeScopedRelation` (the future `Mood`
  tenure), and `gmeow:accordingTo` (vantage) — and `IntentionalMode` is `Emotion`'s
  *conative sibling* under `MentalMoment`, not its parent.
- **The occurrent branch grafts onto mentation, not the mode umbrella.** A felt
  episode (`gmeow:AffectiveExperience`) is a *perdurant*, so it subclasses
  mentation's `gmeow:Experience` / `gmeow:MentalProcess` (typed by a
  `gmeow:MentalProcessType`) and links to the mode it realizes via mentation's
  `gmeow:realizesMentalMoment` — exactly the bridge decision #2's example already
  uses. `gmeow:MentalMoment` *explicitly excludes* occurrents, so the
  mode/experience cut is enforced by the foundation, not merely asserted here.
- **The appraisal axes point *into* teleology.** `goal-relevance` and
  `goal-congruence` read against teleology's `gmeow:Goal` / `gmeow:Desire` /
  `gmeow:counterGoal`; `norm-compatibility` (shame / guilt / pride / indignation)
  reads against the deontic-force-over-goals layer; `agency` attribution reads
  against the acting agent. This is why appraisal mirrors teleology's *"goal
  evaluation is reified and factored"* — an appraisal is the affective analogue of a
  goal evaluation.
- **The `desire` classifier label bridges to `gmeow:Desire`** (a teleology bridge,
  not an emotion type — as the GoEmotions label table already states), and emotions
  **motivate**: an elicitor/target is frequently a goal satisfied or threatened
  (`gmeow:motivates` / `gmeow:satisfiedBy`). Affect and teleology stay *connected,
  never identified*.

**Placement consequence.** Affect stays its own core slice (its consumers and size
budget are its own), but it authors *by grafting*: mental-moment parents and the
bearer/standpoint/tenure idioms come from teleology; the axis, quantitative, and
correspondence machinery come from `logic:`; and only the genuinely affect-specific
vocabulary — emotion types, appraisal dimensions, aesthetic qualities, the
classifier bridge surface — is minted here.

## Core design decisions

### 1. Emotion is a mode, not an event

`gmeow:Emotion` remains an intrinsic mode inhering in one bearer. Its type is an
open value vocabulary. Blends are normal, so `gmeow:emotionType` is not
functional. No emotion taxonomy is canonical: Plutchik-style primaries,
EmotionML-like categories, WordNet-Affect-style terms, folk categories, clinical
constructs, and culturally-specific inventories are all alignments or value
vocabularies, not a single privileged tree.

Future target: introduce `gmeow:AffectiveMoment` as the abstract mental-moment
category for emotions, moods, and affective dispositions if and only if a named
consumer needs the common query surface. `gmeow:Emotion` should then reparent
under it; no compatibility shim should be kept.

### 2. Feeling is an experience, not a synonym for emotion

The technical term should be **Affective Experience**, not plain `Feeling`,
unless the project deliberately chooses shorter naming. It is an occurrent,
phenomenal episode: something there is something-it-is-like to undergo.

Target shape:

```turtle
# illustrative only; not emitted by this design document
ex:feltAngerEpisode
    a gmeow:Experience ;
    gmeow:mentalProcessType gmeow:processAffectiveExperience ;
    gmeow:experiencer ex:agent ;
    gmeow:realizesMentalMoment ex:angerMode .

ex:angerMode
    a gmeow:Emotion ;
    gmeow:emotionBearer ex:agent ;
    gmeow:emotionType gmeow:emotionAnger .
```

The event is the felt occurrence. The mode is what inheres in the agent. A single
mode can be manifested by several experiences over time; a single experience can
manifest or produce multiple affective moments.

### 3. Mood is not just a long emotion

A mood is diffuse, backgrounded, and often objectless. It should not be encoded
as a long-duration `Emotion` merely because the initial slice lacks a mood term.
If an agent-memory or narrative consumer needs mood tracking, mint `gmeow:Mood`
and an open `gmeow:MoodType` vocabulary rather than overloading emotion.
Temporal scope follows the statement layer for the simple case and a tenure
class only when adoption/revision/withdrawal of the mood itself is a consumer
fact.

### 4. Appraisal is observation, not ground truth

An appraisal says that something is read, felt, evaluated, or presented as having
an affective/aesthetic value from a vantage. It does not make the appraised thing
bear the emotion. A song appraised as sad is not itself sad unless the model is
inside a frame where works can bear personified affect; that frame must be
explicit.

The one-cell rule stays strong: one `Appraisal` has one appraised subject and one
vantage. A PAD triple is three appraisal cells that share provenance, vantage,
time, and target. Divergence is represented as multiple coexisting appraisals,
not overwritten values.

### 5. Expression and evidence never entail the state

A facial expression, typed word choice, vocal tremor, elevated heart rate, emoji,
latency spike, or classifier output is evidence for an affective claim. It is not
the emotion itself and does not entail an emotion in canonical logic.

Target shape:

- `AffectiveExpression` or an expression-specific event/signal term belongs only
  if a consumer needs expression records as first-class entities.
- Classifier outputs are claims or observations with model provenance and
  confidence, never privileged truths.
- Physiological values belong in measurement/observation facilities and support
  affective claims by evidence links.
- Self-report about one's own affect outranks third-party attribution for that
  subject's standpoint, but it does not delete or invalidate the third-party
  claim. Both remain queryable.

### 6. Dimensions need frames, scales, and polarity

Valence, arousal, and dominance are not naked numbers. Any serious dimensional
appraisal needs an explicit scale/profile: range, midpoint, direction, units if
applicable, normalization rule, and tradition. The redesign **removes** the
bare-decimal allowance in core: an `appraisalValue` MUST reference an
`AffectScaleProfile` — a greenfield reshape (no compatibility shim), not a floor.
The target design aligns dimensional affect with the general frame/reference-system
doctrine.

Hard rule: transformations between affect scales are computations, not asserted
identity. A model may compute that a 7-point valence score corresponds to a
normalized `[-1, 1]` value under a declared transform, but the original reading
must be retained with its scale.

This dimensional model is developed in full below (§"Affect is a high-dimensional
landscape"): dimensions are the *axes* of a vector space, values are *magnitudes*,
overall intensity is the *norm*, and the axis basis is itself open and contested.

### 7. Type vocabularies stay open and contestable

Every affect inventory is a tradition. GMEOW should seed practical values for
usefulness but never encode a closed set, a total order, or a universal emotion
hierarchy.

- `EmotionType`, `MoodType`, `AestheticQuality`, and dimensional axes are open
  vocabularies of individuals.
- Classifications from multiple traditions coexist.
- `skos:closeMatch` / SSSOM mappings bridge external inventories by reference.
- A projection may emit a closed set only by declaring the projection's loss.

### 8. Suppression handles revision

Affective memory changes. A subject may later reject a prior self-report, a model
may retract an attribution, or a narrative analysis may be superseded by a better
reading. The old claim is not deleted. It is closed, superseded, or suppressed
using the project-wide revision/suppression machinery.

Affect is especially privacy-sensitive. Projection layers must be able to
withhold, coarsen, or suppress affective values without erasing canonical history.

## Affect is a high-dimensional landscape, not a list of labels

The foundational commitment of this redesign: **the canonical *description* of an
affective state is vectorial and relational, not a token drawn from a list.** The
affect itself remains an intrinsic **mode** (`gmeow:Emotion`) or an occurrent
**experience** (`gmeow:AffectiveExperience`) — the vector does not replace the
foundational cut. What is vectorial is the *frame-relative description*: axes carry
measured/appraised **magnitudes**, while bearer, target, elicitor, evidence, and
standpoint carry the **relational structure**. Named emotions are labels for
*regions, prototypes, or composites* in that representation. This is what lets
GMEOW "model up" — describe *schadenfreude*, *saudade*, or *Fernweh* as structure
over foundational axes and relations rather than minting an unanalyzed primitive
for every nameable feeling.

### Two families of axis

Conflating these is the usual modeling error; separating them is the whole point.

1. **Core-affect (experiential) axes** — the felt quality itself: low-dimensional,
   continuous, present even in objectless mood. The robust cross-tradition basis
   is small:
   - **valence** — pleasant ↔ unpleasant (hedonic tone)
   - **arousal** — activated ↔ calm (activation / energy)
   - **dominance / potency** — in-control ↔ controlled
   - **unpredictability** — a fourth axis several traditions need to separate,
     e.g., fear from anger at equal valence and arousal.
2. **Appraisal (cognitive-relational) axes** — what the mind computed about the
   eliciting situation; these *generate and differentiate* emotions that share
   core affect: **novelty / expectedness**, **goal relevance / salience**,
   **goal congruence** (conducive ↔ obstructive), **agency / attribution**
   (self ↔ other-agent ↔ impersonal), **certainty**, **coping potential**,
   **norm / self compatibility** (upholds ↔ violates a standard),
   **temporal orientation** (past ↔ present ↔ prospective), and **object focus**
   (sharply directed ↔ diffuse — the emotion/mood continuum).

The appraisal axes disambiguate same-core-affect states. Fear and anger share
high-arousal negative valence but differ on **agency, certainty, and coping**.
Guilt and shame differ on whether the norm violation attaches to an act or to the
self. Pride and joy differ on self-agency. Regret and dread differ on temporal
orientation. **The direction of the vector is the quality; the labels ride along.**

### Magnitudes, intensity, and gradation

Each axis carries a **magnitude** — a reading on a declared scale/profile
(decision #6: range, midpoint, polarity, transform). Two derived quantities
matter:

- **Overall intensity is a derived view, computed under a declared
  `AffectScaleProfile` / metric profile** (Principle 12) — never a separately
  stored fact. No intensity value is canonical without a named basis, scale
  normalization, weighting policy, and norm/distance function; because the basis is
  non-orthogonal (below), a raw Euclidean ($L^2$) norm is *not* the default —
  coordinate redundancy would inflate it, so the metric is an explicit declared
  weighted / metric-tensor form.
- **Gradation within a quality** (annoyance → anger → rage; sadness → grief →
  despair) is **magnitude scaling along a roughly fixed direction** — exactly the
  Stem 1→2→3 pattern in Ithkuil's affect roots. This is the razor for decision #3
  and §7: intensity bands are *not* new `EmotionType` primitives.

### The vector is a bundle of appraisal cells (no new heavy machinery)

GMEOW already has the representation. A set of `Appraisal` cells that share
vantage, target, elicitor, and time **is** the vector — each cell is one axis
component (`appraisalDimension` + `appraisalValue`). A PAD triple was always three
cells; the full landscape is the same idiom with more axes. Overall intensity and
cross-basis coordinates are *derived views*, never stored cells.

The bundle needs its own **identity** to be queryable, citable, signable,
suppressible, and projectable as a unit: a `gmeow:AffectVectorObservation` (an
observation-set grouping — reuse the repo's observation-group construct if one
already exists) that groups the per-axis cells and names its metric/basis profile
(`vectorProfile` → an `AffectScaleProfile`). One cell per axis is preserved; the
grouping is the stable handle for "the vector reading."

### High-frequency evidence is a dense block, not a triple storm

Continuous physiological or vocal telemetry — per-frame facial cues, voice
latency, heart rate — must **not** materialize ten-plus `Appraisal` triples per
frame; that craters triple-store performance and violates the by-reference blob
doctrine. Serialize a time-series vector block as a single dense binary/columnar
artifact, referenced by `blob_id` + origin (never inlined), attached to a parent
`gmeow:AffectTelemetryStream` / tracking event. Reserve individual `Appraisal`
cell bundles and `AffectVectorObservation` identity for the moments a consumer
actually queries: **macro-level state changes, narrative checkpoints, and explicit
self-reports.** Aggregating the dense block up into cell bundles is a computation
(Principle 12), and its provenance points back at the block. This is how the design
answers real-time streams (see the flagship "affective evidence pipelines"
consumer) without database bloat.

### "Concluded and flat" is not "never checked"

Rejecting `neutral` as an `EmotionType` (§ hard-fail rules) preserves open-world
integrity, but must not blind the agent: downstream logic genuinely needs to
distinguish *a check that ran and found a flat baseline* from *no check at all*.
Record the former explicitly — a `gmeow:AffectEvaluationConcluded` observation with
zero active dimension magnitudes over the target — so "evaluated, no salient
affect" is a positive, queryable fact, distinct from (and never conflated with) the
absence of any evaluation.

### Model up: composition over primitive proliferation

A named emotion is canonically **either**:

- a **prototype / region** — a labelled neighbourhood of the space with a typical
  appraisal signature, used for human communication and external bridging; **or**
- a **composite** — a core-affect vector *bound by relations* to its elicitor,
  target, agency attribution, norm, and, where relevant, *other emotions*.

Worked examples of modeling up rather than minting:

- **schadenfreude** = positive core affect whose **elicitor** is another agent's
  goal-incongruent outcome, with an other-directed **agency** structure and often
  a deservingness/**norm** flavour — modeled up from axes, not left as an
  *unanalyzed* primitive. If `gmeow:emotionSchadenfreude` is minted for usability
  or mapping, it is a named composite/prototype carrying exactly that declared
  structure.
- **saudade** = mixed valence (bittersweet) + a past/absent **target** + a
  prospective longing toward something unrecoverable.
- **bittersweetness** = two coexisting valence-split cells sharing one target —
  superposition, not a new axis.

**Rule.** Never mint an *unanalyzed* primitive. A culturally salient named affect
*may* be minted as a first-class `EmotionType` / affective-prototype individual for
developer utility and mapping — but only when it carries its **decomposition
links**: a core-affect vector signature, typical appraisal axes, its
elicitor/target/agency/norm structure, and external lexical mappings. If a quality
cannot be decomposed at all into the current axes and relations, treat that as
evidence the **axis basis is incomplete** (add an axis, which differentiates
thousands of compounds at once) rather than as licence for an opaque primitive. The
primitive layer stays analysable; the expressive power stays total.

### Honesty about the structure

- **The axes are not orthogonal.** Valence correlates with goal-congruence;
  dominance overlaps coping. This is a *frame*, not an orthonormal basis — think
  landscape coordinates, not independent knobs. GMEOW must never assume
  independence, and must not silently rotate one basis into another. Any distance
  or norm over the basis is therefore taken under an explicit **metric profile** (a
  weighted / metric-tensor form, not raw Pythagorean distance), declared per
  profile and computed outside the logic (Principle 12).
- **The basis itself is open and contested (Principle 9).** PAD, OCC, Scherer's
  component-process axes, and Plutchik's wheel are *different bases / projections*
  of one landscape; they disagree about the number and identity of axes, and the
  disagreement is data. `AppraisalDimension` is already the open axis vocabulary;
  the redesign seeds a basis (the two families above) and **never closes it**.
- **Cross-basis mapping is computation (Principle 12).** Converting a 7-point
  valence to normalized `[-1, 1]`, or PAD coordinates to a wheel angle, is a
  declared transform that retains the original reading — never an asserted
  identity.

### Open questions (flagged, not yet decided)

- The exact **seed basis** GMEOW ships as first-class `AppraisalDimension`
  individuals, and whether to type the two families distinctly
  (`gmeow:CoreAffectDimension` vs cognitive `AppraisalDimension`) or tag them with
  an open `gmeow:dimensionFamily`.
- Whether composites need an explicit `gmeow:AffectComposite` /
  `gmeow:affectiveConstituent` structure, or whether the shared-cell bundle plus
  elicitor/target relations already suffice.
- **RESOLVED — the canonical intensity norm.** Overall intensity is the norm
  $\lVert x \rVert = \sqrt{x^{\mathsf T} G x}$ over the 4-axis core-affect basis
  $\{\text{valence}=0, \text{arousal}=1, \text{dominance}=2,
  \text{unpredictability}=3\}$ (`gmeow:coreAxisIndex` fixes each axis's position).
  The canonical metric $G$ (`gmeow:coreAffectGram`) is the positive-definite
  symmetric $4\times4$ Gram matrix with diagonal all $1$, the valence–arousal
  coupling $G_{01}=G_{10}=\tfrac14$, and every other off-diagonal $0$; it is
  positive-definite by its leading principal minors $1$ and $\tfrac{15}{16}>0$.
  This is grounded in the reusable `math:` numeric layer: $G$ is a
  `math:GramMatrix` of **exact** `math:RationalValue` entries representing a
  positive-definite `math:SymmetricBilinearForm` (`gmeow:coreAffectForm`), which
  induces the `math:Norm` `gmeow:affectMetricTensorNorm` — never a bare Euclidean
  $L^2$ over the non-orthogonal basis. The canonical profile is
  `gmeow:coreAffectMetricPAD` (bipolar $[-1,1]$, `gmeow:metricGram
  gmeow:coreAffectGram`), and the `gmeow affect intensity` CLI exposes the
  computation (`gmeow:fnAffectiveIntensity`): the norm, the metric-aware dominant
  axis (the axis of maximal $G$-weighted contribution, which need not be the
  raw-max axis), and the positive-definiteness certificate (the leading minors).
  The computation runs **outside** the logic over exact rationals (Principle 12);
  the triples declare the metric, they do not compute in it. A missing core axis
  is zero-completed (`gmeow:sparseAxisCompletion`), while an axis a downstream
  contract declares required but a reading omits is a hard fail.

## Modeling razors

Use these razors during implementation review.

| User/data statement | Canonical modeling choice |
| --- | --- |
| "I am angry." | self-report claim about an `Emotion` borne by the speaker; optionally an `AffectiveExperience` if the feeling episode matters |
| "I felt angry for five minutes." | `AffectiveExperience` / `Experience` episode with temporal scope, linked to an `Emotion` mode if the mode is represented |
| "I'm afraid of the dog." vs "The bark frightened me." | one `Emotion`, two relations: `affectiveTarget` (the dog — what it is about) vs `affectiveElicitor` (the bark — what triggered it); never conflate aboutness with cause |
| "I feel schadenfreude." | model up, don't mint: positive core-affect vector whose `affectiveElicitor` is another agent's misfortune + other-directed agency (+ deservingness/norm flavour), not a primitive `emotionSchadenfreude` |
| "She looked angry." | observation of expression; optional low-authority attributed affective claim, not a direct emotion fact |
| "The room felt calming." | `Appraisal` of an environment from a vantage; do not make the room bear calmness as an emotion |
| "The model sounded frustrated." | expression/style observation plus model/classifier provenance; any `Emotion` attribution is a claim, not truth |
| "The user has been anxious all week." | target `Mood` or long-lived `Emotion` only if the bearer/target distinction is clear; use statement time or tenure according to consumer need |
| "This scene is tragic." | aesthetic/narrative `Appraisal`; tragedy is a frame-relative reading, not a global property |
| "The classifier says joy=0.72." | model-output observation with scale/profile, confidence, evidence, and provenance; never self-report authority |

## Sentiment and classifier tools are evidence, not ontologies

Sentiment and emotion classifiers are **not ontologies of emotion**. They are
evidence-producing models with label vocabularies. A Hugging Face output is not
"the user is joyful" — it is "this model, at this revision, over this target
span, emitted this label with this score under this label set." GMEOW should link
them maximally and ingest their exact output shape losslessly, but must never let
a classifier's label set define canonical affect semantics. This is Principle 5
(maximal bridging by reference) applied to model evidence, and Principle 9/12
(contested claims; compute outside the logic) applied to scores.

### The four maximal-linking layers

All four ship from the affect slice into `gmeow.gts`. Authority runs GMEOW-first:
external ontologies and classifiers are linked, projected, and explained; they
never define the canonical model.

| Layer | What it captures | Example |
| --- | --- | --- |
| Canonical GMEOW affect model | emotion, affective experience, mood, appraisal, expression, evidence, scale/profile | `gmeow:Emotion`, `gmeow:Appraisal`, future `gmeow:AffectiveExperience` |
| External ontology mappings | rich semantic bridges to ontology terms | MFOEM, EmotionML, OntoLex-Lemon (Open English WordNet), PROV-O, Web Annotation |
| External classifier label registries | exact lossless identity of model labels | `goemotions:gratitude`, `hf-cardiff:Positive`, `sst2:NEGATIVE` |
| Inference-output observations | actual model runs as claims with provenance, scores, thresholds, evidence | "SamLowe GoEmotions emitted `joy=0.84` over chunk X" |

The last layer matters most: a model run is a provenance-bearing claim, not a
fact about an inner state. The canonicalization pipeline is always:

```text
HF model output
  → exact external classifier label (registry identity, lossless)
  → reviewed SSSOM/SKOS mapping (with confidence + justification)
  → canonical GMEOW term  OR  declared projection-loss note
  → optional supported affective claim (evidence link, never entailment)
```

### Label-set exclusivity: the decision rule

A label set is not just a bag of labels — it carries a **decision structure**, and
that structure is the categorical-simplex/partition vs Bernoulli-product/hypercube
duality. A **single-label** set (`gmeow:decisionArgmax`: SST-2, CardiffNLP TweetEval,
Ekman-7) is a **partition** — its members are mutually exclusive and exhaustive, a
softmax score is a point on the probability simplex, and the decision is that point's
**argmax** (the categorical mode), exactly one label. A **multi-label** set
(`gmeow:decisionIndependentThreshold`: GoEmotions) is a **product of independent
Bernoullis** — a point on the `[0,1]` hypercube, per-label thresholds, zero-or-more
labels. This decision rule is the formal dual of `gmeow:scoreSemantics` (softmax ↔
argmax/exclusive, sigmoid ↔ independent-threshold), and `gmeow:impliesLabelSetDecision`
machine-couples the two so a scoring function and its set's declared rule can never
silently disagree.

Exclusivity is enforced, never assumed. Over a single-label set the producer hard-fails
a threshold policy that admits more than one crossing (mutually-exclusive claims), an
off-simplex softmax distribution, an exact argmax tie, and a score-semantics that
contradicts the set's rule. And the model's argmax is recorded **faithfully as
evidence** — a `gmeow:AffectDecision` (`gmeow:decidedLabel` derived by `gmeow:fnArgmax`,
with `gmeow:decisionCrossedThreshold` and, when a runner-up exists, `gmeow:decisionMargin`)
minted **even when the argmax falls below the claim threshold**. The decision is a fact
about the classifier's output over an external label, never a `gmeow:AffectiveClaim`
about inner affect, so recording it below threshold preserves the model's decision
(maximal information flow) without forcing a claim. It supersedes
`gmeow:AffectEvaluationConcluded` for single-label sets: a decided winner, even a
low-margin one, is the positive "checked" fact, never silence.

### GoEmotions — the reference adapter

GoEmotions (58k Reddit comments, 27 emotion categories + Neutral, explicitly
multi-label) is the first adapter because it is widely used and fine-grained. The
common `SamLowe/roberta-base-go_emotions` model treats it as 28 labels, emits one
sigmoid score per label, and typically applies a 0.5 threshold — though its card
documents per-label threshold optimization, so a single global threshold must not
be assumed canonical; the applied threshold policy travels with each output. The
mapping is **never** `GoEmotions label == gmeow:EmotionType`; it is the full
pipeline above. Illustrative only (not emitted by this design document); external
labels live in a per-registry prefix (`gmeow-goemotions:`), never the canonical
`gmeow:` emotion namespace, so a label can never be mistaken for an emotion type:

```turtle
ex:run-123
    a gmeow:ModelInferenceRun ;
    gmeow:modelIdentifier "SamLowe/roberta-base-go_emotions" ;
    gmeow:modelRevision "PINNED_COMMIT_SHA_REQUIRED" ;
    gmeow:usedInput ex:chunk-456 .

ex:joy-output-123
    a gmeow:AffectClassifierOutput ;
    gmeow:producedBy ex:run-123 ;
    gmeow:classifiedTarget ex:chunk-456 ;
    gmeow:emittedLabel gmeow-goemotions:joy ;
    gmeow:classifierScore "0.84"^^xsd:decimal ;
    gmeow:scoreSemantics gmeow:multiLabelSigmoidScore ;
    gmeow:thresholdApplied "0.50"^^xsd:decimal ;
    gmeow:supportsAffectiveClaim ex:claim-joy-expressed .

gmeow-goemotions:joy
    a gmeow:AffectClassifierLabel ;
    gmeow:memberOfLabelSet gmeow-labelset:GoEmotions ;
    skos:closeMatch gmeow:emotionJoy .
```

`skos:closeMatch`, not `exactMatch`, by default: GoEmotions labels are annotation
categories over Reddit comments, not guaranteed to denote the same ontological
thing as a GMEOW emotion mode, a felt experience, or an expression. SSSOM carries
the subject–predicate–object mapping plus provenance, confidence, and
justification; `exactMatch` is an upgrade *earned* only after review.

GoEmotions labels do not all canonicalize the same way. Every original label,
score, and threshold survives, and every canonicalization is explainable:

| GoEmotions label class | GMEOW handling |
| --- | --- |
| clear emotions (`anger`, `disgust`, `fear`, `joy`, `sadness`, `surprise`, `grief`, `remorse`, `relief`, `gratitude`, `love`, `pride`, `embarrassment`, `amusement`, `excitement`, `nervousness`) | `skos:closeMatch` to a `gmeow:EmotionType`, upgraded to `exactMatch` only after review |
| social/evaluative (`approval`, `disapproval`, `admiration`, `annoyance`, `disappointment`, `caring`, `optimism`) | bridge to `gmeow:Appraisal`, `gmeow:EmotionType`, or a future `gmeow:AffectiveStance` — do not force all into emotion |
| cognitive/epistemic (`confusion`, `curiosity`, `realization`) | bridge across affect + mentation; not pure emotions |
| conative (`desire`) | bridge to teleology's `gmeow:Desire` (or an affective-desire subtype only if the slice later separates them) |
| `neutral` | a classifier label / no-detected-affect output — **not** an `EmotionType`; under open-world semantics it is not proof that no emotion exists |

### Common Hugging Face sentiment tools

Three adapter classes cover most tools. In every case the tool label is a
first-class external identity (`NEG`, `Negative`, `LABEL_0`, `0 -> Negative` are
distinct until mapped) and the output is evidence, not a settled inner state.

- **Binary sentiment** (SST-2-style; the default `sentiment-analysis` pipeline is
  a `text-classification` alias returning label+score): `POSITIVE`/`NEGATIVE`.
  Map to a **valence appraisal of the target text**, not to an emotion —
  *positive sentiment is not joy*:

  ```turtle
  ex:sst2-output
      a gmeow:AffectClassifierOutput ;
      gmeow:emittedLabel gmeow-hf:positive ;
      gmeow:classifierScore "0.9998"^^xsd:decimal ;
      gmeow:canonicalizesAs ex:positive-valence-appraisal .

  ex:positive-valence-appraisal
      a gmeow:Appraisal ;
      gmeow:appraisalOf ex:review-text ;
      gmeow:appraisalDimension gmeow:dimensionValence ;
      gmeow:appraisalValue "1.0"^^xsd:decimal .
  ```

- **Ternary social-media sentiment** (CardiffNLP Twitter RoBERTa
  `Negative`/`Neutral`/`Positive`; BERTweet `NEG`/`NEU`/`POS`): a
  `gmeow:AffectLabelSet` whose members map to `gmeow:dimensionValence` buckets —
  `Negative → below midpoint`, `Neutral → near midpoint / no polarity detected`,
  `Positive → above midpoint` — without discarding the tool's own label identity.

- **Emotion classification** (`j-hartmann/emotion-english-distilroberta-base`,
  Ekman-7: anger, disgust, fear, joy, neutral, sadness, surprise): labels are
  close to `gmeow:EmotionType`, but the output is still model evidence. It becomes
  an attributed classifier observation first, then supports *one of* several
  distinct claim templates — the text expresses joy / the speaker reports joy /
  the speaker likely felt joy / the reader appraised the text as joyful / the
  scene is joy-coded — which GMEOW keeps as separate claims.
- **Zero-shot and generative affect classifiers** (NLI / zero-shot pipelines, LLM
  judges): labels come from a *prompt-supplied* candidate set, so the candidate
  label set, hypothesis template, prompt / system instruction, decoding settings,
  and model revision are all part of the run and label-set provenance — a zero-shot
  "joy" is *not* the same external label identity as GoEmotions `joy`. These are
  the highest-drift adapters and the most in need of pinned, fully-recorded runs.

### GMEOW as the lossless affect interoperability hub

Maximal linking makes GMEOW the hub, with authority always flowing GMEOW-first:

```text
EmotionML / MFOEM / OntoLex / PROV-O / Web Annotation
            ↕
         GMEOW affect
            ↕
GoEmotions / SST-2 / TweetEval / BERTweet / CardiffNLP / j-hartmann / future HF
            ↕
         gmeow.gts + gmeow CLI
```

## External bridges

The affect slice should subsume weak external surfaces by projection and mapping,
not by importing their limitations.

Bridge targets belong in mapping artifacts, not in the canonical ontology body:

- EmotionML category and dimension vocabularies.
- MFOEM / BFO-lineage affect terms where license and reference-only policy allow.
- WordNet-Affect-style lexical groupings.
- Wikidata emotion and aesthetic-quality records via authority links.
- Sentiment-analysis labels as deliberately lossy projections.
- Dataset-specific affect taxonomies used by foundation-corpus or model-eval
  pipelines.

Every bridge must declare whether it is exact, close, broad, narrow, related, or
projection-only. If a target collapses feeling, emotion, expression, and
classification into one slot, the mapping must say so loudly.

Beyond the classifier label registries above, publish first-class **ontology and
linked-data** mapping bundles (these are rich semantic bridges, a distinct layer
from the classifier label registries):

| Target | Link strategy | Why |
| --- | --- | --- |
| MFOEM / Emotion Ontology | SSSOM mappings from GMEOW affect terms to MFOEM terms | MFOEM is an OWL ontology of affective phenomena — emotions, moods, appraisals, subjective feelings (BFO lineage) |
| EmotionML | projection profile + SSSOM category/dimension mappings | EmotionML has vocabulary mechanisms for category, dimension, appraisal, and action-tendency sets |
| PROV-O | map runs/outputs to `prov:Activity`, `prov:Entity`, `prov:Agent`, `prov:wasGeneratedBy`, `prov:wasDerivedFrom` | W3C provenance ontology for the inference-output layer's activity/entity/agent chains |
| Web Annotation | align evidence spans and media segments | annotations over arbitrary resources/segments, incl. text and timed multimedia |
| OntoLex-Lemon / WordNet-Affect / NRC lexicons | lexical-entry and lexical-sense bridges | **Landed** as a `skos:closeMatch` OntoLex-Lemon lexical bridge to Open English WordNet's dereferenceable per-synset "feeling" IRIs (the live successor to the defunct Princeton WordNet-Affect export). The WordNet-Affect affective-label layer and the NRC lexicons carry no resolvable RDF surface and are **recorded declines** (`mappings/declined-bridges.ttl`); their content is carried in-slice |
| Emotion Frame Ontology (a local prefix, never the bare acronym "EFO", which collides with the Experimental Factor Ontology) | `skos:relatedMatch` / `broadMatch` to frame roles | models emotions as DOLCE-aligned semantic frames with roles — but its term IRIs do not dereference and no canonical namespace is published, so it is a **recorded decline** (`gmeow:DeclinedCorrespondence`, `logic:Unsupported`), not a landed bridge; its frame-role structure is carried in-slice by the appraisal-dimension and affective-participant model |
| Croissant / ML metadata | dataset/model metadata projection | ML-ready dataset and model-card metadata for Hugging Face ingest packaging |
| Ithkuil affect roots | reference inventory — `skos:closeMatch` from GMEOW terms to root glosses, with composition notes | a maximally fine-grained, composition-aware set of affective *distinctions* and gradation bands (Stems). A **constructed** language: a reference of distinctions, **not** an empirical or standards authority — lower standing than MFOEM / EmotionML, and useful mainly as a stress-test that the vector + composition model can express every root. It publishes no RDF namespace and no per-root IRI, so it is a **recorded decline** (`gmeow:DeclinedCorrespondence`, `logic:Unsupported`, at lower standing), its distinctions carried in-slice by the vector + composition model |

**EmotionML projection is many-to-one.** `Emotion`, `AffectiveExperience`,
`Appraisal`, and `AffectClassifierOutput` may all project into a single EmotionML
`<emotion>` envelope (whose category/dimension names must come from declared
vocabularies, and which carries intensity and PAD-style dimensions natively). The
projection is therefore lossy by construction: it must carry a loss record naming
which GMEOW source family was collapsed. A projection that flattens mode,
experience, expression, and classifier output into one external "emotion" record
*without* that loss annotation is a hard fail.

## Rust-first implementation expectations

When this design turns into ontology and tooling work, the path is:

1. Author the canonical source in `slices/core/affect/`.
2. Compile it into `gmeow.gts` through the Rust-first pipeline as the terminal
   developer artifact.
3. Generate Python/Pydantic/TypeScript/GraphQL surfaces from the same bundle.
4. Keep affect classifiers, sentiment adapters, and external lexicon ingest as
   producers of attributed GMEOW claims, not as Python-only side channels.
5. Make invalid affect records hard-fail: no missing bearer for intrinsic modes,
   no closed fake enum for emotion types, no unframed scale when the advanced
   dimensional form is used, and no silent downgrade of self-report authority.

No conditional import path, optional affect add-on, or swallowed classifier error
should decide the canonical semantics. Fail early, keep provenance, and project
loss only at the exit gate.

## Hard-fail rules

Per the project's no-optionality / hard-fail stance, these are validation errors,
not warnings:

1. A classifier output missing any of model id, model revision, target, emitted
   label, raw score, or score semantics fails. Full run provenance is required:
   model framework, model task, model + tokenizer + label-set revisions, the
   `id2label` mapping, activation / `functionToApply` (post-processing),
   `topK` / `returnAllScores`, threshold policy (and per-label thresholds when
   used), raw logit and normalized score when available, and a calibration profile
   whenever a score is claimed as a probability/confidence.
2. A Hugging Face label not registered in a `gmeow:AffectLabelSet` artifact fails
   canonical projection.
3. `neutral` — or any no-detected-affect label — modeled as an `EmotionType`
   fails.
4. A model score stored as an epistemic `confidence` without calibration metadata
   fails: a raw sigmoid/softmax score is not a calibrated probability.
5. A classifier output that directly asserts someone's inner affect without a
   claim/evidence boundary fails.
6. A closed enum anywhere in canonical affect fails; closed sets are projection
   artifacts only.
7. A model name without a pinned revision fails reproducibility.
8. A derived affect intensity without a declared basis, scale normalization,
   weighting policy, and norm/distance (metric) function fails. The weighting
   policy and norm function are machine-readable IRIs — a `gmeow:WeightingPolicy`
   individual and a grounded `math:Norm` — never free-text strings. A derived
   intensity that stores a magnitude (`gmeow:appraisalValue`) also fails: the norm
   is a recomputable derived view, never a stored ground fact (Principle 12).
9. A projection that collapses mode, experience, expression, and classifier output
   into one external "emotion" record without a loss annotation fails.
10. A REGISTERED (static) `gmeow:AffectLabelSet` with no declared
    `gmeow:labelSetDecision` fails: its exclusivity is then unknown and the producer
    cannot judge whether more than one crossing is a violation. Enforced in the
    producer (it cannot build a config for a rule-less registered set). A run-scoped
    zero-shot candidate set is minted per run and is legitimately rule-less, so this
    is a producer invariant on registered sets, never a universal cardinality that
    would wrongly flag the minted candidate set.
11. More than one label crossing its claim threshold in one observation over a
    single-label (`gmeow:decisionArgmax`) label set fails: an exclusive set admits at
    most one routed `gmeow:AffectiveClaim` per target. Enforced in the producer AND at
    validation by the `gmeow:ExclusiveClaimShape` twin, so a hand-authored or tampered
    graph is caught even without the producer.
12. A softmax (`gmeow:scoreSoftmax`) score distribution over an exclusive set whose
    scores do not sum to 1 (within a declared epsilon) fails: a categorical decision
    lives on the probability simplex, and off-simplex scores are not a valid
    distribution over a partition.
13. A score-semantics that is inconsistent with its label set's decision rule fails:
    `gmeow:impliesLabelSetDecision` couples softmax → `gmeow:decisionArgmax` and
    sigmoid → `gmeow:decisionIndependentThreshold`, so a softmax score over a
    multi-label set (or a sigmoid over an exclusive set) is a contradiction.
14. An exact top-score tie over an exclusive set fails: `gmeow:fnArgmax` has no
    faithful single winner, so the model decision is ambiguous (a near-tie, by
    contrast, is recorded honestly via `gmeow:decisionMargin`, never silenced).

Single-label vs multi-label exclusivity is the categorical-simplex/partition vs
Bernoulli-product/hypercube duality (see "Where affect sits"): a `gmeow:decisionArgmax`
set's members are a partition scored by a softmax on the simplex and decided by argmax
(`gmeow:fnArgmax`), recorded as a `gmeow:AffectDecision` even below the claim threshold
(faithful evidence, never a forced claim); a `gmeow:decisionIndependentThreshold` set is
independent Bernoullis on the hypercube, mints no decision, and legitimately admits many
crossings. Run-scoped / zero-shot NLI candidate sets carry no reviewed decision rule
(their entailment scores are per-hypothesis, NOT normalized across candidates, so they
are neither a clean simplex nor a clean product space): their decision rule is Unknown,
they mint no `gmeow:AffectDecision`, and the exclusivity guards do not apply.

These extend, not replace, the kernel hard-fails already noted above (missing
bearer for an intrinsic mode, unframed scale on the advanced dimensional form,
silent downgrade of self-report authority).

## Candidate future term surface

This table is not an implementation commitment; it is the target vocabulary map
for future PRs.

| Candidate term | Category | Purpose | Mint when |
| --- | --- | --- | --- |
| `gmeow:AffectiveMoment` | abstract mental-moment category | common superclass for affective modes | a consumer needs a uniform query over emotions, moods, and dispositions |
| `gmeow:AffectiveExperience` | `gmeow:Experience` specialization or process type | first-person feeling episode | agent memory/narrative needs felt episodes distinct from enduring modes |
| `gmeow:processAffectiveExperience` | `gmeow:MentalProcessType` individual | value-vocab tag for affective experience events | the mentation value vocabulary is extended from the affect slice |
| `gmeow:Mood` | intrinsic mode / affective moment | diffuse affective background | long-running mood timelines become a named consumer requirement |
| `gmeow:MoodType` | open value vocabulary | kinds of moods | mood is minted |
| `gmeow:AffectiveDisposition` | disposition / mode | tendency toward affective response | personalization, clinical, or long-term-agent-memory consumers require it |
| `gmeow:affectiveTarget` | object property | target/aboutness of an emotion or appraisal when distinct from bearer | target-directed emotions become first-class beyond `Appraisal` |
| `gmeow:feltAffect` | object property | links an affective experience to the affective mode it manifests or produces | `AffectiveExperience` is minted and the generic mentation bridges are insufficiently discoverable |
| `gmeow:AffectiveExpression` | event/sign/signal | observable expression that evidences an affective claim | expression evidence becomes a named product surface |
| `gmeow:AffectScaleProfile` | reference-frame/profile | declares range, polarity, midpoint, and transform for numeric affect dimensions | dimensional scores move beyond bare compatibility decimals |
| `gmeow:ModelInferenceRun` | activity (PROV-O-aligned) | one execution of a classifier over an input, with pinned model id + revision | the classifier evidence layer is minted |
| `gmeow:AffectClassifierOutput` | emitted model artifact / model-vantage observation | one emitted model output over one target (label + score + semantics) — evidence, *not* the human-level claim | classifier evidence becomes first-class |
| `gmeow:AffectiveClaim` | claim | a richer GMEOW affect claim *supported by* evidence (classifier output, expression, self-report) | the evidence/claim boundary is made explicit |
| `gmeow:AffectClassifierLabel` | external label identity | the exact label from an external model/dataset label set | any classifier label set is registered |
| `gmeow:AffectLabelSet` | registry | a named label vocabulary (GoEmotions, Ekman-7, SST-2, TweetEval sentiment) | classifier labels are ingested |
| `gmeow:modelIdentifier` / `gmeow:modelRevision` | datatype properties | HF repo id + required pinned revision/commit | inference runs are recorded (revision mandatory) |
| `gmeow:emittedLabel` / `gmeow:classifierScore` | properties | the exact external label emitted and its raw score | classifier outputs are recorded |
| `gmeow:scoreSemantics` | open value vocabulary | softmax probability, sigmoid score, calibrated probability, logit, margin | scores are recorded (semantics mandatory) |
| `gmeow:thresholdApplied` | datatype property | threshold used to binarize / select labels | thresholded outputs are recorded |
| `gmeow:canonicalAffectMapping` | reviewed mapping | reviewed SSSOM/SKOS link from a label to a canonical GMEOW term | a label is canonicalized |
| `gmeow:projectionLoss` | annotation | declares what was lost when projecting to a simpler label | a lossy projection is emitted |
| `gmeow:supportsAffectiveClaim` | object property | evidence link from a classifier output to a richer GMEOW claim | classifier evidence supports claims |
| `gmeow:affectiveElicitor` / `gmeow:elicitedBy` | object properties | the event/state/act that *triggered* an affective state — distinct from bearer and `affectiveTarget` | the elicitor is separated from aboutness (Ithkuil OBJ) |
| `gmeow:CoreAffectDimension` | open value vocabulary | the experiential axes (valence, arousal, dominance, unpredictability), distinct from cognitive appraisal axes | the two axis families are typed distinctly |
| `gmeow:dimensionFamily` | property + open vocab | tags an `AppraisalDimension` as core-affect vs cognitive-appraisal without closing the axis set | the seed basis needs family grouping but stays open |
| `gmeow:AffectComposite` / `gmeow:affectiveConstituent` | class / object property | a named emotion defined as a composition of a core vector plus relations and/or other emotions | modeling up needs explicit compounds (schadenfreude, saudade) |
| `gmeow:AffectVectorObservation` | observation-set / group | stable identity for "the vector reading" — groups the per-axis cells + names the metric/basis (`vectorProfile`) | the vector bundle must be queryable, citable, signable, suppressible as a unit |
| `gmeow:DerivedAffectIntensityObservation` (fn `gmeow:fnAffectiveIntensity`) | derived observation / function output | the norm of an affect vector under a *declared* metric profile — a computed view, never a stored ground fact | intensity is exposed to the CLI as a derived observation |
| `gmeow:AffectTelemetryStream` | tracking event / stream | parent for high-frequency evidence, whose dense time-series block is held by `blob_id` + origin (never inlined triples) | continuous physiological/vocal ingest must not become a triple storm |
| `gmeow:AffectEvaluationConcluded` | observation | records that affect was *checked* and found flat (zero active magnitudes) — distinct from never-checked | downstream logic must tell "concluded flat" from "no evaluation" |

The example turtle above also uses illustrative wiring predicates
(`gmeow:producedBy`, `gmeow:classifiedTarget`, `gmeow:usedInput`,
`gmeow:canonicalizesAs`); their final names are settled when the surface lands.

Bias toward value-vocabulary individuals and process-type tags before subclassing.
Subclass only when the foundational category changes.

## Competency questions

An implementation of the full design should answer these questions directly from
GMEOW data and generated developer surfaces:

1. What emotions did an agent self-report during an interval, and which are
   currently displayable?
2. Which affective attributions were made by other agents or models, with what
   confidence and evidence?
3. Which felt episodes realized or produced a given long-lived affective mode?
4. Which appraisals of the same work disagree, and from which vantages?
5. Which narrative events coincide with shifts in valence, arousal, dominance,
   tension, relief, or aesthetic quality?
6. Which external affect labels were projected from richer GMEOW claims, and what
   loss was declared?
7. Which affective claims were suppressed, superseded, or revised, and why?
8. Which classifier outputs support an affective claim, and which self-report
   claims outrank them for the subject's own standpoint?
9. Which axis-vector and relations decompose a given named emotion (is this
   "schadenfreude" a positive vector over another's misfortune?), and which named
   emotions in the data are primitives versus compounds?
10. What is the overall intensity (norm) and dominant axis of an affective state,
    and how do they shift across an interval?

## Non-goals

The affect slice must not become any of these:

- a sentiment-only ontology;
- a closed emotion enum;
- a universal emotion hierarchy;
- a clinical diagnosis module;
- a personality psychology module;
- a facial-expression ontology that entails inner states;
- a projection of EmotionML, MFOEM, WordNet-Affect, Wikidata, or any dataset's
  labels as canonical truth;
- a sentiment-classifier passthrough that treats model labels as canonical
  emotion types or asserts an inner affect as fact;
- a single privileged emotion basis, or a closed set of affective dimensions;
- a primitive term for every nameable feeling instead of modeling compounds up
  from axes and relations;
- a privacy leak in the name of memory richness.

Those may be bridged, extended, or consumed. They do not set the canonical shape.

## Staged build plan

Affect work should land in small, reasoned, slice-first increments:

1. **A0 — design anchor:** this RFC plus docs links. No ontology changes.
2. **A1 — kernel hardening & re-grafting:** ensure the existing `Emotion` /
   `Appraisal` kernel has complete annotations, examples, shapes, and competence
   tests — and **reparent `gmeow:Emotion` under kernel's `gmeow:MentalMoment`**
   (refining its bare `⊑ logic:Mode`) so emotions join the agent-mental-life family
   queried across cognition / epistemics / teleology, adding the slice-local
   `gmeow:AffectiveMoment` grouping. The six current foundational typings are
   verified sound (`Emotion` = `logic:Kind`/`Mode`; the `EmotionType` /
   `AppraisalDimension` / `AestheticQuality` open vocabularies =
   `AbstractIndividualType ⊑ QualityValue`; `Appraisal` = `SubKind ⊑ Observation`)
   and are **preserved, not rewritten** — record the `QualityValue`-as-universal-
   value-genus purist note in passing, no change. Promote the manifest tier
   (`tierExtension → tierCore`) and reword the "extension / thinnest slice" prose,
   keeping the slice IRI stable (see the migration note above).
3. **A2 — feeling bridge:** add the occurrent affective-experience surface using
   mentation's `Experience` and process-type idiom.
4. **A3 — dimensional frames → the landscape model:** replace bare score
   assumptions with the vector model — an open two-family axis basis
   (core-affect plus appraisal) as `AppraisalDimension` individuals, explicit scale/profiles and
   projection transforms, derived (never stored) intensity, and the
   `affectiveElicitor` / `affectiveTarget` + composition surface so compounds
   model up from axes rather than minting primitives.
5. **A4 — evidence spine:** model expression/classifier/physiology evidence as
   observations that support affective claims without entailing them — including
   the classifier bridge surface (`gmeow:ModelInferenceRun`,
   `gmeow:AffectClassifierOutput`, `gmeow:AffectClassifierLabel`,
   `gmeow:AffectLabelSet`) with pinned model revisions and registered label sets.
6. **A5 — bridges:** emit SSSOM/projection artifacts — classifier registries
   first (GoEmotions, then SST-2 / TweetEval / CardiffNLP / BERTweet /
   j-hartmann, zero-shot / LLM judges), then the ontology bridges (MFOEM,
   EmotionML, PROV-O, Web Annotation, Wikidata) and the OntoLex-Lemon lexical
   bridge (by reference to Open English WordNet's dereferenceable per-synset
   IRIs), each carrying a declared preservation judgment in the loss ledger.
   Where a named target carries no resolvable per-term RDF surface — the
   WordNet-Affect affective labels and NRC lexicons, the Emotion Frame Ontology,
   and Ithkuil affect roots — it is NOT bridged (a correspondence would fabricate
   a link to a dead IRI); each is instead a machine-reviewable
   `gmeow:DeclinedCorrespondence` in `mappings/declined-bridges.ttl` carrying its
   rationale, revisit condition, and `logic:preservationKind logic:Unsupported`,
   with its content carried, modeled up, in-slice.
7. **A6 — developer surface:** make `gmeow.gts` and the generated Python surface
   answer the competency questions without RDF knowledge.

At every stage, authored data flows from the affect slice to `gmeow.gts`; the
public CLI and generated schemas consume the bundle; generated files are never
edited as the source of truth.

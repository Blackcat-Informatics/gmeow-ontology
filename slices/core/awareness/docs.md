<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# Awareness

The **awareness** slice adds the third orthogonal axis of mind. Where the
[imagination](../imagination/) slice carries the *content* axis (WHAT is
entertained) and the attitude verbs of [epistemics](../epistemics/) and
[imagination](../imagination/) carry the *attitude* axis (HOW it is held),
awareness carries the **state-of-the-experiencer** axis: the operational mode of
consciousness or processing — waking, asleep, focused, drowsy, dreaming — *within*
which any content or attitude occurs. It is the load-bearing **Principle-5
human↔machine bridge**: a single open vocabulary holds the human awareness modes
**beside** the machine operational modes as siblings, so a stored claim can record
whether it was formed during live online-inference or offline replay, exactly as a
human memory can record whether it was formed awake or in a dream.

## The state-of-the-experiencer axis

A model of mind that records only *what* an agent entertains and *how* it holds it
cannot represent the *state the agent is in* while doing so. The same proposition
believed while **alert and awake** and believed while **drowsy and sedated** is the
same content under the same attitude — but the awareness state differs, and for
provenance that difference matters. Awareness is therefore orthogonal to both
content and attitude: it answers neither *what* nor *how* but *in what state of the
experiencer*.

## Awareness modes (the bridge)

### gmeow:AwarenessMode

`gmeow:AwarenessMode` is a **value vocabulary** (`gufo:AbstractIndividualType` ⊑
`gufo:QualityValue`) whose members are individuals, never subclasses (Principle 9,
the [`gmeow:ContentOrigin`](../imagination/) idiom). One open vocabulary holds the
human and machine modes as **siblings**, bridged by *analogy*, never by an asserted
`owl:sameAs` / `owl:equivalentClass` / `rdfs:subClassOf` (Principle 5).

#### Human modes

| Individual | State |
| --- | --- |
| `gmeow:modeWaking` | ordinary alert wakefulness |
| `gmeow:modeDrowsy` | hypnagogic transition toward sleep |
| `gmeow:modeAsleep` | the general sleep state (umbrella over the stages) |
| `gmeow:modeDreaming` | experiencing a dream (the *act* is `gmeow:processDreaming`, by reference) |
| `gmeow:modeREM` | rapid-eye-movement sleep, the vivid-dreaming stage |
| `gmeow:modeLucidDreaming` | dreaming *with metacognition online* (the lucidity seam) |
| `gmeow:modeMindWandering` | task-unrelated, stimulus-independent thought |
| `gmeow:modeFocused` | concentrated goal-directed attention |
| `gmeow:modeFlow` | absorbed, effortless-attention immersion |
| `gmeow:modeMeditative` | a cultivated contemplative state |
| `gmeow:modeSedated` | pharmacologically depressed consciousness |
| `gmeow:modeComatose` | profound unarousable unresponsiveness |

#### Machine modes — the Principle-5 bridge

| Individual | State | Human analogue (by analogy, **not** equivalence) |
| --- | --- | --- |
| `gmeow:modeOnlineInference` | live forward-pass serving against present input | `gmeow:modeWaking` |
| `gmeow:modeOfflineReplay` | offline rumination over logged context | `gmeow:modeDreaming` |
| `gmeow:modeTraining` | weight-updating learning | (developmental / consolidative) |
| `gmeow:modeSampling` | generative free-running from latent space | mind-wandering / imagining |
| `gmeow:modeDormant` | idle, loaded but not running | `gmeow:modeAsleep` |

The machine modes are **substrate-specific realisations** of the awareness faculty,
*not* asserted equal to the human ones (the mentation program guardrail). The analogue
column is documented in prose; no equivalence triple is minted. `gmeow:awarenessMode`
(open domain, range `gmeow:AwarenessMode`) marks an experiencer with its state, and
is **non-functional and vantage-indexed** (Principle 9): a self-reported mode and an
observer-attributed mode for the same span coexist, whose attribution rides
`gmeow:accordingTo`.

## The arousal ladder and the scalar

### gmeow:AwarenessLevel · gmeow:awarenessScalar

`gmeow:AwarenessLevel` is a second value vocabulary grading **how alert** the
experiencer is, on an ordinal ladder ordered high→low by an integer `gmeow:levelRank`:

| Individual | `gmeow:levelRank` |
| --- | --- |
| `gmeow:levelHyperalert` | 5 |
| `gmeow:levelAlert` | 4 |
| `gmeow:levelRelaxed` | 3 |
| `gmeow:levelDrowsy` | 2 |
| `gmeow:levelObtunded` | 1 |
| `gmeow:levelUnresponsive` | 0 |

The two lowest rungs (`gmeow:levelObtunded`, `gmeow:levelUnresponsive`) map to the
**Glasgow Coma Scale** by reference. `gmeow:levelRank` orders the rungs without
asserting a metric scale; for a continuous, normalised `[0.0, 1.0]` arousal value
(a vigilance index for a human, a temperature-like sampling regime for a machine)
use the `gmeow:awarenessScalar` datatype property, supplied instead of or alongside
the named level.

`gmeow:AwarenessLevel` (which **state of arousal**) is distinct from
`gmeow:AwarenessMode` (which **state**): a `gmeow:modeAsleep` agent carries
`gmeow:levelUnresponsive`, a `gmeow:modeFocused` agent `gmeow:levelAlert`, and the
two axes co-apply.

## The awareness tenure (reification)

### gmeow:AwarenessTenure

`gmeow:AwarenessTenure` reifies *an agent being in a mode over a bounded interval* —
a sleep episode, a focus session, a serving window. It specialises
`gmeow:TimeScopedRelation` from the [temporal](../temporal/) slice, carrying the
experiencer (`gmeow:awarenessSubject` → `gmeow:Agent`), the state
(`gmeow:awarenessMode`), the span (`gmeow:duringInterval` → `gmeow:TimeInterval`),
and the optional arousal (`gmeow:awarenessLevel` and/or `gmeow:awarenessScalar`).
The subject is carried by a **per-branch bearer edge** (`gmeow:awarenessSubject`,
Principle 4) — *not* `gufo:inheresIn`, an alignment target this slice never asserts
(Principle 5).

```turtle
ex:lillithSleep a gmeow:AwarenessTenure ;
    gmeow:awarenessSubject ex:lillith ;
    gmeow:awarenessMode    gmeow:modeAsleep ;
    gmeow:awarenessLevel   gmeow:levelUnresponsive ;
    gmeow:duringInterval   ex:nightInterval .

ex:lillithREM a gmeow:AwarenessTenure ;     # a nested sub-tenure
    gmeow:awarenessSubject ex:lillith ;
    gmeow:awarenessMode    gmeow:modeREM , gmeow:modeDreaming ;
    gmeow:duringInterval   ex:remInterval .

ex:nightInterval a gmeow:TimeInterval ;
    gmeow:hasTemporalFrame gmeow:temporalFrameUTCGregorian ;
    gmeow:startedAtTime "2026-06-15T23:00:00Z"^^xsd:dateTime ;
    gmeow:endedAtTime   "2026-06-16T07:00:00Z"^^xsd:dateTime .
```

Tenures **nest**: a `gmeow:modeREM` sub-tenure sits within a night's
`gmeow:modeAsleep` tenure. Each `gmeow:TimeInterval` carries its
`gmeow:hasTemporalFrame` and `xsd:dateTime` bounds to satisfy the frame-relativity
gate (Principle 11).

## The lucidity seam (by reference)

`gmeow:modeLucidDreaming` is documented **by reference** (Principle 6) as the
composition of two states already modelled elsewhere: it is `gmeow:modeDreaming`
*with metacognition online* — `gmeow:MetacognitiveState` (the
[metacognition](../metacognition/) slice) active *during* a dreaming episode. The
seam is named in prose; **no triple is asserted** into the mentation or
metacognition namespaces, exactly as the dreaming *act* (`gmeow:processDreaming`,
the [mentation](../mentation/) slice) is consumed by reference rather than
re-minted. This is the composition point for the dreaming & lucidity work.

## No truth or reality bit

Awareness is a state of the *experiencer*, never a verdict on the *content*
entertained within it. There is **no** `isReal` / `isTrue` / `isDream` property:
dreamt content is not "false" by virtue of the dream mode — its source is a
reality-monitoring matter (`gmeow:contentOrigin`, [imagination](../imagination/),
by reference), and the dream *state* is the `gmeow:modeDreaming` awareness value.
The experiencer-state axis (awareness), the content-source axis (origin), and any
truth axis stay separate.

## SSSOM alignments (`mappings/equivalences.ttl`)

The external frameworks — NSWO sleep/wake staging, SNOMED CT consciousness concepts,
the Glasgow Coma Scale, and PROV for the AI regimes — are named in prose, with a few
deliberately **low-confidence** `skos:relatedMatch` rows: `gmeow:modeAsleep` to a
sleep/wake concept, `gmeow:levelUnresponsive` to a coma-scale concept, and
`gmeow:modeOnlineInference` to `prov:Activity`. All alignments are **by reference**
(Principle 5); GMEOW imports no external axiom.

## Dependencies

`sliceDependsOn` lists **only** `kernel` and `temporal` — the asserted foreign IRIs
are `gmeow:Agent` (the range of `gmeow:awarenessSubject`, kernel) and
`gmeow:TimeScopedRelation` / `gmeow:duringInterval` / `gmeow:TimeInterval` /
`gmeow:hasTemporalFrame` / `gmeow:temporalFrameUTCGregorian` (the reification seam,
temporal). `mentation` (`gmeow:processDreaming`), `metacognition`
(`gmeow:MetacognitiveState`), and `imagination` (`gmeow:contentOrigin`) are all
consumed **by reference** (prose only, no asserted triples into those namespaces),
so the slice stays DL-clean standalone.

## Verified by construction

The slice-resident structural cells (`tests/structural.ttl`, covered by the cached
slice-spec producer verdict in `make check`)
pin the term set, the value-vocab discipline (individuals, never subclasses), the OPEN
domains of the awareness edges, the `gmeow:AwarenessTenure` ⊑ `gmeow:TimeScopedRelation`
reification, and the absence of any truth/reality bit or `gufo:inheresIn` usage. The two
exact-set invariants a module-scoped `ASK` cannot express — the rank set `{0,1,2,3,4,5}`
and the manifest's `sliceDependsOn` = `{kernel, temporal}` — are pinned by the native Rust
conformance twins in `crates/validate/tests/conformance_cases/conformance_awareness.rs`. The two `examples/`
graphs — a human sleep episode and an AI inference regime — load and validate against the
merged SHACL shapes.

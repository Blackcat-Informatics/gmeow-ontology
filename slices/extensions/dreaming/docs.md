<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The Dreaming extension — experience as offline composition

The dreaming slice is a published composition over the core experiential/mentation
layer. It introduces no new primitives: dreams, dream reports, and dream elements are
built entirely from `gmeow:Experience`, awareness modes, content origins, learning
events, and analogy machinery already declared in upstream slices.

## Doctrine

A **dream** is an instance of `gmeow:Experience` with:

- `gmeow:mentalProcessType gmeow:processDreaming`
- `gmeow:awarenessMode` value `gmeow:modeDreaming` or `gmeow:modeREM`
- `gmeow:contentOrigin gmeow:originImagined`

**Lucid dreaming** is the same composition with
`gmeow:awarenessMode gmeow:modeLucidDreaming` — online metacognition overlaid on an
otherwise offline episode.

**Memory-consolidation dreaming** is an offline `gmeow:LearningEvent` with
`gmeow:learningType gmeow:learningConsolidation`, recombining stored claims via
`gmeow:Analogy` and `gmeow:learningConceptFormation`.

`gmeow:DreamReport` is a **recollection experience**
(`gmeow:mentalProcessType gmeow:processRecollection`,
`gmeow:contentOrigin gmeow:originImagined`) that reports or recalls a dream. It is a
low-metacognitive-reliability, standpoint-indexed claim: competing reports coexist
(Principle 9) and superseded reports are suppressed with `gmeow:displayable false`
(Principle 10) rather than deleted.

`gmeow:dreamElement` links a dream experience to its imagined constituents. The range
is deliberately open: a dream element may be any entity, description, or proposition
the dreamer or analyst identifies.

No subclassing of `gmeow:Experience` is introduced beyond the mentation slice's allowed
reparents; dream kinds are value-tagged compositions, not taxonomic divisions.
Narrative and affect texture are demonstrated in worked examples, not declared as slice
dependencies (Principle 16).

## Reports, whole text and recorded phases

`gmeow:recollects`, owned by [mentation](../../core/mentation/docs.md#gmeowrecollects),
links a recollection to its original `gmeow:Experience`. A `DreamReport` is the
later recollection **event**, not its prose document. A written report is a
separate `gmeow:InformationObject`; it can also `recollects` the dream and
`wasDerivedFrom` the recollection event. This general relation also serves a
diary artifact or a non-dream recollection. It never types all recollections as
dream reports, establishes their accuracy, or makes imagined events waking facts.
Several coequal accounts may recall the same experience.

For one unstructured text of the whole dream, use the written report artifact
with `gmeow:description` carrying its language-tagged prose and `recollects`
pointing to the whole dream. No `DreamPhaseText` class, unnamed phase event or
synthetic phase value is needed. Missing phase records mean phases were **not
recorded**; they do not prove the experience had no phases. A structured or
segmented source can use the existing document and language surfaces when that
structure is actually known.

When phases are recorded, give each phase its own `gmeow:Event` identity and
`gmeow:subEventOf` link to the dream. Put its kind on the phase using the open
`gmeow:EventType` vocabulary and `gmeow:eventType`. A consumer can define values
for threshold, descent, immersion, integration and reorientation in its own
namespace. Occurrences with the same phase kind are still different events.
Record actual temporal extents and ordering separately; a list of phase names
does not establish a chronology.

`Experience` inherits `Event` through `MentalProcess`; `ToolCall` inherits it
through `Activity`. A real tool call that is a constituent of the dreaming
activity can therefore be `subEventOf` the phase event, which is `subEventOf`
the dream. The existing transitive relation supplies the whole-dream link. If
only the whole is known, link the call directly to it without inventing a phase.
A call merely occurring at the same time belongs in the temporal model, not
automatically in the dream's part-whole tree. An imagined tool call described
inside dream content is not a recorded external execution.

Use `gmeow:modeImmersiveDreaming` for the immersive, non-lucid mode with no online
metacognition; it is distinct from lucid dreaming. See
[the worked record](examples/recollection-and-phases.ttl), which includes a
whole unsegmented report, coequal recollections and an actual phase/call tree.

## Consumer uses

- **AI offline generative replay / synthetic experience / counterfactual rehearsal over
  agent memory** (GTS ai-package): planning, data augmentation, and what-if memory
  exploration.
- **Human dream-journaling and sleep-research corpora.**

## Terms

### `gmeow:DreamReport`

A recollection experience in which an agent reports or recalls a dream — a
`gmeow:Experience` whose mental-process kind is `gmeow:processRecollection` and whose
content originates in imagination (`gmeow:contentOrigin gmeow:originImagined`). A
standpoint-indexed, low-metacognitive-reliability claim: competing reports coexist and
superseded reports are suppressed, not deleted.

### `gmeow:dreamElement`

Relates a dream or other imagined experiential episode to one of its imagined
constituents — a figure, object, place, event, or proposition occurring within the dream.
Range intentionally open; not functional, since a dream contains many elements.

## Worked examples

See the slice examples for composed dream episodes and reports:

- `human-dream.ttl` — ordinary REM dream experience and its recollection as a dream report.
- `lucid-dream.ttl` — lucid-dreaming composition with online metacognition.
- `ai-offline-replay.ttl` — agent memory replay modelled as offline consolidation and
  counterfactual rehearsal.

## Attention derived from a recollection

`requestedByDream` links an AttentionRequest to the actual DreamReport
recollection episode from which it was derived, specializing `wasDerivedFrom`.
The DreamReport remains an occurrent; it is not an enduring report document.
The core-owned `attentionMessage` retains the request's exact wording,
language and direction. Derivation neither proves external occurrence of the
dreamed content nor acknowledges or executes the request (Principles 4, 9 and 17).

See [attention provenance](design/ATTENTION-PROVENANCE.md) and the ordinary
[`goal-attempt-satisfaction.ttl`](examples/goal-attempt-satisfaction.ttl)
composition scene. The extension consumes the core request and agency-record
vocabularies; neither core owner acquires a dreaming dependency.

## Messages composed from a dream recollection

inspiredByDream links a Message to the actual DreamReport recollection episode
from which it was composed. It specializes wasDerivedFrom while preserving the
episode as an occurrent. The relation asserts neither delivery nor the external
truth of dream content. This extension depends on core messaging; core messaging
and chat have no dependency on dreaming. See the
[ordinary recollection scene](examples/dream-message.ttl).

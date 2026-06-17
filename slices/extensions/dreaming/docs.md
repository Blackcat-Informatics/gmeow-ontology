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

> Note: The dreaming slice does not declare a hard dependency on the forthcoming
> `concepts` slice because it is not yet in the registry. Concept-formation
> dreaming routes through `gmeow:learningConceptFormation` in the learning slice,
> which is already a declared dependency; no additional slice is required.

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

## Consumer

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

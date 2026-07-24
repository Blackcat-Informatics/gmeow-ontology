<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Learning

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/learning` · **tier: core**

Learning as **process** — the occurrent that transitions an agent along the cognition knowledge
spectrum or the expertise proficiency scale. The cognition slice models the endurant
`gmeow:KnowledgeProficiency` and the expertise slice models the endurant `gmeow:SkillProficiency`;
this slice models the **occurrent** `gmeow:LearningEvent` that **changes** those states — acquisition,
being-taught, consolidation, transfer, and unlearning. Memory-as-storage (flagship store/recall) is
not learning; learning is the event that grew what an agent knows.

## The occurrent/endurant split

The mentation slice reserves `gmeow:MentalProcess` as an umbrella for occurrent mental events.
`gmeow:LearningEvent` reparents under it — never re-roots it — inheriting `gmeow:experiencer` and the
unified mental-timeline stream. The split keeps the gUFO ontological partition clean:

| Layer | Class | Mode |
|---|---|---|
| state | `gmeow:KnowledgeProficiency` (cognition) | endurant — what an agent *knows* |
| state | `gmeow:SkillProficiency` (expertise) | endurant — what an agent *can do* |
| event | `gmeow:LearningEvent` | occurrent — what *changed* those states |

A `gmeow:LearningEvent` links to the enduring states it settled via `gmeow:produces`, to its
start/end levels via `gmeow:fromLevel` / `gmeow:toLevel`, and to its source via `gmeow:learnedFrom`.

### gmeow:LearningEvent

The occurrent that transitions an agent along the cognition knowledge spectrum or the expertise
proficiency scale — learning as it unfolds in time. A `gmeow:MentalProcess` (`gufo:EventType`), it is
borne by exactly one `gmeow:experiencer` (inherited) and joins the agent's single occurrent
mental-timeline stream. The **class** asserts that a learning process occurred; the **variety** is a
`gmeow:LearningEventType` value carried by `gmeow:learningType` (Principle 9, never a subclass).

**Timeline-parity fields — the cross-slice contract.** Like every sibling occurrent on the mental
timeline, a `gmeow:LearningEvent` carries:

- `gmeow:mentalProcessType gmeow:processLearning` — the uniform occurrent query handle (it mirrors
  `gmeow:eventType`), declared once on the mentation spine, so a timeline query filtering by
  `gmeow:mentalProcessType` surfaces learning episodes exactly as a `gmeow:InferenceProcess` carries
  `gmeow:processReasoning`. This is the single canonical occurrent marker — no redundant second value
  is minted (Principle 4); the orthogonal **variety** rides `gmeow:learningType`.
- a temporal placement — `gmeow:eventTime` (an `xsd:dateTime`) **and** `gmeow:eventTemporalFrame
  gmeow:temporalFrameUTCGregorian` (CONSTITUTION P11: a value asserted without its reference frame is
  ill-formed).

## Learning event types

`gmeow:LearningEventType` is an **open** value vocabulary (`gufo:AbstractIndividualType ⊑
gufo:QualityValue`) whose members are **individuals, never subclasses** (Principle 9). More than one
may co-apply to a single event (`gmeow:learningType` is non-functional): a transfer that also
consolidates is both `gmeow:learningTransfer` and `gmeow:learningConsolidation`.

| Value | Kind | What it marks |
|---|---|---|
| `gmeow:learningConceptFormation` | concept formation | agent forms a new category or abstraction |
| `gmeow:learningSkillAcquisition` | skill acquisition | agent moves up the expertise proficiency scale |
| `gmeow:learningBeingTaught` | being taught | agent learns through instruction from a teacher |
| `gmeow:learningConsolidation` | consolidation | agent strengthens or stabilises already-acquired knowledge |
| `gmeow:learningTransfer` | transfer | agent applies knowledge from one domain to a new one |
| `gmeow:learningUnlearning` | unlearning | agent retires or supersedes previously-held knowledge |

Two type values carry **documented routing bridges** (no axiom): `gmeow:learningTransfer` routes to the
inference slice's `gmeow:Analogy` (the source-to-target structure-mapping that warrants the transfer);
`gmeow:learningConceptFormation` bridges to the forthcoming concepts slice. Neither carries a
reasoner-enforced relation in core — the bridges are routing for consumers, not entailments.

**Forgetting is suppression, not a new term.** There is deliberately no `gmeow:forgets` /
`gmeow:forgotten` term. Forgetting is the cognition slice's `gmeow:remembers` withdrawn by
`gmeow:displayable false`; `gmeow:learningUnlearning` names the revision *event*, not a deletion
construct.

### gmeow:LearningEventType · gmeow:learningType

`gmeow:LearningEventType` is the open value vocabulary of learning varieties; `gmeow:learningType`
(domain `gmeow:LearningEvent`, range `gmeow:LearningEventType`) classifies an event by pointing it
at one or more of those values. Non-functional by design — several varieties may co-apply. Extend
the vocabulary by minting a fresh `gmeow:LearningEventType` individual, never by subclassing
`gmeow:LearningEvent` (Principle 9).

## Provenance, trajectory, and product

Three open-range properties carry what the learning came *from*, the levels it moved *between*, and
the state it *produced*. All ranges are left **intentionally open** (Principle 13): the target
classes are either not yet built (`gmeow:Concept`) or retired (`gmeow:Source`, superseded by
`gmeow:CreativeWork` plus a source-role).

### gmeow:learnedFrom

Relates a `gmeow:LearningEvent` to what the agent learned from — the provenance of acquired
knowledge. Open range: typically a `gmeow:CreativeWork`, a teaching `gmeow:Agent`, or a body of
evidence, but the surface is never prematurely closed. Non-functional — a learning event may draw on
several sources. Do not revive the retired `gmeow:Source` class; point at a `gmeow:CreativeWork`
plus a source-role.

### gmeow:fromLevel · gmeow:toLevel

`gmeow:fromLevel` is the proficiency or knowledge level an agent held **before** a learning event;
`gmeow:toLevel` is the level reached **after**. Both have an open range (a cognition
`gmeow:KnowledgeLevel` or an expertise `gmeow:ProficiencyLevel` are equally admitted) and are
non-functional. For a consolidation event `gmeow:toLevel` may equal `gmeow:fromLevel` (the level
is stabilised, not raised).

**Trajectory, not mutation (Principle 10).** `gmeow:fromLevel` / `gmeow:toLevel` record the
endpoints of one event. An agent's standing over time is a **sequence** of reified
`gmeow:KnowledgeProficiency` / `gmeow:SkillProficiency` tenures, each scoped by its own interval
(the temporal `gmeow:TimeScopedRelation` idiom), with the prior tenure kept via `gmeow:displayable
false` rather than overwritten — never the in-place mutation of one relator.

### gmeow:produces

Relates a `gmeow:LearningEvent` to the knowledge state it raised or settled — its product. Open
range: typically a cognition `gmeow:CognitiveState` or `gmeow:KnowledgeProficiency`, an expertise
`gmeow:SkillProficiency`, or (once the concepts slice lands) a `gmeow:Concept`. Non-functional —
one event may settle several states. The occurrent/endurant bridge: the **event** produces the
enduring **state**, keeping the gUFO split clean.

## Teaching — the instruction relator

`gmeow:Teaching` is the `gmeow:Participation`-style relator (`gufo:Relator`) mediating one teacher,
one or more learners, and the subject taught — the being-taught face of learning. Reifying the
instruction makes the roles, period, confidence, and evidence of teaching first-class. The **80% case**
(learner acquires from a source) is a bare `gmeow:LearningEvent` with `gmeow:learnedFrom`; reach for
`gmeow:Teaching` only when the instruction relation itself is the fact of interest (Principle 4).

**Teacher must differ from every learner.** An agent cannot teach itself. This is a **closed-world
well-formedness rule** enforced by `gmeow:TeachingShape` (SHACL) — not a DL axiom, keeping the
reasoned profile within OWL 2 EL. The open-world "some teacher, some learner" mediation rides
`owl:someValuesFrom` restrictions in `module.ttl`.

### gmeow:Teaching

A reified teaching relation (`gufo:Relator`, `gufo:Kind`) — one teacher, one or more learners, and
the subject taught. Three role properties: `gmeow:teacher` (functional, range `gmeow:Agent`),
`gmeow:learner` (non-functional, range `gmeow:Agent`), and `gmeow:subjectTaught` (non-functional,
open range). Co-teaching is **several `gmeow:Teaching` relators** sharing a subject and learner set,
not one Teaching with multiple teachers. Retract a withdrawn role with `gmeow:displayable false`,
never deletion (Principle 10).

### gmeow:teacher · gmeow:learner · gmeow:subjectTaught

`gmeow:teacher` (functional) names the single `gmeow:Agent` giving instruction; `gmeow:learner`
(non-functional) names each `gmeow:Agent` receiving instruction — a class taught together shares one
`gmeow:Teaching`. `gmeow:subjectTaught` (non-functional, open range) records what a Teaching
conveyed — a concept (the forthcoming `gmeow:Concept`), a `gmeow:Skill`, or a `gmeow:Proposition` are
all admitted. `gmeow:TeachingShape` enforces that no agent is both `gmeow:teacher` and `gmeow:learner`
of the same Teaching.

## SSSOM alignments (`mappings/equivalences.ttl`)

Authored in `mappings/equivalences.ttl` and compiled to `mappings/gmeow-learning.sssom.tsv` by
`gmeow compile-mappings`. All by reference (Principle 5) — GMEOW never imports an external axiom.
Alignments are deliberately loose (`skos:closeMatch`, not `equivalentClass`): schema.org models the
*act* of learning/teaching, GMEOW models the *cognitive-state-transition event*; PROV-O models the
activity / provenance face.

| GMEOW | Predicate | Target | Note |
|---|---|---|---|
| `gmeow:LearningEvent` | `skos:closeMatch` | `schema:LearnAction` | schema:LearnAction is the act of gaining knowledge; gmeow:LearningEvent is the state-transition occurrent carrying trajectory, variety, and product |
| `gmeow:Teaching` | `skos:closeMatch` | `schema:TeachAction` | schema:TeachAction is the act of imparting knowledge; gmeow:Teaching is the reified gufo:Relator mediating roles |
| `gmeow:subjectTaught` | `skos:closeMatch` | `schema:teaches` | schema:teaches links a learning resource to a competency; gmeow:subjectTaught links a reified relator to its content |
| `gmeow:LearningEvent` | `skos:closeMatch` | `prov:Activity` | a gmeow:LearningEvent is a prov:Activity that generates a knowledge entity, with richer trajectory and the gUFO occurrent/endurant split |
| `gmeow:learnedFrom` | `skos:closeMatch` | `prov:wasDerivedFrom` | provenance of acquired knowledge vs. entity derivation — closeMatch across the entity/event shift |
| `gmeow:produces` | `skos:closeMatch` | `prov:generated` | both relate a process to its produced entity; gmeow:produces specifies the settled knowledge state |

xAPI (the Experience API) is the strongest external consumer — learning records keyed by
`<http://adlnet.gov/expapi/verbs/learned>` — but it is a JSON Statement model with no stable OWL
class IRIs, so `gmeow:LearningEvent` projects **down** to an xAPI Statement at the projection layer
and is referenced in prose, not mapped as a native alignment cell.

## Dependencies

| Slice | Why |
|---|---|
| `kernel` | `gmeow:Agent` — the experiencer, teacher, and learner domain |
| `mentation` | `gmeow:MentalProcess` (the reparenting hook `gmeow:LearningEvent` rdfs:subClassOf's under), the inherited `gmeow:experiencer`, and `gmeow:mentalProcessType` / `gmeow:processLearning` — the mental-timeline marker every `gmeow:LearningEvent` carries |
| `events` | `gmeow:Event` — the superclass `gmeow:MentalProcess` (hence `gmeow:LearningEvent`) reparents under; the `gmeow:Participation` relator idiom mirrored by `gmeow:Teaching` |
| `cognition` | `gmeow:KnowledgeProficiency`, `gmeow:KnowledgeLevel`, and `gmeow:CognitiveState` — the knowledge-state targets of `gmeow:produces` / `gmeow:fromLevel` / `gmeow:toLevel` |
| `expertise` | `gmeow:SkillProficiency` and `gmeow:ProficiencyLevel` — the skill-state targets of the same properties; the `gmeow:scaleDreyfus` skill scale and its `gmeow:dreyfus*` levels (expertise being their domain slice) referenced by `gmeow:fromLevel` / `gmeow:toLevel` |
| `temporal` | `gmeow:TimeScopedRelation` (the proficiency-tenure parent) and `gmeow:duringInterval` for trajectory sequences |
| `sources` | `gmeow:CreativeWork` — the primary open-range target of `gmeow:learnedFrom` |

## Verified by construction

`gmeow:TeachingShape` (`shapes.ttl`) pins the load-bearing closed-world rules:

- **Exactly one teacher** — `sh:minCount 1 ; sh:maxCount 1` on `gmeow:teacher`, class `gmeow:Agent`.
- **At least one learner, each a `gmeow:Agent`** — `sh:minCount 1 ; sh:class gmeow:Agent` on
  `gmeow:learner` — instruction with no one being taught is not a teaching, and a learner must be an
  agent (matching the `gmeow:teacher` constraint and the `rdfs:range gmeow:Agent`).
- **At least one subject taught** — `sh:minCount 1` on `gmeow:subjectTaught` — instruction is always
  instruction in something.
- **Teacher ≠ learner** — SPARQL constraint on `gmeow:TeachingShape`: a Teaching is violation if any
  `gmeow:teacher` IRI also appears as a `gmeow:learner` of the same node. An agent cannot teach itself.

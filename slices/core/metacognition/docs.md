<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Metacognition

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/metacognition` · **tier: core**

Second-order epistemics — the reflexive turn. The whole epistemic stack is **first-order**: an agent
*believes* a proposition (`epistemics`), *knows about* a subject (`cognition`), *claims* a feature
from a vantage (`standpoint`). Metacognition is the turn whose **subject is one of the agent's own
first-order states or claims**, plus **calibration** — the fit between avowed confidence and
accuracy. For a trustworthy AI memory this is decisive: "how sure am I that I'm right, and do I know
where my knowledge stops" is exactly what separates a calibrated agent from a confabulating one
(Principle 15). The slice mints almost no new machinery — a metacognitive judgement **is** a
`gmeow:StandpointClaim` whose `gmeow:observedFeature` is one of the agent's own claims / mental
moments and whose `gmeow:vantage` is that same agent.

## Reflexivity by reuse, not new machinery

`gmeow:MetacognitiveState` (`gufo:Kind ⊑ gmeow:MentalMoment`) is the endurant second-order mode, and
`gmeow:metaTarget` is the **reflexivity edge** to the first-order state it reflects upon. It is the
co-equal **sibling** of the first-order mental modes, gathered under `gmeow:MentalMoment` (kernel) so
an agent's whole mental life is one queryable family:

| Mode | Slice | Stands toward |
|---|---|---|
| `gmeow:CognitiveState` | `cognition` | a subject the agent knows (objectual, first-order) |
| `gmeow:IntentionalMode` | `teleology` | a state of affairs the agent aims at (conative, first-order) |
| doxastic states | `epistemics` | a proposition the agent holds (first-order) |
| **`gmeow:MetacognitiveState`** | **`metacognition`** | **one of the agent's own first-order states / claims (reflexive, second-order)** |

These are **documented siblings, not subsumed**: there is deliberately **no** `rdfs:subClassOf`
making `gmeow:MetacognitiveState` a kind of `gmeow:CognitiveState`. "Thinking about thinking is a kind
of thinking" would erase the reflexive-vs-first-order distinction — exactly as `gmeow:Question` is not
a `gmeow:Proposition`. The siblinghood lives in prose and the `tests/test_metacognition.py`
no-subsumption guard, never in an axiom.

### gmeow:MetacognitiveState · gmeow:metaTarget

`gmeow:MetacognitiveState` types an agent's second-order mental moment, inhering in exactly one agent.
`gmeow:metaTarget` (domain `gmeow:MetacognitiveState`, **open range**, non-functional) points at the
first-order `gmeow:MentalMoment`, `gmeow:StandpointClaim`, or `gmeow:InferenceCommitment` it reflects
upon. The reflexivity is **conceptual** (the target is the agent's *own* state), never OWL
reflexivity: `gmeow:metaTarget` is deliberately **not** `owl:ReflexiveProperty` (which leaves EL and
is semantically false — the metastate and its target are distinct individuals). Same-agent ownership
is a SHACL concern, not a reasoned axiom (Principle 12).

## Calibration

`gmeow:calibration` (domain `gmeow:MetacognitiveState`, range `gmeow:CalibrationStatus`) assigns the
qualitative direction of the confidence/accuracy gap. `gmeow:CalibrationStatus` is a closed value
vocabulary (`gufo:AbstractIndividualType ⊑ gufo:QualityValue`), its members **individuals, never
subclasses**:

| Value | Reading |
|---|---|
| `gmeow:wellCalibrated` | confidence tracks accuracy — the target of a trustworthy agent |
| `gmeow:overconfident` | confidence exceeds accuracy — the **Dunning–Kruger** pole, the confabulation risk |
| `gmeow:underconfident` | accuracy exceeds confidence — the impostor pole |

The **numeric magnitude** of the gap is `gmeow:calibrationError` — a Brier-style score in `[0,1]`,
**solver-layer** (Principle 12). It is an `owl:AnnotationProperty` by design, exactly like
`gmeow:confidence`: **invisible to the reasoner**, computed externally, and never a materialised
axiom. Modelling it as an `owl:DatatypeProperty` with a domain would make it a reasoned ABox edge and
risk the reasoner touching it — the guard `test_calibration_error_is_a_solver_layer_annotation` pins
it as annotation-only.

Dunning–Kruger is **overconfidence at low competence** — the cognition slice's coexisting-claims
fixture, now *modelled*. The [`dunning-kruger.ttl`](examples/dunning-kruger.ttl) example turns the
reflexive lens on Sam's own self-assessment: a `gmeow:MetacognitiveState` whose `gmeow:metaTarget` is
Sam's self-attributed `gmeow:KnowledgeProficiency`, carrying `gmeow:calibration gmeow:overconfident`,
with the assessor's `gmeow:calibrationError` riding the calibration claim. The calibration is itself a
vantage-indexed claim (Principle 9); GMEOW records the judgement, it does not adjudicate a verdict.

### gmeow:CalibrationStatus · gmeow:calibration · gmeow:calibrationError

The closed status vocabulary, the property that assigns it (non-functional — a self-assessment and an
external assessment coexist), and the solver-layer Brier-style magnitude annotation. The qualitative
direction and the numeric gap are kept distinct: `gmeow:calibration` reasons (an EL range axiom over a
closed vocab); `gmeow:calibrationError` does not (an annotation the reasoner never sees).

## Known-unknowns

`gmeow:awareOfNotKnowing` (domain `gmeow:Agent`, **open range**, non-functional) records the
**boundary** of an agent's knowledge — a *recognised* gap (second-order), not the open-world silence
of an unasserted triple. It is the reflexive complement of the cognition knowledge spectrum: where
`gmeow:isAwareOf … gmeow:hasMastered` record what an agent knows, this records a recognised edge of
that knowledge.

A known-unknown **motivates a question by reference** — with **no new bridge term**. The recognised
gap mints a `gmeow:Question` by **reusing inquiry's open-domain `gmeow:evokes`** (the not-known subject
sits legally in `evokes`'s subject position) and the inquiry spine (`gmeow:seeksToKnow`); there is no
`gmeow:motivates` partner (Principle 6), and the bridge is documented routing, not an entailment. The
[`known-unknown.ttl`](examples/known-unknown.ttl) example shows the trustworthy-agent move: surfacing
what it does not know as an open inquiry rather than confabulating an answer.

### gmeow:awareOfNotKnowing

The known-unknown edge. Flat (no `rdfs:subPropertyOf`), open range, domain `gmeow:Agent` — the cheap
surface a trustworthy memory reads to surface its blind spots. The inquiry bridge reuses
`gmeow:evokes`; this slice asserts no `owl:subPropertyOf` into the inquiry slice.

## Epistemic self-trust

`gmeow:epistemicSelfTrust` (domain `gmeow:Agent`, **open range**, non-functional) is feeling-of-knowing
— the reliability an agent accords to one of its **own** epistemic faculties or sources (its memory,
perception, reasoning). It is the flat 80% surface; its **rich, graded form is a
`gmeow:StandpointClaim`** whose `gmeow:vantage` is the agent and whose `gmeow:observedFeature` is the
agent's own faculty, carrying `gmeow:trustLevel` **by reference to the `trust` slice** — self-trust is
trust-about-oneself, the `gmeow:trustor` and `gmeow:trustee` coinciding. The trust machinery is
**reused, not re-minted** (Principle 6); this slice asserts no triples into the trust slice.

### gmeow:epistemicSelfTrust

The self-trust edge. Use it for reliance on one's *own* faculty; trust in an external agent or source
is the trust slice's `gmeow:TrustAssertion` directly. Promote to the standpoint-claim idiom carrying
`gmeow:trustLevel` when the degree, scale, or vantage of self-trust is itself the fact.

## Reflection & belief revision

`gmeow:eventTypeReflection` (`a gmeow:EventType`, the `gmeow:eventTypeDeception` pattern) types an
event of an agent **reviewing its own reasoning**. A reflection may install a typed `gmeow:Attack` on
one of the agent's own `gmeow:Argument` individuals (the `inference` slice), triggering
**belief revision as suppression** (Principle 10): the defeated conclusion-claim is set
`gmeow:displayable false` and the prior inference is **retained as audit**, never deleted. The
[`reflection-revision.ttl`](examples/reflection-revision.ttl) example walks it: Lillith reflects on
her own inductive inference, surfaces an undercutting attack (a firewall blocked the ping), installs
it via `gmeow:Attack`, and suppresses the defeated conclusion — the reflection → revision link is
a documented bridge, never an axiom.

The boundary with the `mentation` slice is documented (Principle 4): `gmeow:eventTypeReflection` types
the reviewable occurrent *kind*; a reflection that unfolds as a reasoning *process* is a
`gmeow:MentalProcess` — one canonical source per fact, never both for the same individual.

### gmeow:eventTypeReflection

The reflection-act value individual. Type a reflection `gmeow:Event` with it; record revision by
installing a typed `gmeow:Attack` on the agent's own argument and suppressing the defeated conclusion
with `gmeow:displayable false`.

## Alignments (`mappings/equivalences.ttl`)

The slice's alignments are **deliberately prose-only**: its grounding literatures —
**Nelson & Narens'** metamemory model, **Flavell's** metacognition, and the **Brier-score**
calibration tradition — have no stable external IRI vocabulary to map to, so the mapping set carries
the scholarly grounding in its comment and seeds **no** native alignment cell rows (the same
intentional non-mapping the inquiry slice applies to its question-type vocabulary). The reused
first-order quantities and the cross-slice bridges carry their alignments in their home slices.

## Dependencies

| Slice | Why |
|---|---|
| `kernel` | `gmeow:MentalMoment` (the mode genus) and `gmeow:Agent` (the known-unknown / self-trust domain) |
| `epistemics` | the first-order doxastic states and credence that metacognition assesses — referenced, the reflexive `gmeow:metaTarget` points at them |
| `standpoint` | `gmeow:StandpointClaim` — a metacognitive judgement *is* a standpoint claim over the agent's own state (`gmeow:vantage` = the agent) |
| `observations` | the Observation spine the metacognitive claim rides (`gmeow:vantage`, `gmeow:observedFeature`) |
| `cognition` | `gmeow:CognitiveState` (the first-order sibling) and `gmeow:KnowledgeProficiency` (the Dunning–Kruger `gmeow:metaTarget`) |

Bridges to `inquiry` (`gmeow:evokes`), `inference` (`gmeow:Attack`, `gmeow:Argument`,
`gmeow:InferenceCommitment`), and `trust` (`gmeow:trustLevel`) are **reused by reference** and documented, not declared as
dependencies — the slice asserts no axioms into them (Principle 9).

## Verified by construction

`tests/test_metacognition.py` pins the load-bearing shape of the slice:

- **Reflexive mode, one gUFO metaclass** — `gmeow:MetacognitiveState` is a `gufo:Kind ⊑
  gmeow:MentalMoment` carrying exactly one gUFO metaclass.
- **Sibling no-subsumption guard** — no `rdfs:subClassOf` to `gmeow:CognitiveState` /
  `gmeow:IntentionalMode` / `gmeow:StandpointClaim`.
- **`gmeow:metaTarget` open & characteristic-free** — domain `gmeow:MetacognitiveState`, no
  `rdfs:range`, no `rdfs:subPropertyOf`, and **none** of Reflexive / Irreflexive / Transitive /
  Symmetric / Functional.
- **Calibration value vocab** — `gmeow:CalibrationStatus` is `gufo:AbstractIndividualType ⊑
  gufo:QualityValue`; the three statuses are individuals, never subclasses; `gmeow:calibration` ranges
  over the closed vocab.
- **Solver-layer guard** — `gmeow:calibrationError` is an `owl:AnnotationProperty`, **not** an
  `owl:DatatypeProperty` / `owl:ObjectProperty`, with no domain / range.
- **Flat, open-range Agent edges** — `gmeow:awareOfNotKnowing` and `gmeow:epistemicSelfTrust` are
  object properties with domain `gmeow:Agent`, no `rdfs:range`, no `rdfs:subPropertyOf`.
- **Reflection EventType** — `gmeow:eventTypeReflection` is an individual of `gmeow:EventType`.
- **No truth / status bit** — none of `isCalibrated` / `isReflected` / `isTrue` appears in any triple
  position; no locally-declared property carries an `xsd:boolean` range.
- **Bridges documented, not axiomatised** (Principle 9) — no `rdfs:subPropertyOf` from
  `gmeow:awareOfNotKnowing` into `gmeow:evokes` or from `gmeow:epistemicSelfTrust` into the trust
  properties.
- **Annotation completeness** (Principle 8) — all 11 locally-declared terms carry an `rdfs:label`, a
  `skos:definition`, and `rdfs:isDefinedBy` the metacognition slice IRI.

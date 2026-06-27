<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Mentation architecture — coherence map

> **Status.** This document is the north-star architecture for GMEOW's mentation model. It is a
> coherence map, not an implementation. The concrete ontology slices are implemented as child
> slices, and this file is the reference those slices are judged against.

---

## 1. Guiding principles

The mentation architecture is constrained by the following Constitution principles. Each slice
must either comply or amend the Constitution explicitly; silent violation is not allowed.

- **Principle 1 — SOTA by being SOTA.** Model mentation the way it *should* be modelled, not the
  way legacy AI or psychology vocabularies happen to model it. The bridge to weaker surface forms
  lives in the mapping DSL, not in the canonical core.
- **Principle 4 — One canonical source; everything else a generated lossy projection.** Every
  class, property, and axiom in this design is authored once in a slice under `slices/` and
  projected outward. No generated artifact may be hand-edited to satisfy this design.
- **Principle 5 — Maximal superset, maximal bridging — by reference.** Link human-facing cognitive
  vocabularies (MFOEM, Cognitive Atlas, CPO) and machine-facing ones (xAPI, schema.org Q&A) by
  SSSOM/EDOAL reference; never copy their axioms into the canonical core.
- **Principle 6 — Greenfield.** Where existing slices carry a weaker modelling choice for
  mentation-related terms, replace it with the optimal one; do not keep an inferior term for
  backwards compatibility.
- **Principle 9 — Inclusive without overtyping; self-assertion is top authority.** A human belief
  and a model probability are co-equal as standpoint-indexed claims; neither is the privileged
  ground truth. The unified observation stance applies to mental content too: every mentation
  assertion is attributed, dated, confidence-weighted, and vantage-relative.
- **Principle 12 — Compute outside the logic.** Credence calibration, attention-weight
  aggregation, chain-of-thought derivation, and replay generation are computed by external
  engines. The ontology records the *claims* those engines produce, not the computations
  themselves.
- **Principle 13 — The product is a tool; the ontology is its engine.** The mentation model must
  surface through flat JSON/Pydantic, MCP tools, and the GTS `ai-package` without requiring
  consumers to understand gUFO or OWL.
- **Principle 14 — Grounded agent memory and claim provenance are the flagship.** Mental content
  is stored as attributed claims, recalled with filters, and revised by suppression (`displayable
  false`) — never deletion.
- **Principle 15 — Every module earns its consumer.** Each of the nine slices below names the
  product or worked example that justifies it; none is added for modelling pleasure alone.
- **Principle 16 — A small core; everything else a published extension.** `concepts`, `inquiry`,
  `mentation`, `inference`, `learning`, `imagination`, `metacognition`, and `awareness` are core
  slices. `dreaming` is an extension (`extensions/dreaming/`) that stress-tests the core and is
  published as a separate bundle.
- **Principle 17 — The logic itself is canonical; OWL is a projection.** The canonical mentation
  TBox will eventually be expressed in the RDF 1.2-native `logic:` core. The OWL 2 DL form is a
  generated, reasoner-tested projection, not the ceiling of what the model may say.

---

## 2. Upper-ontology spine

GMEOW already grounds everything in gUFO (see [`docs/foundational-bridging.md`](./foundational-bridging.md)).
The mentation architecture uses only two new top-level commitments:

- `gmeow:MentalMoment ⊑ gufo:IntrinsicMode` — an endurant mental state inhering in an agent at
  a time (a belief, an intention, a wondering, a perceptual gestalt).
- `gmeow:MentalProcess ⊑ gmeow:Event` — an occurrent mental happening extended in time (inferring,
  recalling, imagining, attending, dreaming). `gmeow:Event` is the occurrent class supplied by the
  existing `slices/core/events/` slice.

That split is non-negotiable: a `MentalMoment` is a *mode* that can be present at an instant; a
`MentalProcess` is an *event* with temporal parts and phases.

### Bridge relation vocabulary

Three relations carry mentation across the spine. They must remain distinct; no slice may
collapse them into one overloaded relation.

| Relation | Domain → Range | Meaning |
|---|---|---|
| `gmeow:realizesMentalMoment` | `MentalProcess` → `MentalMoment` | A process manifests or makes present an existing mental capacity or state — e.g., a `gmeow:MentalProcess` typed `gmeow:processPerception` realizes a perceptual `gmeow:MentalMoment`. |
| `gmeow:producesMentalMoment` | `MentalProcess` → `MentalMoment` | A process creates a new `MentalMoment` that did not exist before — e.g., an `gmeow:InferenceProcess` `gmeow:producesMentalMoment` a new belief `gmeow:MentalMoment`. |
| `gmeow:updatesMentalTenure` | `MentalProcess` → `TimeScopedRelation` | A process revises, extends, or closes a held, time-scoped mental tenure. |

> **Why not `gmeow:realizes` / `gmeow:produces`?** The generic properties already serve the WEMI
> / creative-works and learning slices. To keep one canonical source per term (Principle 4), the
> mentation spine mints its own precise bridge properties instead of overloading the existing ones.

`gmeow:realizesMentalMoment` is manifestation-like; `gmeow:producesMentalMoment` is creation-like;
`gmeow:updatesMentalTenure` is revision-like. A single process may realize one moment via
`gmeow:realizesMentalMoment`, produce another via `gmeow:producesMentalMoment`, and update a third
via `gmeow:updatesMentalTenure`, but those are three different facts.

---

## 3. Four axes of mentation

Every mental term is positioned along four independent axes. Crossing them is deliberate:
mixing them up is the most common failure mode in cognitive ontology.

### Axis I — Endurant vs. Occurrent

- **Endurant**: `MentalMoment`, its sub-modes, and the commitments/relators it participates in.
- **Occurrent**: `MentalProcess` and its sub-events (inferring, attending, recalling, etc.).

No class may be both at once. If a slice needs both faces, it mints a moment class *and* a process
class and links them with `gmeow:realizesMentalMoment`/`gmeow:producesMentalMoment`/`gmeow:updatesMentalTenure`.

### Axis II — Content types

Mental moments and processes are *about* something. The content axis is typed by direction of fit
and cognitive role, not by syntactic form:

- **Proposition** — representational content with truth-conditions (belief, judgment).
- **Goal** — teleological content with satisfaction-conditions (intention, plan, want).
- **Question** — erotetic content with answer-conditions (wondering, query, open problem).
- **Concept** — categorical content used to classify (concept possession, category activation).
- **Imagined / suppositional content** — content entertained without truth or action commitment
  (pretence, counterfactual, generative sample).

These content types are orthogonal to Axis I: a belief is an endurant with propositional content;
inferring is an occurrent process with propositional content.

### Axis III — Order / reflexivity

- **First-order** — mental content directed at the world (belief that it is raining, goal to buy
  milk).
- **Second-order / metacognition** — mental content directed at another mental state (belief that
  I am overconfident, uncertainty about a memory).
- **Global experiencer-state / awareness** — a modal background that modulates how all first- and
  second-order states are experienced or generated (wakefulness, sleep, focused attention,
  diffuse rumination, online sampling vs. offline training).

Awareness is not itself a content type; it is a cross-cutting mode whose values change the
operating regime of the agent.

### Axis IV — Cross-cutting machinery

These mechanisms apply to every moment and process, regardless of the other three axes:

- **Standpoint** — every mental claim is indexed to a vantage (Principle 9; see
  [`docs/standpoints.md`](./standpoints.md)).
- **Confidence / credence / probability** — a graded attitude attached to a moment, not a content
  type of its own.
- **Provenance** — source, derivation, evidence span, and temporal scope (Principle 14).
- **Temporal scoping** — valid-time, transaction-time, and event phases are recorded explicitly.
- **Suppression-not-erasure** — outdated or withdrawn mental content is marked `displayable false`,
  never deleted (Principle 14).
- **Unified-observation stance** — a measurement, a perception, and an inferred claim are the same
  reified construct: an attributed, dated, confidence-weighted claim from a vantage (Principle 9).

---

## 4. Human↔AI bridging table

The table below maps each faculty to a human face and a machine/AI face. It does **not** assert
identity; it treats them as shared faculty architecture realized in substrate-specific ways.

| Faculty | Human face | Machine/AI face | Bridge properties |
|---|---|---|---|
| Belief / credence | Subjective probability / confidence | Logits, probabilities, token likelihood | `derivedCredence`, `calibrationEvidence` |
| Attention | Selective focus | Attention weights / activation maps | `hasModelObservable` |
| Memory | Recall / recognition | Context window / RAG retrieval / trained weights | `implementationSubstrate` |
| Inference | Reasoning / deduction / abduction | Chain-of-thought / tool-call derivation | `computationalCorrelate` |
| Imagination | Suppositional / counterfactual thought | Generative sampling / latent rollouts | `contentOrigin` |
| Dreaming | Offline consolidation / replay | Offline generative replay / synthetic rehearsal | `computationalCorrelate` |
| Awareness mode | Wake / sleep / focused / diffuse | Online / offline / training / sampling regime | `implementationSubstrate` |
| Metacognition | Uncertainty estimation / calibration | Model confidence / calibration curves | `derivedCredence`, `calibrationEvidence` |

These bridges are authored as SSSOM/EDOAL mapping cells in the DSL, not as `owl:sameAs`
assertions. The machine face is a *computational correlate* of the faculty, not a reduction of it.

---

## 5. Slice roster & dependency DAG

The mentation epic is delivered through nine slices. `mentation` is the keystone: it defines the
shared top-level classes and relations that the other slices specialize. `dreaming` is an
extension and a composition stress-test.

| Slice | Directory | Role |
|---|---|---|
| `concepts` | `slices/core/concepts/` | Concept possession, category activation, concept learning. |
| `inquiry` | `slices/core/inquiry/` | Questions, problems, investigation states, answer conditions. |
| `mentation` | `slices/core/mentation/` | Keystone: `MentalMoment`, `MentalProcess`, `gmeow:realizesMentalMoment`, `gmeow:producesMentalMoment`, `gmeow:updatesMentalTenure`. |
| `inference` | `slices/core/inference/` | Reasoning processes and the claims they produce or update. |
| `learning` | `slices/core/learning/` | Learning processes and the memories/competencies they update. |
| `imagination` | `slices/core/imagination/` | Suppositional, counterfactual, and generative content. |
| `metacognition` | `slices/core/metacognition/` | Second-order mental states: confidence, calibration, uncertainty. |
| `awareness` | `slices/core/awareness/` | Global experiencer-states and awareness modes. |
| `dreaming` | `slices/extensions/dreaming/` | Extension; offline replay / rehearsal as a composition stress-test. |

### Build order

```text
concepts
   │
   ▼
inquiry
   │
   ▼
mentation        ← keystone
   │
   ├──► inference
   │
   └──► learning
           │
           ▼
      imagination
           │
           ▼
      metacognition
           │
           ▼
      awareness
           │
           ▼
      dreaming   ← extension / stress-test
```

Extension-dependency rules apply: `dreaming` depends on core slices only and is built as a
separate GTS bundle. No extension→extension edge is permitted.

---

## 6. External linkage roster

Each slice bridges to external vocabularies by reference. The list below is the target set, not an
import list. Axioms from these sources are never copied into GMEOW; they are linked through the
mapping compiler.

| Slice | External vocabularies / ontologies |
|---|---|
| `concepts` | SKOS, OntoLex, Cognitive Atlas, gUFO/BFO/DOLCE/SUMO |
| `inquiry` | schema.org Q&A, Cognitive Paradigm Ontology (CPO), CIDOC / CRMinf |
| `mentation` | gUFO, BFO, DOLCE/DUL, SUMO |
| `inference` | PROV-O, CRMinf, W3C PROV constraints |
| `learning` | xAPI, Cognitive Atlas, MFOEM |
| `imagination` | OntoLex, Cognitive Atlas, generative-AI model cards (by reference) |
| `metacognition` | MFOEM, PROV-O, calibration / uncertainty vocabularies |
| `awareness` | Sleep / consciousness clinical vocabularies, MFOEM, gUFO/BFO |
| `dreaming` | Sleep clinical vocabularies, MFOEM, Cognitive Atlas |

The foundational bridge is documented separately in
[`docs/foundational-bridging.md`](./foundational-bridging.md). The mapping pattern is documented
in [`docs/projections.md`](./projections.md).

---

## 7. Guardrails for implementers

1. **Separate occurrent processes from relators and commitments.** Inference is a process; the
   inferred belief is a `MentalMoment`; the commitment/attribution that records *who* inferred it
   and *from what evidence* is a relator. Do not fold all three into one class.
2. **Distinguish `gmeow:realizesMentalMoment`, `gmeow:producesMentalMoment`, and
   `gmeow:updatesMentalTenure`.** A process can do all three, but the relation used must match the
   ontological job. Never collapse them into a single generic "causes" or "results in" property.
3. **Human↔AI mappings are shared faculty architecture, not identity.** `attention` is not
   identical to transformer attention weights; it is the same faculty realized in a biological
   substrate and in a computational substrate. Bridge properties (`hasModelObservable`,
   `computationalCorrelate`, `implementationSubstrate`) carry the mapping, never `owl:sameAs`.

---

## 8. Glossary

- **`MentalMoment`** — An endurant mental state inhering in an agent at a time, modelled as a
  sub-class of `gufo:IntrinsicMode`. Examples: a belief, an intention, a wondering, a perceptual
  gestalt.
- **`MentalProcess`** — An occurrent mental happening extended in time, modelled as a sub-class of
  `gmeow:Event`. Examples: inferring, recalling, attending, imagining, dreaming.
- **`Experience`** — A standpoint-indexed, temporally-scoped `MentalProcess` (occurrent) whose content is phenomenally or functionally present to the agent. The one declared subclass of `MentalProcess` in the mentation slice; finer experiential kinds are typed via `mentalProcessType` rather than via further subclasses.
- **`realizesMentalMoment`** — Manifestation relation: a `MentalProcess` realizes an already-potential `MentalMoment` — the process makes present or actualises an existing capacity or state rather than creating something new. Distinct from the creative-works relation `gmeow:realizes` (Expression to Work).
- **`producesMentalMoment`** — Creation relation: a `MentalProcess` brings a new `MentalMoment` into being — the process is the causal origin of a fresh belief, knowledge-state, or perceptual claim. Distinct from the learning-slice relation `gmeow:produces`.
- **`updatesMentalTenure`** — Revision relation: a `MentalProcess` revises, extends, or closes an existing `TimeScopedRelation` representing a held mental tenure (a belief tenure, knowledge tenure, or other time-scoped mental holding).
- **`contentOrigin`** — An annotation property indicating how mental content came about
  (perception, inference, imagination, testimony, generative sampling, etc.). It types the
  *genesis* of the content, not its truth value.
- **`originGenerated`** — A value of `contentOrigin` for content produced by a generative or
  constructive process (human imagination, LLM sampling, synthetic rehearsal). Generated content
  is not presumed false; it is explicitly flagged for reality-monitoring.
- **`implementationSubstrate`** — A bridge property that records the concrete medium or mechanism
  realizing a faculty in a particular agent (biological neural tissue, context window, RAG index,
  trained parameter tensor, sleep stage).
- **`computationalCorrelate`** — A bridge property that links a mental faculty or process to a
  machine-computable analogue (chain-of-thought derivation, attention map, calibration curve,
  offline replay) without reducing the faculty to that analogue.
- **`hasModelObservable`** — A bridge property that points to a machine-observable token of a
  mental state (e.g. attention weights, activation map, logit distribution) that can serve as
  evidence for an attribution.
- **`derivedCredence`** — A confidence or credence value attached to a `MentalMoment` as a
  standpoint-indexed claim. It is not a probability unless a probability frame is explicitly
  declared.
- **`calibrationEvidence`** — Evidence used to justify or assess a `derivedCredence`, such as a
  track record, a calibration curve, or a model-evaluation run. Always linked by reference, never
  materialised as part of the moment itself.

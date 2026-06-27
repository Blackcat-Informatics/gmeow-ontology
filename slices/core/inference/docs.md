<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# inference

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/inference` · **tier: core**

Peirce's tetrad — **deduction, induction, abduction, and analogy** — modelled as the *epistemic face*
of a `logic:` derivation: what an agent believes when it accepts a reasoning step's conclusion. The
conclusion of every inference is an ordinary `gmeow:StandpointClaim` (vantage = the reasoner,
observedFeature = the concluded proposition, `gmeow:claimModality` = the mode's default). The four
modes differ by a **value plus a warrant, never by subsumption** — the deception discipline applied to
reasoning. Strength rides the existing `logic:` axes; modality reuses the existing
`gmeow:StandpointModality`; nothing here re-mints a truth bit (Principle 6).

## The endurant/occurrent split

The issue body first proposed a single `gmeow:Inference` that was *both* a `gufo:Relator` (an
endurant) and a `gmeow:MentalProcess` (an occurrent). That double-types a class across gUFO's master
endurant/occurrent split — the very distinction the mentation program exists to keep clean — so
the design was corrected into two classes joined by a bridge:

- **`gmeow:InferenceProcess`** `⊑ gmeow:MentalProcess` — the **occurrent** reasoning episode (a
  perdurant): the reasoning as it unfolds in time, borne by one `gmeow:experiencer`, carrying
  `gmeow:mentalProcessType gmeow:processReasoning`, and `gmeow:producesMentalMoment` the belief it
  creates. This is the reparenting hook the mentation slice reserves.
- **`gmeow:InferenceCommitment`** `⊑ gufo:Relator` — the **endurant** structured argument relation
  (premises × conclusion × warrant × defeaters), the reified Toulmin/Peircean commitment.
- **`gmeow:hasInferenceCommitment`** bridges the process to the commitment it instantiates.

`gufo:Relator` is reached only via `rdfs:subClassOf`; the sole gUFO master metaclass on
`InferenceCommitment` is `gufo:Kind` (the `gmeow:Commitment` idiom). This keeps every class to exactly
one master and the two faces of inference ontologically distinct yet linked.

## Three tiers of fidelity

Flat-first, reify on demand (Principle 4):

1. **Flat spine** (the 80% case) — hang `gmeow:inferenceMode` and `gmeow:inferredFrom` directly on the
   conclusion `gmeow:StandpointClaim`. No reification.
2. **Reified commitment** — a `gmeow:InferenceCommitment` when the argument structure (premises,
   conclusion, warrant, defeaters) matters.
3. **Occurrent process** — a `gmeow:InferenceProcess` when the reasoning episode joins the agent's
   mental timeline (the mentation spine).

## The four modes

| Mode | substrate | truth-preserving | default modality | defeasible | competing |
|---|---|---|---|---|---|
| `gmeow:modeDeduction` | a `logic:` proof-trace under a monotonic profile | yes (rel. premises) | `gmeow:unequivocal` | no | no |
| `gmeow:modeInduction` | generalisation over a sample; `logic:evidenceStrength` | no | `gmeow:probable` | yes | rival generalisation |
| `gmeow:modeAbduction` | inference to the best explanation (IBE) + back-check | no | `gmeow:conceivable`→`gmeow:probable` | yes | **yes** |
| `gmeow:modeAnalogical` | source→target structure-mapping (Gentner) | no | `gmeow:conceivable`→`gmeow:probable` | yes | rival mappings |

At the `logic:` derivation layer the deductive mode is read off the profile and never separately
tagged (Principle 4); `gmeow:modeDeduction` is the legitimate way to record, at the *epistemic/memory*
layer, that the agent reached a claim by deduction.

## Classes

### gmeow:InferenceProcess

The occurrent reasoning episode (`gufo:EventType ⊑ gmeow:MentalProcess`). Carries `gmeow:experiencer`,
`gmeow:mentalProcessType gmeow:processReasoning`, a temporal frame (the events idiom), and
`gmeow:producesMentalMoment` (the belief created); reaches its argument via `gmeow:hasInferenceCommitment`.

### gmeow:InferenceCommitment

The endurant argument relation (`gufo:Kind ⊑ gufo:Relator`). Mediates `gmeow:premise` (≥1),
`gmeow:conclusion` (exactly 1, a claim), `gmeow:warrant`, `gmeow:inferenceModeOf`, and
`gmeow:hasDefeater`. Retained as audit when its conclusion is suppressed.

### gmeow:Analogy

A reified structure-mapping (`gufo:Kind ⊑ gufo:Relator`): `gmeow:analogicalSource`,
`gmeow:analogicalTarget`, the element-pair `gmeow:hasCorrespondence` relations, and a solver-layer
`gmeow:systematicity` score (Gentner). The warrant of an analogical commitment points at an Analogy.

### gmeow:Correspondence

A single mapped element-pair within an Analogy (`gufo:Kind ⊑ gufo:Relator`):
`gmeow:correspondingSource` and `gmeow:correspondingTarget`, each exactly one.

### gmeow:InferenceMode

The Peircean mode value vocabulary (`gufo:AbstractIndividualType ⊑ gufo:QualityValue`):
`gmeow:modeDeduction`, `gmeow:modeInduction`, `gmeow:modeAbduction`, `gmeow:modeAnalogical`.

### gmeow:DefeaterKind

The Pollock defeater value vocabulary (`gufo:AbstractIndividualType ⊑ gufo:QualityValue`):
`gmeow:defeaterRebutting` (attacks the conclusion) and `gmeow:defeaterUndercutting` (attacks the
inferential link).

### gmeow:InferenceTenure

The time-scoped fact (`gufo:SituationType ⊑ gmeow:TimeScopedRelation`) that a commitment's conclusion
was held over an interval — opened on acceptance, closed when a defeater fires (`gmeow:tenureOf` names
the commitment; `gmeow:duringInterval` carries the period).

## Abduction — inference to the best explanation

An abductive `gmeow:InferenceCommitment` names the fact to explain (`gmeow:explanandum`) and the
competing `gmeow:candidateHypothesis` claims; each hypothesis `gmeow:explains` the explanandum (a
solver back-check) and carries a solver-layer `gmeow:explanatoryScore`. There is **no `isBest` bit**
(Principle 12): the winner's `gmeow:claimModality` is promoted `conceivable`→`probable`, and the losers
are suppressed (`gmeow:displayable false`), never erased. Rivals are linked by the symmetric
`gmeow:competesWith` (irreflexive by SHACL, keeping the DL profile clean).

## Belief revision is suppression

A fired defeater (`gmeow:hasDefeater`, kinded by `gmeow:defeaterKind`) sets the conclusion-claim
`gmeow:displayable false` and **closes** the `gmeow:InferenceTenure` (an end on its interval); the
`gmeow:InferenceCommitment` is retained as audit (Principle 10). The whole episode — how the agent
believed, and why it stopped — stays queryable. This is the headline demonstration for the
agent-memory consumer (Principle 15): an LLM stating *how* it reached a claim and revising belief
without deleting history.

## Alignment

By reference only (Principle 5; see `mappings/equivalences.ttl`): PROV-O (`InferenceProcess` ⟶
`prov:Activity`, `inferredFrom` ⟶ `prov:wasDerivedFrom`); CRMinf (`InferenceProcess` ⟶
`I5_Inference_Making`, `InferenceCommitment` ⟶ `I1_Argumentation`, `StandpointModality` ⟶
`I6_Belief_Value`); Wikidata for the four modes (curl-verified). Peirce's original tetrad, Toulmin's
warrant model, Gentner's Structure-Mapping Engine, and Pollock's defeater distinction are referenced
in prose (no stable SSSOM-suitable namespace).

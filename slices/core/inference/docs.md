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

Inference is not a single class. Modelling it as one relation that was *both* a `gufo:Relator` (an
endurant) and a `gmeow:MentalProcess` (an occurrent) would double-type a class across gUFO's master
endurant/occurrent split — the very distinction the mentation program exists to keep clean — so the
design splits it into two classes joined by a bridge:

- **`gmeow:InferenceProcess`** `⊑ gmeow:MentalProcess` — the **occurrent** reasoning episode (a
  perdurant): the reasoning as it unfolds in time, borne by one `gmeow:experiencer`, carrying
  `gmeow:mentalProcessType gmeow:processReasoning`, and `gmeow:producesMentalMoment` the belief it
  creates. This is the reparenting hook the mentation slice reserves.
- **`gmeow:InferenceCommitment`** `⊑ gufo:Relator` — the **endurant** structured argument relation
  (premises × conclusion × warrant), the reified Toulmin/Peircean commitment.
- **`gmeow:hasInferenceCommitment`** bridges the process to the commitment it instantiates.

`gufo:Relator` is reached only via `rdfs:subClassOf`; the sole gUFO master metaclass on
`InferenceCommitment` is `gufo:Kind` (the `gmeow:Commitment` idiom). This keeps every class to exactly
one master and the two faces of inference ontologically distinct yet linked.

## Three tiers of fidelity

Flat-first, reify on demand (Principle 4):

1. **Flat spine** (the 80% case) — hang `gmeow:inferenceMode` and `gmeow:inferredFrom` directly on the
   conclusion `gmeow:StandpointClaim`. No reification.
2. **Reified commitment** — a `gmeow:InferenceCommitment` when the argument structure (premises,
   conclusion, warrant) matters.
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
`gmeow:conclusion` (exactly 1, a claim), `gmeow:warrant`, and `gmeow:inferenceModeOf`. The
argumentation layer wraps it in a `gmeow:Argument` to bear typed conflict and support. Retained as
audit when its conclusion is suppressed.

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

### gmeow:InferenceTenure

The time-scoped fact (`gufo:SituationType ⊑ gmeow:TimeScopedRelation`) that a commitment's conclusion
was held over an interval — opened on acceptance, closed when the solver evaluates the corresponding
`gmeow:Argument` as `gmeow:acceptanceOut` via a `gmeow:ArgumentEvaluation` (`gmeow:tenureOf` names
the commitment; `gmeow:duringInterval` carries the period).

## The argumentation layer (Dung / ASPIC+)

A reasoning step rarely stands alone — arguments support and attack one another, and which survive is
decided under a *named acceptability semantics*. The `gmeow:Argument` is the attackable framework node
(`gufo:Kind ⊑ gufo:Relator`) that wraps at most one Toulmin step (`gmeow:argumentInferenceStep` →
`gmeow:InferenceCommitment`); it rests on the premises it actually **uses** (`gmeow:hasPremiseUse` →
`gmeow:PremiseUse`, *available ≠ used*) via the rule it applies (`gmeow:hasInferenceApplication` →
`gmeow:InferenceApplication`, rule + substitution), and concludes (`gmeow:argumentConclusion`).

**Typed attacks name what they attack.** A `gmeow:Attack` carries a `gmeow:attackSource`, a
`gmeow:attackTarget` (a `gmeow:AttackTarget` — the named union of `gmeow:PremiseUse`,
`gmeow:InferenceApplication`, or conclusion `gmeow:StandpointClaim`), and a `gmeow:attackKind`:

- `gmeow:attackUndermine` — attacks a **premise** (the `gmeow:PremiseUse`);
- `gmeow:attackUndercut` — attacks the **warrant** (the `gmeow:InferenceApplication`): grants the
  premises but denies they support the conclusion;
- `gmeow:attackRebut` — attacks the **conclusion** directly, arguing for its contrary.

These three (`gmeow:AttackKind`, `gufo:AbstractIndividualType ⊑ gufo:QualityValue`) subsume the older
Pollock two-way rebutting/undercutting; *undermine* is the premise-directed third. Self-attack is
forbidden in SHACL, not OWL (the `gmeow:competesWith` discipline — keeps the DL profile clean).

**Support accrues.** Each `gmeow:Support` (`gmeow:supportSource` → `gmeow:supportTarget`) is its own
instance, so several independent supports for one conclusion *combine* rather than counting once.

**Acceptability is computed, never asserted.** A `gmeow:ArgumentEvaluation` records the solver verdict:
`gmeow:evaluatesArgument`, the named `gmeow:underSemantics` (a `logic:ArgumentationSemantics` —
`logic:GroundedArgumentation` / `logic:PreferredArgumentation` / `logic:NoArgumentation` /
`logic:PolicySpecificArgumentation`), an `gmeow:acceptanceStatus` (`gmeow:acceptanceIn` /
`gmeow:acceptanceOut` / `gmeow:acceptanceUndecided` — the Dung labelling), and its
`gmeow:extensionMember`s. There is **no `accepted`/`isBest` bit** (Principle 12); under a credulous
(preferred) semantics several evaluations of one argument coexist, one per extension.

## Abduction — inference to the best explanation

An abductive `gmeow:InferenceCommitment` names the fact to explain (`gmeow:explanandum`) and the
competing `gmeow:candidateHypothesis` claims; each hypothesis `gmeow:explains` the explanandum (a
solver back-check) and carries a solver-layer `gmeow:explanatoryScore`. There is **no `isBest` bit**
(Principle 12): the winner's `gmeow:claimModality` is promoted `conceivable`→`probable`, and the losers
are suppressed (`gmeow:displayable false`), never erased. Rivals are linked by the symmetric
`gmeow:competesWith` (irreflexive by SHACL, keeping the DL profile clean).

## Belief revision is suppression

When the solver evaluates a `gmeow:Argument` as `gmeow:acceptanceOut` via a
`gmeow:ArgumentEvaluation` — a verdict to which a `gmeow:Attack` contributes — suppression follows:
the conclusion-claim is marked `gmeow:displayable false` and the `gmeow:InferenceTenure` is
**closed** (an end on its interval); the `gmeow:InferenceCommitment` and its `gmeow:Argument` are
retained as audit (Principle 10). The whole episode — how the agent believed, and why it stopped —
stays queryable. This is the headline demonstration for the agent-memory consumer (Principle 15): an
LLM stating *how* it reached a claim and revising belief without deleting history.

## Alignment

By reference only (Principle 5; see `mappings/equivalences.ttl`): PROV-O (`InferenceProcess` ⟶
`prov:Activity`, `inferredFrom` ⟶ `prov:wasDerivedFrom`); CRMinf (`InferenceProcess` ⟶
`I5_Inference_Making`, `InferenceCommitment` ⟶ `I1_Argumentation`, `StandpointModality` ⟶
`I6_Belief_Value`); Wikidata for the four modes (curl-verified). Peirce's original tetrad, Toulmin's
warrant model, Gentner's Structure-Mapping Engine, Pollock's defeater distinction (now the typed
`gmeow:AttackKind`), and AIF / ASPIC+ / Dung abstract argumentation (`gmeow:Argument`, `gmeow:Attack`,
`gmeow:Support`, `gmeow:ArgumentEvaluation`) are referenced in prose (no stable SSSOM-suitable namespace).

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Cognitive Assessment

> The cognitive-assessment member of the GMEOW Logic design set ([`LOGIC.md`](LOGIC.md)). It gives
> the meaning of evaluating reasoning in `logic:`: a reified, factored assessment construct that
> carries reasoning quality, reliability, calibration, and metacognitive posture across independent
> dimensions rather than on one ordinal scale. The reasoning whose quality it assesses is the typed
> reasoning result of [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md); the credence/confidence factoring
> it reads is the factored claim modality of [`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md).

## Why cognitive assessment is its own layer

The logic produces reasoning: derivations, explanations, inferences with warrants and defeaters, and
a typed result that records how much an answer is trusted. Assessing *that reasoning* — how good it
is, how reliable the reasoner is, whether confidence is calibrated, whether the reasoner knows the
limits of its own knowledge — is a distinct, second-order concern. It ranges over reasoning the
logic already carries, and it is multidimensional: a reasoner may be highly competent yet badly
calibrated, fluent in a domain yet unable to transfer, or confident and wrong. A single ordinal
familiarity-to-mastery scale collapses these independent axes into one number and loses exactly the
distinctions assessment exists to draw. The cognitive-assessment layer keeps them factored.

The layer is authored over GMEOW's own vocabulary of mind. Reasoning is carried by
`gmeow:InferenceProcess` and committed to through `gmeow:InferenceCommitment`, each with a
`gmeow:premise`, a `gmeow:conclusion`, a `gmeow:warrant`, and an `gmeow:inferenceMode` drawn from
deduction, induction, abduction, and the analogical mode; defeaters attach through
`gmeow:hasDefeater` with a `gmeow:DefeaterKind` of rebutting or undercutting. Second-order states are
`gmeow:MetacognitiveState`, pointing at what they are about through `gmeow:metaTarget`. Reasoning
unfolds as a `gmeow:MentalProcess`, and competence changes through a `gmeow:LearningEvent`. The
cognitive-assessment layer of `logic:` adds the *structured judgment* over these — the assessment
construct and its dimensions — as values the engine computes with.

## The cognitive assessment is reified

The canonical record of how good some reasoning is, or how capable a reasoner is, is a **cognitive
assessment**: a reified construct carrying

- the **assessed agent** — whose cognition is judged;
- the **subject** and its **granularity** — a single inference, a body of reasoning, a domain of
  competence, or the reasoner as a whole;
- the **task or context** — what the assessment holds for, since competence is context-relative;
- the **dimension** — which factored axis (below) is being judged;
- the **scale and level** — the value on that dimension's own scale, ordinal or continuous;
- the **evaluator and standpoint** — who holds the judgment, attributed through `gmeow:accordingTo`;
- the **evidence** — the reasoning, observations, or record the judgment rests on;
- the **method** — how the judgment was reached;
- the **valid interval** — the time over which the judgment holds;
- a **confidence** — how strongly the judgment itself is held, carried under the `Uncertainty` facet.

An assessment is vantage-indexed like any other claim: a self-assessment and an observer's
assessment of the same agent are two coexisting records, never reconciled into one verdict. Because
the construct is reified, an assessment is itself a subject other assessments can range over — an
evaluation of the evaluator — without any new machinery.

## The factored dimensions

A cognitive assessment names exactly one dimension, and the dimensions vary independently:

- **awareness** — whether the agent registers the subject at all;
- **declarative familiarity** — what the agent knows *that*;
- **conceptual understanding** — whether the agent grasps *why*, the structure behind the facts;
- **procedural competence** — what the agent knows *how* to do;
- **transfer** — whether competence in one context carries to another, the dimension a single ladder
  most conspicuously hides;
- **explanatory ability** — whether the agent can produce an account that others can follow and
  check, the seam to the explanatory and systematicity scores below;
- **calibration** — whether the agent's confidence matches its accuracy;
- **metacognition** — whether the agent models its own competence, including the limits of its own
  knowledge.

Each is its own scale; an assessment on one says nothing about another. The set is an open value
vocabulary of dimensions, extended by naming a new dimension rather than by reshaping the construct,
the same orthogonal-factorization discipline the foundation applies to claim modality and to
contextual scope.

## Canonical representation

The assessment construct has one canonical form. A `logic:CognitiveAssessment` is a reified node
carrying its subjects and values through dedicated properties; the dimension, scale, and level are a
value vocabulary, not a class hierarchy, so a new dimension is a new individual rather than a schema
change.

| Element | Canonical form |
| --- | --- |
| assessment | `logic:CognitiveAssessment`, a reified node, vantage-indexed via `gmeow:accordingTo` |
| assessed agent | `logic:assessedAgent → gmeow:Agent` |
| subject + granularity | `logic:assessmentSubject` with `logic:subjectGranularity` (inference / body-of-reasoning / domain / agent) |
| task or context | `logic:assessmentContext` |
| dimension | `logic:assessmentDimension`, an individual of `logic:AssessmentDimension` (the value vocabulary above) |
| scale + level | `logic:assessmentScale` with `logic:assessmentLevel`, ordinal or continuous on the named scale |
| evaluator | `logic:assessmentEvaluator` / standpoint, attributed via `gmeow:accordingTo` |
| evidence | `logic:assessmentEvidence` → the reasoning, results, or record cited |
| method | `logic:assessmentMethod` |
| valid interval | `gmeow:duringInterval` (temporal slice) |
| confidence | `logic:confidence` on the assessment |

The dimensions — awareness, declarative familiarity, conceptual understanding, procedural competence,
transfer, explanatory ability, calibration, metacognition — are `logic:AssessmentDimension`
individuals; an assessment names exactly one. Because the assessment is reified, it is itself a
subject another assessment can range over, with no new machinery. The projections weaken the
construct predictably, and each lowering records what it drops in the preservation ledger
([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md)).

## Reasoning quality over the inference modes

Quality is assessed against the mode of the reasoning being judged, because a good induction and a
good deduction answer to different standards. A deduction is judged by the validity of its
`gmeow:warrant`; an induction by the breadth and representativeness of the evidence its warrant
generalizes; an abduction by its `gmeow:explanatoryScore` relative to the hypotheses it
`gmeow:competesWith`; the analogical mode by the `gmeow:systematicity` of the correspondence it
rests on. Defeat is part of quality: a `gmeow:hasDefeater` of kind rebutting attacks the conclusion,
one of kind undercutting attacks the warrant, and an assessment of reasoning quality reads both. The
standards an argument is held to — the acceptability of an inference under attack — are the typed
argumentation account of
[`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md#argumentation-and-epistemic-standards); the
cognitive-assessment layer applies those standards as the criterion of a quality dimension rather
than restating them.

The explanatory, systematicity, and similar scores are solver-layer measurements: they rank and
compare reasoning, and they are recorded as annotations on the assessment, never asserted as
reasoned axioms. They feed an assessment; they are not entailments of one.

## Result assurance and reasoner reliability are distinct

A single reasoning result carries assurance *about that run*; it does not establish that an agent or
engine is reliable as a disposition. The two are separate constructs.

A **result assurance** (`logic:ResultAssurance`) targets one `logic:ReasoningResult` and reads its
fields — completion, preservation, the witnesses it carries, and the assumptions it rests on
([`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md#the-reasoning-result)). The fields are interpreted
*jointly*: `supported` with `incomplete` is provisional, whereas `supported` with
complete-for-the-fragment is conclusive within that fragment. A correctly returned `both` is
high-assurance about a contradictory input, not a defect.

A **reliability assessment** is a `logic:CognitiveAssessment` on the reliability dimension whose
target is an agent or engine, not a single run. It is relative to a **reference class** — a task,
domain, or distribution — and rests on *many* results with their expected or observed outcomes; its
level is a metric over that series (accuracy, soundness-violation rate, recall, calibration,
robustness) across a stated interval. A reliable reasoner may correctly return `both` when its
evidence is contradictory, and a completed, exact, internally supported result may still be wrong
because its premises are wrong or its solver has a systematic defect. Reliability is therefore read
from the series against the reference class, never from one result's assurance.

## Calibration and metacognitive posture

Calibration is the match between confidence and accuracy, and the metacognition vocabulary carries
it directly: `gmeow:calibration` records a `gmeow:CalibrationStatus` of well-calibrated,
overconfident, or underconfident, and `gmeow:calibrationError` records the magnitude as a
solver-layer measurement. Calibration is assessed against the four distinct quantitative predicates
the foundation keeps apart — `logic:confidence`, `logic:probability`, `logic:weight`, and
`logic:evidenceStrength`
([`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md#confidence-probability-weight-and-evidence)) — so a
calibration judgment compares a stated confidence against an observed frequency without confusing
either for a probability model or an evidential weight. The credence the agent holds is distinct
again: it is the doxastic-commitment axis of the factored claim modality
([`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md#factored-claim-modality)), and the cognitive-assessment
layer reads that factoring rather than restating it.

A calibration assessment names an explicit **forecast protocol**, because "confidence matched against
frequency" is underdetermined without one. It carries the forecast or credence being scored, the
corresponding outcome, the mapping from the forecast value to a probability where the forecast is not
already one, the reference class and observation window over which frequency is measured, the scoring
rule (a Brier or logarithmic score), and the treatment of censored or unresolved outcomes. Solver
`logic:weight` and `logic:evidenceStrength` values are not themselves calibratable probabilities;
they enter a calibration only through a declared mapping to one, never directly.

Metacognitive posture is the second-order content the metacognition vocabulary carries: a
`gmeow:MetacognitiveState` aimed by `gmeow:metaTarget` at the agent's own reasoning,
`gmeow:awareOfNotKnowing` for the recognized limits of knowledge, and `gmeow:epistemicSelfTrust` for
the degree to which an agent relies on its own judgment. A reflection — an event typed by
`gmeow:eventTypeReflection` — reviews the agent's own reasoning and may install a `gmeow:hasDefeater`
on the agent's own inference commitment, revising belief by suppression rather than erasure. The
metacognition dimension of an assessment reads this posture as its subject.

## Assessment over recorded reasoning

Cognitive assessment ranges over reasoning that the provenance layer has recorded, so a judgment can
be traced to the episode it judges. Reasoning carried out through a model invocation links its
products back through `gmeow:wasGeneratedBy` to a `gmeow:ModelInvocation`, and a `gmeow:ToolCall`
records which tool an agent used, called by which invocation, with what arguments and result. An
assessment cites these as its evidence: the reasoning it judges, the inference commitments it rests
on, and the invocations that produced them are all in one provenance graph, so an assessment is
auditable against the reasoning it grades rather than standing apart from it.

## Competence over time

Competence changes, and the change is recorded rather than overwritten. A `gmeow:LearningEvent`
carries an agent from one competence level to another — concept formation, skill acquisition, being
taught, consolidation, transfer, or unlearning — and a sequence of assessments over the same subject
and dimension, each with its own valid interval, is the trajectory of that competence. A later
assessment never edits an earlier one: the earlier judgment remains, attributed and time-scoped, and
the trajectory is the ordered series, the same suppression-not-erasure discipline the rest of the
logic keeps.

## Projection to the coarse knowledge ladder

The cognition vocabulary carries a coarse, monotonic knowledge ladder —
`gmeow:hasMastered ⊑ gmeow:understands ⊑ gmeow:knowsAbout ⊑ gmeow:isAwareOf`, chained by
`rdfs:subPropertyOf` so the deeper edge entails every shallower one. That ladder is a **generated
coarse view** of the multidimensional assessment, not an independent account: each edge is emitted
from assessments by a declared threshold over the relevant dimensions. `gmeow:isAwareOf` is emitted
when the awareness dimension clears its threshold, `gmeow:knowsAbout` from declarative familiarity,
`gmeow:understands` from conceptual understanding, and `gmeow:hasMastered` from procedural competence
and transfer together. The thresholds are a declared profile, so the ordinal chain is a projection a
consumer that needs one number can read, while the factored assessment stays the canonical record —
the ladder never re-establishes mastery, understanding, knowledge, and awareness as a single ordinal
entailment chain in place of the independent dimensions.

## Where it connects, and what it is not

The cognitive-assessment layer judges reasoning; it does not perform it. It reads the typed reasoning
result rather than producing one, applies the argumentation standards rather than defining them, and
reads the factored claim modality rather than restating it. It is the second-order counterpart of
the goal-and-action layer ([`LOGIC-TELEOLOGY.md`](LOGIC-TELEOLOGY.md)): where teleology assesses
*whether a goal is met*, cognitive assessment assesses *how good the reasoning toward it is*, and an
action schema's observation is the seam where executing an action feeds the evidence an assessment
cites. On the foundational spine, an assessment is a claim about an agent's intrinsic cognitive
modes, so it composes with identity, rigidity, and the rest of the UFO⁺ sorts of
[`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md) rather than standing beside them.

## Conformance obligations

The conformance corpus ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md)) carries this layer through
named cases that pin its distinctions, at least:

- a correct `both` reasoning result that yields high **result assurance** and does **not** lower the
  reasoner's **reliability** assessment;
- a `completed` result distinguished from an `incomplete` one in result assurance, read jointly with
  the support field;
- a **calibration** over an explicit forecast/outcome series with a declared scoring rule and
  reference class, kept apart from a single confidence value;
- the **projection** from a multidimensional assessment to the coarse
  `gmeow:isAwareOf … gmeow:hasMastered` ladder under a declared threshold profile;
- two coexisting **contested assessments** of one agent on one dimension, both retained.

## Constitutional alignment

Every assessment is a vantage-indexed claim carried through the statement layer, never a global
verdict on a mind; competence change is recorded as a trajectory of time-scoped assessments, never
an overwrite, so the history stays auditable; and the explanatory, calibration, and reliability
scores stay solver-layer measurements rather than reasoned axioms. The layer adds one orthogonal
dimension — the assessment of reasoning — composed with the rest of the reasoning contract rather
than re-bundling it, the same compositional discipline the logic enforces everywhere.

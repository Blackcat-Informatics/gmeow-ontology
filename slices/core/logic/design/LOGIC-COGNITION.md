<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — The Cognitive-Assessment Layer

> The cognitive-assessment member of the GMEOW Logic design set (see [`LOGIC.md`](LOGIC.md)): the
> factored, multidimensional evaluation of reasoning — quality, reliability, calibration, and
> metacognitive posture — over the typed reasoning result ([`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md)).

## What cognitive assessment is

Cognitive assessment is the factored evaluation of an agent's reasoning. It asks not merely *what an
agent concluded* but *how good that reasoning is* — across the quality of the inferential steps, the
reliability of the result they produced, the calibration of the agent's avowed confidence against its
accuracy, and the agent's second-order posture toward its own reasoning. The layer sits over the typed
`logic:ReasoningResult` ([`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md)) and the reified argument relations
of the inference layer, and it reads them rather than recomputing them.

It refuses the single ordinal awareness→mastery ladder. A lone score on one axis cannot represent
competence across subjects, tasks, and contexts: an agent fluent at a procedure may misunderstand the
concept beneath it; an agent that explains a domain well may be badly calibrated about its own grasp of
it. Collapsing those into one number manufactures a verdict the evidence never supports — the precise
collapse the foundation's factored claim modality and factored justification refuse
([`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md)). Assessment is therefore **multidimensional**, with each
dimension assessed and queried independently.

Assessment is also **vantage-indexed**. Every assessment is an assessment-*according-to-an-evaluator*,
never a global verdict on the agent. A self-assessment, a teacher's assessment, and an automated
grader's assessment of the same reasoning are three coexisting vantage-indexed claims, contestable like
any other, never one privileged truth. This is the standpoint discipline of
[`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md) applied to evaluation: an assessment is always *from
somewhere*.

## The CognitiveAssessment construct

A cognitive assessment is **reified** because a bare score discards everything that makes it
interpretable. The construct gathers, on one assessment, the parameters that a single number silently
conflates:

- the **assessed agent** — whose reasoning is evaluated;
- the **subject and granularity** — what the assessment is about, and at what grain (a whole domain,
  one topic, one inferential step);
- the **task or application context** — the conditions under which competence is claimed, since
  competence is task-relative, not absolute;
- the **dimension** — which of the factored axes (below) this assessment measures;
- the **scale and level** — the ordered value vocabulary the level is drawn from, and the level itself,
  with the scale named so two assessments on different scales are never silently compared;
- the **evaluator and standpoint** — the vantage the assessment is made from;
- the **evidence** — the reasoning, observations, and results the assessment reads, with
  `logic:evidenceStrength` carrying how strongly that evidence warrants the level;
- the **method** — how the level was arrived at;
- the **valid interval** — the period over which the assessment holds, so competence over time is a
  sequence rather than a mutation;
- the **confidence** — the evaluator's `logic:confidence` in its own assessment, a second-order
  quantity kept distinct from the level itself.

A **`gmeow:MetacognitiveState`** is the distinguished case where the assessed agent and the evaluator
are the same: a self-assessment, second-order, whose subject is one of the agent's own first-order
mental moments or claims. Its reflexivity edge is `gmeow:metaTarget`, whose range stays open — the
target is a first-order `gmeow:MentalMoment`, a standpoint claim, or a `gmeow:InferenceCommitment`. The
self-assessment and an external assessment of the same reasoning coexist as vantage-indexed claims; one
agent's confidence in its own grasp is exactly as contestable as any other evaluator's.

## Factored dimensions

The competence of an agent is read along independent axes, each assessed on its own and none implying
another:

- **awareness** — that a subject exists at all, the edge a known-unknown sits just past;
- **declarative familiarity** — knowing-that: facts held about the subject;
- **conceptual understanding** — grasp of the structure beneath the facts, what connects to what and
  why;
- **procedural competence** — knowing-how: the capacity to carry out the relevant procedure;
- **transfer** — applying competence in one domain to a genuinely new one, the analogical reach a
  `gmeow:LearningEvent` of variety transfer records;
- **explanatory ability** — making the reasoning intelligible to another, distinct from being able to
  perform it;
- **calibration** — the fit between avowed confidence and actual accuracy;
- **metacognition** — the agent's accurate model of its own competence and its boundaries.

Each is assessed independently, and **collapsing them is the loss this layer refuses**. An agent may be
procedurally fluent yet conceptually thin; declaratively rich yet unable to transfer; accurate yet
badly calibrated. The factored construct keeps each axis queryable so a consumer reads the profile, not
a flattened rank.

## Reasoning quality

The quality of a particular `gmeow:InferenceCommitment` — the reified Toulmin/Peirce argument relation
mediating its premises, conclusion, warrant, and defeaters — reads from the structure of the argument
itself, not from a separate grade.

The **`gmeow:InferenceMode`** sets the ceiling. Deduction (`gmeow:modeDeduction`) is truth-preserving
and non-defeasible: granted its premises and the applicability of its rule, the conclusion stands, and
quality reduces to the soundness of the step and the acceptance of its premises. Induction, abduction,
and analogy (`gmeow:modeInduction` / `gmeow:modeAbduction` / `gmeow:modeAnalogical`) are defeasible:
their conclusions are warranted to a degree, never guaranteed, so their quality is graded.

For a defeasible step, quality reads the **`gmeow:warrant`** strength — the Toulmin licence carrying
its `logic:evidenceStrength` — and the support standing of the **`gmeow:premise`** set. It then reads
any **`gmeow:hasDefeater`**, distinguishing by `gmeow:DefeaterKind`: a `gmeow:defeaterRebutting`
attacks the conclusion (gives reason to believe its negation), while a `gmeow:defeaterUndercutting`
attacks the inferential link (removes the warrant without asserting the negation). A live defeater of
either kind lowers the quality of the step it touches, and which kind it is tells a consumer *what* was
weakened — the conclusion or the link.

Abductive and analogical quality additionally read the solver-layer scores: an abductive commitment's
quality reflects its `gmeow:explanatoryScore` (how well the hypothesis accounts for its explanandum),
and an analogical commitment's quality reflects the `gmeow:systematicity` of its mapping (the depth and
interconnection of its structural correspondences). These are read, not reasoned (see
[the solver boundary](#the-solver-boundary)).

## Reliability

Reliability is read off the typed `logic:ReasoningResult`, not computed as a separate number. The five
fields each contribute a distinct dimension of how far a result can be trusted:

- **`completeness`** — `complete-for-fragment` means the answer is trustworthy *within its certified
  bound*: a hard guarantee, not a global one. `incomplete` means the answer is provisional, the search
  having not exhausted the space. `unknown` means the engine cannot characterize its own completeness.
- **`information`** — `supported` and `opposed` are conclusive verdicts (a proof with no counterproof,
  or its converse); `both` is a witnessed contradiction within one context; `neither` is the conclusive
  open-world silence of a finished search. These are reliable signals. `undetermined` means *no verdict
  was reached* — the search did not finish — so its silence establishes nothing. `not-evaluated` means
  *no semantics were available* — there was nothing to run. Reliability reads the difference: a
  conclusive verdict is dependable, a non-verdict is not.
- **`evaluation`** — `completed` is a result the engine stood behind; `budget-exhausted` is a result
  truncated by its resource allowance, whose answers are honestly incomplete rather than false.
- **`preservation`** — the set of polarities and the unsupported-construct set record what a lowering
  dropped. A result that passed through an under-approximation may have lost consequences relevant to
  the query, which a consumer must weigh before relying on it.

Reliability is therefore a **reading** of these fields, never a score laid over them. The result
already says, precisely, how far it can be trusted; the assessment layer surfaces that reading rather
than re-encoding it.

## Calibration and metacognitive posture

Calibration is the fit between an agent's avowed confidence and its actual accuracy. The qualitative
reading is **`gmeow:CalibrationStatus`** carried on a `gmeow:MetacognitiveState` via
`gmeow:calibration`: `gmeow:wellCalibrated` (confidence tracks accuracy), `gmeow:overconfident`
(confidence exceeds accuracy), and `gmeow:underconfident` (accuracy exceeds confidence). The numeric
magnitude of the gap is the solver-layer `gmeow:calibrationError`, a Brier-style score recorded but
never reasoned over (Principle 12).

The posture has further second-order components. **`gmeow:awareOfNotKnowing`** records a known-unknown
— an agent's recognition of a gap in its own knowledge, the explicit edge of what it knows. A
recognised gap is the inquiry seam: it raises a question rather than sitting as mere absence.
**`gmeow:epistemicSelfTrust`** records feeling-of-knowing — the reliability an agent accords one of its
own faculties or sources. And the reflection act, typed `gmeow:eventTypeReflection`, is an agent
reviewing its own reasoning; a reflection may install a `gmeow:hasDefeater` on the agent's own
`gmeow:InferenceCommitment`, revising belief as suppression rather than erasure.

Calibration depends on keeping the foundation's quantitative axes apart, exactly as
[`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md) requires: **credence** (an agent's graded degree of
belief) is not **confidence** (`logic:confidence`, a source's avowed certainty), is not **probability**
(`logic:probability`, which requires a declared model), is not **weight** (`logic:weight`, a solver
ranking). Calibration is the fit between *avowed confidence* and *actual accuracy* — a relation between
two of these axes, not a value on any single one. The assessment layer reuses these axes by reference
and never redefines them.

## Provenance of reasoning

An assessment attaches to reasoning whose origin is recorded, so that *whose* reasoning, produced
*how*, is part of the assessment rather than lost behind it. The reasoning episode is a
`gmeow:InferenceProcess` — the occurrent borne by its `gmeow:experiencer` — reaching its argument
structure through its `gmeow:InferenceCommitment`. For machine reasoning, the origin is a
`gmeow:ModelInvocation` (carrying `gmeow:usedModel`) and the `gmeow:ToolCall`s it issued (each carrying
`gmeow:usedTool` and, when the harness exposes it, `gmeow:calledByInvocation`).

Because an assessment names its assessed agent and its evidence, **assessing an AI agent's reasoning
quality and calibration is first-class**: an evaluator reads the model invocation and its tool calls,
reads the reasoning result they produced, and records a `CognitiveAssessment` of that reasoning's
quality, reliability, and calibration along the factored dimensions — vantage-indexed to the evaluator,
exactly like any assessment of a human agent.

## Competence over time

Competence is not static, and the layer represents its change without erasing its history. A
`gmeow:LearningEvent` is the occurrent that transitions competence — an acquisition, a consolidation, a
transfer, an unlearning — carrying its `gmeow:fromLevel` and `gmeow:toLevel` and producing the enduring
state it raised.

Because every assessment carries a valid interval, a **competence trajectory** or a **calibration
trajectory** is a sequence of vantage-indexed assessments ordered in time, each holding over its own
interval. A rise in competence is a sequence of assessments, never the mutation of one; a superseded
assessment is retained and suppressed, not deleted. This is **suppression-not-erasure**: the trajectory
remains fully auditable, the earlier assessment recoverable, and a later assessment along a different
vantage coexists with rather than overwrites an earlier one.

## The solver boundary

Three quantities this layer reads are **recorded numeric structure for the solver, never reasoned
axioms** (Principle 12): `gmeow:explanatoryScore` (the abductive ranking), `gmeow:systematicity` (the
analogical mapping score), and `gmeow:calibrationError` (the Brier-style miscalibration magnitude).
They are computed externally and carried as annotations, invisible to the reasoner. No entailment is
drawn from them; there is no derived *best* bit. The assessment layer reads these scores to inform a
quality or calibration reading, exactly as the inference layer ranks rivals by them without asserting a
winner — keeping heavy numeric computation out of the reasoned core and on the solver side of the
boundary the foundation draws.

## Constitutional alignment

The cognitive-assessment layer refuses the single score: competence is a profile across independently
assessed, independently queryable dimensions, never one ordinal rank. Every assessment is
vantage-indexed — an assessment-according-to-an-evaluator, contestable like any standpoint claim, never
a global verdict. The layer reuses the `logic:` uncertainty axes — `logic:confidence`,
`logic:probability`, `logic:weight`, `logic:evidenceStrength` — by reference and never duplicates them,
keeping credence, confidence, probability, and weight apart as the foundation requires. And competence
over time is a sequence of valid-interval-scoped assessments under suppression-not-erasure: a
superseded assessment is retained and recoverable, never erased — the same compositional discipline the
rest of the design set enforces everywhere.

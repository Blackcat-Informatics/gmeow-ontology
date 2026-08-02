<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Enactment: Prescription, Enactment, and Commitment

> The **enactment** member of the GMEOW Logic design set ([`LOGIC.md`](LOGIC.md)). It gives the
> meaning of *carrying a plan out durably* in `logic:`: the immutable versioned prescription, the
> immutable occurrence that enacts it, the hash-linked journal and checkpoints that make the
> occurrence resumable, the hierarchical refinement that turns an abstract step into an executable
> subgraph, and the commitment layer that records what became true **outside** the graph and what is
> known about it. The plan it enacts is [`LOGIC-TELEOLOGY.md`](LOGIC-TELEOLOGY.md)'s `logic:Plan`,
> unmodified. The state-change substrate is [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md); the Rust
> operational façade whose constructs this layer twins ontologically is
> [`LOGIC-SESSION.md`](LOGIC-SESSION.md); the lens laws that govern revision and restore are in
> [`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md). The domain binding — work clusters,
> guidance sets, and the operator review projection — is charted outside this slice, in
> `slices/core/work-orchestration/design/WORK-ORCHESTRATION.md`; **no term this charter governs
> names a `gmeow:` term**.

## The Prescription → Enactment → Commitment thesis

Three things are routinely collapsed into one word, and every durable-execution defect this layer
exists to prevent is a consequence of that collapse. A **prescription** is what is to be done: a
normative artifact that says nothing about whether it was done, is immutable once versioned, and is
content-addressed so that an approval or an audit can name *exactly* the text that was authorized.
An **enactment** is one occurrence of doing it: a descriptive record bound to exactly one
prescription version and exactly one closed input snapshot, immutable in its bindings and extended
only by an append-only journal. A **commitment** is what became true outside the graph as a result:
a provider accepted a payment, an actuator moved, a message left the building. The first two are
things GMEOW owns; the third is a thing GMEOW **describes but never performs**. The kernel therefore
carries the third as a *knowledge* record over an external fact — never as an assertion of that fact.

The prescription layer mints **no second plan concept**. `logic:Plan` is already defined as *a
transaction program whose primitive operations invoke action schemas and whose success criterion is a
goal expression* ([`LOGIC-TELEOLOGY.md`](LOGIC-TELEOLOGY.md#plans-and-nondeterministic-outcomes)).
Every structure a durable enactment needs from its prescription — ordering and serial composition,
guarded choice, concurrency, iteration, fallback, nondeterministic outcome sets, and the weak /
strong / strong-cyclic success quantification — is already there, in the operators of the transaction
layer. What is genuinely absent is not a plan concept but **version identity, occurrence identity,
and commitment knowledge**. Those, and only those, are what the kernel mints. Minting a second plan
class beside `logic:Plan` would be a Principle-4 second source of truth for the same phenomenon, and
would immediately fork means–end search, the projection surfaces, and the correspondence laws.

Because the kernel adds versioning, occurrence, and commitment **over** `logic:Plan` rather than
beside it, every domain that can already be written as a plan becomes enactable without a second
vocabulary. The six named domains are not analogies; each is a literal reading of the same terms:

| Domain | `logic:Plan` reads as | Its primitive operations invoke | `logic:planGoal` is | One `logic:Enactment` is |
| --- | --- | --- | --- | --- |
| workflow | the workflow definition | task action schemas | an achievement goal over the produced artifacts | one run of the workflow |
| DAG | the dependency graph, with `logic:planFlowEdge` carrying the edges | stage invocations | an achievement goal: every terminal output materialized | one build |
| repeatable guide | the written procedure | the steps a reader performs | the guide's stated end state | one walkthrough by one reader |
| curriculum | the programme of study | learning and assessment activities | competency attainment, `logic:AndDecomposition` over the named outcomes with `logic:OrDecomposition` over elective routes | one learner's enrolment, or one cohort's term |
| SCADA control plan | the control program or batch recipe | commands, interlock checks, and setpoint writes | a `logic:MaintenanceGoal` over a setpoint — *hold 80 ± 2 °C across the batch interval* | one control session over one unit |
| organizational guide | the policy-with-procedure | the procedural obligations the policy prescribes | a maintenance goal over a compliance state | one instance of applying the guide to one case |

Two of these readings are load-bearing rather than decorative, and they are the reason the kernel is
phrased generally in the first place.

The **curriculum** reading forces hierarchical refinement to be first-class. A course is prescribed
at the level of *modules*, and a module is decomposed into activities by a named, reusable,
applicability-conditioned method — the prerequisite structure is not a property of a single learner's
enrolment but of the programme. That is exactly a `logic:DecompositionMethod`, and it is what makes a
continuing enactment *use* its history rather than merely record it. It also forces
`logic:PriorEnactmentRequirement`: a precondition whose subject is not a state of the world but the
terminal status of *other enactments* — prerequisites for a course, certification currency for a
SCADA operator, a completed prior shift for a handover, an upstream DAG stage for a downstream one.
Nothing in the existing action-theory precondition facet expresses a condition over other
occurrences, and every one of the six domains needs it.

The **SCADA** reading forces the maintenance-goal law. A control program's success criterion is not
"the setpoint was reached once"; it is "the setpoint held across the interval". So a successful
enactment must **not** be permitted to license `logic:GoalEvaluationCompleted` on a
`logic:MaintenanceGoal` beyond its own interval — the constraint
`logic:MaintenanceGoalNotClosedByEnactmentConstraint`. The identical law is what makes a continuing
curriculum outcome, a recurring organizational compliance guide, and a weekly review cluster behave
correctly: one good week does not close a continuing goal. Had the kernel been phrased as
work-orchestration vocabulary, this would have looked like a quirk of one domain instead of the
general property of maintenance goals that it is.

The commitment layer is where the thesis earns its third term. A dispatch is not an effect: it is an
*intent* to cause one. An attempt is not a receipt. A receipt is not proof of durability unless the
provider's declared delivery semantics say the acknowledgement implies it. And the absence of a
receipt is not the absence of an effect — it is the absence of *knowledge*, which is a different
thing and a different next action. The kernel therefore separates the **protocol position** (where
the exchange got to) from the **commitment knowledge** (what is known about whether the effect
happened), and carries the second on a Belnap-style knowledge order rather than a status flag. This
is what makes "the process crashed after persisting the intent" a state the system can *derive a next
action from*, rather than an unrepresentable hole.

## Identity criteria

Every minted term states what makes two instances the same, because an operational record whose
identity is undecided is a record two writers will duplicate and no reader can join on. The rule
throughout: **content-addressed identity for immutable artifacts, occurrence identity for
occurrences, and tuple identity for reified relations.** A digest is reused from `gmeow:contentDigest`
and `gmeow:versionFingerprint`; the kernel mints no digest property of its own.

| Term | Identity criterion — two instances are the same exactly when… | Explicitly NOT identified by |
| --- | --- | --- |
| `logic:PrescriptionVersion` | their `gmeow:contentDigest` over the canonical rendering of the versioned `logic:Plan` is equal **and** they name the same `logic:prescriptionOf` plan | the version label, the authoring time, or the author |
| `logic:InputSnapshot` | their content digest over the closed, order-independent input set is equal | the query or generator that produced the set |
| `logic:Enactment` | they are the same occurrence — an occurrence is identified by its IRI, minted once and never reused | the (prescription version, input snapshot) pair; two enactments may legitimately share both, and conflating them destroys the repeat distinction |
| `logic:EnactmentCohort` | they group the same prescription version and the same generation/schedule anchor | the count or membership of enactments, which grows |
| `logic:TransitionJournal` | it is the journal of exactly one enactment; identity is that enactment's identity | its current head, which advances |
| `logic:JournalEntry` | equal `logic:journalPrevHead`, `logic:journalDeltaIdentity`, and `logic:journalOutcomeTag` — from which `logic:journalNewHead` is a function | the wall-clock time of the commit |
| `logic:SnapshotDelta` | equal content address over both anchors, the sorted digest of the additions, each suppression's digest in list order, and the step budget | the internal quad order within either dataset |
| `logic:EnactmentCheckpoint` / `logic:StepCheckpoint` | equal `logic:checkpointDescriptorHash` (the fold of all seven identity axes) **and** equal `logic:journalHead` | any single axis; a match on six of seven is a mismatch |
| `logic:ContinuationResume` / `Repeat` / `Revise` | value individuals of `logic:ContinuationKind`; identity is the individual | the observable outcome, which may coincide across kinds |
| `logic:PriorEnactmentRequirement` | equal (required prescription version, required terminal status, requiring step) triple | the particular prior enactment that discharges it |
| `logic:Deviation` | equal (enactment, deviating step, authorizing commitment, recorded departure digest) tuple | the reason prose |
| `logic:DecompositionMethod` | equal content digest over the method's subtask structure **and** equal applicability condition | the method's label; two labels over one structure are one method |
| `logic:RefinementEpisode` | equal (refined step, serving goal, world snapshot, attributed vantage, decision time) tuple | the candidate set produced, which is the episode's output |
| `logic:RefinementCandidateSet` | equal episode **and** equal `logic:ClosureEntry`-certified roster | the ordering of the roster |
| `logic:RefinementRejection` | equal (candidate, `logic:RejectionKind`, witness) triple | the rejection message |
| `logic:PreparationStep` / `logic:PreparationReceipt` | the step by its position in the prescription; the receipt by (preparation step, enactment, evidence digest) | the preparing agent |
| `logic:PinnedExecutableSubgraph` | equal content digest over the ordered, fully-refined step graph | the episode that selected it; two episodes may pin the identical subgraph |
| `logic:OperationalCapabilityGap` | equal (blocked step, missing capability contract, enactment) triple | the proposal that responds to it |
| `logic:CapabilityGapProposal` | equal gap **and** equal content digest over its eight mandatory fields | the proposing vantage |
| `logic:Advisory` | equal (producing model identity, prompt/context assembly, output digest) triple | the fact that a human agreed with it |
| `logic:SearchFragment` | the named fragment individual; identity is the IRI | the budget under which a search ran within it |
| `logic:DispatchIntent` | equal content digest over its nine mandatory bindings, *including* the normalized arguments and `logic:expectedJournalHead` | the dispatching process, retry counter, or wall-clock time |
| `logic:ArgumentNormalization` | the named recipe individual; two recipes producing equal output on one input are still two recipes | agreement on a sample of inputs |
| `logic:EffectAttempt` | equal (intent, `logic:providerCorrelationId`) pair | the local attempt counter; a redelivery under one correlation identity is one attempt |
| `logic:ExternalEffectReceipt` | equal (attempt, provider-side receipt identity) pair | the time the receipt was observed locally |
| `logic:ExternalOutcomeUnknown` / `logic:ReconciliationImpossible` | equal (attempt, boundary-entry journal head) pair | the number of probes since |
| `logic:IdempotencyContract` | equal (provider, operation, idempotency key derivation, retention window, support evidence) — its five mandatory fields | the individual key values it produces |
| `logic:DeliverySemantics` | the value individual | the provider that declares it |
| `logic:ReconciliationProbe` | equal (attempt, probe method, probe decision time, attributed clock) tuple | the verdict returned |
| `logic:ReconciliationAttempt` | equal (attempt under reconciliation, probe roster) pair | the verdict reached |
| `logic:ReconciliationVerdict` | the value individual (`Committed` / `NotCommitted` / `StillUnknown`) | the probe that produced it |
| `logic:ContradictionAdjudication` | equal (contradiction witness, adjudicating operator identity, decision time) triple | the direction of the decision |
| `logic:CompensationAttempt` | equal (compensating action schema, `logic:compensatesEffect` target receipt, intent digest) triple | the forward attempt's identity alone; one forward effect may be compensated by several distinct attempts |
| `logic:CompensationOutcome` | equal (compensation attempt, observed receipt) pair | the eligibility predicate that licensed the attempt |
| `logic:ResourceLease` | equal (resource, scope, `logic:fencingIdentity`) triple; the fencing identity is strictly monotone per resource | the holder, which changes across renewals |
| `logic:ClockAttribution` | equal (clock source identity, reading, reading frame) triple | the evaluation that consumed it |
| `logic:ApprovalCommitment` | equal (approved dispatch-intent digest, enactment/step, authorized operator identity, governing policy, decision, validity window) — its six mandatory bindings | the detachment state, which advances |
| `logic:AuthorizationProof` | the proof term's own recomputed goal under `proof::check`; a proof is identified by what it *proves*, never by what it claims to prove | the cited IRI set, which a forgery can copy exactly |
| `logic:ApprovalBindingFault` | the value individual (one of four) | the record that exhibits it |
| `logic:DeadlineConsumption` | equal (bounded resource, enactment, attributed clock, consumption reading) tuple | the deadline itself |
| `logic:ActionableFrontier` | equal (enactment, classifying standpoint, saturation witness) triple | the label set, which is derived |
| `logic:FrontierLabel` | the value individual; each is a total function of the axis tuple, and carries the tuple witness that produced it | the operator's reading of it |
| `logic:SkipReason` | the value individual | the step that carries it |
| `logic:OperationOutcome` | the value individual (one of six) | the evidence payload it carries |
| `logic:ContextAssembly` | equal (served enactment/step, inclusion roster digest, exclusion roster digest, assembly budget) tuple | the decision that followed it |
| `logic:LivenessCertificate` | equal (axis, transition relation digest) pair | the run that checked it |
| `logic:AxisTransitionLaw` | equal (axis, formula digest) pair | the constraint that enforces it |

Three of these criteria are adversarial on purpose. An **enactment is not** identified by its
(prescription version, input snapshot) pair, because `logic:ContinuationRepeat` exists precisely to
run the same version over the same inputs again; identifying them would make repeat unrepresentable.
An **effect attempt is** identified by the provider correlation identity rather than a local counter,
because that is the only identity both sides of the boundary can agree on — a local counter is
invisible to the provider and therefore cannot be reconciled against. And an **authorization proof**
is identified by its recomputed goal, not by its citations: `explain::assert_faithful` verifies only
that cited IRIs appear in the trace, so a forgery citing entirely real IRIs passes it. Recomputation
is what makes the criterion unforgeable.

## The seven orthogonal axes

An operational record has several independent things true of it at once, and every attempt to fold
them into one status enumeration produces a value set whose members are products of two questions —
`ReadyAuthorized` versus `ReadyApprovalRequired` is readiness × approval, not one axis with two
values. The kernel therefore carries **seven** axes, and the governing rule is absolute:

> **No property carries values from two axes.** A property whose value set spans axes is a conflation
> by construction, and `logic:LifecycleAxisNonConflationConstraint` rejects it at
> `logic:severity "Violation"`. Operator-facing composites (`logic:FrontierLabel`) are **derived**
> total functions of the axis tuple that carry the tuple witness, never authored values.

| # | Axis | Question it answers | Carrier | Provenance |
| --- | --- | --- | --- | --- |
| 1 | enactment state | Where is this occurrence as a whole? | `logic:EnactmentState` | minted, closed value class |
| 2 | step state | Where is this one step? | `logic:StepState` | minted, closed value class |
| 3 | approval state | Where is this social commitment in its detachment lifecycle? | `logic:ApprovalState` | minted, closed value class |
| 4 | effect protocol position | How far did the external exchange get? | `logic:EffectProtocolPosition` | minted, closed value class |
| 5 | effect commitment knowledge | What is *known* about whether the effect happened? | the property `logic:effectKnowledge` over the existing `logic:InformationState` | **property minted; value class reused unchanged** |
| 6 | goal-evaluation status | Has the success criterion been conclusively judged? | `logic:GoalEvaluationStatus` | **reused unchanged** |
| 7 | recurrence state | Where is this generated occurrence slot in its schedule? | `logic:RecurrenceState` | minted, closed value class |

Five value classes are minted. The sixth axis mints a **property only** — re-minting a status
enumeration over external-effect knowledge would create a second truth-value vocabulary beside
`logic:InformationState`, which is the exact "no second status enum" failure the structural bans
forbid. The seventh mints **nothing at all**; `logic:GoalEvaluationStatus` already carries
`GoalEvaluationCompleted` / `GoalEvaluationUndetermined` / `GoalEvaluationUnsupported` and already
enforces the conclusiveness discipline this layer needs.

### Why protocol position and commitment knowledge are two axes

They vary independently, and the pair that proves it is ordinary: an attempt at protocol position
`EffectReceipted` whose commitment knowledge is `InfoBoth`. The provider returned a receipt (position
advanced) *and* a later reconciliation probe reported the operation absent from the provider's ledger
(knowledge became contradictory). Neither value determines the other. Position advances by protocol
events and is a labelled transition system; knowledge advances by evidence and is a knowledge order.
A single enumeration covering both would have to enumerate their product, and would silently license
exactly the read the kernel exists to forbid — inferring "the effect happened" from "we got as far as
receipting".

### The axis tuple

The axis tuple of a step is `(enactment state, step state, approval state, effect protocol position,
effect knowledge, goal-evaluation status, recurrence state)`. It is the kernel's canonical operational
coordinate: checkpoints record all seven, the frontier projection is a total function from the tuple
to a label, and the liveness certificates quantify over reachable tuples. A checkpoint that records
six of seven is rejected — a restore that cannot re-establish an axis cannot certify that the resumed
occurrence is the one that was suspended.

## State-transition laws and liveness

Each axis carries exactly one law, and the seven laws are not all the same *shape*. Five axes carry a
transition-relation-membership law; one carries an order-monotonicity law; one carries a
minting-and-supersession law. Stating this taxonomy is not pedantry: authoring a monotonicity axis as
a transition relation is what produces the classic bug in which a probe that *learns more* is
recorded as an illegal state change and refused.

### The shared `logic:Formula` shape

Every membership law compiles into one `logic:Formula` tree of the following shape, and
`logic:AxisTransitionLaw` carries it:

```text
logic:Formula                                   # the law
  logic:forall
    logic:quantifiedVariable  [ termIndex 0 ; termVariable "R"  ]   # the record
    logic:quantifiedVariable  [ termIndex 1 ; termVariable "S1" ]   # prior value
    logic:quantifiedVariable  [ termIndex 2 ; termVariable "S2" ]   # posterior value
    logic:quantifiedVariable  [ termIndex 3 ; termVariable "T"  ]   # journal position
  logic:antecedent
    logic:and
      logic:Formula [ relation logic:axisValueAt ;                  # atomic predication
                      argument ( ?R ?S1 ?T ) ]
      logic:Formula [ relation logic:axisValueAtSuccessor ;
                      argument ( ?R ?S2 ?T ) ]
  logic:consequent
    logic:Formula [ relation logic:axisTransitionAdmitted ;
                    argument ( <the axis IRI> ?S1 ?S2 ) ]
```

The atomic predications are `logic:ArgumentStructure` nodes: a reified `logic:relation` over ordered
zero-based `logic:argument` term-carriers, each carrying exactly one of `logic:termIri` /
`logic:termVariable`. The admitted pairs are **data** — a `logic:axisTransitionAdmitted` extension
authored per axis — so the law itself is one formula per axis and the relation it reads is the table
below. This is what lets a single Rust property test quantify over the authored relation instead of
over hand-written cases, and it is why widening an axis is a data edit that the liveness test
immediately re-checks rather than a formula rewrite.

Each law is enforced by a `logic:Constraint` at `logic:severity "Violation"` whose
`logic:Formula` is the negation-carrying integrity form of the above; none carries a
`logic:formalizes` edge onto a shape, because a hand-authored shape would be a second source of truth.

### Axis 1 — `logic:EnactmentState`

| From | Admitted successors |
| --- | --- |
| `EnactmentPending` | `EnactmentRunning`, `EnactmentCancelled` |
| `EnactmentRunning` | `EnactmentPaused`, `EnactmentCompleted`, `EnactmentFailed`, `EnactmentCancelled` |
| `EnactmentPaused` | `EnactmentRunning`, `EnactmentCancelled`, `EnactmentFailed` |
| `EnactmentCompleted` · `EnactmentCancelled` · `EnactmentFailed` | — (quiescent) |

`EnactmentPaused → EnactmentRunning` is the *only* edge `logic:ContinuationResume` may traverse; a
`logic:ContinuationRepeat` does not traverse it at all, because a repeat mints a **new** enactment at
`EnactmentPending`. The two look identical in a naive log and are structurally distinct here.

### Axis 2 — `logic:StepState`

| From | Admitted successors |
| --- | --- |
| `StepPending` | `StepReady`, `StepSkipped`, `StepCancelled` |
| `StepReady` | `StepRunning`, `StepWaiting`, `StepSkipped`, `StepCancelled` |
| `StepRunning` | `StepWaiting`, `StepCompleted`, `StepFailed`, `StepCancelled` |
| `StepWaiting` | `StepReady`, `StepCancelled`, `StepFailed` |
| `StepCompleted` · `StepSkipped` · `StepCancelled` · `StepFailed` | — (quiescent) |

`StepWaiting → StepReady` rather than `StepWaiting → StepRunning`: a wait that resolves re-enters the
action gate. This is forced by the teleology layer's own semantics — a `logic:NotificationWaitSchema`
whose `logic:ExternalSignal` arrives yields `logic:GateUndetermined` until re-probed, and a resumed
step whose precondition has since been falsified must be denied, not run. Routing a resolved wait
straight to running would let a stale gate verdict authorize execution.

### Axis 3 — `logic:ApprovalState` (the Singh detachment lifecycle)

| From | Admitted successors |
| --- | --- |
| `ApprovalNull` | `ApprovalCreated` |
| `ApprovalCreated` | `ApprovalDetached`, `ApprovalExpired`, `ApprovalCancelled`, `ApprovalReleased` |
| `ApprovalDetached` | `ApprovalDischarged`, `ApprovalViolated`, `ApprovalReleased` |
| `ApprovalDischarged` · `ApprovalExpired` · `ApprovalViolated` · `ApprovalCancelled` · `ApprovalReleased` | — (quiescent) |

The load-bearing omission is `ApprovalDetached → ApprovalCancelled`. Once the antecedent has been
satisfied — the exact dispatch intent exists, its digest matches, the operator's window is open — the
debtor's unilateral withdrawal is a **violation**, not a cancellation. `ApprovalReleased` remains
admitted from `Detached` because the creditor may always waive what is owed to it. That asymmetry is
what turns "the approval must bind the exact intent" from a comparison someone remembers to write
into a detachment law with four distinct fault codes.

### Axis 4 — `logic:EffectProtocolPosition`

| From | Admitted successors |
| --- | --- |
| `EffectIntended` | `EffectAttempted`, `EffectAbandoned` |
| `EffectAttempted` | `EffectReceipted`, `EffectReconciling`, `EffectAbandoned` |
| `EffectReceipted` | `EffectCompensating` |
| `EffectReconciling` | `EffectReconciled`, `EffectAbandoned` |
| `EffectReconciled` | `EffectCompensating` |
| `EffectCompensating` | `EffectCompensated`, `EffectReconciling` |
| `EffectReceipted` · `EffectReconciled` · `EffectCompensated` · `EffectAbandoned` | quiescent |

Four positions are both quiescent and non-final: `EffectReceipted` and `EffectReconciled` owe nothing
further, yet a *newly motivated* compensation may leave them. The kernel distinguishes **quiescent**
(no obligation outstanding) from **final** (no admitted successor at all), and states its liveness
properties over quiescence, because a saga that has settled successfully must count as live even
though compensation remains reachable from it. `EffectCompensating → EffectReconciling` is admitted
because a compensation whose own outcome is unknown re-enters the boundary — compensation is an
external effect and inherits every property of one.

### Axis 5 — `logic:effectKnowledge`: a monotonicity law, not a transition relation

The knowledge axis is the one axis whose law is not membership in an authored pair set:

```text
∀ a, k₁, k₂, t .  effectKnowledgeAt(a, k₁, t) ∧ effectKnowledgeAtSuccessor(a, k₂, t)
                    →  knowledgeOrderLeq(k₁, k₂)
```

— the same `logic:forall` / `logic:antecedent` / `logic:consequent` tree as the membership laws, with
`logic:axisTransitionAdmitted` replaced by the authored `logic:knowledgeOrderLeq` relation of the next
section. Enforced by `logic:ReconciliationKnowledgeMonotonicityConstraint`. The constraint fires only
on **malformed records** — a genuine `Committed`-then-`NotCommitted` disagreement is not an authoring
error; it is knowledge moving `InfoSupported → InfoBoth`, which is *up* the order and therefore
admitted, then routed to `logic:ContradictionWitness` and attributed adjudication.

### Axis 6 — `logic:GoalEvaluationStatus`: a minting-and-supersession law

A `logic:GoalEvaluation` is a reified record over (goal, world, evaluator, time, criterion). Its
status does not transition in place — a changed verdict **mints a new attributed evaluation and
supersedes the prior one** through the supersession quartet, so both remain auditable and two
vantages that disagree coexist. The law is therefore:

```text
∀ e, s₁, s₂ .  goalEvaluationStatus(e, s₁) ∧ goalEvaluationStatus(e, s₂)  →  s₁ = s₂
```

together with `logic:MaintenanceGoalNotClosedByEnactmentConstraint`: no `logic:Enactment` whose own
state is `EnactmentCompleted` may license `GoalEvaluationCompleted` on a `logic:MaintenanceGoal`
outside the enactment's own interval.

### Axis 7 — `logic:RecurrenceState`

| From | Admitted successors |
| --- | --- |
| `RecurrenceScheduled` | `RecurrenceActive`, `RecurrenceSkipped`, `RecurrenceSuspended`, `RecurrenceEnded` |
| `RecurrenceActive` | `RecurrenceSuspended`, `RecurrenceEnded` |
| `RecurrenceSuspended` | `RecurrenceScheduled`, `RecurrenceActive`, `RecurrenceSkipped`, `RecurrenceEnded` |
| `RecurrenceSkipped` · `RecurrenceEnded` | — (quiescent) |

The axis is carried by the generated **occurrence slot**, not by the enactment and not by the
generator. A skipped occurrence is a first-class recorded value on this axis; it is never a missing
enactment, and the distinction is what stops a reader from mistaking a deliberate exception for a
gap in the record.

### Liveness — the three properties, and why they are not optional

The transition laws prove that bad moves are refused. Nothing in them proves the system cannot get
**stuck**, which is the dominant real failure of durable sagas: not an illegal transition, but an
intent nobody will ever attempt, a lease nobody will ever break, a step waiting on a signal that
cannot arrive. `logic:LivenessCertificate` asserts three properties per axis over the authored
transition relation, and they are machine-checked, not asserted:

**Deadlock-freedom.** Every reachable, non-quiescent axis value has at least one admitted successor:

```text
∀ v .  reachable(v) ∧ ¬quiescent(v)  →  ∃ v' . axisTransitionAdmitted(axis, v, v')
```

**Reachability (no dead state).** Every reachable value reaches a quiescent one along admitted
transitions — stated over the transitive closure of the admitted relation:

```text
∀ v .  reachable(v)  →  ∃ v' . quiescent(v') ∧ admittedReaches(axis, v, v')
```

This is the property that catches an axis widened by one value with no exit, which is otherwise
invisible until an operator finds a record that can never be closed.

**No orphan intent.** Every persisted `logic:DispatchIntent` is eventually attempted or explicitly
abandoned with a recorded reason; there is no third possibility:

```text
∀ i .  DispatchIntent(i)  →  ( ∃ a . attemptOfIntent(a, i) )
                            ∨ ( ∃ w . intentAbandoned(i, w) ∧ abandonmentReason(w, _) )
```

Enforced by `logic:OrphanIntentConstraint`. This is the property that makes an intent persisted
immediately before a crash *recoverable rather than lost*: on restart it is either attempted or
abandoned, and never sits indefinitely in a state no derivation looks at.

Deadlock-freedom and reachability are checked over the **product** of the five membership axes as
well as per axis, because a tuple can be individually live on each coordinate and jointly stuck — a
step at `StepWaiting` whose approval is at `ApprovalExpired` has an admitted successor on each axis
and no admissible joint move. Checking only per-axis liveness would prove nothing about the system
the operator actually faces.

## The ⊑ₖ knowledge order

The commitment layer is a knowledge lattice, and the kernel defines the order explicitly because
**the order does not exist in the codebase today and the value that looks like it is a trap**.

### The trap, stated plainly

`logic:InformationState` (`crates/logic/src/result.rs:247`) is the Belnap four —
`Supported`, `Opposed`, `Both`, `Neither` — **plus two explicit non-results**, `Undetermined` and
`NotEvaluated`, which `result.rs:239-246` documents as *"never interchangeable with `Neither`"*. The
enum derives `Ord`. That derived `Ord` is **declaration order** — `Supported < Opposed < Both <
Neither < Undetermined < NotEvaluated` — and it is **not** ⊑ₖ. It is worse than merely unrelated: it
places `NotEvaluated`, which carries the *least* information of any value, at the **top**, exactly
inverting the order, while accidentally agreeing with ⊑ₖ on `Supported < Both`. A partially-correct
order is the most dangerous kind, because a spot check passes.

> **The derived `Ord` is for deterministic keying only** — `BTreeMap` ordering and canonical row sort,
> the determinism contract's fourth clause. **No kernel law may read it.** Every monotonicity check,
> every eligibility predicate, and every reconciliation constraint reads the authored
> `logic:knowledgeOrderLeq` relation defined here, never `<=` on the Rust value.

### The order the kernel uses

The kernel admits five values for `logic:effectKnowledge` — `InfoNotEvaluated`, `InfoUndetermined`,
`InfoSupported`, `InfoOpposed`, `InfoBoth` — and orders them:

```text
                 InfoBoth              ⊤  (proof and counterproof)
                 /      \
        InfoSupported  InfoOpposed        (proof only / counterproof only)
                 \      /
              InfoUndetermined            (we looked; no verdict)
                    |
              InfoNotEvaluated         ⊥  (we could not look)
```

`x ⊑ₖ y` iff y carries at least as much evidence as x. It is a **bounded lattice**: reflexive,
antisymmetric, transitive, with bottom `InfoNotEvaluated`, top `InfoBoth`, and all binary joins —
notably `InfoSupported ⊔ InfoOpposed = InfoBoth`, which is the whole point. This is authored as the
`logic:knowledgeOrderLeq` relation and pinned by a Rust property test asserting the three order laws
and join totality, and asserting agreement with the Rust `InformationState` value set.

`InfoNotEvaluated ⊏ₖ InfoUndetermined` is a real strictness, not a tidy-up. `NotEvaluated` means the
engine *could not look* — no probe was possible, no semantics were available. `Undetermined` means a
probe ran and returned no verdict. Knowing that a reconciliation probe executed and was inconclusive
is strictly more than knowing nothing, and it licenses a different next action (retry the probe versus
establish a probe channel at all). Collapsing them would erase the distinction between
`logic:ExternalOutcomeUnknown` and `logic:ReconciliationImpossible`, which are the two genuinely
different boundary situations an operator faces.

### Why `InfoNeither` is banned for effect knowledge

`InformationState::classify` (`result.rs:335`) yields `Neither` **only when the run is conclusive** —
`evaluation == Completed || completeness == CompleteForFragment`. That is a **closed-world reading**:
"we looked exhaustively and there is neither proof nor counterproof". For an internal derivation over
a declared closure that is correct and useful.

External commitment is **never** closed-world. The provider's state is not in our EDB; no local
saturation, however complete, is evidence about it. Admitting `InfoNeither` on
`logic:effectKnowledge` would therefore let a conclusive local run label an *unreachable provider* as
"conclusively no effect" — a consumer reads that as "the payment definitely did not go through" and
retries. That is the precise failure this layer exists to prevent, arrived at through a value that
looks principled. The ban is enforced two ways: by a `logic:Constraint` + `logic:Formula` at
`logic:severity "Violation"` (**not** a five-member `owl:oneOf`, which is not EL-safe and would red
the profile gate), and by a negative fixture asserting `InfoNeither` on an external effect and
tripping exactly that gate and no other.

The ban also explains why `InfoNeither` is not merely "unused but harmless" in the order above.
Belnap's own knowledge order places `Neither` at ⊥ — *no information*. The repo's `Neither` is the
opposite: a **conclusive** verdict, which under any honest reading sits *above* `Undetermined`.
Importing the textbook lattice unexamined would have placed a conclusive closed-world verdict at the
bottom of the knowledge order. Excluding the value from the admitted set removes the question rather
than answering it wrongly.

### Why `InfoBoth` is a reachable sink with a mandatory escape

`InfoBoth` is not an error state; it is the honest label for a real, common situation — the provider
returns a receipt and the provider's own ledger later reports the operation absent. Under ⊑ₖ it is
the top, so no further evidence can move off it, and a knowledge order alone would leave the record
stuck there for ever. The escape is therefore **not** a knowledge move: a `logic:ContradictionWitness`
is raised carrying both grounds, and resolution is an attributed
`logic:ContradictionAdjudication` — an operator identity, a decision time, a governing policy, and a
recorded direction. The adjudication is a *new record*, not a mutation of the knowledge value, so the
contradiction stays in the graph and the decision is attributable to whoever made it. At the sheaf
level this is the same phenomenon as a non-vanishing gluing obstruction over disagreeing local
sections: the disagreement is contained and made explicit rather than resolved by fiat.

### Compensation eligibility is an upward-closed predicate

This is the one place where a point-value predicate would be actively unsound, so the kernel states
it as a predicate over evidence, never as an equality on the knowledge value:

```text
compensationEligible(e)  ≡   ( ∃ p . proofOfCommit(p, e) )
                           ∧ ¬( ∃ r . adjudicatedRetraction(r, e) )
```

`proofOfCommit` holds at `InfoSupported` **and** at `InfoBoth` — a witnessed contradiction still
contains a proof of commitment. Eligibility is therefore **upward-closed** in ⊑ₖ: once eligible,
more evidence never makes a record ineligible.

Writing the predicate as `logic:effectKnowledge = InfoSupported` instead would be a live defect.
A reconciliation probe that *increases* knowledge — moving `InfoSupported → InfoBoth` by discovering
a contradicting provider report — would flip the equality false and **revoke** compensation
eligibility. An operator who learns more would be able to do less, and the monotonicity the
reconciliation law asserts would be violated by the eligibility rule sitting next to it. The only
thing that withdraws eligibility is an `adjudicatedRetraction`: an attributed decision that the
forward effect did not occur, which is a new authored record and not a consequence of a knowledge
move. `logic:CompensationEligibilityConstraint` enforces the upward-closed form, and it is authored
so that deriving eligibility asserts **nothing** about whether the compensation will succeed —
`logic:CompensationSuccessRequiresReceiptConstraint` is a separate obligation with its own code.

## The reuse ledger

One row per candidate term named in the source requirements, with an explicit `REUSE` or `MINT`
verdict and, for every mint, the identity criterion that forces it. The governing question is not
"is there something similar?" but "does an existing term have the **same identity criterion**?" A
term with a different identity criterion is a different term, and reusing it would make two things
indistinguishable that consumers must distinguish.

| Candidate term | Verdict | Identity criterion forcing the mint, or the reason for reuse |
| --- | --- | --- |
| `WorkCluster` | **MINT** `gmeow:WorkCluster` (domain binding, not the kernel) | Identified by its continuing goal plus the standpoint that holds it — a *domain* identity binding a `gmeow:Goal`, a `gmeow:Norm`, and a `gmeow:EventSchedule`. It cannot live in `logic:`, which takes no axiom edge into `gmeow:`. The kernel-side continuing structure it needs is already covered by `logic:PrescriptionVersion` + supersession, so the kernel mints no cluster class of its own. |
| `WorkPlanVersion` | **MINT** `logic:PrescriptionVersion` | Identified by the content digest of the versioned plan plus its `logic:prescriptionOf` target. `logic:Plan` has IRI identity and is mutable-by-revision; a version is content-addressed and immutable. Two different identity criteria, therefore two terms. Named domain-neutrally: a curriculum version and a SCADA recipe version are the same construct. |
| `WorkRun` | **MINT** `logic:Enactment` (the candidate name is not minted) | Identified as an occurrence, not by its (version, snapshot) pair — because a repeat legitimately shares both. `logic:TransactionOutcome` records one program run's success under executional entailment and is identified by (program, start state); it is a *verdict*, not a durable resumable occurrence with a journal, checkpoints, and a continuation kind. The candidate name is rejected under the generality obligations: a curriculum enrolment and a SCADA control session are not "work runs". |
| `PlanExecution` | **MINT** `logic:Enactment` (the candidate name is not minted; it is the same term as the row above) | The same identity criterion, reached from the other direction. The "execution" spelling is rejected because it is already taken by a hand-authored process-model surface that this kernel **supersedes and removes** rather than retains beside it: keeping a second execution class as a projection surface would be a second source of truth by construction, and lossy compatibility for consumers belongs in generated projections, never in the canon. One occurrence class, one name. |
| `PlanRefinementEpisode` | **MINT** `logic:RefinementEpisode` | Identified by (refined step, served goal, world snapshot, vantage, decision time). Nothing existing reifies a *bounded, attributed act of deciding how to decompose*; `logic:refinesGoal` is a standing relation between goals with no episode, no budget, and no closed candidate roster. |
| `PreparationReceipt` | **MINT** `logic:PreparationReceipt` | Identified by (preparation step, enactment, evidence digest). Distinct from `logic:ExternalEffectReceipt`, whose identity involves a *provider-side* receipt identity: a preparation receipt attests local readiness work, and conflating the two would let an internal readiness note be read as evidence of an external commitment. |
| `DispatchIntent` | **MINT** `logic:DispatchIntent` | Identified by the digest over its nine mandatory bindings including the normalized arguments and `logic:expectedJournalHead`. No existing term binds an action-schema version, a normalized argument set, an approval requirement, and a journal head into one immutable pre-effect record. Carrying the expected head is what makes no-blind-retry a structural property of the hash chain rather than an ABox rule. |
| `EffectAttempt` | **MINT** `logic:EffectAttempt` | Identified by (intent, `logic:providerCorrelationId`) — a correlation identity **the provider can also see**. A `gmeow:Event` typed by a `logic:ActionSchema` is an occurrence with local identity only and therefore cannot be reconciled against a provider. |
| `ExternalEffectReceipt` | **MINT** `logic:ExternalEffectReceipt` | Identified by (attempt, provider receipt identity). An observed, attributed record of what the far side said — never a derivation. `logic:EffectRecordsAreObservedNotDerivedConstraint` forbids it carrying a derivation provenance IRI, which is exactly how the ontology distinguishes asserted from derived. |
| `ExternalOutcomeUnknown` | **MINT** `logic:ExternalOutcomeUnknown` | Identified by (attempt, boundary-entry journal head). It is the boundary class at `logic:effectKnowledge` = `InfoUndetermined`. Distinct from its sibling `logic:ReconciliationImpossible` (the `InfoNotEvaluated` reading: provider gone, no query API, retention elapsed), which names a **different next action** and previously had no home. |
| `IdempotencyContract` | **MINT** `logic:IdempotencyContract` | Identified by its five mandatory fields (provider, operation, key derivation, retention window, provider-support evidence). The retention window is the field that makes it more than documentation: a retry after the provider's key-retention window has elapsed is *exactly* a duplicate effect, and `logic:IdempotencyRetentionConstraint` reads the window to say so. |
| `ReconciliationProbe` | **MINT** `logic:ReconciliationProbe` | Identified by (attempt, probe method, decision time, attributed clock). A probe is a *question asked at a time on someone's clock*; `logic:GateProbe` pairs a schema with a state and held capabilities to gate an action, a different question with a different identity. |
| `ReconciliationAttempt` | **MINT** `logic:ReconciliationAttempt` | Identified by (attempt under reconciliation, probe roster). One reconciliation attempt may issue several probes; identifying it with a single probe would make a multi-probe reconciliation unrepresentable and its verdict unattributable. |
| `CompensationAttempt` | **MINT** `logic:CompensationAttempt` | Identified by (compensating schema, `logic:compensatesEffect` target receipt, intent digest). The existing `logic:compensation` facet names *which action undoes a schema's realized outcome* — a type-level fact. An attempt is an occurrence bound to the exact forward receipt, and one forward effect may be compensated by several distinct attempts. |
| `ApprovalGrant` | **MINT** `logic:ApprovalCommitment` (the candidate name is not minted) | Identified by its six mandatory bindings: the exact dispatch-intent digest, the enactment/step, the authorized operator identity, the governing policy, the decision, and the validity window. A "grant" is a snapshot of one state of a social commitment; modelling the commitment itself makes revocation, delegation, escalation, quorum, and expiry first-class instead of absent, and turns "exact binding" into a detachment law. `gmeow:Commitment` is the intentional-mode relator between agents and is not the artifact that authorizes one intent. |
| `OperationalCapabilityGap` | **MINT** `logic:OperationalCapabilityGap` | Identified by (blocked step, missing capability contract, enactment). It is **disjoint** from `gmeow:CapabilityGap`, which reifies a committed *reasoning-profile* expressiveness gap. One says "this engine profile cannot express that"; the other says "no available tool can do this work". Same word, different identity criteria, different remedies. |
| `CapabilityGapProposal` | **MINT** `logic:CapabilityGapProposal` | Identified by (gap, digest over its eight mandatory fields): missing capability, required contract, expected inputs, expected outputs, expected effects, verification method, security/lifecycle needs, and the blocked step. All eight are gated by `logic:CapabilityGapProposalCompletenessConstraint`; a seven-field gate would let the blocked step be omitted, which is the field that makes the proposal actionable. |
| `gmeow:EventSeries` | **REUSE** | Reused as the cluster's **occurrence series** — the group of generated occurrences reached through `gmeow:hasRecurrenceRule` / `gmeow:seriesOccurrence`. Each occurrence *generates* a `logic:Enactment`; `gmeow:EnactmentIsNotOccurrenceConstraint` forbids identifying the two. |
| `gmeow:RecurrenceRule` | **REUSE** | Reused verbatim as the recurrence generator. Re-minting a recurrence rule in either new surface is a second source of truth for calendar semantics, and is banned by the structural cell `saRecurrenceReusesCalendar` with a fail witness that mints a local rule. |
| `gmeow:EventSchedule` | **REUSE** | Reused as the **mediator**: it binds the template event, the recurrence rules, the time zone, and the generated occurrences. The schedule is the generator, not the generated — the calendar slice's own shipped rule, reused rather than re-stated. A deliberately unenacted occurrence is a `gmeow:ScheduleException`, never a missing enactment. |
| `logic:Plan` and `logic:planBody` / `logic:planFlowEdge` / `logic:planGoal` / `logic:planSuccessMode` | **REUSE** | The prescription itself, unchanged. `logic:PlanExecutionConcept` is deliberately not minted; the structural cell `saNoSecondPlanConcept` proves it. |
| `logic:ActionSchema` and the eight action-theory facets | **REUSE** | Precondition, effect, invariant, `logic:actionResource`, capability, nondeterministic outcome set, observation, and compensation are the schema-level vocabulary the commitment layer dispatches; none is re-minted at the occurrence level. |
| `logic:TransactionStep` / `logic:Path` / `logic:TransactionOutcome` | **REUSE** | The executed step, the executed run, and the executional-entailment verdict. An enactment *has* these; it is not one of them. |
| `logic:GoalEvaluationStatus` | **REUSE** unchanged | Axis 7 of the reasoning-result vocabulary is exactly the kernel's goal-evaluation axis; re-minting it would trip the no-second-status-enum ban. |
| `logic:InformationState` | **REUSE** as the range of `logic:effectKnowledge`, minus `InfoNeither` | See the ⊑ₖ section. The property is minted; the value class is not. |
| `logic:ContradictionWitness` | **REUSE** | The `InfoBoth` escape route already has a witness class; the kernel adds only the adjudication record. |
| `logic:AdviceGuidance` | **REUSE**, and held strictly apart from `logic:Advisory` | See the non-conflation rules. |
| `logic:ClosureEntry` | **REUSE** | Certifies the refinement candidate roster as closed, rather than a bespoke "isClosed" boolean. |
| `logic:BudgetBoundedResource` / `logic:BudgetExhausted` / `logic:resourceExhausted` / `logic:consumedBudget` | **REUSE** | Every budget in the kernel. `logic:DeadlineConsumption` is the one mint here, because a deadline is consumed against an **attributed clock** and the existing budget terms carry no clock attribution. |
| `logic:Correspondence` with its `get`/`put` legs | **REUSE** | `logic:ContinuationRevise` **is** a `put`; checkpoint-restore **is** `get∘put`. The lens laws replace what would otherwise be ad-hoc revision constraints. |
| `gmeow:contentDigest` / `gmeow:versionFingerprint` | **REUSE** | The kernel mints **no** digest property. |
| `gmeow:supersededBy` and the supersession quartet | **REUSE** | The only revision path for a prescription version, and the mechanism by which a superseded goal evaluation stays auditable. |
| `gmeow:CapabilityGap` | **REUSE**, disjoint | Retained for what it means; explicitly disjoint from the operational gap. |

## Non-conflation rules

Each rule below is a pair of terms a reader will merge unless stopped, the reason merging them is a
defect rather than a simplification, and the mechanism that stops it.

**Operational capability gap ≠ `gmeow:CapabilityGap`.** `gmeow:CapabilityGap` reifies a committed
*reasoning-profile* expressiveness gap — the OWL profile cannot express this axiom. A
`logic:OperationalCapabilityGap` says no available tool can perform this work. The remedies are
disjoint: one is answered by a different reasoning contract, the other by acquiring or building a
capability. Enforced by an explicit disjointness axiom plus a structural cell
(`saOperationalGapDisjointFromCapabilityGap`) with a fail witness typing one record as both.

**Enactment ≠ event occurrence.** A recurring schedule generates *occurrences*; an occurrence
generates an enactment. Identifying them makes a skipped occurrence indistinguishable from a missing
run, makes two enactments of one occurrence (a resume after a crash, then a repeat) unrepresentable,
and destroys the schedule's "generator, not generated" discipline. Enforced by
`gmeow:EnactmentIsNotOccurrenceConstraint` on the binding side.

**Prescription ≠ enactment.** The first is normative and immutable; the second is descriptive and
occurrence-identified. Merging them is what produces in-place plan mutation — the defect
`logic:PrescriptionVersionImmutabilityConstraint` exists to refuse, with supersession as the only
revision path. A related and equally load-bearing corollary: an enactment **pins** its version at
start, and the cluster's active-version pointer moves independently
(`logic:InFlightVersionPinConstraint`). Without the pin, revising a prescription silently rewrites
what an in-flight occurrence is doing.

**Advice ≠ authority.** A model-generated decomposition, ranking, summary, or draft is a
`logic:Advisory`: attributed, first-class, and useful. It may **not** be a `logic:pinAuthority`, may
not stand as an approval decision, and may not carry a `logic:resultProof`
(`logic:AdvisoryNeverAuthorityConstraint`). The positive form of the rule is stronger than the
prohibition: authority is carried by a `logic:AuthorizationProof` whose goal `proof::check`
**recomputes** rather than trusts, so an advisory dressed as an authorization fails as a head
mismatch. A negative convention someone must remember becomes an unforgeable positive criterion.

**Compensation ≠ inverse.** A compensating action is a **new action** with its own preconditions, its
own capability and resource requirements, its own external effect, and its own possibility of
failure. It is not a guaranteed inverse and does not restore the prior state. Two separate
constraints, with two separate violation codes, carry the two independent obligations:
`logic:CompensationNotInverseConstraint` (typing a compensation as a guaranteed inverse is refused)
and `logic:CompensationSuccessRequiresReceiptConstraint` (marking a compensation successful without a
receipt is refused). Sharing one code would let a fixture pass by tripping the wrong obligation.

**Protocol position ≠ commitment knowledge.** Axes 4 and 5 vary independently; `EffectReceipted` with
`InfoBoth` is an ordinary state. Folding them lets "we got as far as receipting" be read as "the
effect happened". Enforced by `logic:LifecycleAxisNonConflationConstraint`, which refuses any property
whose value set spans both.

**`logic:actionResource` ≠ `logic:resourcePolicy`.** The first is an action-theory facet: what an
action *consumes or requires*, the seam to `logic:competesForResource` and to `logic:ResourceLease`.
The second is a `logic:ReasoningContract` → resource-policy property: the **engine's** budget for a
derivation. A search that exhausts `logic:resourcePolicy` returns an honest incomplete result; an
action that cannot obtain its `logic:actionResource` is gated. Conflating them makes an engine budget
look like a domain resource shortage and vice versa — two entirely different operator actions.

**`logic:Advisory` ≠ `logic:AdviceGuidance`.** This is the hardest of the eight, because the names are
near-homonyms and the distinctiveness gate is strict. `logic:AdviceGuidance` is an **advice carrier in
the slice-quality apparatus**: a first-class, term-scoped guidance record that formalizes a `useWhen`
cell so positive applicability advice is machine-active rather than prose. Its subject is a *term*,
its lifetime is the vocabulary's, and its consumer is the quality advisor. `logic:Advisory` is an
**attributed model output within one enactment**: its subject is a decision, its lifetime is that
occurrence's, its consumer is an operator, and its defining property is that it carries no authority.
They share a word and nothing else. The rule: an `logic:Advisory` never `logic:formalizes` a term, and
a `logic:AdviceGuidance` is never bound to an enactment, a step, or a candidate.

## HTN methods, pinned subgraphs, and the declared search fragment

### Methods, not one-off refinements

A pinned executable subgraph without a named method schema makes every refinement a one-off. The
consequence is precise and fatal to the thesis: a *continuing* enactment would record its refinement
history and never be able to **use** it, because nothing in a past episode is addressable by a future
one. `logic:DecompositionMethod` is therefore first-class — a named, versioned, content-addressed
decomposition schema with explicit applicability conditions, mapping one abstract step to an ordered
subtask structure.

Methods are what make the kernel serve the six domains rather than one: course → modules → activities
(a curriculum's programme structure *is* a method library), phase → steps (a SCADA recipe's phase
expansion), policy → procedures (an organizational guide's implementation), abstract task → subplan
(a workflow's dynamic refinement), stage group → stages (a DAG's fan-out), and section → instructions
(a repeatable guide). In every case the method is an asset of the *prescription*, reused across
enactments, and the episode is the act of selecting among applicable methods here and now.

A refinement episode is **attributed** (`gmeow:vantage`) and **bounded** (`logic:resourcePolicy`), and
produces a `logic:RefinementCandidateSet` whose roster is certified closed by a `logic:ClosureEntry`
rather than asserted closed by a flag. Each rejected candidate carries a `logic:RefinementRejection`
with a closed `logic:RejectionKind` (precondition / capability / resource / approval) and a witness —
so "why not that one?" is answerable from the record, per candidate, rather than reconstructed.

### The pinned-subgraph immutability argument

`logic:PinnedExecutableSubgraph` is immutable and content-addressed, and this is a security property
before it is a hygiene property. An approval binds a dispatch-intent digest; a dispatch intent names
the pinned subgraph and the action-schema version it invokes. If the subgraph could be edited in
place, an operator would be approving a **moving target**: approve the benign expansion, mutate it,
dispatch the malicious one, and every digest downstream still matches because the digest was taken
over a mutable node. The same argument forces `logic:ArgumentNormalization` to pin its *recipe*: two
normalizations of the same arguments are an approval bypass — approve the benign serialization,
dispatch a materially different one that normalizes to the same digest. The kernel reuses the
repository's existing canonicalization doctrine verbatim under a domain tag: runtime handles are never
hashed; only resolved IRI/N3 surfaces enter a provenance digest.

Revision of a pin is therefore a **new pin plus supersession**, exactly as revision of a prescription
version is. `logic:pinAuthority` carries a proof term rather than a bare reference to whoever pinned
it, so the authority behind a pin is checkable rather than merely cited.

### The declared decidable search fragment

Hierarchical planning is undecidable in general: with arbitrary recursive methods, plan existence for
an HTN problem is not decidable. The kernel does not pretend otherwise and does not hide behind a
budget. `logic:SearchFragment` **declares** the fragment within which means–end search is decidable,
by three admission conditions, all checkable statically over the method set:

1. **Total order within each method.** Every `logic:DecompositionMethod`'s subtask structure is
   totally ordered. Partially-ordered HTN planning is where the undecidability bites hardest; total
   order removes the interleaving that produces it.
2. **Acyclic method dependency.** The graph whose nodes are abstract task types and whose edges run
   from a task to the task types appearing in an applicable method's expansion is acyclic. No method
   set may reintroduce recursion through a cycle.
3. **Bounded decomposition depth.** A declared maximum depth, carried on the fragment.

The three conditions together bound the decomposition tree, so search terminates and a **closed**
candidate roster is a claim the engine can certify rather than assert.

The corresponding honesty requirement is that the two ways a search can fail to be closed are **not**
collapsed. A method set violating any admission condition is **out of fragment** and yields a typed
refusal — `logic:OperationOutcome` `OutcomeUnsupportedFragment` with its
`logic:UnsupportedFragmentCondition`, mirroring the engine's existing typed unsupported disposition. A
method set inside the fragment whose search exceeded its budget yields `OutcomeIncomplete` with a
step-budget `logic:IncompleteCause`. These are different facts about the world with different
remedies (restructure the methods versus raise the budget), and merging them into
"incomplete-by-budget" would misdirect every operator who hit the first one.

Closedness itself is never asserted. It is certified from the engine's own
`StrataProgress.saturated_preds` — the provably-final predicate set under a budget cut, carried
publicly as the completion frontier — and projected onto `logic:frontierSaturationWitness`. The kernel
mints **no second frontier**: presenting an incomplete roster as closed is made structurally
impossible rather than forbidden by convention.

## P12 and P17 — bounded search under a declared budget

Principle 17 already licenses this layer's computation, in terms: *`logic:` is deliberately
Turing-complete: a computational substrate, not merely a description language.* Bounded means–end
search over a declared decidable fragment, under an explicit `logic:resourcePolicy` budget, returning
a typed outcome that distinguishes complete, budget-cut, and out-of-fragment, is exactly the
solver-side computation P17 contemplates. **No constitutional amendment is required and none is
proposed.** Constitutional amendment is an owner-only act through the governance surface, and it is
out of bounds for this work.

The apparent tension is with `crates/logic/src/teleology.rs:13-19`, which states that the teleology
evaluator does *no means–end search — it never finds a plan*, and that *every computation is a pure
function over the given structure*. That block is a **module purity contract**, golden-pinned beside
the determinism contract, and it is true and valuable exactly as written. It constrains
`crate::teleology`; it does not constrain `logic:` the language, and it makes no claim about other
modules.

The resolution is placement, not amendment. Means–end search lives in
`crates/logic/src/reason/enactment/`, a different module, under its own contract:

- the teleology evaluator's four-clause determinism contract is **copied verbatim** — insertion-order
  enumeration, first-wins dedup, reused provenance minting, canonical row sort — plus the
  no-optionality clause, and golden-pinned, so the search lane cannot become the one
  non-deterministic evaluator;
- every search runs under a declared `logic:SearchFragment` and an explicit `logic:resourcePolicy`
  budget, driven through the existing budget and step-governor chain, so there is no second budget;
- the outcome is a `logic:OperationOutcome`, so an incomplete search is *labelled* incomplete rather
  than presented as a closed answer.

`teleology.rs` gains **one cross-reference line** naming where search lives and under which budget.
Its purity contract is preserved word for word: weakening it would destroy a real invariant that the
conformance runner depends on, in exchange for nothing.

## Generality obligations

Every kernel term must satisfy this checklist for **all six** named domains — workflow, DAG,
repeatable guide, curriculum, SCADA control plan, organizational guide. A term failing any row is
mis-scoped and is either renamed, generalized, or moved to the domain binding. The checklist is not
advisory: worked examples enact the identical kernel with no domain-specific term, and a term that
cannot be exercised generally is a term that leaked an assumption.

1. **The name reads correctly in all six.** If the local name only makes sense for one domain, it is
   wrong. `logic:Enactment`, not `logic:WorkRun`; `logic:PrescriptionVersion`, not
   `logic:WorkPlanVersion`; `logic:ApprovalCommitment`, not `logic:ManagerSignoff`.
2. **Its identity criterion is domain-neutral.** No criterion may depend on a concept only one domain
   has. A criterion mentioning a work item, a ticket, a course code, or a tag number is disqualified.
3. **It names no `gmeow:` term.** Not in an axiom, not in a range, not in a `skos:definition`, not in
   a comment. A grounding slice that cites a downstream specializer by name has inverted the
   dependency direction; the domain-typed edge belongs in the binding slice, authored as
   `gmeow:X rdfs:subPropertyOf logic:Y` with a `gmeow:` range.
4. **It has a non-trivial reading in each domain, and they differ.** A term whose six readings are all
   the same sentence is probably a domain term wearing a general name. `logic:ResourceLease` passes:
   exclusive actuator control in SCADA, a seat lock in a curriculum, a distributed lock in a workflow,
   an exclusive engine handle in a DAG, a room booking in an organizational guide, a checked-out
   instrument in a guide.
5. **Its constraints are true in all six.** A constraint true only under one domain's operating
   assumptions is a domain constraint. The maintenance-goal law passes because it is a property of
   maintenance goals, not of any domain.
6. **Its value classes are closed and exhaustive across the six.** A closed value class missing the
   value one domain needs forces that domain to abuse another value. Each axis was enumerated against
   all six before closing.
7. **It is grounded.** Every quantity — budget, cost, deadline consumption, severity, satisfaction
   degree — grounds in `math:` with a dimension; every textual surface — guidance, rubric, dissent
   prose, model draft — grounds in `lang:`. A quantitative term with a bare literal is carrying an
   ungrounded number.
8. **It survives the projection.** Every external-system correspondence carries an explicit
   preservation judgment, and any non-exact judgment names its expressiveness boundary. A term that
   can only be expressed by pretending a lossy target is faithful is not ready.
9. **Its negative case is authored.** Every mint carries at least one single-defect counter-example
   that trips its named gate **and no other**. A constraint with no fail witness is prose.
10. **It survives the six-domain example sweep.** The generality examples enact the kernel over a DAG
    workflow, a curriculum, and a SCADA control plan with **no** work-orchestration term present; a
    term that cannot appear in those without a domain binding belongs in the binding slice.

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Teleology: Goals and Action

> The goal-and-action member of the GMEOW Logic design set ([`LOGIC.md`](LOGIC.md)). It gives the
> meaning of goal-directed reasoning in `logic:`: structured goal expressions, the reified goal
> evaluation of which `gmeow:satisfiedBy` is the conclusive view, action schemas and their
> occurrences, goal decomposition, and goal conflict. State change is in
> [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md); the typed worlds these constructs reason over are
> in [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md); the deontic force that ranges over goals is the
> norms vocabulary, aligned here. Carrying a `logic:Plan` out durably — versioning it, enacting it,
> refining it, and recording what it committed outside the graph — is
> [`LOGIC-ENACTMENT.md`](LOGIC-ENACTMENT.md), which reuses `logic:Plan` unchanged and mints no second
> plan concept. openEHR Task-Planning (PROC) ↔ `logic:Plan` is a process-axis
> correspondence — the prescriptive↔descriptive seam is a lossy lens
> ([`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md); see the canonical process model).

## Why goals and action are their own layer

A transaction path says **how** state changes; an intention says **what** an agent commits to; an
action says **what is done** to move from one state to the next. These are distinct, and the logic
keeps them distinct. The teleology layer is the structure that *motivates and evaluates*
transactions: it carries the goals an agent holds, the structured conditions under which a goal
counts as met, the actions whose execution a transaction path records, and the ways goals refine and
collide. Means–end search — finding a plan that reaches a goal — is a solver-layer computation; the
teleology layer is the representation that search reads and writes, not the search itself.

That solver-layer search has a declared home: the **enactment lane**
([`LOGIC-ENACTMENT.md`](LOGIC-ENACTMENT.md)), where bounded hierarchical means–end search runs over
`logic:DecompositionMethod` applicability within a declared decidable `logic:SearchFragment` and under
an explicit `logic:resourcePolicy` budget, returning a typed outcome that distinguishes a complete
search from a budget cut and from an out-of-fragment method set. It lives in
`crates/logic/src/reason/enactment/` and copies this layer's four-clause determinism contract verbatim.
The native teleology evaluator's own purity contract is unchanged by that placement: it still performs
no means–end search and never finds a plan — every computation it makes remains a pure function over
the given structure.

The layer is authored over GMEOW's own teleological vocabulary rather than reinvented. A
`gmeow:Goal` is the propositional content — a described state of affairs that situations satisfy via
`gmeow:satisfiedBy`. The commitment-graded intentional trichotomy is `gmeow:Desire` (wanted, not
committed), `gmeow:Intention` (internally committed), and `gmeow:Commitment` (committed toward
another agent); the first two are intrinsic modes inhering in one agent through `gmeow:intentBearer`,
the third a relator mediating `gmeow:committedAgent` and `gmeow:commitmentBeneficiary`. Every mode
aims at exactly one goal through `gmeow:intentionGoal`, and an intentional moment explains the events
it gives rise to through `gmeow:motivates`. The flat `gmeow:hasGoal` carries the common case;
`gmeow:IntentionTenure` reifies adoption and abandonment over an interval. The teleology layer of
`logic:` adds the *reasoning structure* over these holdings — expressions, evaluation, action, and
decomposition — as values and constructs the engine computes with.

## Canonical representation

Every construct this layer names has one canonical form in the RDF/IR model, so a goal structure is
data the engine reads and writes rather than prose. A `gmeow:Goal` carries its structure through
`logic:hasGoalCondition` to a `logic:GoalExpression`; the flat `gmeow:hasGoal` and `gmeow:satisfiedBy`
edges are the surface views these richer forms generate.

| Construct | Canonical form | Attachment | Identity |
| --- | --- | --- | --- |
| goal expression | `logic:GoalExpression`, a typed IR term whose node kind is one of the variants below | `gmeow:Goal —logic:hasGoalCondition→ logic:GoalExpression` | structural: two expressions with the same node kind, operands, and bound situation type are the same term ([LOGIC-IR.md](LOGIC-IR.md)) |
| goal evaluation | `logic:GoalEvaluation`, a reified node | one per (goal, situation/world, evaluator, time, criterion) | by that tuple of subjects, vantage-indexed via `gmeow:accordingTo` |
| action schema | `logic:ActionSchema`, a reified type | names precondition, effect, invariant, resource, capability, outcome set, observation, compensation | by IRI |
| action occurrence | a `gmeow:Event` typed by an `logic:ActionSchema` | `gmeow:Event —logic:instantiatesSchema→ logic:ActionSchema`, linked to its motive by `gmeow:motivates` | by IRI |
| goal decomposition | `logic:` dependency relations between `gmeow:Goal` individuals | `logic:refinesGoal`, `logic:contributesTo`, `logic:necessaryFor`, `logic:sufficientFor`, grouped by `logic:AndDecomposition` / `logic:OrDecomposition` | relation edges |
| plan | `logic:Plan`, a transaction program (below) | `logic:Plan —logic:planGoal→ gmeow:Goal`, its steps invoking action schemas | by IRI |

`gmeow:intentionGoal` stays functional and ranges over `gmeow:Goal`: an intentional mode aims at
exactly one goal, and that goal — not the expression — is what the mode points at. Goal structure
lives on the goal through `logic:hasGoalCondition`, so the functional edge is untouched. Goal
decomposition links `gmeow:Goal` individuals through the `logic:` dependency relations above; these
carry *satisfaction dependency* and are distinct from `gmeow:properPartOf`, which carries structural
part-hood. A sub-goal may be both a proper part and `logic:necessaryFor` its parent: the flat
mereology spine is the surface view, and the `logic:` dependency relations are the precise canonical
form means–end search reads.

A `logic:GoalExpression` is compositional structure carried as a typed IR term: RDF authoring of a
goal condition compiles into that term, the term serializes back to canonical RDF through the IR's
term encoding, and structural equality decides expression identity ([LOGIC-IR.md](LOGIC-IR.md)). The
projections weaken it predictably — OWL and the flat surface carry `gmeow:hasGoal` and a conclusive
`gmeow:satisfiedBy` but not the expression structure — and each lowering records that loss in the
preservation ledger ([LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md)).

## Structured goal expressions

A bare `gmeow:Goal` names a target; a **goal expression** states the structure of that target so the
engine can evaluate it compositionally. A goal expression is one of:

- **atomic** — satisfied exactly when a named situation type obtains;
- **conjunctive** / **disjunctive** — satisfied when all, or any, of its sub-expressions are;
- **achievement** — satisfied once the target first obtains along a path;
- **maintenance** — satisfied only while the target holds at every state of an interval, and
  violated at the first state it fails;
- **avoidance** — the dual of maintenance: satisfied while a proscribed situation never obtains;
- **optimization** — directed at a measure, satisfied to the degree the measure is extremized rather
  than by a crisp boundary;
- **conditional** — a target that applies only when a guard situation holds;
- **deadline-window** — an achievement or maintenance target indexed to a bounding interval, after
  which the window closes.

Achievement and maintenance are path properties, evaluated over the ordered states of a transaction
path ([`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md)); optimization is directed at an **objective
value** with a stated direction and unit, recorded as a quantity on the evaluation (below), never
folded into a truth value and never into an uncertainty measure. A goal expression is a typed term in
the intermediate representation ([`LOGIC-IR.md`](LOGIC-IR.md)), so the same construct compiles toward
whichever consumer a projection serves and records its loss when a target form cannot hold the
distinction.

## Goal evaluation is reified and factored

Whether a goal is met is a vantage-indexed claim, never a global verdict, and "met" is itself several
independent questions. The canonical record is a **reified goal evaluation** — `logic:GoalEvaluation`
— over a goal, the situation or world it is judged against, the evaluator holding the judgment, the
time of judgment, and the criterion applied. Its outcome is factored into independent axes rather than
one collapsed label:

- **satisfaction status** — `satisfied`, `partial`, `violated`, or `unsatisfied`;
- **feasibility status** — `feasible`, `blocked`, `infeasible`, or `unknown`;
- **lifecycle status** — `active`, `retired`, or `superseded`;
- **evaluation status** — `completed`, `undetermined`, or `unsupported`;
- **satisfaction degree** — a quantity, for a goal met to a degree rather than crisply.

The axes vary independently: a goal can be `partial` in satisfaction, `blocked` in feasibility, and
`active` in lifecycle at once, and the engine records each rather than choosing a single token. A
maintenance or avoidance goal whose window has not closed has evaluation status `undetermined` —
holding so far is not conclusive satisfaction; it reaches `completed` only when the window closes
(satisfied) or the target fails (violated). Two evaluators disagreeing about the same goal are two
coexisting evaluations, each attributed through `gmeow:accordingTo`; the engine holds both rather than
electing a winner.

The binary `gmeow:satisfiedBy` relation is the **conclusive projection** of this record, and the
source-of-truth rule is explicit: evaluations are canonical, and a `gmeow:satisfiedBy` edge is
generated from each evaluation whose satisfaction status is `satisfied` and whose evaluation status is
`completed`. A directly authored `gmeow:satisfiedBy` edge — the common authoring case —
deterministically expands into a default `logic:GoalEvaluation` carrying `satisfied` / `completed`
status under the asserting vantage. The flat and reified records therefore never disagree: one is
always derived from the other.

The quantities an evaluation carries are kept apart, because none substitutes for another.
**Satisfaction degree** (how much of the goal is achieved), **objective value** with an **objective
direction** and a stated unit or frame (the measured quantity an optimization goal extremizes), and
**utility or preference** (how desirable an outcome is) are distinct from the uncertainty measures the
foundation already separates — `logic:confidence`, `logic:probability`, `logic:weight`, and
`logic:evidenceStrength`
([`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md#confidence-probability-weight-and-evidence)). An uncertainty
measure may annotate a satisfaction degree or an objective value, but a confidence in an evaluation is
never the degree to which the goal is met, and a probability over outcomes is never the objective value
being optimized.

## Action: schemas and occurrences

An action is what carries one state of a path to the next. The teleology layer distinguishes the
**action schema** — the reusable kind of action — from the **action occurrence** — a particular
event executed along a particular path. An occurrence is a `gmeow:Event`, the same perdurant an
intentional moment links to through `gmeow:motivates`; the schema is the type it instantiates.

An action schema declares the structure means–end reasoning reads:

- a **precondition** — the situation that must hold for the action to apply;
- an **effect** — the change the action introduces, expressed as the supersession primitives of the
  transaction layer, never as physical erasure;
- an **invariant** — what the action must preserve across its execution;
- a **resource** — what the action consumes or requires, the seam to resource-competing goal
  conflict below;
- a **capability** — what an agent must be able to do to execute the schema;
- a **nondeterministic outcome set** — the alternative effects an action may have, each a distinct
  successor situation, so an action whose result is not fixed is represented faithfully rather than
  collapsed to a single effect;
- an **observation** — what executing the action reveals, the seam to the cognitive layer
  ([`LOGIC-COGNITION.md`](LOGIC-COGNITION.md));
- a **compensation** — the action that undoes this one, the basis for recoverable transactions.

An action schema grounds the elementary state transitions of a transaction path: executing an
occurrence is the elementary update whose `ins`/`del` effects the path records as supersession
([`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md#elementary-updates-are-supersession-never-erasure)).
The schema names *what an action does*; the path records *that it was done and what changed*.

All eight facets are *executed* by the native teleology evaluator, not merely declared. The
**precondition** and **capability** facets gate an action through the action-gate verdict (a
`logic:GateProbe` pairs a schema with a state and the held capabilities, yielding
`logic:GateAdmitted` or `logic:GateDenied`). The **invariant** facet extends that gate: an invariant
the action would not preserve at the state is a hard, surfaced denial naming the breached invariant —
never a silent pass. The **effect** facet is applied over a `logic:TransactionStep` (which
`logic:instantiatesSchema` from a predecessor state to a successor): the successor situation's support
is computed as `predecessor ∪ ins \ del`, and every deleted support is recorded through the
append-only supersession quartet (`logic:activeInState`, `logic:validUntilState`,
`logic:retiredByTransaction`, `logic:supersededBy`) so a retired support stays recoverable and the
predecessor state still carries it — `del` is supersession, never erasure. The **actionResource**
facet is the representation-level resource seam: a state supplies resources through
`logic:resourceSupply`, and an action requiring a resource the state does not supply, or one flagged
`logic:resourceExhausted`, is gated — this evaluates the represented resource facet only and is *not*
a real build engine-lock, though it is exactly the structure the resource-competing goal conflict
(`logic:competesForResource`) reads. The **observation** facet is surfaced: the observation value an
action reveals (`logic:reveals`) is materialized, and a plan whose `logic:planBranch` branches are
conditioned on it (`logic:branchObservation`, selected by a `logic:branchGuard` matching the revealed
situation) is read as a policy — the evaluator surfaces `logic:selectedBranch` and the branch's
`logic:nextActionSchema`, so the policy's next action is chosen from what an observation revealed
rather than fixed in advance.

A precondition may carry a **valid-time currency guard** — a `logic:FreshnessGuard` (via
`logic:freshnessGuard`) that names the precondition it `logic:guardsPrecondition` and the maximum
admissible datum age (`logic:freshnessHorizon`, an xsd:duration). At the action gate the datum's
`logic:datumRecordedAt` is aged against the probe's `logic:decisionTime`; when the age exceeds the
horizon the gate verdict is neither admit nor deny but `logic:GateUndetermined` — the datum is
treated as *undetermined*, not stale-but-usable, so the evaluator declines to gate on an out-of-date
value rather than admitting on it or denying a value that merely aged. This is the same
conclusiveness discipline the goal side applies with `logic:GoalEvaluationUndetermined` for a
deadline window that has not yet closed, carried down to the action gate: a judgment withheld on
insufficiently-current input rather than guessed. Only fixed-length horizons (weeks / days / hours /
minutes / seconds) are admissible; a nominal-length span (years / months) is a hard error, and a
guarded precondition with no `logic:datumRecordedAt`, or a probe with no `logic:decisionTime`, is a
hard, surfaced error — never a silent pass. It is GMEOW's native generalization of a decision-rule
*currency* annotation (a measurement relied upon only if recorded within N of the decision).

A schema whose completion the engine can learn of **only by being externally told** is a
`logic:NotificationWaitSchema` — a manual-notification or callback-driven completion. It
`logic:awaitsSignal` a `logic:ExternalSignal`, a situation the engine never computes from the state
but only receives. At the gate a wait whose signal has not obtained is `logic:GateUndetermined` —
the *same* withheld-judgment value a stale datum yields, carrying a distinct `logic:awaitingSignal`
witness rather than a `logic:gateUndeterminedReason` — because the wait is **pending**, not denied:
the signal may still arrive. In a plan a wait step is not executable until its signal obtains, so the
transaction path **halts at the wait** rather than fabricating an un-signalled completion. This is the
prescriptive↔descriptive epistemic boundary made canonical: what the engine knows of the world is
limited to what it is told, and a wait names exactly which external fact it is still waiting for.

## Plans and nondeterministic outcomes

A **plan** (`logic:Plan`) is a transaction program whose primitive operations invoke action schemas
and whose success criterion is a goal expression. As a program it carries the structure a decomposed
goal alone cannot — ordering and serial composition, guards, branching, concurrency, loops, and
fallback behaviour — all in the operators of the transaction layer
([`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md)). A plan whose next action is chosen from what an
earlier action's observation reveals is a **policy**: a plan whose branches are conditioned on
observations rather than fixed in advance. Plan adoption and revision are recorded as an intention is
— a `logic:Plan` is held over an interval and revised by suppression, never mutated.

Because an action schema may have a nondeterministic outcome set, plan success is quantified over
those outcomes rather than left to the bare executional-success reading of the transaction layer,
under which the mere existence of one favourable path would count as success. A plan therefore
declares its success quantification:

- a **weak plan** succeeds when *some* outcome path reaches the goal;
- a **strong plan** succeeds when *every* outcome path reaches the goal;
- a **strong-cyclic plan** succeeds when every *fair* execution is guaranteed to reach the goal in
  finitely many steps, allowing retried loops through recoverable failures.

Compensation is tied to the **realized outcome**, not to the schema as a whole: each branch of a
nondeterministic outcome set names the compensation that recovers from *it*, so recovery matches what
actually happened rather than applying one generic undo across every branch.

## Goal decomposition

Goals form a directed structure finer than bare mereology. The decomposition relations are:

- **refines** — a sub-goal that makes a coarser goal precise;
- **contributes-to** — a sub-goal whose satisfaction advances a goal without settling it;
- **necessary-for** — a sub-goal that must be satisfied for the parent to be;
- **sufficient-for** — a sub-goal whose satisfaction settles the parent on its own.

A goal carries an **AND-decomposition** when all its necessary sub-goals must hold together, and an
**OR-decomposition** when any one sufficient sub-goal settles it; the two compose to the
and-or goal graph over which means–end search runs. Decomposition is typed apart from the generic
mereology spine on purpose: `properPartOf` says a goal is structurally part of another, while
`necessary-for` and `sufficient-for` carry the satisfaction dependency that planning needs and that
part-hood alone does not express. The graph is the data; the search across it is a solver-layer
computation, carried under the reasoning contract rather than asserted as axioms.

## Goal conflict

Two goals conflict when they cannot both be satisfied, and the kind of conflict determines what the
engine does about it. The conflict relations are:

- **contradictory** — satisfying one entails violating the other in every world;
- **contrary** — the two cannot both be satisfied, though both may fail;
- **prevents** — achieving one closes the path to the other;
- **competes-for-resource** — both draw on a resource an action schema declares, and the supply is
  insufficient for both;
- **temporally-incompatible** — each is achievable, but not within the windows both require;
- **deontically-incompatible** — a norm makes the conjunction impermissible even where it is
  physically reachable;
- **lower-priority-under-policy** — under a governing priority ordering established by an applicable
  policy, one goal is deprioritised in favour of the other; the conflict is resolved by policy
  precedence, not by logical, temporal, or deontic impossibility.

A goal's `gmeow:counterGoal` is the strongest of these: constitutive opposition, the named shadow
that partly defines what the goal means, symmetric and irreflexive. The remaining conflict kinds are
ordinary relations between independently-meaningful goals. A witnessed conflict is surfaced as a
finding, with the conflicting goals and the conflict kind named; it is contained as
context-indexed information rather than silently resolved, the same discipline the context algebra
applies to contradiction ([`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md#inconsistency-across-contexts-and-context-indexed-entailment)).

## Norms and deontic force over goals

A goal prescribes nothing on its own. Deontic force arrives with the norms vocabulary, which ranges
`gmeow:prescribedConduct` over goals and conduct: a `gmeow:Norm` carries a `gmeow:DeonticModality` —
obligation, prohibition, permission, or recommendation — issued by a `gmeow:normIssuer` (a
specialization of `gmeow:accordingTo`, so every norm is attributed) and borne by a
`gmeow:normBearer`. Norms carry a `gmeow:AuthorityLevel` and order one another through
`gmeow:overrides`, whose precedence is recorded as a `gmeow:PrecedenceTenure` rather than asserted as
a transitive global fact. Deontic obligation and prohibition are evaluated against the deontic-ideal
worlds of the typed context algebra. Deontic accessibility is **serial**: every context from which an
obligation is read has at least one accessible ideal world, so the universal condition is never
vacuously satisfied; a context with no accessible ideal world yields evaluation status `undetermined`,
not a vacuous obligation. An **obligation** holds when the prescribed goal is satisfied in every
accessible ideal world. A **prohibition** is the deontic treatment of the goal's *negation*: it holds
when the goal's negation is supported in every accessible ideal world — which, under the paraconsistent
and partial-truth readings of the context algebra, is stronger than the goal merely failing to be
supported, since failure of support and support for the negation are not interchangeable
([`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md#inconsistency-across-contexts-and-context-indexed-entailment)).
The dependency points from the norms vocabulary toward this layer — norms range over goals, never the
reverse — so the goal structure stays free of deontic commitment.

## Where it connects, and what it is not

The teleology layer sits at the centre of a chain the logic keeps as four distinct links: an
**intention** is what an agent commits to (`gmeow:Intention` / `gmeow:Commitment`); a **plan** is the
decomposed goal structure and the action schemas selected to reach it; an **action** is the schema
whose occurrence executes a step; and the **transaction path** is how the state changes as those
occurrences run. Each link names its own thing and refers to its neighbours without absorbing them —
the intention → plan → action → transaction-path chain
([`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md#where-it-connects-and-what-it-is-not)).

Goal-directed and deontic reasoning are *uses* of the typed worlds, not new world types: a goal
expression is evaluated against goal-directed and deontic-ideal worlds in the same typed context
algebra that carries alethic, epistemic, and counterfactual reasoning
([`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md#worlds-modality-and-counterfactuals--a-typed-context-algebra)).
On the foundational spine, a goal is a social object, an intention an intrinsic mode, a commitment a
relator, and the events that satisfy goals are perdurants — the UFO⁺ sorts of
[`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md), so teleology composes with identity, rigidity, and
mereology rather than standing beside them.

## Conformance obligations

The conformance corpus ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md)) carries this layer through
named cases that pin its distinctions, at least:

- a **conjunctive** goal versus a **disjunctive** goal over the same sub-targets;
- a **maintenance** goal that fails midway through a path, distinguished from one whose window is
  still open and so evaluates `undetermined`;
- a **conditional** goal whose guard situation is false, which prescribes nothing;
- **partial satisfaction** held apart from low **confidence** in a `satisfied` evaluation;
- a **weak** plan versus a **strong** plan over the same nondeterministic outcome set;
- **outcome-specific compensation**, where two outcome branches recover by different actions;
- a deontic context with **no accessible ideal world**, which evaluates `undetermined` rather than
  yielding a vacuous obligation;
- **contested goal evaluations**, where two vantages disagree and both records are retained;
- an **invariant breach** that denies the action, surfaced as a `logic:GateDenied` verdict naming
  the breached invariant rather than passing silently;
- an **effect applied as supersession**, where the successor situation's support is `ins`ed and
  `del`ed with the retired support recorded append-only (recoverable), never erased;
- a **resource exhaustion** that gates the action, where a required `logic:actionResource` is
  supplied but flagged exhausted;
- an **observation-conditioned policy**, where the branch whose guard matches the revealed
  situation is selected and its next action schema surfaced;
- a **guarded choice** (`logic:Choice`), where the left sub-program is selected when the guard
  holds and the right when it does not — the structural program combinator behind policy dispatch,
  typed apart from a conditional formula connective;
- a **concurrent composition** (`logic:ConcurrentComposition`), where two sub-programs advance
  with interleaved steps — the composition operator, distinct from the correctness concerns
  (`logic:SerializabilityCriterion`, `logic:IsolationLevel`, `logic:ConcurrencyControlProtocol`);
- an **iteration** (`logic:Iteration`), where a body sub-program repeats while a loop condition
  holds — structurally expressible with `logic:iterationBody` and `logic:iterationCondition`,
  the program-level counterpart of the loop quantification `logic:StrongCyclicPlanSuccess` covers;
- a **fallback** (`logic:Fallback`), where the primary sub-program is attempted and the alternate
  runs on failure — typed apart from action-level `logic:compensation`, which undoes an action
  that ran and reached a realized outcome.

## Constitutional alignment

Goal satisfaction is a vantage-indexed claim carried through the statement layer, not a global
verdict; goal and intention revision is supersession, never erasure, so an abandoned intention is a
closed tenure that remains auditable; means–end search and goal-decomposition computation stay at the
solver layer, while the ontology records the goals, their structure, and who holds them. The layer
adds one orthogonal dimension of reasoning — goals and the actions that pursue them — composed with
the rest of the reasoning contract rather than re-bundling it, the same compositional discipline the
logic enforces everywhere.

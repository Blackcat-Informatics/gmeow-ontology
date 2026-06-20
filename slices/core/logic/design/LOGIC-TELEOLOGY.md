<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — The Teleological Goal and Action Layer

> The goal-and-action member of the GMEOW Logic design set (see [`LOGIC.md`](LOGIC.md)): how `logic:`
> reasons about goals, intentions, plans, and action, and how they connect to state change
> ([`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md)) and to norms.

## What this layer authors

This layer authors the **goal structure** that motivates and evaluates the state changes of the
Evolution `transaction-path` facet ([`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md)). It is the
teleological reading of action: *why* an agent moves a path from one state to the next, and *whether*
the situation reached counts as the one aimed at. A `gmeow:Goal` asserts nothing and prescribes
nothing — it is a described state of affairs (a gufo:SocialObject). It is **satisfied by** situations
(`gmeow:satisfiedBy`), **held by** agents (the intentional modes, or the flat `gmeow:hasGoal`
shortcut), and **evaluated from** standpoints. Satisfaction is a vantage-indexed claim, never a global
verdict: that an agent reached the situation it wanted, and that another agent disputes the
achievement, are two coexisting claims, not a contradiction to be settled. The deontic force that
*requires* a goal lives one slice over, in norms; the goal itself carries none.

## Goals and intentional modes

The intentional family is **commitment-graded**, and the grade is the type of the moment. A
`gmeow:Desire` is wanting without commitment — the agent would welcome the goal's satisfaction but has
not settled on pursuing it. A `gmeow:Intention` is internal commitment — the agent has settled on the
goal, though no other agent holds them to it. Both are intrinsic modes (a gufo:IntrinsicMode): they
inhere in exactly one agent, named by the functional `gmeow:intentBearer`, and aim at exactly one goal
through the functional `gmeow:intentionGoal`. A `gmeow:Commitment` is the social grade — committed
*toward* at least one other agent — and it is a gufo:Relator, not a mode: it mediates a
`gmeow:committedAgent` and one or more distinct `gmeow:commitmentBeneficiary`. An intention or
commitment **motivates** events through `gmeow:motivates`, the teleological link a transaction path's
elementary updates connect back to.

These constructs ride the foundation's **flat-first, reify-on-demand** discipline
([`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md)) at three depths:

- **Flat** — `gmeow:hasGoal` from an agent to a goal covers the common case, commitment grade
  unspecified. It is the 80% shortcut: an agent holds a goal, and nothing about grade, tenure, or
  standpoint needs to become first-class.
- **Reified mode** — a `gmeow:Desire`, `gmeow:Intention`, or `gmeow:Commitment` is minted when the
  commitment grade itself matters: who is bound, toward whom, and whether the holding is mere wanting
  or settled resolve.
- **Reified tenure** — a `gmeow:IntentionTenure` is minted when *adoption and revision over time* are
  the fact of interest. A goal adopted in one interval and abandoned in the next is an opened-then-closed
  tenure, never an erasure: withdrawal is **suppression, not deletion**, the same supersession
  discipline state change obeys in [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md).

## Structured goal expressions

`gmeow:intentionGoal` is functional — one mode aims at one goal — and that functionality is the right
shape, but real goals are compound: *reach orbit **and** retain fuel reserve*, *ship by Friday **or**
ship the reduced build*. A **GoalExpression** is the design construct that carries this structure: it
is the structured content a `gmeow:intentionGoal` points at, so the property stays functional and its
single value is one — possibly composite — expression. One intention pointing at one structured
expression is cleaner than one intention pointing at many independent goals, because the *combination
logic* is part of what the agent intends, not an accident of how many goal individuals happen to be
listed. A GoalExpression takes one of these forms:

- **atomic** — a single `gmeow:Goal`, the leaf;
- **conjunction** / **disjunction** — *all of* / *at least one of* its sub-expressions;
- **achievement** — bring about a state of affairs the start state lacks;
- **maintenance** — keep a state holding across an interval (a constraint on the whole path, not a
  single state);
- **avoidance** — keep a state from ever holding along the path;
- **optimization** — drive a quantity toward an extremum rather than meet a threshold;
- **conditional** — pursue the sub-expression only when an antecedent holds;
- **deadline / temporal-window** — satisfy the sub-expression within a bounded interval.

The leaves are `gmeow:Goal` individuals; the combinators and temporal qualifiers are the structure
above them. A maintenance or avoidance expression is inherently path-shaped: its satisfaction is a
property of the sequence of states the transaction path executes through, exactly the path semantics
of [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md).

## Goal evaluation

Whether a situation satisfies a goal is a claim, and a claim has an evaluator, a time, and a
standpoint. A **GoalEvaluation** is the design construct that reifies that claim. It carries the goal,
the situation or world it is judged against, the evaluator or standpoint, the time of judgement, the
criterion applied, and the resulting degree or status. Status is one of:

- **satisfied** — the goal is met;
- **partial** — met to a degree short of full;
- **violated** — a maintenance or avoidance goal whose protected state was breached;
- **blocked** — unmet, with an identified obstacle;
- **infeasible** — unmet with no reachable satisfying situation;
- **unknown** — not determinable from what is recorded;
- **superseded** — the goal was withdrawn or replaced before the question resolved.

The binary `gmeow:satisfiedBy` is a **generated projection** of the GoalEvaluations whose status is
*satisfied* and whose standpoint is unequivocal and conclusive — the flat edge a planner or renderer
reads when it wants the achievement link without the evaluation apparatus. It is the reify-on-demand
pair to the evaluation, not a competing record. Evaluation is **vantage-indexed** throughout: disputed
satisfaction is several coexisting GoalEvaluations carrying different evaluators, never a global
verdict the reasoner is asked to adjudicate. The same goal is *satisfied* from one standpoint and
*partial* from another, and both stand.

## Action theory

Goals say *what* state an agent aims at; an **ActionSchema** says *what an action does to state*. It is
the design construct that describes an event **type** — a reusable specification of a state transition,
distinct from any particular performance of it. An ActionSchema carries:

- **precondition** — the state in which the action is applicable;
- **effect** — the change it makes to the successor state;
- **invariant** — what it must preserve across the transition;
- **resource requirement** — what it consumes or holds;
- **capability** — what the performing agent must be able to do;
- **nondeterministic outcome** — the alternative effects a single application may yield;
- **observation** — what becomes knowable to the agent by performing it;
- **compensation / rollback** — the action that undoes its effect.

Each performance of a schema is an **ActionOccurrence**: a particular event, with provenance and a
time, that instantiates the schema's type. Because schemas describe event types, they sit exactly where
the foundation's type-level dispositional reasoning lives ([`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md))
— a schema's precondition and effect are real whether or not the action is ever performed, with no
event token entailed. An ActionSchema's effect **grounds an elementary state transition** of the
transaction path: the schema is the typed account of *how* one step carries one state to the next,
which the path then executes and records as supersession-not-erasure.

## Goal decomposition

A complex goal is broken into sub-goals, and that breakdown is a **dedicated structure**, never the
generic `properPartOf` of the mereology spine — a sub-goal is not a *spatial* part of its parent, and
borrowing parthood would let supplementation and extensionality fire where they have no meaning. The
decomposition relations are their own:

- **refinesGoal** — a sub-goal that makes a parent goal more specific or operational;
- **contributesTo** — a goal whose satisfaction advances another without guaranteeing it;
- **necessaryFor** — a goal that must be satisfied for the parent to be satisfiable;
- **sufficientFor** — a goal whose satisfaction is enough for the parent.

These hang on **AND/OR refinement nodes**: an AND node holds when all its children hold, an OR node
when at least one does. A goal tree is therefore an explicit graph of refinement and contribution
edges over `gmeow:Goal` individuals — the structure a plan reads to decide which sub-goals to pursue,
kept apart from both mereology and the conjunction/disjunction *content* of a GoalExpression (a
GoalExpression composes what one mode intends; the decomposition graph relates standing goals across
modes and agents).

## Goal conflict

`gmeow:counterGoal` names the **constitutive opposite** — the shadow that partly defines a goal, an
oath and its betrayal — and it is symmetric and stronger than a lexical antonym. Conflict in general is
finer, and the layer names each kind it must keep apart:

- **logically-contradictory** — the goals' satisfying situations cannot co-hold in any state;
- **contrary** — they can both fail but cannot both succeed;
- **prevents** — satisfying one forecloses satisfying the other;
- **competes-for-resource** — both draw on a resource that cannot meet both;
- **temporally-incompatible** — both are satisfiable, but not within the windows each requires;
- **deontically-incompatible** — a norm makes pursuing one impermissible while another is pursued;
- **lower-priority-under-policy** — both can hold, but a declared precedence policy ranks one below.

Each is a **recorded claim**, indexed to whoever asserts it, never a reasoner entailment that the goals
*are* in conflict. Which conflict obtains, and what it does to a plan, is read by the solver over the
recorded edges, the same Principle-12 boundary norms draw for precedence.

## Where it connects

The chain from motive to executed state change has four distinct links, and conflating any two of them
is the collapse this layer exists to prevent:

- an **intention** says *what an agent commits to* — a `gmeow:Intention` or `gmeow:Commitment` aimed
  at a goal;
- a **plan** says *what is intended to be done* — a goal decomposition together with the action
  schemas selected to satisfy it;
- an **ActionSchema** says *how an elementary transition changes state* — the typed account of one
  step;
- a **transaction path** says *how state changes* — it executes, advancing the path and recording each
  `ins` / `del` as supersession ([`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md)).

**Goal-worlds** are uses of the typed context algebra of
[`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md#worlds-modality-and-counterfactuals--a-typed-context-algebra),
not a new world type. A goal picks out the situations and worlds that realize it; goal-directed
reasoning and deontic-ideal reasoning are both readings over the typed contexts, each over its own
typed accessibility relation. A deontic claim is truth in the **deontically-ideal** accessible contexts
of an issuer — which is why the bare accessibility superproperty licenses no cross-type inference: that
a goal is *believed* reachable does not make it *permitted*, and that a state is *deontically ideal*
does not make it the goal an agent *holds*.

**Norms** range over goals at the seam: `gmeow:prescribedConduct` explicitly ranges over `gmeow:Goal`,
so an obligation, prohibition, permission, or recommendation can prescribe a goal as readily as an
event type. Deontic incompatibility between a goal and a norm, and the resolution of competing
prescriptions, are recorded claims — `gmeow:overrides` is pairwise and **not transitive**, and
`gmeow:PrecedenceTenure` carries the time-scoped precedence — settled by the solver over those claims,
never entailed by the reasoner. There is no ought, only ought-according-to: every prescription names
its `gmeow:normIssuer`, and two normative systems that prescribe opposite goals coexist without
inconsistency because neither is asserted.

## Constitutional alignment

Goals and action here obey the same discipline the rest of the foundation enforces. Satisfaction is
**vantage-indexed** — there is no global "the goal is satisfied," only GoalEvaluations from named
standpoints, the binary `gmeow:satisfiedBy` a projection of the conclusive ones. The intentional family
is **flat-first, reify-on-demand** — `gmeow:hasGoal`, then a mode, then an `gmeow:IntentionTenure`,
each promoted only when its extra structure earns its keep. The status, conflict, and expression
vocabularies are **open value vocabularies**, extended by minting an individual, never a subclass. And
adoption obeys **suppression-not-erasure** — a withdrawn intention is a closed tenure carried with
`gmeow:displayable false`, never a deleted triple, exactly as every `del` on the transaction path is a
supersession of a support and never an erasure of the record.

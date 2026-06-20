<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Teleology: Goals and Action

> The goal-and-action member of the GMEOW Logic design set ([`LOGIC.md`](LOGIC.md)). It gives the
> meaning of goal-directed reasoning in `logic:`: structured goal expressions, the reified goal
> evaluation of which `gmeow:satisfiedBy` is the conclusive view, action schemas and their
> occurrences, goal decomposition, and goal conflict. State change is in
> [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md); the typed worlds these constructs reason over are
> in [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md); the deontic force that ranges over goals is the
> norms vocabulary, aligned here.

## Why goals and action are their own layer

A transaction path says **how** state changes; an intention says **what** an agent commits to; an
action says **what is done** to move from one state to the next. These are distinct, and the logic
keeps them distinct. The teleology layer is the structure that *motivates and evaluates*
transactions: it carries the goals an agent holds, the structured conditions under which a goal
counts as met, the actions whose execution a transaction path records, and the ways goals refine and
collide. Means–end search — finding a plan that reaches a goal — is a solver-layer computation; the
teleology layer is the representation that search reads and writes, not the search itself.

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
path ([`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md)); optimization is directed at a measure carried
under the `Uncertainty` facet of the reasoning contract
([`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md)), never folded into a truth value. A goal expression is a
typed term in the intermediate representation ([`LOGIC-IR.md`](LOGIC-IR.md)), so the same
construct compiles toward whichever consumer a projection serves and records its loss when a target
form cannot hold the distinction.

## Goal evaluation is reified; `satisfiedBy` is its conclusive view

Whether a goal is met is a vantage-indexed claim, never a global verdict. The canonical record is a
**reified goal evaluation**: a goal, the situation or world it is judged against, the evaluator
holding the judgment, the time of judgment, the criterion applied, and a status drawn from a fixed
set — **satisfied**, **partial**, **violated**, **blocked**, **infeasible**, **unknown**, or
**retired** (the goal it judged is no longer pursued). Two evaluators disagreeing about the same goal are two coexisting evaluations, each
attributed through `gmeow:accordingTo`; the engine holds both rather than electing a winner.

The binary `gmeow:satisfiedBy` relation is the **conclusive projection** of this record: it relates a
goal to a situation exactly when a goal evaluation reaches the *satisfied* status under a stated
vantage. Authoring the flat relation directly is the common case, and reading it back is the common
query; the reified evaluation is what carries the partial, blocked, and contested judgments the flat
relation cannot. The richer record never overrides the flat one — it is the structure the flat
relation summarizes.

A partial or graded evaluation carries its degree under the `Uncertainty` facet and through the four
distinct quantitative predicates the foundation keeps separate —
`logic:confidence`, `logic:probability`, `logic:weight`, and `logic:evidenceStrength`
([`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md#confidence-probability-weight-and-evidence)) — so a
confidence in an evaluation is never mistaken for a probability that the goal obtains.

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
  physically reachable.

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
a transitive global fact. Deontic obligation and prohibition are evaluated against the
deontic-ideal worlds of the typed context algebra: an obligation holds when the goal it prescribes is
satisfied in every deontically-accessible ideal world, a prohibition when it is satisfied in none.
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

## Constitutional alignment

Goal satisfaction is a vantage-indexed claim carried through the statement layer, not a global
verdict; goal and intention revision is supersession, never erasure, so an abandoned intention is a
closed tenure that remains auditable; means–end search and goal-decomposition computation stay at the
solver layer, while the ontology records the goals, their structure, and who holds them. The layer
adds one orthogonal dimension of reasoning — goals and the actions that pursue them — composed with
the rest of the reasoning contract rather than re-bundling it, the same compositional discipline the
logic enforces everywhere.

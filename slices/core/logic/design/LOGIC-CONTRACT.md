<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — The Reasoning Contract

> Status: canonical target architecture for GMEOW's reasoning configuration. This document
> defines how a reasoning request is specified. It is a member of the GMEOW Logic design set
> (see [`LOGIC.md`](LOGIC.md)); the formal semantics each facet selects are made precise in
> [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md).

## Why a contract, not a profile

The dimensions GMEOW reasons along — stable-model versus well-founded versus Horn consequence,
classical versus explicit versus default negation, open- versus closed-world, world-indexed
versus unindexed, static versus transactional, probabilistic versus weighted, paraconsistent
versus explosive — are **mostly orthogonal**. They are not competing points on a single axis;
they are independent choices that compose.

Collapsing them into a short list of named profiles repeats the mistake GMEOW refuses elsewhere:
it freezes a product of independent decisions into a handful of points and makes the unchosen
combinations inexpressible — or, worse, silently approximated by the nearest named point.

The projection doctrine that governs the rest of the system — *one orthogonal, explicit canonical
form; generate the convenient simplified surface* — is therefore turned inward, onto reasoning
configuration itself. The canonical form is the **`logic:ReasoningContract`**: a selection of
values across orthogonal facets. The simplified surface is the **named preset**, generated as a
bundle of facet values rather than offered as an indivisible alternative.

## The facets

A `logic:ReasoningContract` selects exactly one value on each facet below. Each facet is an open
value vocabulary — individuals, never subclasses — so new values join without a schema change.

| Facet | Concern it settles | Illustrative values |
|---|---|---|
| Consequence | which entailment relation holds | FOL · Horn · well-founded · stable-model · FDE/LP paraconsistent |
| Negation | what "not" means | classical · explicit (strong) · negation-as-failure |
| Closure | what unstated facts mean | open-world · predicate-scoped closed-world |
| Context | what truth is relative to | unindexed · world-indexed · standpoint-indexed |
| Evolution | whether state changes | static · state-transition · transaction-path |
| Uncertainty | how graded belief is carried | none · probabilistic · weighted · fuzzy |
| Argumentation | how conflicts resolve | none · grounded · preferred · policy-specific |
| Revision | how change is absorbed | monotonic · entrenchment-revision · truth-maintenance |
| Equality | when two terms are one | RDF-equality · explicit-equality · optional unique-name |
| Resource | what the engine guarantees | certified-complete fragment · bounded/incomplete |
| Projection | which surface the answer targets | canonical · OWL · Datalog · Common Logic · validation-only |

The facets are deliberately independent. **Evolution = transaction-path** (the state-change
semantics specified in [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md)) is orthogonal to whether
Consequence is Horn, well-founded, stable-model, or probabilistic — which is precisely why
state-change reasoning is a *facet value*, not a parallel profile. In the same way,
paraconsistency (Consequence = FDE/LP) composes with world-indexing (Context) and with
counterfactual revision (Revision) without any one being a mode of the others.

## Presets

A preset is a named `logic:ReasoningContract` that fixes a common, supported facet combination.
The historical profile names survive only as presets — facet bundles the compiler expands before
anything else runs:

- the positive-Horn preset = consequence:Horn · negation:none · closure:open-world · revision:monotonic · resource:certified-complete;
- the stratified-negation preset = consequence:Horn · negation:negation-as-failure (stratified);
- the well-founded preset = consequence:well-founded · negation:negation-as-failure;
- the stable-model preset = consequence:stable-model · negation:negation-as-failure;
- the procedural preset = consequence:Horn · with cut and builtins · resource:bounded (operational, not declarative);
- the probabilistic preset = uncertainty:probabilistic · over a declared dependency model.

Presets are sugar. An author references a preset *or* assembles a contract from facets directly;
either way the canonical form the engine reasons over is the facet set.

## The compatibility matrix and the `unsupported` verdict

Not every point in the facet product is implementable, decidable, or coherent. The compiler holds
an explicit **compatibility matrix** — data, not buried control flow — recording, per facet
combination, whether it is supported, under which `Resource` value, and with what decidability
class (see [`LOGIC-SEMANTICS.md` § Decidability](LOGIC-SEMANTICS.md)). A contract is validated
against the matrix before any reasoning begins.

The cardinal rule: **an unsupported combination resolves to `unsupported` — it is never silently
approximated.** A request that names, for instance, probabilistic stable models, or paraconsistent
counterfactual revision, or scoped negation-as-failure inside generated counterfactual states,
either resolves to a supported (possibly bounded) evaluation, or it is reported as `unsupported`
in the typed result (see [`LOGIC-SEMANTICS.md` § The reasoning result](LOGIC-SEMANTICS.md)).
Quiet substitution of a nearby semantics is the one outcome the contract exists to forbid.

This is the mechanism that lets the reasoning layer grow new power without the independently
powerful dimensions acquiring incompatible meanings when combined: the engine either has a defined
semantics for a contract, or it says so plainly.

## Where a contract is attached

Every reasoning surface carries a contract: each conformance case, the reasoning command, the
memory tooling, deep validation, and every generated proof or coherence certificate. The contract
identity travels with the result (see [`LOGIC-SEMANTICS.md` § The reasoning result](LOGIC-SEMANTICS.md)),
so an answer is never interpretable apart from the contract it was produced under.

## Constitutional alignment

This is the canonical-source / generated-surface doctrine applied to reasoning configuration, and
the co-equal-open-vocabulary doctrine applied to the facets: no facet is privileged, none is
silently collapsed into another, and the simplified named surface is generated from the orthogonal
canonical form rather than substituted for it.

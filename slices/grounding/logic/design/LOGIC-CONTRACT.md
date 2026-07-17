<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — The Reasoning Contract

> How GMEOW's reasoning configuration is specified. This document
> defines how a reasoning request is specified. It is a member of the GMEOW Logic design set
> (see [`LOGIC.md`](LOGIC.md)); the formal semantics each facet selects are made precise in
> [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md).

## Why a contract, not a profile

The dimensions GMEOW reasons along — least-model versus well-founded versus stable-model
semantics, classical versus explicit versus default negation, open- versus closed-world,
world-indexed versus unindexed, static versus transactional, probabilistic versus weighted,
classical versus paraconsistent truth — are **largely independent**. They are not competing
points on a single axis; they are separate choices that compose.

Collapsing them into a short list of named profiles repeats the mistake GMEOW refuses elsewhere:
it freezes a product of independent decisions into a handful of points and makes the unchosen
combinations inexpressible — or, worse, silently approximated by the nearest named point.

The projection doctrine that governs the rest of the system — *one orthogonal, explicit canonical
form; generate the convenient simplified surface* — is therefore turned inward, onto reasoning
configuration itself. The canonical form is the **`logic:ReasoningContract`**: a selection across
the facets below. The simplified surface is the **named preset**, generated as a bundle of facet
selections rather than offered as an indivisible alternative.

**The contract is also the execution selector.** Beyond selecting *what soundness means*, the
`logic:ReasoningContract` selects *which physical strategy* the runtime uses: its resource and
fragment facets drive **fragment-routing** — the runtime evaluates a request with the weakest
sufficient strategy and pays for heavy machinery only on the genuine residue (decidability-as-
projection applied to performance). One typed object, two roles: soundness and plan. See
[`LOGIC-RUNTIME.md` § The native physical engine](LOGIC-RUNTIME.md#the-native-physical-engine--execution-and-optimization).

## The facets

A `logic:ReasoningContract` makes a selection on each facet below. Each facet draws from an open
value vocabulary — individuals, never subclasses — so new values join without a schema change.
The facets are **not uniformly single-valued, and they are not all mutually independent.** Some
settle a single value; some carry a *set* of permitted values; one is a *map* keyed by predicate
or context; one is *multi-dimensional*; two are not semantic selections at all but standing
requests the engine must honour. Each facet's cardinality is stated explicitly so that no reader
assumes a facet behaves like a single switch when it does not.

### Semantic facets

These facets settle the meaning of an entailment.

| Facet | Cardinality | Concern it settles | Illustrative values |
|---|---|---|---|
| Formula fragment | single value | the syntactic class of admitted formulae | FOL · Horn · Datalog · existential-rules |
| Model semantics | single value | which models are selected | classical · least-model · well-founded · stable-model |
| Truth / inconsistency semantics | **structured selection** | how truth and contradiction are evaluated | a Belnap-family configuration — algebra + admissible-valuation (gap/glut) policy + designated set — yielding FDE · LP · K3 · classical · … |
| Negation operators | **set of values** | which "not" operators a program may use | explicit (strong) · default (negation-as-failure) · both |
| Closure | **map: predicate / context → value** | what unstated facts mean, scoped per predicate or context | open · closed (per key) |
| Context | **multi-dimensional index** | what a truth is relative to, on several axes at once | world · standpoint · time · path |
| Evolution | single value | whether and how state changes | static · state-transition · transaction-path |
| Uncertainty | **set of measures** | how graded belief is carried, possibly several at once | none · probabilistic · weighted/ranking · fuzzy |
| Argumentation | single value | how conflicts resolve | none · grounded · preferred · policy-specific |
| Revision | single value | how change is absorbed | monotonic · entrenchment-revision · truth-maintenance |
| Equality | single value | when two terms are one | RDF-equality · explicit-equality · optional unique-name |

Three cardinalities deserve emphasis because the earlier framing flattened them:

- **Negation is a set, not a single value.** A program may legitimately use explicit (strong)
  negation *and* negation-as-failure together; the facet records the *permitted operator set*,
  not one chosen operator.
- **Closure is a map, not a global flag.** Closed-world assumption is normally scoped — applied to
  some predicates or contexts and not others. The facet is a mapping from predicate (or context)
  to `open` or `closed`, with a default for unlisted keys; a single global value is merely the
  degenerate case where every key maps the same way.
- **Context is multi-dimensional.** A single result may be indexed simultaneously by world,
  standpoint, time, and path. These are independent index axes that coexist on one result rather
  than alternative values of one slot.
- **Uncertainty may carry several measures.** Probabilities and ranking/preference weights can
  coexist on the same result; the facet records the *set* of measures in force, not one.

### Truth values, admissible valuations, and the designated-value policy

Paraconsistency is **not** a single interchangeable setting. It is modelled in **three** parts that
vary independently:

1. a **truth algebra** — the **Belnap bilattice** with the four values *true*, *false*, *both*
   (over-determined / glut), and *neither* (under-determined / gap);
2. an **admissible-valuation set** (the **gap/glut policy**) — which of the four values a valuation is
   *allowed* to assign. Forbidding the gap, forbidding the glut, or forbidding both carves the classic
   sub-logics out of the *same* algebra; and
3. a **designated-value set** — which values count as *designated*, i.e. which count as "holding" for
   the purpose of consequence.

It is a **formal error** to say FDE and LP differ only in their designated values. In the standard
presentations **both designate *true* and *both***; what distinguishes them is the
**admissible-valuation set** — LP forbids the *neither* gap (every atom is at least true or false),
while FDE admits all four values. K3 is the dual restriction (it forbids the *both* glut), and classical
logic forbids both gap and glut. Only the three components together separate these related logics:

| Logic | Admissible values | Designated |
|---|---|---|
| FDE | true, false, both, neither | true, both |
| LP | true, false, both | true, both |
| K3 | true, false, neither | true |
| classical | true, false | true |

The contract therefore selects the algebra, the admissible-valuation (gap/glut) policy, and the
designated-value set **independently**. Collapsing any two of them — and in particular treating "FDE/LP"
as one value, or reducing the FDE↔LP difference to the designated set — erases exactly the choices that
distinguish these logics.

### Standing requests, not semantic selections

Two further facets are part of every contract but do not name a semantics. They are kept outside
the semantic-facet framing above so they are never mistaken for an entailment choice.

| Facet | Cardinality | What it is |
|---|---|---|
| Resource | **execution policy (several independent properties)** | the engine's execution discipline — e.g. *certified fragment* **and** a *budget/bound* — held together, not collapsed to one value |
| Projection | **output request naming one or more targets** | which surface(s) the answer is rendered to — canonical, OWL, Datalog, Common Logic, validation-only — possibly several at once |

**Resource is an execution policy.** It carries independent properties — whether a certified,
complete fragment is required, *and* what budget or bound applies — which hold simultaneously; it
is not a single "complete vs. bounded" toggle.

**Projection is an output request.** It is what the caller wants the answer rendered as, and it may
name multiple targets in one contract; it never alters which entailments hold.

### The DAG-workflow resource

A concrete Resource-facet value makes the doctrine above tangible for **process** reasoning:
`logic:DagWorkflowResource` is a `logic:ResourcePolicy` individual — a sibling of the certified-fragment
policy `logic:CertifiedFragmentResource` — that requires a plan's control/dataflow graph to be a
loop-free (acyclic) DAG: no `logic:Iteration`, and an acyclic `logic:dataflowConsumes` / flow-edge
closure. On an acyclic plan the schedule is a topological order with a hard termination guarantee, and
the result reports `logic:CompleteForFragment`.

Because Resource is an *execution policy* holding independent properties together (above), a contract
may carry both a DAG-workflow requirement and a budget at once; the DAG requirement constrains only
the execution discipline, never the model semantics. A named contract requests it on the Resource
facet: `logic:DagWorkflowContract` carries `logic:resourcePolicy logic:DagWorkflowResource`. It is a
**contract, not a profile** — it fixes only the execution-discipline facet, so it is deliberately
*not* a `logic:ReasoningPreset` (those bundle a model-semantics selection). The dogfooded build
pipeline runs `logic:executedUnderContract logic:DagWorkflowContract`.

The unsupported rule applies verbatim: a plan **outside** the acyclic fragment stays valid canonically
under a non-DAG contract but resolves to `unsupported` (`logic:EvaluationUnsupported`) under this
policy, the offending cycle disclosed by `logic:dagCycleWitness` — never quietly approximated to a
nearby acyclic plan. The preservation consequences of that verdict, and the drops external
workflow-engine lowerings impose, are specified in [`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md),
*The DAG-workflow profile and cyclic-plan preservation*.

The facets compose, but with the cardinalities above respected. **Evolution = transaction-path**
(the state-change semantics specified in [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md)) is
independent of whether the model semantics is least-model, well-founded, or stable-model — which
is precisely why state-change reasoning is a *facet value*, not a parallel profile. Likewise a
paraconsistent truth semantics composes with multi-dimensional context indexing and with
counterfactual revision without any one being a mode of the others. **Probabilistic reasoning is a
measure on the Uncertainty facet, never a model-semantics value** — graded belief is carried
alongside whatever model semantics is in force, not in place of one.

## Presets

A preset is a named `logic:ReasoningContract` that fixes a common, supported selection across the
facets. The historical profile names survive only as presets — facet bundles the compiler expands
before anything else runs:

- the positive-Horn preset = fragment:Horn · model-semantics:least-model · negation operators:∅ · closure:open everywhere · revision:monotonic · resource:certified fragment;
- the stratified-negation preset = fragment:Horn · model-semantics:least-model · negation operators:{default} (stratified);
- the well-founded preset = model-semantics:well-founded · negation operators:{default};
- the stable-model preset = model-semantics:stable-model · negation operators:{default};
- the procedural preset = fragment:Horn · native derivational builtins · resource:budget-bounded (cut is retired);
- the probabilistic preset = uncertainty:{probabilistic} over a declared dependency model — a measure carried alongside the chosen model semantics, not a model semantics of its own.

Presets are sugar. An author references a preset *or* assembles a contract from facets directly;
either way the canonical form the engine reasons over is the full facet selection, cardinalities
and all.

## Compatibility as a feature model, and the `unsupported` verdict

Not every selection across the facets is implementable, decidable, or coherent. But the set of
supported selections is **not** held as an enumeration over the facet product. With the facets now
split — and several of them set-valued, map-valued, or multi-dimensional — that product is far too
large to list, and listing it would grow combinatorially with every new facet value.

Compatibility is therefore expressed as a **feature model**: a **constraint graph** of *local*
rules — pairwise and small-clause — relating individual facet values to one another. Each rule is
data, not buried control flow, and states a single local fact: that two values require each other,
exclude each other, or together demand a particular `Resource` value or decidability class (see
[`LOGIC-SEMANTICS.md` § Decidability](LOGIC-SEMANTICS.md)). The set of supported contracts is then
**computed** from these constraints rather than written out. A contract is checked against the
constraint graph before any reasoning begins: it is supported exactly when it violates no rule.

This keeps the description linear in the number of facet values even though the space of
combinations it characterises is exponential, and it lets a new facet value be admitted by adding a
few local rules rather than re-tabulating a product.

The cardinal rule is unchanged: **an unsupported combination resolves to `unsupported` — it is
never silently approximated.** A request that names, for instance, probabilistic stable models, or
a paraconsistent truth semantics under counterfactual revision, or closed-world closure on
predicates inside generated counterfactual states, either resolves to a supported (possibly
bounded) evaluation, or it is reported as `unsupported` in the typed result (see
[`LOGIC-SEMANTICS.md` § The reasoning result](LOGIC-SEMANTICS.md)). Quiet substitution of a nearby
semantics is the one outcome the contract exists to forbid.

This is the mechanism that lets the reasoning layer grow new power without the independent
dimensions acquiring incompatible meanings when combined: the engine either has a defined semantics
for a contract, or it says so plainly.

### Feature-model completeness & coverage

The constraint graph is **authoritative in Rust**: the `RULES` table in `crates/logic-compile/src/compat.rs`
is the single source of truth, and the `logic:CompatibilityRule` individuals in `module.ttl` are a
**lossy documentation projection** of it (Principle 17), pinned together by a cross-check in
`crates/logic-compile/src/ir/tests.rs`. The full catalog (`ALL_RULE_IDS`) is:

| Rule id | Forbidden combination | Why |
|---|---|---|
| `RuleNoProbabilisticStableModel` | `logic:ProbabilisticMeasure` + `logic:StableModelSemantics` | no calibrated probability mass over the multiple incomparable answer sets stable-model semantics admits |
| `RuleNoParaconsistentCounterfactualRevision` | a gap/glut-admitting `logic:admissibleValuation` **or** the `logic:BelnapBilattice` truth algebra + `logic:EntrenchmentRevision` | closest-world selection that generates the counterfactual states is undefined over gappy/glutty valuations |
| `RuleNoClosedWorldInCounterfactual` | `logic:ClosedWorldClosure` (default closure **or** any per-key closure entry) + `logic:EntrenchmentRevision` | the generated counterfactual states are open-ended, so reading absence as falsehood inside them is unsound |
| `RuleProbabilisticRequiresModel` | `logic:ProbabilisticMeasure` without a declared `logic:ProbabilityModel` | graph-dependent; enforced in the front-end, where the source graph is available (no `RULES` table entry) |

Because the feature model is *computed* rather than enumerated, its correctness is something to be
**proved over the whole facet space, not spot-checked**. Three layers guard it:

1. **Exhaustive oracle sweep** (`compat.rs` tests) — enumerates the full cross-product of the
   rule-participating facet value domains and checks every contract's verdict against an
   *independent, hand-written* oracle of the forbidden combinations. This pins **both** directions
   at once: no forbidden contract is ever silently approximated to `Supported` (no false-supported),
   and no sound contract is ever rejected (no false-unsupported).
2. **Completeness guard** — the sweep also asserts that **every** table rule fires somewhere in the
   swept domain. A future rule keying on a facet/value the sweep does not vary fails this guard,
   forcing the swept domains to be extended in lockstep with the rule table — coverage can never
   silently fall behind the feature model.
3. **Property tests** (`crates/logic/tests/proptest_invariants.rs`) — `check` is total and
   deterministic over arbitrary (junk-included) facet strings, and a contract built from only
   non-trigger values is always `Supported`: unrecognised facet content never fabricates an
   `unsupported` verdict.

## Where a contract is attached

Every reasoning surface carries a contract: each conformance case, the reasoning command, the
memory tooling, deep validation, and every generated proof or coherence certificate. The contract
identity travels with the result (see [`LOGIC-SEMANTICS.md` § The reasoning result](LOGIC-SEMANTICS.md)),
so an answer is never interpretable apart from the contract it was produced under.

Provider-aware queries extend that identity with an immutable, query-scoped external-relation
manifest. The manifest pins each provider, artifact generation, model, relation/schema,
annotation-algebra identity and dimension, order/limit policy, preservation claim, and the shared
call/row budget. It is an explicit execution-DAG input, not an optional dependency or ambient
registry. The combined hash governs the same profile gate and native fixpoint as an ordinary query;
a provider cannot select another semantics, demote annotations to a post-hoc score, or bypass the
answer's preservation ledger. Changing any manifest field changes the query contract identity.

Completion remains a contract claim. A provider may return a complete empty ordered prefix, but a
failure, stale generation, cancellation, exhausted governor, or uncertified prefix is incomplete and
must cross the typed non-result boundary. Receipts retain that distinction and bind the result or
failed attempt to the RDF source generation and engine descriptor that observed it.

## Constitutional alignment

This is the canonical-source / generated-surface doctrine applied to reasoning configuration, and
the co-equal-open-vocabulary doctrine applied to the facets: no facet is privileged, none is
silently collapsed into another, and the simplified named surface is generated from the orthogonal
canonical form rather than substituted for it.

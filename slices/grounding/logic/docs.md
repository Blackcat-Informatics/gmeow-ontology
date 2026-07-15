<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Logic — the `logic:` reasoning layer

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/logic` · **tier: core**
> The maximally expressive, RDF 1.2-native logic in which GMEOW's model is authored, and of which
> OWL, RDFS, SHACL, Datalog, Prolog, N3, SPARQL, and gUFO are typed, lossy projections. BFO,
> DUL, OBO/RO, SUMO, YAMATO, and OpenCyc are explicit commitment-shifting bridge views —
> Constitution **Principle 17**.

This slice is the home of **GMEOW Logic (`logic:`)**, the canonical reasoning language for GMEOW.
The `logic:` namespace is the authoritative ground; the full UFO⁺ foundational vocabulary is
minted here. External formalisms are projections or typed bridge views of that ground, never a
second source of truth.

The vocabulary covers the UFO⁺ foundational sorts
(`Kind`/`SubKind`/`Phase`/`Role`/`Category`/`Mixin`/`RoleMixin`/`PhaseMixin`/`Relator`/`Event`/`Situation`
and the wider superset spine), the foundation relations
(`rigidlyAppliesTo`/`suppliesIdentity`/`mediates`), the world and modal terms
(`World`/`accessibleFrom`/`counterfactualOf` and the typed context algebra), the four normatively
distinct quantitative axes (`probability`/`confidence`/`weight`/`evidenceStrength`), the
probabilistic model vocabulary (`ProbabilityModel`/`FullIndependence`/`DependencyModel` and their
joint-outcome terms), the semantic profiles (as named presets of the reasoning contract), the
OntoUML discipline diagnostics (`violation`/`rigidityViolation`/`dischargeObligation`/`witnessRequiredViolation`),
the preservation-polarity vocabulary (`PreservationKind`/`preservationKind`/`complexityClass` and
their named individuals), and the rule-body structural properties (`negatedBody`/`distinctBody`).
All terms are declared as **standalone terms** — they carry no `gmeow:` parentage, because `gmeow:`
is a generated lossy projection of `logic:`, not its ground.

## The design set

The design is split by genre into documents under [`design/`](./design/), so it can be
implemented against rather than only read:

| Document | Genre | Realized state | Contents |
| --- | --- | --- | --- |
| [`design/LOGIC.md`](./design/LOGIC.md) | manifesto | realized | vision, doctrine, lineage |
| [`design/LOGIC-FOUNDATION.md`](./design/LOGIC-FOUNDATION.md) | charter | realized (a few named engine-selection strategies not yet engine-backed) | the `gmeow:logic` upper-ontology charter — the gUFO ⊇ baseline, the criticism ledger, the greenfield feature map, the Ithkuil precision ethos, the four-box organization |
| [`design/LOGIC-CONTRACT.md`](./design/LOGIC-CONTRACT.md) | configuration | realized | the reasoning contract — the orthogonal facets a reasoning request selects; named profiles as presets; the compatibility matrix |
| [`design/LOGIC-IR.md`](./design/LOGIC-IR.md) | intermediate representation | realized (`crates/logic-compile`) | the typed, full first-order IR every source compiles into and every projection out of; the per-lowering preservation judgment |
| [`design/LOGIC-SEMANTICS.md`](./design/LOGIC-SEMANTICS.md) | formal semantics | realized | the unified core, triple-term/assertion rules, the reasoning result, modality, the typed context algebra, decidability |
| [`design/LOGIC-TRANSACTION.md`](./design/LOGIC-TRANSACTION.md) | state change | realized (incl. `logic:SerializationAnomaly` non-serializable-schedule findings) | Transaction Logic — path semantics, serial conjunction, updates as supersession, the state-change facet |
| [`design/LOGIC-TELEOLOGY.md`](./design/LOGIC-TELEOLOGY.md) | goal/action layer | realized (`crates/logic/src/teleology`) | goals, intentional modes, structured goal expressions, reified goal evaluation, action schemas, goal decomposition and conflict; the intention → plan → action → transaction-path chain |
| [`design/LOGIC-COGNITION.md`](./design/LOGIC-COGNITION.md) | cognitive assessment | realized | the multidimensional cognitive-assessment construct — factored dimensions of reasoning quality, reliability, calibration, and metacognitive posture; reasoning quality over the inference modes; reliability over the typed reasoning result |
| [`design/LOGIC-RUNTIME.md`](./design/LOGIC-RUNTIME.md) | runtime | realized | solver architecture, the materialization–resolution seam, graph versioning, generated artifacts |
| [`design/LOGIC-CONFORMANCE.md`](./design/LOGIC-CONFORMANCE.md) | contract | realized (`conformance/logic/cases/*`, `generated/logic/projection-report.ttl`) | the conformance corpus and the loss-ledger preservation contract |
| [`design/LOGIC-SHACL-AF.md`](./design/LOGIC-SHACL-AF.md) | computation surface | realized (`crates/logic-compile/src/projections/shacl_af.rs`) | derivation/aggregation (map/reduce) authored as canonical `logic:` rules and projected to a SHACL-AF `sh:SPARQLRule` surface — computation added to the canon and emitted, never bolted onto SHACL; the projectable fragment, the declared exit loss, and the placement/purity rule |
| [`design/LOGIC-RDFQUERY.md`](./design/LOGIC-RDFQUERY.md) | query surface | **design-only** — self-declared "design, P15-gated, language not committed"; no parser or grammar is authored yet | RDFQuery framed as a front-end that parses into `logic:` (which already projects out to SPARQL/SHACL/N3/OWL), not a stack bolted onto SPARQL; P15-gated, language not committed, with the named first consumer |
| [`design/LOGIC-REFERENCES.md`](./design/LOGIC-REFERENCES.md) | appendix | realized (`metadata/references.ttl`) | external standards, theory, and engines cited — staged for the `metadata/references.ttl` ledger |

## What it commits to

- **The logic is canonical; OWL is a projection of it, not its ceiling.** `logic:` is RDF 1.2-native
  and Turing-complete by intent; decidability is a property of a *projection* or a declared
  contract facet, never a cap on what the canonical model may say.
- **A reasoning request is a typed contract over orthogonal facets.** Every reasoning surface
  carries an explicit `logic:ReasoningContract` — a selection of values for consequence relation,
  negation kind, closure assumption, context indexing, state-change mode, uncertainty handling, and
  so on. Named profiles are **presets**: named bundles of facet values that the compiler expands
  before evaluation. Presets are sugar over the canonical facet set; they are not indivisible
  alternatives, and combinations no preset anticipates are expressible directly. The full facet
  vocabulary, preset definitions, and compatibility matrix are in
  [`design/LOGIC-CONTRACT.md`](./design/LOGIC-CONTRACT.md).
- **One typed, full first-order IR.** Every `logic:` source compiles into a single typed
  intermediate representation covering the full first-order fragment and beyond. Every output —
  OWL, Datalog, N3, the Common Logic dialects, the canonical RDF 1.2 serialization — is a
  projection *of* the IR; every external dialect ingested is parsed *into* the same IR. There is
  no separate "Datalog-plus-extensions" form; that fragment is a subset reached by lowering.
  The IR, its node kinds, and the per-lowering preservation judgment are in
  [`design/LOGIC-IR.md`](./design/LOGIC-IR.md).
- **State change is an orthogonal facet, not a separate profile.** Transaction Logic (the Evolution
  = `transaction-path` facet) gives path semantics, serial conjunction, and updates as supersession
  rather than erasure. It composes with any choice of consequence relation — Horn, well-founded,
  stable-model, or probabilistic — because state change and entailment relation are independent
  concerns. The state-change semantics are in [`design/LOGIC-TRANSACTION.md`](./design/LOGIC-TRANSACTION.md).
- **The foundational ontology (UFO⁺) is authored in `logic:`.** gUFO is the primary generated
  down-projection; BFO, DOLCE, and SUMO are generated bridge views, not truth-preserving
  projections. The OntoUML disciplines are `logic:` rules the native solver evaluates; their
  equivalents survive as projection-conformance tests over the gUFO downcast.
- **A single canonical native solver is the normal development authority.** It runs forward
  materialization and backward goal resolution; the classical OWL tools operate as secondary
  validators for exported subsets, not as the authority.
- **Verified by construction.** One shared language-neutral conformance corpus, carried by the
  preservation judgments of every lowering and audited by the loss ledger (Principle 7).

## The reasoning contract

A reasoning request does not select a mode from a fixed list of profiles. It assembles a
`logic:ReasoningContract`: a selection of values across orthogonal, independently-varying facets.
The facets include the model semantics (least-model/Horn, well-founded, stable-model, …), the
negation operators (a *set* drawn from explicit/strong and default/negation-as-failure), the
truth/inconsistency semantics (classical, or a Belnap-family configuration — algebra plus
admissible-valuation policy plus designated set — yielding FDE, LP, K3, …), the closure assumption
(a *map*: open-world, or predicate-scoped closed-world), the context index (a *multi-dimensional*
index: unindexed, world-indexed, standpoint-indexed, time-indexed, path-indexed), the evolution
mode (static, state-transition, transaction-path), the uncertainty handling (a *set* of measures:
none, probabilistic, weighted, fuzzy), and others. There is no single "consequence" facet — what an
entailment means is settled jointly by several of these. Each facet is an open value vocabulary;
new values join without a schema change.

The historical profile names — positive Horn, stratified negation-as-failure, well-founded,
stable-model, procedural Prolog, probabilistic — survive as **presets**: named contracts with
a standard facet bundle that the compiler expands before evaluation. An author may reference a
preset or assemble a contract from facets directly; the canonical form the engine reasons over
is always the facet set.

Not every point in the facet product is implementable or coherent. The compiler holds an explicit
**compatibility matrix** and validates every contract against it before any reasoning begins. The
governing rule: an unsupported combination resolves to `unsupported` — it is never silently
approximated by the nearest supported semantics.

## The typed context algebra

`logic:` replaces a single generic `logic:World` with a typed algebra of distinct contexts, each
with its own typed accessibility relation:

| Context type | What it is | Typed accessibility relation |
| --- | --- | --- |
| `logic:PossibleWorld` | an alethic possibility | epistemically-possible / alethically-accessible |
| `logic:EpistemicContext` | what an agent knows or believes | doxastically-accessible |
| `logic:Standpoint` | a named perspective truth is relative to | sharpens (a refinement poset) |
| `logic:Scenario` | a hypothesized situation under consideration | scenario-entertains |
| `logic:State` | one state of affairs along a history | (successor within a path) |
| `logic:History` / `logic:Path` | an ordered run of states | temporally-succeeds |
| `logic:ReferenceFrame` | a frame of measurement or canon | frame-relative-to |
| `logic:NarrativeFrame` | an in-universe representational canon | depicts / in-frame |

Deontic and counterfactual reasoning are *uses* of these contexts rather than separate types. The
generic `logic:accessibleFrom` superproperty exists for uniform traversal and provenance, never for
inference: cross-type entailment requires an explicit bridge rule that names both types and states
the consequence relation it carries.

The most consequential rule of the algebra: **an unindexed statement holds in an
`gmeow:unspecifiedStandpoint` — it is unspecified, not universal.** Universality is something the
author asserts explicitly; absence of an index says nothing about what holds where. This is the
context-algebra counterpart of the open-world assumption: silence about context is not a claim of
universal context-independent truth.

## Transaction Logic and state change

State change is the `transaction-path` value of the Evolution facet. A query in this facet is
evaluated over a **path** — an ordered sequence of states — rather than over a single state.
Serial conjunction is a program combinator (do φ, then ψ) typed apart from ordinary conjunction,
which holds at a state. Updates are supersession, never erasure: a `del` retires one active support
in the successor state and records supersession provenance; displayability remains a separate
disclosure policy, and the substrate remains append-only and fully auditable.

The hypothetical operator — testing a transaction without committing its effects — is the value
`HypotheticalExecution` of the orthogonal `logic:ExecutionMode` facet (a sibling of the Evolution
facet, not a value within it), and is distinct from modal possibility (◇): it asks whether a program
*can execute*, not whether a proposition holds in some accessible world. Modal possibility (◇) stays
computed as the standard translation over the appropriate typed accessibility relation
(`logic:epistemicallyPossible` / `logic:doxasticallyAccessible`), never over the bare
`logic:accessibleFrom` superproperty and never reified as a class. Concurrent
Transaction Logic extends the path model to interleaved execution, surfacing non-serializable schedules
as findings.

## Constitutional role

This slice realizes Constitution **Principle 17**: `logic:` is the canonical reasoning language, and
OWL, RDFS, SHACL, Datalog, Prolog, N3, SPARQL, and the gUFO/BFO/DOLCE upper ontologies are generated
lossy projections of it. The `logic:` namespace and the UFO⁺ foundational surface are declared as
standalone terms that add no axioms to the reasoned core until they pass the formalization lifecycle
([`design/LOGIC-FOUNDATION.md`](./design/LOGIC-FOUNDATION.md)). The OntoUML disciplines are `logic:`
rules the native solver evaluates, and the matching enforcement gates live in
[`governance/constitution.ttl`](../../../governance/constitution.ttl).

## Terms

All terms are authored in the `logic:` namespace
(`https://blackcatinformatics.ca/logic/`), **standalone by design**: `logic:` is the canonical
ground and `gmeow:` (with gUFO, OWL, SHACL, …) is a generated lossy projection of it
(Principle 17), so no `logic:` term carries `gmeow:` parentage.

**UFO⁺ sorts and superset spine.** The foundational sorts
(`Kind`/`SubKind`/`Phase`/`Role`/`Category`/`Mixin`/`RoleMixin`/`PhaseMixin`/`Relator`/`Event`/`Situation`)
and the wider superset spine
(`Endurant`/`Perdurant`/`Object`/`Aspect`/`Substantial`/`AbstractIndividual`/`Quality`/`Mode`/
`QualityValue`/`QualitySpace`/`Process`/`Participation`/`Fluent`/`Collection`/`Quantity`/
`FunctionalComplex`/`FixedCollection`/`VariableCollection`/`Individual`/`ConcreteIndividual`).

**Foundation relations.** `rigidlyAppliesTo`/`suppliesIdentity`/`mediates`/`instanceOf`/`orderedType`
and the mereology properties (`partOf`/`properPartOf`/`memberOf`/`temporalPartOf`).

**Reasoning contract and presets.** A `ReasoningContract` is an independent selection across the
orthogonal reasoning facets (formula fragment, model semantics, negation, truth algebra, closure,
revision, resource policy, …). The six historical profile names — `PositiveHornProfile`,
`StratifiedNAFProfile`, `WellFoundedProfile`, `StableModelProfile`, `ProceduralPrologProfile`,
`ProbabilisticProfile` — survive as `ReasoningPreset` named individuals: sugar that `logic:expandsToFacet`
expands into a full contract facet bundle, documented term-by-term in [`module.ttl`](./module.ttl).

**Execution-commitment facet.** The `ExecutionMode` facet is orthogonal to the Evolution facet:
commitment (commit vs. discard) and state-change shape are independent dimensions. The sandbox
operator is realized as `logic:HypotheticalExecution` — a facet value, not modal possibility (◇),
which stays computed as the standard translation over the appropriate typed accessibility relation
(`logic:epistemicallyPossible` / `logic:doxasticallyAccessible`), never over the bare
`logic:accessibleFrom` superproperty, and is never reified as a class. The selector property is
`logic:executionMode`.

| Term | Kind | Description |
|---|---|---|
| `logic:ExecutionMode` | `owl:Class` | The facet value class of the execution-commitment facet — whether a Transaction-Logic program's effects are committed or discarded. Orthogonal to `logic:EvolutionMode`. |
| `logic:CommittedExecution` | `logic:ExecutionMode` | The execution-mode value under which a transaction program's elementary updates are committed — the effects advance the path and persist. The default execution mode. |
| `logic:HypotheticalExecution` | `logic:ExecutionMode` | The sandbox/hypothetical operator: a transaction program is executed to test whether it would succeed, with effects discarded. Distinct from modal possibility (◇), which is computed over the appropriate typed accessibility relation (`logic:epistemicallyPossible` / `logic:doxasticallyAccessible`), not the bare `logic:accessibleFrom` superproperty, and never reified. |
| `logic:executionMode` | `owl:ObjectProperty` | Relates a `logic:ReasoningContract` to the `logic:ExecutionMode` it selects. Single-valued on the contract; independent of `logic:evolution`. |

**World/modal terms.** `World`/`accessibleFrom`/`counterfactualOf` and the typed context
algebra described above. The two kernel typed-context classes are:

| Term | Kind | Description |
|---|---|---|
| `logic:PossibleWorld` | `owl:Class` | A typed alethic context — a world of alethic possibility; the contexts its typed accessibility relation `logic:epistemicallyPossible` ranges over. |
| `logic:Path` | `owl:Class` | An ordered run of states (a history) over which `logic:temporallySucceeds` is read; the Transaction-Logic state sequence whose positions are `gmeow:State` configurations. |

The five typed accessibility relations are:

| Term | Kind | Description |
|---|---|---|
| `logic:epistemicallyPossible` | `owl:ObjectProperty` | The typed alethic accessibility relation between `logic:PossibleWorld`s — a world is epistemically (alethically) accessible from another. |
| `logic:doxasticallyAccessible` | `owl:ObjectProperty` | The typed doxastic accessibility relation between `gmeow:EpistemicContext`s — what an agent's beliefs hold accessible. |
| `logic:deonticallyIdeal` | `owl:ObjectProperty` | The typed deontic accessibility relation: a context is deontically ideal — an ought-world — relative to an issuer. |
| `logic:temporallySucceeds` | `owl:ObjectProperty` | The typed temporal accessibility relation along a `logic:Path`: one `gmeow:State` succeeds another in the ordered run of states. |
| `logic:counterfactuallyCloser` | `owl:ObjectProperty` | The typed counterfactual accessibility relation under a declared closeness ordering (Lewis comparative similarity). |

None of these are asserted `rdfs:subPropertyOf logic:accessibleFrom`; the generic superproperty relationship is in prose only.

**Quantitative axes.** `probability`/`confidence`/`weight`/`evidenceStrength` are four
normatively distinct predicates. A confidence score, a calibrated probability, a solver ranking
weight, and an evidential warrant are not interchangeable; treating arbitrary confidence metadata
as a probability model is a named failure mode the vocabulary exists to prevent.

**Probabilistic model vocabulary.** `ProbabilityModel`/`probabilityModel`/`FullIndependence`/`DependencyModel`/
`correlates`/`JointOutcome`/`jointOutcome`/`outcomeAssignment`/`jointProbability` — the explicit
independence and dependency model machinery that probabilistic inference requires. Probabilistic
inference under `ProbabilisticProfile` is only available when a model is declared; the vocabulary
never silently assumes independence over bare `probability` annotations.

**OntoUML discipline diagnostics.** `Discipline`/`violation`/`rigidityViolation`/`dischargeObligation`/
`witnessRequiredViolation` and the discipline named individuals (`StereotypeCardinality`/`MixIden`/
`FreeRole`/`MixRig`/`RelComp`). These are derived diagnostics entailed by the lowering rules, never
asserted by hand; their presence is a machine-checkable proof that a class or individual breaks the
named discipline.

**Rule-body structural properties.** `negatedBody` (a negation-as-failure body atom, lifting a rule
out of positive Horn) and `distinctBody` (an inequality guard for detecting distinct values, leaving
stratification unchanged).

**Preservation-polarity vocabulary.** `PreservationKind`/`preservationKind`/`complexityClass` and
their named individuals
(`ExactPreservation`/`SoundUnderApproximation`/`CompleteOverApproximation`/`ValidationOnly`/`InconsistencyPreserving`/`InconsistencyReflecting`).
These record the answer-preservation guarantee each projection makes alongside its decidability
class; "lossy" is not enough — a consumer needs to know what a projection guarantees, not only what
it omits.

**Type and meta-type taxonomy.** `Type`/`EndurantType`/`RelationshipType`/`MaterialRelationshipType`/
`ComparativeRelationshipType`/`AbstractIndividualType`/`ConcreteIndividualType`/`Sortal`/`NonSortal`/
`RigidType`/`AntiRigidType`/`SemiRigidType`/`NonRigidType` — the higher-order meta-types that
classify the foundation sorts by identity-supply and modal rigidity axes.

**Scoped open/closed world vocabulary.** `WorldBoundary`/`closedUnder` — the construct that confines
the closed-world assumption to a declared scope while leaving the rest of the model open-world.

**Builtins.** `Builtin`/`invokesBuiltin` and the named primitive individuals
(`builtinStringConcat`/`builtinDateDifference`/`builtinArithmetic`). Builtins are the lightweight
derivational primitives a foundation genuinely needs; heavy domain computation stays external.
Builtin-invoking rules require `ProceduralPrologProfile` and record the loss when projected to a
declarative semantics.

**Property characteristic markers.** `transitiveProperty`/`asymmetricProperty`/`irreflexiveProperty`
— the `logic:` property-characteristic markers that allow `properPartOf` to carry the transitive
∧ asymmetric ∧ irreflexive strict-partial-order combination that OWL 2 DL globally forbids.

**Occurrent refinements (the YAMATO process/event axis).** `causalPartOf` — a transitive
specialization of the mereology relation `temporalPartOf` (causal ⊆ temporal), so a causal
dependency is always a temporal part but not conversely. `occurrentBoundary` over the closed value
vocabulary `OccurrentBoundary` { `Open`, `Closed` } types an occurrent as an on-going action
(*arrive*) or a completed unitary event (*arrival*) — a **value, not a subclass** (Principle 9).
`occurrentConstituent` (range `Process`) is the constitutive "events are made of processes"
relation, deliberately kept distinct from both temporal and causal parthood. The
`OccurrentConstraint` violation family records the entailed diagnostics: `OccurrentChangeAsymmetry`
(a `Closed` occurrent must bear no time-varying `Fluent` — the process≠event change-asymmetry) and
`OccurrentBoundaryMismatch` (an occurrent must not be both `Open` and `Closed`), both enforced by
the native foundation rules.

All terms are documented individually in [`module.ttl`](./module.ttl) and the design set above.

### gmeow:sharpens

The one `gmeow:`-side seam the foundation names: `logic:accessibleFrom` (the
Kripke accessibility relation between worlds) generalizes the standpoint
sharpening poset asserted on the `gmeow:` side via `gmeow:sharpens`. In the
typed context algebra, `gmeow:sharpens` is the typed accessibility relation for
`logic:Standpoint` contexts — a transitive, reflexive refinement poset distinct
from the doxastic, alethic, and temporal accessibility relations of the other
context types. The specialization is declared on the `gmeow:` side, never minted
as an axiom in this module, so the `logic:` foundation stays standalone
(Principle 17).

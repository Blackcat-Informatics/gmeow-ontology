<p align="center">
  <a href="https://github.com/Blackcat-Informatics/gmeow-ontology">
    <img src="https://raw.githubusercontent.com/Blackcat-Informatics/gmeow-ontology/main/docs/gmeow-logo.svg" alt="GMEOW logo" width="120" height="120">
  </a>
</p>

# `gmeow-logic` — Rust Reasoning Engine Core

[![crates.io](https://img.shields.io/crates/v/gmeow-logic.svg)](https://crates.io/crates/gmeow-logic)
[![docs.rs](https://docs.rs/gmeow-logic/badge.svg)](https://docs.rs/gmeow-logic)
[![License](https://img.shields.io/badge/license-AGPL--3.0--only-blue.svg)](./LICENSE)
[![Repository](https://img.shields.io/badge/repo-Blackcat--Informatics%2Fgmeow--ontology-181717.svg)](https://github.com/Blackcat-Informatics/gmeow-ontology)

> **An LLM output is a claim, not a truth.**

`gmeow-logic` is the Rust core of the **GMEOW reasoning engine**. It models
possible worlds as native RDF 1.2 statement partitions and provides world-indexed
entailment queries, governed materialization and proof provenance.

---

## What this crate is

`gmeow-logic` is the Rust `logic:` engine — the production engine
that backs world construction, entailment queries, and provenance capture.

The runtime includes a **world-indexed storage layer**, typed IR lowering,
semi-naive positive evaluation, stratified and non-monotone solvers, and a
restricted existential chase. Production rules remain typed from compiler IR
through evaluation; no intermediate executable rule-text projection is used.

---

## Static certification and deterministic budgets

`certify` checks the typed rule IR against the declared semantic profile. Every
check is a sufficient condition and necessarily incomplete: because termination
is undecidable, a clean verdict proves membership in the declared fragment while
a violation means the inexpensive structural proof did not succeed.

`materialize_program(program, input, limits, profile)` is the production entry
point. Its budget is a deterministic step limit enforced inside the native
evaluator. Asserted EDB facts are retained, derived rows are a sound prefix, and
an exhausted run is explicitly incomplete. Wall-clock limits are rejected because
they cannot preserve reproducible output. Typed existential callers use
`materialize_existential_rules`; only the repo-owned performance fixtures retain a
small benchmark-only textual TGD adapter.

Program materialization evaluates ordinary rules and conjunctive existential
heads in one indexed fixed point. Canonical positive `∀x. B(x) → ∃z. H(x,z)`
formulas lower directly into that execution, with shared and lexically scoped
witnesses across all head conjuncts. Single binary existential heads use the same
admission as reified n-ary tuples. Termination and stratification include both
producer families; mathematical builtins also declare the structural predicates
they read. One sorted commit charges the shared budget across all worlds.
A partial run exposes only completed predicate extensions in its frontier.

Source grammar eligibility is distinct from extension completion. Ordinarily a
structured interpretation waits for its grammar writers to finish. An input-bound
constructor certificate can instead prove that every possible writer preserves
closed `oneOf` lists: cells belong to fresh frontier-Skolem constructors, their
field values respect witness reuse, all rest paths terminate, and source terms
cannot alias those constructors. Ordinary, property and external grammar writers
participate in that check. Admission still validates every committed selector and
list premise; a partial firing cannot authorize an incomplete list. The recorded
world-bound basis does not declare a predicate or world complete and does not
certify chase termination. A generative feedback cycle can therefore be refused
without a budget while its explicitly bounded execution remains incomplete.

Prepared witness layouts and ordinary join plans are reused across selected
datasets. Existential triggers are revisited when a body predicate changes;
annotations fold the same selected physical lineage without replaying joins or
reasserting an intermediate closure. Materializations retain their chase
admission and decomposable witness recipes.

`reason_program` uses this joint executor for the selected ordinary and formula
rules together with the fixed native rules. It reads native statements, including
default-graph facts and literal-valued annotations, directly into world partitions.
`reason_program_certified` retains the chase admissions, witness recipes and
ordered premises from that execution. These are execution-fragment certificates;
they do not certify complete coverage of an authored source corpus or contextual
scope semantics. Distinct source graphs that collide on the current string world
key are rejected at admission.

`NativeSourceAdmission` borrows the compiler's immutable source inventory and
accounts for every ordinary statement, annotation carrier, quotation and lowering
route. `reason_admitted_theory_within` binds that account to the actual immutable
compiler source, claim ingress, prepared program, input contract and selected
domains before execution. Its private live completion is issued by that same
executor after validating the resulting native evidence. Serialized observations
cannot restore completion authority. The profile admits all supplied documents
and source roots; it refuses narrowed source projections. Explicit module roots
select one composed theory containing top-level obligations and the roots' actual
transitive imports. Compiler-issued resolution binds the exact native declarations,
roots, closure and import evidence to the immutable source. Portable requests and
reports do not restore that authority. Without module roots the operation keeps
the top-level selection and its module-scope boundaries. Input ingestion and its
identity retain only that selected default graph, while claim-envelope inspection
and source accounting retain every
supplied graph. Source edits and domain, modal or contextual requirements cannot
introduce an unselected world; such requirements refuse explicitly. Unsupported and
unread source remains named even when a faithful native fragment proves a clash.
Selected procedural negation-as-failure rules are named source boundaries and refuse this
proof operation before execution: absence cannot authorize a classical refutation.
The generic program and selected-theory execution APIs retain procedural NAF.
Membership in `logic:Thing` or `owl:Thing` remains accountable when asserted or
concluded. A universal-class membership premise prevents complete source accounting,
because native closure does not establish membership for every element. Formula
accounting tracks classical polarity through negation, implication and equivalence,
including premises exposed by negation normalization. Independent ordinary clashes
remain faithful refutations beside that incomplete accounting. Classical FOF export
has an independent admission and cannot borrow native completion.

Selected source and program views borrow the same compiler publication and retain
original axiom, rule and formula indices. Excluded modules contribute no premise.
Owned syntax is not an independent assertion, and unavailable required syntax
blocks its selected owner as a whole. Shared support survives through actual
admitted owners. Typed module claims use a distinct source-resolved assertion
route; module membership is not added to the evidence-only metadata allow-list.
Standpoint, time, modality and graph boundaries remain independent.

Composed FOF export and raw proof export/replay lack composition-bound authority.
The CLI resolves and reports the request before refusing those selected outputs;
required output failure removes stale bytes. Native attestations and the existing
top-level proof/replay contract remain distinct.
Output paths must be distinct from selected sources, proof inputs and one another;
aliases refuse before any output changes.

Native positive schema joins also participate in that fixed point: domain,
range, subproperty propagation, inverse/symmetric/transitive properties, and
value restrictions can consume schema facts derived by authored rules and wake
their consumers. Predicates selected from data are bound directly over the same
indexed store. Schema and data premises remain in the execution trace. A finite
vocabulary reachability analysis selects the potentially active laws; a bounded
plan cache keys the selected laws and complete predicate inventory, retaining no
world facts or results. Unknown predicate effects remain conservative dependency
edges. Combining those effects with invented terms requires a joint termination
certificate or an explicit execution budget. The shared chase certifier analyzes
all three statement positions, including values that become predicates. Its
conservative pair projections preserve every concrete firing and may introduce
additional abstract joins; a terminating abstract closure bounds the concrete
one. These are immutable rule analyses, never executable RDF projections or
rewrites of source data. An incomplete or refused analysis cannot authorize an
unbounded run.

Selected-program list laws use complete, witnessed lists from that same indexed
store. Union, intersection, enumeration and pairwise-disjointness consequences
can consume list cells derived in later rounds and wake their authored consumers.
Intersection membership streams candidates from one listed class's type index
and checks every member in the same world, without scanning unrelated types or
materializing a candidate table. Empty intersections use explicit resource-domain
laws whose effects also participate in joint termination admission. Nominal
closure requires explicit inequality against every member, retaining the actual
orientation of each premise and never assuming unique names. These finite guards
participate in the producer dependency graph; their conservative termination
analysis includes empty lists instead of treating missing members as absent
producers. Late type or inequality facts wake the same authored consumers.
Property-chain joins stream paths of arbitrary admitted length without collecting
intermediate join tables. Their evidence carries the declaration, every list edge
and every data edge. A shared cache retains at most 64 completed lists and 256 KiB
of native ID payload per world and round; a cache miss only repeats the traversal.
Incomplete lists wait for further producers and fail admission if the fixed point
cannot complete them. Cyclic or ambiguous structure cannot authorize a consequence.
The remaining DL reader uses the same complete-list admission and recognizes
referenced shared tails; malformed pairwise lists withhold its coverage claim.

Remaining DL augmentation and modal evaluation follow that joint rule closure.
Maximum-cardinality clashes select a sufficient pairwise distinct subset, so
additional fillers with unknown equality cannot hide a contradiction. The clash
retains the exact bound literal, qualifying types, selected fillers and actual
inequality premises; it makes no unique-name assumption.
Budgeted reasoning and incremental ground insertion charge their new DL facts and
modal verdicts against the remaining derivation allowance. Incomplete DL blocks
modal evaluation; any budget cut withholds the final consistency verdict.
The remaining sequential phases do not establish a general fixed point for all
DL and modal consequences that feed back into authored rules.

## Foundation lowering

Foundation lowering is the move of the five OntoUML structural disciplines from the native gUFO
checks (`crates/validate/src/gufo.rs`) into executable `logic:` IR rules that materialize
`logic:violation` and related diagnostic quads over `logic:` facts.

### What it does

Five disciplines are expressed as in-world `logic:StratifiedNAFProfile` Datalog rules. The rules
derive `?C logic:violation <label>` facts for:

- `logic:StereotypeCardinality` — a class with zero or more than one stereotype;
- `logic:MixIden` — identity-overlap (a `Kind` with a `Kind` proper-ancestor, or a non-`Kind`
  sortal not tracing to exactly one `Kind`);
- `logic:FreeRole` — an anti-rigid sortal with no rigid ancestor;
- `logic:MixRig` — a rigid sortal with an anti-rigid-type ancestor;
- `logic:RelComp` — a concrete `Relator` mediating fewer than two distinct relata.

Positive cross-world rigidity cannot be expressed as an in-world Datalog rule because the chase
is world-local. It is evaluated by a bounded closure pass over the finite materialized world set,
emitting `logic:rigidityViolation` quads in the world where rigidity persistence fails: a world
admitted from the typing world by typed accessibility records, each asserted in the graph of its
subject (`w logic:epistemicallyPossible w'` in `w`, or a chain of
`w1 logic:temporallySucceeds w`, `w2 logic:temporallySucceeds w1`, ...,
`w' logic:temporallySucceeds wn` records, each asserted in its own subject's graph), in which the
individual still exists and carries admissible evidence of the loss — an
explicit `logic:notInstanceOf` assertion or a closed-world closure of its `rdf:type` statements that
the one `logic:ReasoningContract` of that world declares (a world that asserts two or more
contracts is refused, naming them). An absence of a typing is evidence only where
no asserted type of the individual reaches the type through `logic:subClassOf` triples asserted in
that world. `Kind`, `SubKind` and `Category` are rigid by
stereotype; a `logic:rigidlyAppliesTo` record holds one instance of any other type, and is refused
on a type with an anti-rigid stereotype. The anti-rigidity witness policy (`witness-obligation` /
`witness-required` / `schema-only`) is a companion pass that emits `logic:dischargeObligation` /
`logic:witnessRequiredViolation` for every instance of a `Role`, `Phase`, `RoleMixin` or
`PhaseMixin` type per the declared policy without ever suppressing a violation; the witness is
discharged only by the same admissible evidence, and a semi-rigid `Mixin` owes none.

### Native Rust evaluator

The lowering is **evaluated natively in Rust** by `crates/logic/src/foundation.rs` (entry point:
`gmeow_logic::foundation::evaluate(&WorldStore, AntiRigidityPolicy)`). The in-world discipline
rules, the cross-world rigidity closure, and the anti-rigidity witness passes all run inside that
evaluator, which emits fully-provenanced quads (reifiers + derivation IDs via the shared
`provenance` recipe).

There is no Python emitter and no fallback (no-optionality doctrine: an invalid input or an
ambiguous closure entry makes `evaluate` return an error, not a degraded result). The
`conformance/logic/cases/foundation/` corpus goldens certify the Rust evaluator directly.

### Stratified NAF

The discipline rules need stratified NAF to express absence (a class with no stereotype, a sortal
with no rigid ancestor); the native evaluator partitions the rule set into strata and chases each
to fixpoint before the next. Foundation lowering is gated on `"foundation_lowering": true` in
`profile.json`; cases that do not opt in add zero quads.

---

## Stratum-C counterfactuals

`query(...)` resolves a `.logic` program that declares a counterfactual
(`:- counterfactual(W_cf, W_base).` + `:- assume(p(s, o)).`) by constructing a
transient, isolated world rather than reading the base world directly
(`src/counterfactual.rs`). The base world is minimally revised by a **functional
overwrite** that admits the antecedent; an over-determined antecedent is arbitrated
by the **declared entrenchment ordering** (`src/entrenchment.rs`, reusing
`gmeow:overrides` / `gmeow:strongerThan` / `gmeow:moreSevereThan` / `gmeow:sharpens`).

This is the **only generative, budgeted, possibly-incomplete** stratum — the only
place a chase is spawned on the fly. Its status field extends the budget vocabulary:

- `unknown` — a genuine (incomparable) entrenchment tie. The default deterministic
  profile **declines to branch**; exactly one world or `unknown`, never an arbitrary pick.
- `incomplete` — a hard budget tripped: the nested-counterfactual `depth_budget(N)`,
  or the opt-in `LewisCredulousProfile`/`LewisSkepticalProfile` branch budget over the
  closest worlds. Never unbounded.

Isolation preserves paraconsistency: `W_cf` is a fresh named graph, the base store is
never mutated, and nothing leaks back. Constructed worlds are content-keyed
(`counterfactual_world_key`, `src/versioning.rs`) for memoization. The corpus lives
under `conformance/logic/cases/worlds-C/`.

---

## Probabilistic / weighted layer

Under `logic:ProbabilisticProfile`, `query(...)` routes to the probabilistic evaluator
(`src/probabilistic.rs`) instead of the backward-goal dispatcher. It computes **exact
marginals by weighted model counting**: it enumerates every total choice θ over the
probabilistic facts, computes the least Herbrand model of `(Horn rules ∪ deterministic
facts ∪ θ's true facts)` per choice, and sums `P(θ)` over the choices whose model derives
each query binding. Each answer binding carries a `probability`; the computation is
`#P-hard` in general (the certifier records `probabilistic/#P-hard`), which suits the
tiny, fully-enumerable conformance corpora.

Query-layer directives carry the model and the probabilistic facts (the compiled surface
for the `logic:` terms, as `:- counterfactual(...)` is for `logic:counterfactualOf`):

- `:- probability_model(full_independence | dependency).` — the declared `logic:ProbabilityModel`.
- `:- probability(pred(S, O), p).` — an independent Bernoulli fact (`logic:probability`).
- `:- joint(p, atom1, atom2, …).` — one `logic:JointOutcome` of a dependency model (the
  listed atoms true, the rest false; outcomes must sum to one).
- `:- confidence(pred(S, O), c).` — `logic:confidence` metadata on an asserted fact.

Three structural guards enforce the epistemic-hygiene contract:

- **confidence ≠ probability** — only `:- probability` / `:- joint` enter the marginal; a
  confidence-annotated fact is deterministic (marginal `1.0`), the value never promoted.
- **model required** — probabilistic facts with no declared model return status `unknown`;
  independence is never silently assumed.
- **cut is rejected** — `!` belongs only to `ProceduralPrologProfile`.

The corpus lives under `conformance/logic/cases/profiles/probabilistic-*`.

---

## World-isolation discipline

- **A world is a named graph.** Every insert targets a specific named graph IRI;
  every query is scoped to a single named graph. No cross-world union queries
  are provided: a triple inserted into world `A` is never visible through a
  query on world `B`.
- **World-indexed only.** The public API exposes only `insert_quad(world, s, p, o)`
  and `quads_in_world(world)`. There is deliberately no dataset-union method.
- **Same conformance corpus.** The unit tests validate isolation semantics
  that this crate satisfies.

---

## Build

> **Toolchain requirement:** nightly Rust is required for the native engine's
> `portable_simd` kernels. The repo ships a `rust-toolchain.toml` at the root;
> `cargo` and `rustup` pick it up automatically.

```bash
cargo build -p gmeow-logic
```

## Test

```bash
cargo test -p gmeow-logic
```

---

## Library API

The supported entry point for an external **runtime** consumer is the
[`crate::runtime`] module: one import path (`use gmeow_logic::runtime::*`)
over the whole store → snapshot → dispatch → result chain, plus the self-describing
`EngineContract` runtime pin. Its module docs carry the stability, thread-safety,
refusal, and forward-compatibility contract, and a worked end-to-end example.

```rust
use gmeow_logic::runtime::WorldStore;

let store = WorldStore::new()?;

// Insert triples into two isolated worlds
store.insert_quad("http://world/A", "http://ex.org/s", "http://ex.org/p", "http://ex.org/o1");
store.insert_quad("http://world/B", "http://ex.org/s", "http://ex.org/p", "http://ex.org/o2");

// World-indexed query: only world A's quads are returned
let a_quads = store.quads_in_world("http://world/A");
assert_eq!(a_quads.len(), 1);

// List worlds
let mut worlds = store.worlds();
worlds.sort();
assert_eq!(worlds, vec!["http://world/A", "http://world/B"]);
# Ok::<(), gmeow_errors::Diag>(())
```

---

## Developer documentation

- [Logic Runtime Architecture](../../slices/grounding/logic/design/LOGIC-RUNTIME.md)
- [Logic Semantics](../../slices/grounding/logic/design/LOGIC-SEMANTICS.md)
- [Project Rationale](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/RATIONALE.md)
- [GMEOW Constitution](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/CONSTITUTION.md)
- [Repository AGENTS.md](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/AGENTS.md)

### Building and testing locally

```bash
cd crates/logic
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

---

## Project and community

`gmeow-logic` is developed by [Blackcat Informatics® Inc.](https://blackcatinformatics.ca)
as part of the [GMEOW ontology and tooling](https://github.com/Blackcat-Informatics/gmeow-ontology)
suite.

Related packages:

- `gmeow-gts` — Graph Transport Substrate format engine (Rust)

---

## License and copyright

Copyright © 2026 Blackcat Informatics® Inc.

This crate is licensed under the **GNU Affero General Public License v3.0 only**
(AGPL-3.0-only) — see the
[`LICENSE`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE)
file in the repository root. Separate proprietary/commercial terms are available;
contact `licensing@blackcatinformatics.ca`.

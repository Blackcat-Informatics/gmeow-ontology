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
possible worlds as [oxigraph](https://oxigraph.org/) named graphs and provides
world-indexed entailment queries, gated against the same language-neutral
conformance corpus as `gmeow-gts`.

---

## What this crate is

`gmeow-logic` is the Rust `logic:` engine — the production engine
that backs world construction, entailment queries, and provenance capture. It
supersedes the earlier Python reference oracle, which has been retired.

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
emitting `logic:rigidityViolation` quads in the world where rigidity persistence fails. The
anti-rigidity witness policy (`witness-obligation` / `witness-required` / `schema-only`) is a
companion pass that emits `logic:dischargeObligation` / `logic:witnessRequiredViolation` per the
declared policy without ever suppressing a violation.

### Native Rust evaluator

The lowering is **evaluated natively in Rust** by `crates/logic/src/foundation.rs` (entry point:
the `gmeow_logic.foundation(nquads, policy)` PyO3 binding). The in-world discipline rules, the
cross-world rigidity closure, and the anti-rigidity witness passes all run inside that evaluator,
which emits fully-provenanced quads (reifiers + derivation IDs via the shared `provenance` recipe).

The earlier Python oracle (`src/gmeow_tools/logic_foundation.py`) has been **retired** — there is
no Python emitter and no fallback (no-optionality doctrine: a missing `gmeow_logic` extension is a
hard failure, not a degraded path). Golden-fixture parity is preserved by construction: the
`conformance/logic/cases/foundation/` corpus goldens are unchanged and now certify the Rust
evaluator directly, with no oracle in the path.

### Stratified NAF

The discipline rules need stratified NAF to express absence (a class with no stereotype, a sortal
with no rigid ancestor); the native evaluator partitions the rule set into strata and chases each
to fixpoint before the next. Foundation lowering is gated on `"foundation_lowering": true` in
`profile.json`; cases that do not opt in add zero quads. (For non-foundation programs that declare
stratifiable negation, the Python `materialize_program()` path retains its own `enable_naf`
stratified chase; programs the stratified oracle cannot compute fall through to a lossy positive
materialization with the loss recorded.)

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

## Oracle-parity discipline

- **A world is a named graph.** Every insert targets a specific named graph IRI;
  every query is scoped to a single named graph. No cross-world union queries
  are provided: a triple inserted into world `A` is never visible through a
  query on world `B`.
- **World-indexed only.** The public API exposes only `insert_quad(world, s, p, o)`
  and `quads_in_world(world)`. There is deliberately no dataset-union method.
- **Same conformance corpus.** The unit tests validate isolation semantics
  that both the retired Python oracle and this crate satisfy identically.

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

let store = WorldStore::new();

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

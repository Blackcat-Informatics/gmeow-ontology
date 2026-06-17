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

`gmeow-logic` is the Rust counterpart of the Python reference oracle
(`src/gmeow_tools/`) for the `logic:` vocabulary. It is the production engine
that backs world construction, entailment queries, and provenance capture.
Python remains the conformance oracle (slow, simple, correct); this crate is
the fast path.

The current scope is the **world-indexed storage layer**: an in-memory
`WorldStore` wrapping oxigraph that enforces isolated named graphs as worlds.
Nemo-based rule materialization and PyO3 bindings are included.

---

## Static certifier and the budget governor (issue #502)

`certify(rules, profile)` is the Rust mirror of the Python oracle
(`gmeow_tools.logic_certify`). It parses Nemo `.rls` text with Nemo's own parser
(reusing the engine's surface, never a second IR) and produces a
`CertificationVerdict` whose JSON shape, violation strings, and SCC-cycle
rendering are **byte-identical** to the oracle, so the `oracle ≡ engine` gate
diffs them directly. Every check is a *sufficient* condition and is *necessarily
incomplete*: because termination is undecidable, a clean verdict proves
membership in the declared decidable/terminating fragment, while a violation only
proves that the cheap structural condition does not hold — the program may still
terminate.

### Budget enforcement is post-hoc — and honest about it

`materialize(rules, input, max_rule_firings=None, max_answers=None, time_ms=None)`
adds an optional resource budget with this contract:

- **Asserted EDB input facts are always kept in full.** A budget never drops a
  given input quad; only **derived (IDB)** quads are bounded.
- **The count ceilings (`max_rule_firings`, `max_answers`) are engine-independent
  and deterministic.** Both engines run the chase to full fixpoint and then
  truncate the derived set *post-hoc* to the canonical-sort prefix of the
  **complete** derivation — a prefix of the `(graph, S, P, O)` sort, never a
  fabricated row. The kept quads are stamped `budget_status = "exhausted"` and the
  run is marked incomplete. Because both the Python oracle and Nemo compute the
  same complete fixpoint and truncate the same canonical prefix, the count
  ceilings give **identical** verdicts on both engines.
- **Only `time_ms` is a mid-chase cut, and only in Python.** The Python oracle can
  stop the chase early on the wall clock; Nemo's `reason()` runs to fixpoint with
  no native budget hook, so on the Rust side `time_ms` bounds only *post-fixpoint*
  work (decode + bookkeeping), not the chase itself. This is the one budget
  parameter whose behaviour is engine-dependent.
- **Rejecting genuinely non-terminating rule sets is the static certifier's job,
  up front — not the governor's to interrupt.** The governor never sees a
  non-terminating program it is expected to halt.
- **Kept results are always a sound subset of the full fixpoint** — never a false
  answer.

This keeps `oracle ≡ engine` truthful rather than convenient: under the count
ceilings the verdict, kept set, and budget strings match the oracle exactly; the
only engine-dependent budget is `time_ms`, and that divergence is **named here,
not glossed** (the same contract appears in the `certify.rs` and `py.rs` doc
comments). With all three budget parameters `None` (the default), `materialize`
output is byte-identical to the pre-#502 behaviour: chase order preserved, every
quad `"ok"`.

## Foundation lowering (issue #503)

Foundation lowering is the move of the four OntoUML structural disciplines from external Python
checks (`src/gmeow_tools/reasoning_lint.py`) into executable `logic:` IR rules that materialize
`logic:violation` and related diagnostic quads over `logic:` facts.

### What it does

Three of the four disciplines are expressed as in-world `logic:StratifiedNAFProfile` Datalog rules
emitted by `foundation_rules()` in `src/gmeow_tools/logic_foundation.py`. The rules derive
`?C logic:violation <label>` facts for:

- `logic:StereotypeCardinality` — a class with zero or more than one stereotype;
- `logic:MixIden` — identity-overlap (a `Kind` with a `Kind` proper-ancestor, or a non-`Kind`
  sortal not tracing to exactly one `Kind`);
- `logic:FreeRole` — an anti-rigid sortal with no rigid ancestor;
- `logic:MixRig` — a rigid sortal with an anti-rigid-type ancestor;
- `logic:RelComp` — a concrete `Relator` mediating fewer than two distinct relata.

The fourth discipline — positive cross-world rigidity — cannot be expressed as an in-world Datalog
rule because the chase is world-local. It is evaluated by a bounded closure pass
(`cross_world_rigidity_violations()`) over the finite materialized world set, emitting
`logic:rigidityViolation` quads in the world where rigidity persistence fails.

### Python-authoritative; Rust mirror deferred

The lowering is **Python-authoritative**: `logic_foundation.py` generates the Nemo `.rls` rule
text that the Rust engine runs. Because the Nemo rule text is opaque to the Rust layer, the
in-world foundation rules run identically on both engines and are covered by the existing
`oracle ≡ engine` parity gate. The cross-world rigidity closure and anti-rigidity obligation
passes are Python oracle-level computations over the materialized world set; the Rust engine
receives their output folded into the materialized quad set.

A Rust-native emitter (`crates/logic/src/foundation.rs`) is deferred; it is not required by any
issue-503 acceptance criterion.

### `enable_naf` addition to the oracle

Foundation lowering requires stratified NAF to express absence (a class with no stereotype, a
sortal with no rigid ancestor). The `materialize_program()` function in `logic_materialize.py`
accepts an `enable_naf` parameter (added in #503): when `True`, the rule set is partitioned into
strata using the same dependency-graph stratification as the certifier, and each stratum is chased
to fixpoint before the next. With `enable_naf=False` (the default) the behaviour is byte-identical
to pre-#503: a single-stratum fixpoint with no NAF evaluation. Foundation lowering is gated on
`"foundation_lowering": true` in `profile.json`; cases that do not opt in add zero quads.

---

## Stratum-C counterfactuals (issue #505)

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

## Probabilistic / weighted layer (issue #506)

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
  that both the Python oracle and this crate must satisfy identically.

---

## Build

> **Toolchain requirement:** nightly Rust is required. The `nemo` engine (a
> hard dependency) uses unstable features (`macro_metavar_expr`,
> `iter_intersperse`, `slice_swap_unchecked`) that are not available on stable.
> The repo ships a `rust-toolchain.toml` at the root that pins the channel to
> `nightly`; `cargo` and `rustup` pick this up automatically.

```bash
cargo build -p gmeow-logic
```

Via the Makefile:

```bash
make logic-build
```

---

## Test

```bash
cargo test -p gmeow-logic
```

Via the Makefile:

```bash
make logic-test
```

---

## Library API

```rust
use gmeow_logic::store::WorldStore;

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

- [Logic Runtime Architecture](../../slices/core/logic/design/LOGIC-RUNTIME.md)
- [Logic Semantics](../../slices/core/logic/design/LOGIC-SEMANTICS.md)
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
- Python oracle: `src/gmeow_tools/` (PyPI: `gmeow`)

---

## License and copyright

Copyright © 2026 Blackcat Informatics® Inc.

This crate is licensed under the **GNU Affero General Public License v3.0 only**
(AGPL-3.0-only) — see the
[`LICENSE`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE)
file in the repository root. Separate proprietary/commercial terms are available;
contact `licensing@blackcatinformatics.ca`.

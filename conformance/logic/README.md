<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Conformance Corpus

The Python oracle and the Rust core — and any future port — certify against **one shared,
language-neutral corpus of cases**: static files, not re-derived assertions. This is the contract
that lets the slow, correct reference and the fast Rust engine coexist and *provably agree*
(Constitution Principle 7), exactly as issue #277 promotes the GTS §18 vectors into a `conformance/`
directory so any implementation certifies against the same files.

This corpus is the **executable specification of GMEOW Logic's hardest invariants** — no engine can
drift from them without a red build. It is the implementation contract for the EPIC #497 rungs.

> **Status: scaffold.** The category directories are empty (`.gitkeep`) because cases cannot be
> authored before the `logic:` surface syntax and the engine exist (the later EPIC rungs). This
> directory and the [runner contract](runner/README.md) are created now, by issue #498, so every
> subsequent rung populates a category that already has a home and a verification gate.

Normative source: [`../../slices/core/logic/design/LOGIC-CONFORMANCE.md`](../../slices/core/logic/design/LOGIC-CONFORMANCE.md).

## Layout

```text
conformance/logic/
  cases/<category>/<case>/
    input.logic.ttl          # logic: source (+ optional adapter owl:* / gufo:)
    profile.json             # declared semantic + world-types + requested mode + expected decidability class
    queries/<q>.logic        # goal-resolution / counterfactual queries (optional)
    expected/
      materialized.nq        # derived quads; worlds are named graphs
      verdicts.json          # world-indexed truth/modality + reasoning_lint-equivalent verdicts
      witnesses.nq           # contradiction witnesses as GMEOW statement graphs
      projections/           # OWL-DL / OWL-EL / Datalog / N3 / gUFO downcast + the preservation ledger
      explanation/<q>.md     # prose-explanation skeletons (faithful-by-construction)
      answers/<q>.json       # expected goal/counterfactual answer sets
  runner/                    # the language-neutral runner contract (see runner/README.md)
  README.md
```

## Categories and their verification contract

Each row is *input artifact → the expected-output check that gates engine parity*. The four
highlighted properties (lint-equivalence, the no-occurrence gate, no base-world leakage, explanation
faithfulness) are the load-bearing invariants.

| Category | Input artifact | Verification (expected output) | Rung |
|---|---|---|---|
| `foundation/` | `input.logic.ttl` with UFO⁺ stereotype, subsumption, and mediation facts | `materialized.nq` contains the expected `logic:violation` / `logic:rigidityViolation` / `logic:dischargeObligation` quads; the lint-equivalence gate proves the derived offending-class sets match `reasoning_lint.py` exactly | #503 |
| `worlds-A/` | standpoint / deception / narrative source | `materialized.nq` carries each world as its own named graph; the contested claim coexists (Crimea `conceivable` vs `refuted`), neither privileged | #501 |
| `worlds-B/` | risk cascade, teleology, or norms source | `materialized.nq` proves **exactly zero `Event` instances** are generated (the no-occurrence gate), with type-level force present | #501 |
| `worlds-C/` | counterfactual antecedent query | `answers/<q>.json` confirms the consequent and `materialized.nq` shows **no leakage** of the constructed world into the base graph; a deterministic revision yields one world, a genuine tie returns `unknown` | #505 |
| `projections/` | any `input.logic.ttl` | `projections/` (OWL DL/EL, Datalog, N3, gUFO) compare by isomorphism; the preservation ledger matches | #500 |
| `decidability/` | a profile-tagged source | a decidable profile is certified, a violating one is flagged, and budget exhaustion returns `unknown` / `incomplete` | #502 |
| `profiles/` | rule sets under each declared semantics | answers match the declared semantic profile (PositiveHorn, StratifiedNAF, WellFounded, StableModel); cut appears only under `ProceduralPrologProfile`; under `ProbabilisticProfile` (`probabilistic-*` cases, #506) each binding carries a `probability` from weighted model counting under a declared model, `logic:confidence` is never read as a probability, and probabilistic facts with no declared model return `unknown` | #502/#504/#506 |
| `explanation/` | a failed-constraint or derivation query | the `explanation/<q>.md` skeleton validates that **every cited IRI appears in the trace** — no justification outside the proof | #501 |
| `paraconsistency/` | a cross-world contradiction | `materialized.nq` confines the contradiction to separate graphs (no explosion); a within-world contradiction emits `witnesses.nq` | #501 |

## The `foundation/` category

Implemented in v3 (#503). Cases live under `cases/foundation/<case>/`. Each case's `profile.json`
must carry `"foundation_lowering": true` to activate the discipline rules and the post-materialization
passes; cases that omit it are not foundation cases.

### `profile.json` fields

| Field | Type | Meaning |
|---|---|---|
| `"foundation_lowering"` | boolean | Must be `true` to activate foundation lowering. When absent or `false` the case is processed without discipline rules and the expected output must be byte-identical to what the non-lowering engine would produce. |
| `"anti_rigidity_policy"` | string | Governs the instance-level anti-rigidity obligation/witness facet. Closed enum: `"witness-obligation"` (default, emits `logic:dischargeObligation`), `"schema-only"` (emits nothing at the instance level), `"witness-required"` (emits `logic:witnessRequiredViolation` absent a materialized counter-world). Absent means `"witness-obligation"`. An unknown value is a hard failure. |

### Vocabulary in `materialized.nq`

The diagnostic quads a foundation case's `materialized.nq` may contain, all in the
`https://blackcatinformatics.ca/logic/` namespace:

| Predicate | Subject | Object | Produced by |
|---|---|---|---|
| `logic:violation` | offending class IRI | one of the five discipline labels below | in-world Datalog rules (all three lowered disciplines) |
| `logic:rigidityViolation` | instance IRI | rigid type IRI | cross-world closure pass (fired only with ≥2 materialized worlds) |
| `logic:dischargeObligation` | instance IRI | anti-rigid type IRI | anti-rigidity pass under `"witness-obligation"` policy |
| `logic:witnessRequiredViolation` | instance IRI | anti-rigid type IRI | anti-rigidity pass under `"witness-required"` policy, when no counter-world is materialized |

Discipline label individuals (objects of `logic:violation` quads):

| Label | Anti-pattern |
|---|---|
| `logic:StereotypeCardinality` | a class with zero or more than one stereotype |
| `logic:MixIden` | identity-overlap (`reasoning_lint.identity_overlap`) |
| `logic:FreeRole` | anti-rigid sortal with no rigid ancestor (`reasoning_lint.anti_rigidity_discipline`, FreeRole half) |
| `logic:MixRig` | rigid sortal with an anti-rigid-type ancestor (`reasoning_lint.anti_rigidity_discipline`, MixRig half) |
| `logic:RelComp` | concrete Relator mediating fewer than two distinct relata (`reasoning_lint.relator_mediation`) |

### Cases shipped in v3

Six cases under `cases/foundation/` (one stub `.gitkeep` + five populated):

| Case | Discipline exercised |
|---|---|
| `exactly-one-stereotype/` | `logic:StereotypeCardinality` (zero-stereotype and conflicting-stereotype branches) |
| `identity-overlap-mixiden/` | `logic:MixIden` |
| `free-role/` | `logic:FreeRole` |
| `mixrig-kind-under-role/` | `logic:MixRig` |
| `relcomp-under-mediated/` | `logic:RelComp` |
| `cross-world-rigidity/` | `logic:rigidityViolation` + `logic:dischargeObligation` (multi-world) |

The lint-equivalence gate (`tests/test_logic_foundation_lint_equivalence.py`) proves that for the
three type-level disciplines the derived offending-class sets match `reasoning_lint.py` by full-map
equality. The cross-world rigidity soundness suite is in `tests/test_logic_rigidity.py`.

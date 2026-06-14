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
| `foundation/` | `input.logic.ttl` with UFO⁺ types (rigidity, identity-supply, mediation) | `verdicts.json` reproduces the `reasoning_lint.py` verdicts exactly (isomorphic); the gUFO downcast passes every OntoUML anti-pattern check | #503 |
| `worlds-A/` | standpoint / deception / narrative source | `materialized.nq` carries each world as its own named graph; the contested claim coexists (Crimea `conceivable` vs `refuted`), neither privileged | #501 |
| `worlds-B/` | risk cascade, teleology, or norms source | `materialized.nq` proves **exactly zero `Event` instances** are generated (the no-occurrence gate), with type-level force present | #501 |
| `worlds-C/` | counterfactual antecedent query | `answers/<q>.json` confirms the consequent and `materialized.nq` shows **no leakage** of the constructed world into the base graph; a deterministic revision yields one world, a genuine tie returns `unknown` | #505 |
| `projections/` | any `input.logic.ttl` | `projections/` (OWL DL/EL, Datalog, N3, gUFO) compare by isomorphism; the preservation ledger matches | #500 |
| `decidability/` | a profile-tagged source | a decidable profile is certified, a violating one is flagged, and budget exhaustion returns `unknown` / `incomplete` | #502 |
| `profiles/` | rule sets under each declared semantics | answers match the declared semantic profile (PositiveHorn, StratifiedNAF, WellFounded, StableModel); cut appears only under `ProceduralPrologProfile` | #502/#504/#506 |
| `explanation/` | a failed-constraint or derivation query | the `explanation/<q>.md` skeleton validates that **every cited IRI appears in the trace** — no justification outside the proof | #501 |
| `paraconsistency/` | a cross-world contradiction | `materialized.nq` confines the contradiction to separate graphs (no explosion); a within-world contradiction emits `witnesses.nq` | #501 |

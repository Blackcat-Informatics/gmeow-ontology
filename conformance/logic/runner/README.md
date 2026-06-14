<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Conformance Runner Contract

The runner is **language-neutral**: every engine — the Python oracle, the Rust core, and any future
JS/Go port — implements one adapter, and the runner diff-compares each output against the case's
`expected/` files. Identical-files-or-red-build is what makes "oracle ≡ engine" (Principle 7) a
machine-checked guarantee rather than a hope.

> **Status: contract only.** No runner is implemented yet, and no `make conformance` target is added
> by this scaffold — both land with the engine (the EPIC #497 rungs), per
> [`../../../slices/core/logic/design/LOGIC-CONFORMANCE.md`](../../../slices/core/logic/design/LOGIC-CONFORMANCE.md)
> §Runner contract. This file fixes the interface the rungs must obey.

## The adapter

An engine implements one function:

```text
run(input, mode) -> {
  materialized,    # an RDF dataset (named graphs = worlds)
  verdicts,        # world-indexed truth/modality + reasoning_lint-equivalent verdicts (JSON)
  witnesses,       # contradiction witnesses as a GMEOW statement graph (RDF)
  projections,     # the generated OWL-DL/EL, Datalog, N3, gUFO downcasts + preservation ledger
  explanations,    # per-query prose-explanation skeletons
  answers,         # per-query goal/counterfactual answer sets (JSON)
}
```

`input` is a case directory (`input.logic.ttl` + `profile.json` + optional `queries/`); `mode`
selects the engine/fragment (e.g. `native`, `owl-dl`, `owl-el`, `datalog`). The Python oracle
implements it first (the executable spec); the Rust core must pass the identical corpus; a port
self-certifies the same way.

## Comparison semantics

No case may depend on iteration order. Each expected artifact is compared by a fixed rule:

| Output | Comparison |
|---|---|
| `materialized.nq`, `witnesses.nq`, `projections/*` (RDF) | **graph isomorphism** (blank-node-aware) |
| `verdicts.json`, `answers/*.json` | **canonical JSON** — sorted keys, normalized literals |
| `explanation/*.md` | the **cited-IRI and rule-IRI skeleton**, *not* the surface prose |

The explanation rule is load-bearing: a language model may vary the wording, but never the set of
axioms, rules, and sources it cites (the faithful-by-construction property — a generated explanation
may cite only IRIs that appear in the proof trace or witness graph it explains).

## Wiring (deferred)

When the first engine lands, the runner wires into a `make conformance` target and — once the native
solver is the `make check` reasoning authority — into `make check`. Until then this corpus is a
specification, not a gate.

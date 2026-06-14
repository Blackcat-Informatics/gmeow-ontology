<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# GMEOW Logic — Conformance Corpus and Preservation Contract

> Status: implementation contract for `logic:`. This is the **conformance** member of the
> [GMEOW Logic document set](LOGIC.md#the-document-set), and the piece future implementations will
> actually obey. It defines the language-neutral conformance corpus (the artifact that enforces
> engine parity) and the loss-ledger preservation contract (what a projection *guarantees*, not just
> what it drops). The engine under test is in [LOGIC-RUNTIME.md](LOGIC-RUNTIME.md); the semantics it
> must satisfy are in [LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md).

## Why a corpus

The Python oracle and the Rust core — and any future port — certify against **one shared,
language-neutral corpus of cases**: static files, not re-derived assertions. This is the contract that
lets the slow, correct reference and the fast Rust engine coexist and *provably agree* (Principle 7),
exactly as issue #277 promotes the GTS §18 vectors into a `conformance/` directory so any
implementation certifies against the same files. The reasoning corpus is that contract for `logic:`,
and it is the **executable specification of the design's hardest invariants** — no engine can drift
from them without a red build.

## Corpus layout

The corpus lands under `conformance/logic/` once the `logic:` vocabulary exists (cases cannot be
authored before the surface syntax does). Its shape:

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
  runner/                    # the language-neutral runner contract
  README.md
```

## Categories and their verification contract

Each row is the material the design grounds, as *input artifact → the expected-output check that gates
engine parity*. The four highlighted properties (lint-equivalence, the no-occurrence gate, no
base-world leakage, explanation faithfulness) are the design's load-bearing invariants.

| Category | Input artifact | Verification (expected output) |
|---|---|---|
| `foundation/` | `input.logic.ttl` with UFO⁺ types (rigidity, identity-supply, mediation) | `verdicts.json` reproduces the `reasoning_lint.py` verdicts exactly (isomorphic); the gUFO downcast passes every OntoUML anti-pattern check |
| `worlds-A/` | standpoint / deception / narrative source | `materialized.nq` carries each world as its own named graph; the contested claim coexists (Crimea `conceivable` vs `refuted`), neither privileged |
| `worlds-B/` | risk cascade, teleology, or norms source | `materialized.nq` proves **exactly zero `Event` instances** are generated (the no-occurrence gate), with type-level force present |
| `worlds-C/` | counterfactual antecedent query | `answers/<q>.json` confirms the consequent and `materialized.nq` shows **no leakage** of the constructed world into the base graph; a deterministic revision yields one world, a genuine tie returns `unknown` |
| `projections/` | any `input.logic.ttl` | `projections/` (OWL DL/EL, Datalog, N3, gUFO) compare by isomorphism; the preservation ledger matches |
| `decidability/` | a profile-tagged source | a decidable profile is certified, a violating one is flagged, and budget exhaustion returns `unknown` / `incomplete` |
| `profiles/` | rule sets under each declared semantics | answers match the declared semantic profile (PositiveHorn, StratifiedNAF, WellFounded, StableModel); cut appears only under `ProceduralPrologProfile` |
| `explanation/` | a failed-constraint or derivation query | the `explanation/<q>.md` skeleton validates that **every cited IRI appears in the trace** — no justification outside the proof |
| `paraconsistency/` | a cross-world contradiction | `materialized.nq` confines the contradiction to separate graphs (no explosion); a within-world contradiction emits `witnesses.nq` |

## Runner contract

An engine implements one adapter —
`run(input, mode) → {materialized, verdicts, witnesses, projections, explanations, answers}` — and the
runner diff-compares each output to `expected/`. The Python oracle implements it first (the executable
spec); the Rust core must pass the identical corpus; a JS/Go port self-certifies the same way. It wires
into a `make conformance` target and, once the engine lands, into `make check`.

**Determinism and comparison.** No case may depend on iteration order. RDF outputs (`materialized.nq`,
`witnesses.nq`, projections) compare by **graph isomorphism**; `verdicts.json` and `answers/` compare as
**canonical JSON** (sorted keys, normalized literals); **explanations compare on the cited-IRI and
rule-IRI skeleton**, never the surface prose — a language model may vary the wording, never the set of
axioms, rules, and sources it cites (the faithful-by-construction property from
[LOGIC-RUNTIME.md](LOGIC-RUNTIME.md#explanation-as-projection-logic-to-prose)).

## The loss ledger and preservation polarity

Loss is explicit and machine-readable, reusing the convention already in the mapping layer rather than
inventing a vocabulary. `gmeow:lossyDrop` (`dsl/mappings/vocabulary.ttl:237`) records, per target, what
structure a projection drops; the mapping compiler already aggregates such notes into projection headers
(`ProfileBinding.lossy_drops`, `src/gmeow_tools/mapping_dsl.py:151,493`; `mapping_compile.py:1778`).
`generated/logic/projection-report.ttl` aggregates these across every logic and foundation projection.

For *reasoning* projections, "lossy" is not enough. A consumer needs to know **what a projection
guarantees about answers**, not only what it omits. The ledger therefore records, per projection, both a
complexity/decidability class (EL → PTIME, DL → decidable/N2EXPTIME, Datalog → terminating/PTIME-data)
and a **preservation polarity**:

| Kind | Meaning |
|---|---|
| `logic:ExactPreservation` | same answers as canonical for the declared query class |
| `logic:SoundUnderApproximation` | anything the projection entails is canonically valid, but it may miss answers |
| `logic:CompleteOverApproximation` | the projection may overgenerate but will not miss answers |
| `logic:ValidationOnly` | detects some invalidity, but is not an entailment relation |
| `logic:InconsistencyPreserving` | a canonical inconsistency appears in the projection |
| `logic:InconsistencyReflecting` | a projection inconsistency implies a canonical inconsistency |

The vocabulary:

```text
logic:preservationKind
  logic:ExactPreservation ;
  logic:SoundUnderApproximation ;
  logic:CompleteOverApproximation ;
  logic:ValidationOnly ;
  logic:InconsistencyPreserving ;
  logic:InconsistencyReflecting .
```

A projection report entry can then say not just "dropped modality," but, for example: *"OWL-EL is a
`logic:SoundUnderApproximation` for subclass entailments under profile `X`; PTIME."* This is what lets
the BFO/DOLCE bridge views ([LOGIC-MIGRATION.md](LOGIC-MIGRATION.md#foundation-projection-and-discipline))
be labelled honestly: a bridge view is typically `logic:ValidationOnly` or carries no preservation
claim at all, never `logic:ExactPreservation`, unless a specific subfragment is certified. The
`projections/` corpus category checks that each emitted projection matches its declared preservation kind
and complexity class — overclaiming preservation is a red build, exactly like dropping a fact silently.

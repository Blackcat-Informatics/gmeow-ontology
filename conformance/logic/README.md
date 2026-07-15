<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Conformance Corpus

The native Rust runtime — and any future independent implementation — certifies against **one
shared, language-neutral corpus of cases**: static files, not re-derived assertions. This is the
contract that keeps every implementation aligned with the same executable specification
(Constitution Principle 7).

This corpus is the **executable specification of GMEOW Logic's hardest invariants** — no engine can
drift from them without a red build. It is the implementation contract for the rungs.

> **Status: active.** The category directories contain the live native-runtime corpus, including
> foundation, correspondence, projection, profile, transaction, and world-semantics cases. The
> [runner contract](runner/README.md) and `make conformance` gate keep their expected outputs in
> sync with the production engine.

Normative source: [`../../slices/grounding/logic/design/LOGIC-CONFORMANCE.md`](../../slices/grounding/logic/design/LOGIC-CONFORMANCE.md).

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
| `foundation/` | `input.logic.ttl` with UFO⁺ stereotype, subsumption, and mediation facts | `materialized.nq` contains the expected `logic:violation` / `logic:rigidityViolation` / `logic:dischargeObligation` quads. The disciplines are enforced natively — the relator-mediation discipline runs over the whole ontology (`whole_bundle_relcomp_gate`), with `crates/validate/src/gufo.rs` retained as the regression oracle the lowering is validated against | — |
| `worlds-A/` | standpoint / deception / narrative source | `materialized.nq` carries each world as its own named graph; the contested claim coexists (Crimea `conceivable` vs `refuted`), neither privileged | — |
| `deception/` | an event of `gmeow:eventTypeDeception` carrying a divergent `heldStandpoint` / `projectedStandpoint` doxastic-claim pair, with no asserted intent | `materialized.nq` derives a live positive-control witness over the divergence data (proving the engine reasons over the structure) yet contains **no derived `gmeow:deceptiveIntentClaim`** — the engine-level complement of the standing `logic:IntentNotFromDeceptionObligation`: intent is evidence, never entailment, of a held/projected gap | deception family |
| `derivation/` | a monotone positive-Horn `logic:Rule` composing terms across two or more domain slices (`indirect-contributor`, `wemi-instantiation`, `org-membership-propagation`, `tenure-claim-surfacing`) | `materialized.nq` is a strict superset of `input.nq` carrying the rule's derived `gmeow:` head edge — the cross-slice inference materializes natively (`logic:ExactPreservation`), never `SoundUnderApproximation` residue | derivation family |
| `worlds-B/` | risk cascade, teleology, or norms source | `materialized.nq` proves **exactly zero `Event` instances** are generated (the no-occurrence gate), with type-level force present | — |
| `worlds-C/` | counterfactual antecedent query | `answers/<q>.json` confirms the consequent and `materialized.nq` shows **no leakage** of the constructed world into the base graph; a deterministic revision yields one world, a genuine tie returns `unknown` | — |
| `correspondence/` | `input.logic.ttl` authoring `logic:Correspondence` individuals, with optional compositions declared in `profile.json` | `correspondence-gates.json` is mandatory and pins the law, overclaim, round-trip, mnemomorphism, and composition verdicts; when `expected/projections/` is committed, the OWL-DL / OWL-EL / Datalog / N3 / gUFO / canonical-RDF projection outputs and preservation ledger also compare against their goldens | correspondence family |
| `projections/` | any `input.logic.ttl` | `projections/` (OWL DL/EL, Datalog, N3, gUFO) compare by isomorphism; the preservation ledger matches | — |
| `decidability/` | a profile-tagged source | a decidable profile is certified, a violating one is flagged, and budget exhaustion returns `unknown` / `incomplete` | — |
| `profiles/` | rule sets under each declared semantics | answers match the declared semantic profile (PositiveHorn, StratifiedNAF, WellFounded, StableModel); cut appears only under `ProceduralPrologProfile`; under `ProbabilisticProfile` (`probabilistic-*` cases) each binding carries a `probability` from weighted model counting under a declared model, `logic:confidence` is never read as a probability, and probabilistic facts with no declared model return `unknown` | — |
| `explanation/` | a failed-constraint or derivation query | the `explanation/<q>.md` skeleton validates that **every cited IRI appears in the trace** — no justification outside the proof | — |
| `paraconsistency/` | a cross-world contradiction | `materialized.nq` confines the contradiction to separate graphs (no explosion); a within-world contradiction emits `witnesses.nq` | — |
| `holonic/` | a `logic:properPartOf` mereology spine over domain entities (goals, actions, agent turns) with `profile.json` carrying `foundation_lowering: true` and `StratifiedNAFProfile` | `materialized.nq` carries engine-derived holon-kernel verdicts: `logic:isHolon` on interior nodes (root and leaf excluded), `logic:assessmentVerdict` for emergence (`logic:Emergent` / `logic:Aggregate` / `logic:EmergenceUnknown`) per property-theory pair, non-transitive downward-constraint quads, and `logic:agencyVerdict` agency-profile bindings; explanation skeletons cite only IRIs that appear in the trace | — |

## The `foundation/` category

Implemented in v3. Cases live under `cases/foundation/<case>/`. Each case's `profile.json`
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

The foundation disciplines are enforced by the native lowering, not by an external lint. The
relator-mediation discipline (RelComp) runs over the whole ontology in
`crates/logic/tests/coherence_gate.rs` (`whole_bundle_relcomp_gate`), reading mediation by entity
count; `crates/validate/src/gufo.rs` is retained as the regression oracle the lowering is validated
against. The foundation-lowering and cross-world rigidity soundness suites are the native tests in
`crates/logic/src/foundation/` and the conformance cases here.

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The preference extension — a preference is never automatically a global winner

A contextual comparison over a candidate set will happily report "the best one" and
let a consumer read it as a fact of the whole world. This extension refuses that
shortcut. It records each comparison as a `gmeow:vantage`-indexed, defeasible,
proof-carrying `gmeow:PreferenceObservation`; it keeps conflicting evaluators as
**coexisting** cells, none privileged (Principle 9 — no single slot to win); and it
wraps every derived ranking (pairwise, listwise, DAG/hypergraph, Pareto, scalarized —
including a DPO training view) as a lossy, disclosed `gmeow:ProjectionReceipt` that
names every relation it dropped. Its governing rule, made **executable**: a global
winner over a candidate set is a `math:GlobalSection` of the comparison's cellular
sheaf, and it is a legitimate global fact **IFF** the per-vantage local orders glue
into that section over the **whole** base (degree-zero sheaf cohomology, H⁰) **and**
the `math:GluingObstruction` (H¹) is discharged; a Condorcet cycle is literally a
non-vanishing H¹ class and non-trivial holonomy of the preference
`math:connectionOfSheaf`. This is the isomorphic sibling of `semantic-topology`'s
"a computed feature is never automatically a global truth."

Consumer: **lillith_decodes** (manifest, Principle 15) — the cross-organ
proof-carrying preference ADR, whose deployment substrate is **Cl(12) bundles over
E8-structured 4096-d spaces** (dim Cl(12) = 2¹² = 4096; the E8 `math:RootSystem` /
`math:WeylGroup` / `math:Lattice` organizes the base).

## Grounding here, instance data in the consumer

This slice is the **vocabulary** — bridge terms only. It grounds nothing itself: it
**composes** the shipped grounding referents (the `math:` sheaf / Clifford / E8 /
connection objects and the `logic:` preservation loss ledger) with the core
candidate / observation / standpoint / evidence / provenance spine, and it
hand-authors no `sh:NodeShape`/`sh:PropertyShape` (validation is authored as
`logic:Constraint` and EL-safe OWL axioms in `module.ttl`; the pipeline **derives**
the SHACL). The compiled RDF 1.2 preference **instance data** — the actual candidate
embeddings, comparison complexes, sections, obstructions, and model deltas of a
running analysis — lives in the consumer `lillith_decodes`, not here.

Cross-slice reuse obeys Principle 16 (DAG): axioms anchor **only** on core +
`logic`/`math` grounding. Norms (`Assessment`/`Rubric`/`Criterion`/`EvaluationVerdict`/
`Condition`), model-serving (`ModelArtifact`/`ModelDeployment`), semantic-topology,
and embedding-projection (`VectorSpaceContract`) are **extensions** and are reused
**only** at the instance level (Principle 5) and named by-reference as
`gmeow:TermEquivalence` in `mappings/equivalences.ttl` — never a subclass/domain/range
axiom.

## The pipeline

```text
CandidateSet ─hasCandidate─┐
   (closed-world roster)   │
                           ▼
   PreferenceObservation ──vantage──►  evaluator (co-equal cell, P9)
     │ observedFeature (candidate pair/set + relation)
     │ comparisonContext (task·world·standpoint·time·policy·generation)
     │ strictlyOver / preferentiallyEquivalentWith / incomparableWith
     ▼
   ComparisonCompilation ──compiles──►  math:CellComplex (0/1/2-cells)
     │                                   └► math:CellularSheaf (Clifford-fiber stalks)
     ▼
   ConsensusSection = math:GlobalSection (H⁰)   ◄── legal winner IFF whole-base + discharged
   DisagreementObstruction = math:GluingObstruction (H¹)  ◄── Condorcet cycle / Pareto void
     │                                   + non-flat math:connectionOfSheaf holonomy
     ▼
   ProjectionReceipt {pairwise|listwise|dag|pareto|scalarized}
     │ logic:preservationKind + DisclosedLossEntry (every dropped relation)
     ▼
   ModelConfigDelta (content-addressed 11-tuple) ─► PreferenceLearningEvent
     ▼
   LifecycleReceipt {promotion|rejection|rollback|evaluation}
        promotion ≠ activation authority (ConsumerImprovementGate is authoritative)
```

## Design-set / realized-state

| Artifact | Kind | State |
|---|---|---|
| `design/PREFERENCE-SHEAF.md` | design charter | built |
| Candidate set + comparison context (`module.ttl`) | vocabulary | built |
| Vantage-indexed `PreferenceObservation`/`PreferenceClaim` | vocabulary | built |
| Strict / tie / incomparability relations | vocabulary | built |
| Sheaf consensus/obstruction core + `NoGlobalWinnerWithoutConsensusConstraint` | vocabulary + gate | built |
| Hard/soft verifier orthogonality gates | vocabulary + gate | built |
| Proof/counterproof/counterexample + projection receipts + loss ledger | vocabulary + gate | built |
| Model-config delta (content-addressed tuple) + geometric seam | vocabulary + gate | built |
| Promotion/rejection/rollback/evaluation receipts + activation-authority gate | vocabulary + gate | built |
| Examples (pass witnesses) + counter-examples (fail witnesses) + competency | tests | built |
| General k-criterion Pareto law / projected-DAG acyclicity | second-order boundary | `logic:expressivenessBoundary logic:SecondOrder` (honest boundary, never a faked formula) |
| First-class `math:CliffordBundle`/`AssociatedBundle`/`SpinorBundle` | grounding object | **absent from `math:` — surfaced as a separate grounding follow-up** (an extension may not author grounding); the Cl(12)-bundle-over-E8 structure is expressed here compositionally (`math:CellularSheaf` + Clifford-fiber `math:SheafStalk`s over an E8-organized `math:CellComplex` + `math:connectionOfSheaf`) |

See [`design/PREFERENCE-SHEAF.md`](design/PREFERENCE-SHEAF.md) for the full thesis and
the rule→gate map.

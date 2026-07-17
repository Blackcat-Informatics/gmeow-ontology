<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The semantic-topology extension — a computed feature is never automatically a global truth

Topological data analysis over a claim/evidence graph will happily report a
"cluster", a "loop", or a "void" and let a consumer read it as a fact of the whole
corpus. This extension refuses that shortcut. It compiles RDF 1.2
claim/evidence/proof structures into declared `math:` complexes and filtrations,
runs persistence computations whose results are **attributed wrappers** around the
raw `math:` objects, and wraps every computed feature as a standpoint-relative,
**defeasible** `gmeow:TopologyClaim`. Its governing rule, made executable: a
computed topological feature over a standpoint's sub-evidence is a
`math:LocalSection`, and it is a **global truth IFF** the local data extend to a
`math:GlobalSection` (degree-zero sheaf cohomology, H⁰); the H¹
`math:GluingObstruction` is the computable witness of *why they may not*.

Consumer: **lillith_decodes** (manifest, Principle 15).

## Grounding here, instance data in the consumer

This slice is the **vocabulary** — bridge terms only. It grounds nothing itself: it
**composes** the shipped grounding referents (the `math:` topology objects and the
`logic:` loss ledger) with the core claim/evidence/standpoint spine, and it
hand-authors no `sh:NodeShape`/`sh:PropertyShape` (validation is authored as
`logic:Constraint` and EL-safe OWL axioms in `module.ttl`; the pipeline **derives**
the SHACL). The compiled RDF 1.2 claim/evidence/proof **instance data** — the actual
complexes, filtrations, diagrams, sections, and obstructions of a running analysis —
lives in the consumer `lillith_decodes`, not here.

## The pipeline

```text
Source ─(core)─ EvidenceSpan ─compilesEvidence⁻¹─┐
                        │ cellSource (⊑ wasDerivedFrom)   ComplexCompilation
                   math:Cell ─────────────────────────►  (standpoint, world, time, scenario;
                        │                                  recordsCorrespondence + recordsPreservation)
   compilesToComplex ↓                    ↓ compilesToFiltration
   math:SimplicialComplex          math:Filtration
                                         │ computationOverCompilation⁻¹
                              TopologyComputation ─employsAnalysis→ math:PersistentHomology
                                         │ producesResult
                              TopologyResult ─resultArtifact→ math:PersistenceDiagram/Barcode/…
                                         │ claimResult⁻¹ / claimForComputation⁻¹
                              TopologyClaim ⊑ StandpointClaim
                                 (vantage; logic:confidence via math:StabilityCalibrationRecord)
                                 claimLocalSection → math:LocalSection      (the computed feature)
                                 extendsToGlobalSection → math:GlobalSection (H⁰ — global truth IFF present)
                                 dischargesObstruction → math:GluingObstruction (H¹ — why it may not)
```

## Doctrine

- **Local, not global.** A computed feature is a `math:LocalSection` over a
  standpoint's sub-evidence. It certifies *nothing* global on its own. Global truth
  holds only when the local data lift to a `math:GlobalSection`, and the
  `gmeow:AssertedTopologyResultConstraint` forbids a raw result asserted as global
  truth (`gmeow:assertsGlobalTruth`) without both the claim wrapper and a discharged
  H¹ `math:GluingObstruction`.
- **Compose, don't re-ground.** The `math:` topology objects and the `logic:` loss
  ledger are shipped grounding. This slice mints only bridge terms over them; it
  authors no `logic:GroundingCorrespondence` and mints no preservation vocabulary
  (Principle 19/17/5).
- **Audit + honesty.** Every compilation MUST record a `gmeow:CellSourceCorrespondence`
  (cells trace to evidence spans) and a `gmeow:CompilationPreservationRecord` (the
  `logic:` loss ledger names what it preserved or dropped, Principle 17).
- **Attributed and defeasible.** A result lands as a `gmeow:vantage`-attributed
  `gmeow:TopologyClaim` with a `gmeow:Finding`-carried status, carrying a
  theorem-warranted `logic:confidence` (via `math:StabilityCalibrationRecord`,
  underwritten by `math:bottleneckStabilityTheorem`) — never an asserted `logic:Formula`
  or a global-truth bit.
- **Results reference the mathematics.** A `gmeow:TopologyResult` *references* a raw
  `math:` object through `gmeow:resultArtifact`; conflating the two is `gmeow:ResultObjectConflation`.

## Terms

### gmeow:ComplexCompilation · gmeow:compilesEvidence · gmeow:compilesToComplex · gmeow:compilesToFiltration

The compiler-profile activity (⊑ `gmeow:Activity`) that reads the core claim/evidence
spine (`gmeow:compilesEvidence` → `gmeow:EvidenceSpan`) and produces a
`math:SimplicialComplex`/`math:CellComplex` (`gmeow:compilesToComplex`) with a
`math:Filtration` (`gmeow:compilesToFiltration`). Its build provenance rides the
existing `gmeow:wasGeneratedBy`/`gmeow:wasDerivedFrom`.

### gmeow:CellSourceCorrespondence · gmeow:cellSource · gmeow:recordsCorrespondence

The per-compilation audit record mapping cells to sources. `gmeow:cellSource`
(⊑ `gmeow:wasDerivedFrom`) relates a `math:Cell` to the `gmeow:EvidenceSpan` it was
built from — reusing the provenance spine, not a parallel one. Every compilation MUST
`gmeow:recordsCorrespondence` one (`gmeow:CompilationMustRecordCorrespondence`).

### gmeow:CompilationPreservationRecord · gmeow:recordsPreservation

The per-compilation preservation/loss judgment, stated in the shipped `logic:` loss
ledger and nothing new: `logic:preservationKind` (exact / sound-under-approximation /
lossy-lens / unsupported) and, when lossy, a `logic:expressivenessBoundary`. Every
compilation MUST `gmeow:recordsPreservation` one (`gmeow:CompilationMustRecordLoss`).

### gmeow:compilationStandpoint · gmeow:compilationWorld · gmeow:compilationTimeScope · gmeow:compilationScenario

The Req-5 context-binding properties over the shipped core/logic contexts: the
`gmeow:Standpoint` the topology is relative to, the `logic:PossibleWorld` it is
evaluated at, the `gmeow:TimeScopedRelation` it holds over, and (optionally) the
`gmeow:Scenario` it is entertained under. A compilation missing the required triad is
`gmeow:CompilationMissingContext`.

### gmeow:TopologyComputation · gmeow:computationOverCompilation · gmeow:employsAnalysis · gmeow:producesResult · gmeow:TopologyResult · gmeow:resultArtifact

The definite bridge activity binding compilation → computation → result: it runs over a
compilation (`gmeow:computationOverCompilation`), employs a `math:PersistentHomology`
(`gmeow:employsAnalysis`), and produces a `gmeow:TopologyResult` (`gmeow:producesResult`)
that references a raw `math:` object (`gmeow:resultArtifact` → a
`math:PersistenceDiagram`/`math:PersistenceBarcode`/`math:PersistenceLandscape`/`math:BettiSummary`/`math:HodgeDecomposition`/`math:Holonomy`).
Pointing `gmeow:producesResult` at a raw `math:` object is `gmeow:ResultObjectConflation`.

### gmeow:TopologyClaim · gmeow:claimResult · gmeow:claimForComputation · gmeow:claimLocalSection · gmeow:extendsToGlobalSection · gmeow:dischargesObstruction · gmeow:assertsGlobalTruth

The GOVERNING truth-apt wrapper: `gmeow:TopologyClaim ⊑ gmeow:StandpointClaim` (the
EL-safe axiom), held from a `gmeow:vantage`, carrying its feature as a
`math:LocalSection` (`gmeow:claimLocalSection`). It is a global truth only when it
`gmeow:extendsToGlobalSection` (a `math:GlobalSection`, H⁰) **and**
`gmeow:dischargesObstruction` (a `math:GluingObstruction`, H¹). A
`gmeow:TopologyComputation` may set `gmeow:assertsGlobalTruth` only under such a
discharging claim (`gmeow:AssertedTopologyResultConstraint`). Its `logic:confidence` is
theorem-warranted via a `math:StabilityCalibrationRecord`.

### Conformance failures

`gmeow:TopologyConformanceFailure` (root) with `gmeow:CompilationMissingContext`,
`gmeow:TopologyClaimMissingStatus`, and `gmeow:ResultObjectConflation`
— each a typed, queryable `logic:Situation` a derived `sh:SPARQLConstraint`
raises.

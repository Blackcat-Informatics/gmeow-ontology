<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# risk

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/risk` · **tier: extension**

Hazards, type-level causation, cascades, and mitigations (the risk design) —
counterfactual causal structure **without counterfactual machinery**.

## The type-level move

Cascades relate event **types**, never event instances: nothing in this
slice asserts that anything occurred, and the no-occurrence gate makes that
executable (the test suite asserts the full fixture set entails zero
`gmeow:Event` instances from this slice's terms). PROV lineage and CRM
influence are token-level; OBO RO is the rare type-level causation
vocabulary — that asymmetry is why this slice exists.

## Shape of the model

- **`Hazard ⊑ gufo:Disposition`** (first Disposition use in GMEOW): a
  bearer's disposition toward harm, `manifestedAsType` → the feared
  `EventType`(s). A hazard that never manifests is fully real.
- **Flat type-level links** `typeCauses` / `typeEnables` / `typePrevents` /
  `typeMitigates` (EventType → EventType, never transitive — chain
  composition is solver work, P12) → promote to **`CausalLink`**
  (antecedent × consequent × mandatory `causalModality`
  [necessitates / promotes / enables / prevents, open] × localizable
  mechanism prose × solver-input `linkStrength`). Causal links are claims:
  standpoint-indexed, confidence-bearing; two analysts' divergent links
  coexist (P9).
- **`Cascade ⊑ SocialObject`** — a *named* failure narrative ("Trust
  Collapse"), entered at `cascadeFirstLink`, walked by `linkNext`
  (non-functional → branching failure trees; SHACL-acyclic), graded by the
  ordered **`SeverityLevel`** vocabulary (GranularityLevel pattern, fourth
  use; ISO 31000 / IEC 60812 anchor terminology only).
- **`Mitigation`** binds a measure to a **`RiskFactor`** (the named umbrella
  over CausalLink + Hazard — generator-visible range, relator-arity
  visible end). The *measure* range is deliberately **open** (the
  `tenurePosition` precedent, fourth use): a `gmeow:Norm`, a `logic:Plan`, or an
  engineered design control are all measures, and no single class subsumes the
  three without distorting one. Whether a mitigation *worked* is a
  vantage-indexed claim, never an entailment.

## Deferred to the compiler-arc window

OBO RO rows (linkage-only, BFO lineage), MITRE D3FEND
(countermeasure ↔ Mitigation), STIX 2.1/ATT&CK, UCO, ConceptNet/ATOMIC;
projections to bowtie JSON (near-structural; drops causalModality — declared),
FMEA CSV (drops chain order — declared), STIX bundles. Target list fixed in the alignment ledger.

## Terms

### gmeow:Hazard · gmeow:hazardBearer · gmeow:manifestedAsType · gmeow:hazardSeverity

A bearer's disposition toward harm — the first `gufo:Disposition` use in GMEOW, and
the reason this slice needs no counterfactual machinery: a hazard that never
manifests is still fully real. `hazardBearer` (functional, mandatory) names the one
entity it inheres in; `manifestedAsType` (non-functional, at least one) the feared
`EventType`(s), never an `Event` — the type level is where counterfactuals live
without machinery. `hazardSeverity` grades a standalone hazard (optional).

### gmeow:typeCauses · gmeow:typeEnables · gmeow:typePrevents · gmeow:typeMitigates

Flat type-level causal claims between event kinds — bring about, make possible,
block, or reduce. DELIBERATELY NOT TRANSITIVE: chain composition is solver work
(P12). Each is statement-layer indexed; `typeCauses` `pairsWith` `CausalLink`, the
reified form to promote to when modality, mechanism, or strength rides on the claim.

### gmeow:CausalLink · gmeow:linkAntecedent · gmeow:linkConsequent · gmeow:linkMechanism · gmeow:linkStrength

The reified type-level causal claim — antecedent kind × consequent kind × modality,
with optional mechanism prose and solver-input strength. `linkAntecedent` and
`linkConsequent` (both functional, mandatory, and distinct — nothing type-causes
itself) carry the kinds; `linkMechanism` the localizable how-prose; `linkStrength` a
solver-INPUT weight, never an output written back as assertion (P12).

### gmeow:CausalModality · gmeow:causalModality

The force of a causal link — an OPEN vocabulary seeded with necessitates, promotes,
enables, prevents. Distinct from doxastic `standpointModality` and ontic
`hasDeterminacy`: causal modality is what the link CLAIMS about the world's
mechanics. `causalModality` is functional and mandatory: if you reified, the modality
is your reason.

### gmeow:Cascade · gmeow:cascadeFirstLink · gmeow:linkNext · gmeow:cascadeSeverity

A NAMED chain of causal links — a `SocialObject` failure narrative that exists
independent of anything occurring. Entered at `cascadeFirstLink` (functional,
mandatory), walked by `linkNext` (non-functional → branching failure trees,
SHACL-acyclic, not transitive), graded by mandatory `cascadeSeverity` — an ungraded
cascade is just a story.

### gmeow:SeverityLevel · gmeow:moreSevereThan

An OPEN, ordered severity vocabulary (catastrophic ≻ severe ≻ moderate ≻ minor; the
kernel GranularityLevel pattern, fourth use) anchored terminologically to ISO 31000 /
IEC 60812 in docs, never axioms. `moreSevereThan` is transitive ON LEVELS ONLY —
cascades themselves are never ordered transitively.

### gmeow:RiskFactor · gmeow:Mitigation · gmeow:mitigationMeasure · gmeow:mitigationCounters

`RiskFactor` is the named umbrella over CausalLink + Hazard (a generator-visible range,
never instantiated directly). `Mitigation` is a reified countermeasure binding: a
measure set against a RiskFactor it counters. `mitigationMeasure` has an intentionally
OPEN range (the `tenurePosition` precedent) because a `gmeow:Norm`, a `logic:Plan` and
an engineered control are genuinely heterogeneous; `mitigationCounters` ranges over RiskFactor
(a bowtie barrier or a source control).

### gmeow:MitigationStatus · gmeow:mitigationStatus

The lifecycle status of a mitigation — an OPEN vocabulary seeded with proposed,
active, retired; retirement is suppression-shaped (kept with its status, never
deleted, P10). `mitigationStatus` is non-functional: status-over-time rides
`validFrom`/`validUntil` on the statement, single-valuedness per base graph is SHACL's
job.

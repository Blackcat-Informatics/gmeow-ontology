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
  `tenurePosition` precedent, fourth use): a `gmeow:Norm` or a Procedure
  plugs in without extension→extension dependency (P16). Whether a
  mitigation *worked* is a vantage-indexed claim, never an entailment.

## Deferred to the compiler-arc window

OBO RO rows (linkage-only, BFO lineage), MITRE D3FEND
(countermeasure ↔ Mitigation), STIX 2.1/ATT&CK, UCO, ConceptNet/ATOMIC;
projections to bowtie JSON (near-structural; drops causalModality — declared),
FMEA CSV (drops chain order — declared), STIX bundles. Target list fixed in the alignment ledger.

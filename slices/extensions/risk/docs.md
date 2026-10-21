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
solver-input `math:Quantity`, never an output written back as assertion (Principle 12).
The quantity names its dimension, finite numeric magnitude and an analysis reference
frame declaring the interpretation of the scale. Weights need not lie in the unit
interval: a weight of 2.5 is valid on a declared scale. An unweighted causal link
remains legal. Literal weights are rejected; there is one mathematical quantity owner.

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

## Modeled risk likelihoods

A `RiskLikelihoodAssessment` is an ordinary held `Observation` about a risk factor
or cascade. It requires an `observedFeature` and at least one `observationResult`
typed `math:ProbabilityValue`. Every reported result has finite numeric magnitudes,
a typed `logic:ProbabilityModel`, and a `math:ProbabilityEvent` assigning the
value through `math:probabilityOfEvent`. Each such event names a typed
`math:ProbabilitySpace` through `math:eventInSpace`; the mathematics owner checks
its sample space, sigma algebra and measure. Its probability lies in [0,1] and
is dimensionless. A reference frame alone cannot substitute for the required model.

A probability event is a set of modeled outcomes, never an actual occurrence.
Recording the likelihood of a hazard does not entail that its feared event
happened. Multiple model-bound results and competing assessments coexist.
Ordinal severity, causal modality, solver weights and epistemic confidence keep
their distinct meanings; none implies a probability value or supplies a model.
An arbitrary numeric risk score uses the declared-scale quantity contract; the
likelihood contract applies only when the score denotes an occurrence probability.

Bayesian analyses retain `math:PriorDistribution`, `math:LikelihoodFunction` and
`math:PosteriorDistribution` with explicit `math:hasPrior` and `math:hasLikelihood`
links. A prior is a distribution over parameters before data; a posterior is
the updated distribution, rather than another name for a confidence number.
Other risk probability models do not acquire a Bayesian interpretation.
See [bayesian-risk.ttl](examples/bayesian-risk.ttl) for a recorded predictive result
with prior, likelihood, data and posterior provenance.
See [modeled-risk.ttl](examples/modeled-risk.ttl) for separate categorical,
weighted and probability-bearing records. Computing propagation and Bayesian
updates remains solver work; these records do not run a parallel evaluator.

### Exact values remain exact

Weights and risk probabilities can co-type their quantity as `math:RationalValue`,
with `math:isExact true`, integer `math:numerator` and non-zero integer
`math:denominator`. A probability of 1/3 requires no decimal approximation.
The imported rational shape checks the pair, integer datatypes and non-zero
denominator. The mathematical rational-probability law compares integer numerator and
denominator by sign to check [0,1] exactly. Its generated procedural projection
checks the pair without division or a bounded integer conversion; decimal
value-range facets alone do not establish this bound. An exact rational carrier
supplies no parallel decimal magnitude; mixed carriers are rejected. The exactness
flag is a single boolean true, so an additional false or mistyped flag cannot hide
behind a valid true flag.

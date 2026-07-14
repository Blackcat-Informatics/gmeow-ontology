<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — Analysis, Topology, and Geometry

> The **analysis-and-geometry charter** of the GMEOW Mathematics design set: calculus and analysis
> (limits, derivatives, integrals, series, special functions), topology, and differential geometry
> (manifolds, charts, tensor fields, metrics — including the Lorentzian metric the relativity case
> needs). It builds on the expression AST and the measure charter
> ([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md),
> [`MATHEMATICS-MEASURE-AND-DIMENSION.md`](MATHEMATICS-MEASURE-AND-DIMENSION.md)); it holds the
> **math side** of the physical-reference-frame flagship-adjacent case, leaving spacetime and SR/GR to
> a downstream physics slice. Anchors (OpenMath calculus CDs, DLMF, mathlib) are in
> [`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md); gates in
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's `shapes.ttl`, competency queries, and the
> projection loss ledger.

## Purpose

Calculus, topology, and geometry share one discipline in GMEOW: **the objects of the continuous are
structured, not evocative.** A derivative is an operator application, not the string "dy/dx"; a
manifold declares its dimension and structure kind, not merely the label "manifold"; a complement
names its ambient space and its complement-semantics. Ambiguity in the continuous is where silent
error hides, so this charter hard-fails on it.

## Calculus and analysis

Core classes: `math:Limit`, `math:LimitResult`, `math:LimitOutcome`, `math:Derivative`,
`math:PartialDerivative`, `math:DifferentialOperator`, `math:Integral` (from the measure charter),
`math:Series`, `math:Sequence`, `math:Convergence`, and `math:SpecialFunction`.

Core properties: `math:limitOf`, `math:limitPoint`, `math:hasLimitResult`, `math:limitOutcome`,
`math:limitResultValue`, `math:derivativeOf`, `math:withRespectToVariable`, `math:derivativeOrder`,
`math:seriesTerm`, `math:convergesTo`, and `math:convergenceMode`.

Every operator here is a **binder over the expression AST**: `d/dx`, `∂`, `∫`, `∑`, and `lim` are
`math:BindingExpression`s binding the variable of differentiation, integration, or summation
([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md)). A `math:Derivative` names what it
differentiates, with respect to which variable, at which order; a `math:Limit` names its expression,
its limit point, and — where it matters — the direction/mode; a `math:Series` and `math:Sequence`
carry `math:Convergence` with its mode (pointwise, uniform, in-measure, …). Special functions align
to **DLMF** by equation ID and to OpenMath `calculus1`/`transc1`.

Where a limit's **evaluated result** matters — not just the limit expression, but *what it runs to* —
a `math:Limit` carries a structured `math:LimitResult` through `math:hasLimitResult`. The result is a
first-class object, never a bare literal: it names its `math:limitOutcome` (one of the four
`math:LimitOutcome` individuals `math:convergesFinitely`, `math:divergesToPositiveInfinity`,
`math:divergesToNegativeInfinity`, `math:divergesWithoutLimit`) and, where the outcome has one, its
`math:limitResultValue`. The value **agrees with the outcome** by construction: a convergent result
carries a finite literal; a result diverging to a pole carries `math:PositiveInfinity` or
`math:NegativeInfinity` (the same signed extended-real points a `math:totalMass` or an
`math:extendedRealValue` ranges over, which is why `math:limitResultValue` is an honest `rdf:Property`
spanning literal and pole); a result diverging without a limit — an oscillating `sin(1/x)` at `0` —
carries no value at all, because there is no point of `ℝ̄` to name. The result is **optional** on a
limit (a limit may still carry only its expression, point, and mode), but once stated it says which of
convergence or the three modes of divergence the limit is, so `lim = ∞` is never a silent overload of a
finite value. A result missing its outcome, or carrying a value that disagrees with it, is ill-formed
(`math:UnderspecifiedLimitResult`). This is where a grounding layer
becomes analysis: derivatives are maps of function spaces
([`MATHEMATICS-NUMBERS-AND-SETS.md`](MATHEMATICS-NUMBERS-AND-SETS.md)), and integration is the measure
charter's operator.

## Topology

Core classes: `math:TopologicalSpace` (from the object layer), `math:OpenSet`, `math:ClosedSet`,
`math:Neighbourhood`, `math:ContinuousMap`, `math:Homeomorphism`, `math:CompactSpace`,
`math:ConnectedSpace`, `math:Homotopy`, and `math:HomologyGroup`.

Core properties: `math:hasOpenSet`, `math:isContinuous`, `math:separationAxiom`, `math:isCompact`,
`math:isConnected`, and `math:homotopyEquivalentTo`.

A `math:TopologicalSpace` is given by its open sets (or a basis); continuity is a declared property of
a map (preimages of open sets are open), not an assumption; connectedness and the T0–T4 separation
axioms are first-class and each backed by a real first-order `logic:Formula` defining law
(`math:connectednessLaw`, `math:t0SeparationLaw` … `math:normalitySeparationLaw`), mirroring
`math:continuityLaw`. Compactness is the lone second-order property — a finite subcover over an
arbitrary family is not first-order axiomatizable — so rather than a faked formula it is carried as an
honest loss-ledger boundary (`math:compactnessBoundary`, `logic:expressivenessBoundary logic:SecondOrder`).
Homotopy and homology give the algebraic-topology bridge (a
`math:HomologyGroup` is a `math:AbelianGroup`, [`MATHEMATICS-ALGEBRA.md`](MATHEMATICS-ALGEBRA.md)).
Little external ontology exists — the content is in prover libraries (mathlib, Isabelle/AFP), so the
depth is **authored** and **cited**.

### Persistent homology — filtrations, lifetimes, and stability

Core classes: `math:Filtration`, `math:FiltrationStage`, and `math:PersistenceLifetime`.

Core properties: `math:hasFiltrationStage`, `math:filtrationThreshold`, `math:stageStructure`,
`math:filtrationIndexKind`, `math:filtrationAmbient`, `math:overFiltration`, `math:persistenceFeature`,
`math:bornAt`, and `math:diesAt`.

A `math:Filtration` is a monotone family of substructures of an ambient object indexed by a real-valued
threshold ε: each `math:FiltrationStage` pairs a `math:filtrationThreshold` (a `gmeow:Quantity`) with
the `math:stageStructure` present at it (a `math:TopologicalSpace`), and for thresholds ε₁ ≤ ε₂ the
stage at ε₁ is contained in the stage at ε₂. The containment is the **existing** transitive
`math:subsetOf`, not a new order relation; the nesting law that ε₁ ≤ ε₂ *entails* containment is
authored as the first-order `logic:Formula` `math:filtrationMonotonicityLaw`, a sibling of
`math:continuityLaw` and `math:connectednessLaw` over the reified stage/threshold/structure signature.
The SHACL gate enforces only structural presence (a filtration has stages; each stage has a threshold
and a structure) — monotonicity is a law, not a shape, exactly as for the separation axioms.

A `math:PersistenceLifetime` is the birth–death interval of one topological feature — a
`math:HomologyGroup` generator, a hole of some dimension — across a filtration: it names the filtration
(`math:overFiltration`), the feature (`math:persistenceFeature`), the threshold the feature appears at
(`math:bornAt`, a `gmeow:Quantity`), and the threshold it disappears at (`math:diesAt`). Its persistence
is death − birth: a long-lived feature is signal, a short-lived one is noise. An **essential** feature
never dies within the filtration, so its `math:diesAt` is the individual `math:PositiveInfinity` rather
than a finite threshold — the same extended-real range `math:totalMass` carries, which is why
`math:diesAt` is a bare `rdf:Property` (its range spans a finite quantity individual and
`math:PositiveInfinity`, and neither DL property kind admits both).

The bedrock stability result is the **bottleneck stability theorem** (`math:bottleneckStabilityTheorem`,
a `math:Theorem` held under `math:computationalTopologyTheory` and warranted by
`math:cohenSteinerEdelsbrunnerHarer2007`): for tame functions *f*, *g* on a common space, the bottleneck
distance between their persistence diagrams satisfies d\_B(Dgm(*f*), Dgm(*g*)) ≤ ‖*f* − *g*‖\_∞ — the
persistence-diagram map is 1-Lipschitz. Its kept corollary (correct by the triangle inequality on the
two independently-bounded endpoints, |Δbirth| ≤ δ and |Δdeath| ≤ δ ⇒ |Δ(death − birth)| ≤ 2δ) is that a
feature's persistence value can shift by at most 2‖*f* − *g*‖\_∞ under such a perturbation. No
Wasserstein-distance bound is asserted: it needs extra hypotheses this statement does not verify. This
theorem is what turns the persistence of a feature into a *warranted* credence
([`MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md`](MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md)), the
persistence-calibration surface that lands as `logic:confidence` on a latent-meaning claim. Like the
rest of the topology depth, the content is authored and cited (mathlib/AFP; Edelsbrunner–Harer,
*Computational Topology*).

## Differential geometry and manifolds

Core classes: `math:Manifold`, `math:SmoothManifold`, `math:ComplexManifold`,
`math:RiemannianManifold`, `math:LorentzianManifold`, `math:Chart`, `math:Atlas`,
`math:CoordinateMap`, `math:TangentSpace`, `math:TensorField`, `math:MetricTensor`, and
`math:Complement`.

Core properties: `math:manifoldDimension`, `math:manifoldStructureKind`, `math:hasChart`,
`math:chartDomain`, `math:coordinateMap`, `math:targetCoordinateSpace`, `math:metricTensor`,
`math:tensorFieldOn`, `math:ambientSpace`, and `math:complementSemantics`.

A `math:Manifold` declares its **dimension** and its **structure kind** (topological, smooth, complex,
Riemannian, Lorentzian) — a manifold without both is ill-formed. A `math:Chart` names its domain,
coordinate map, and target coordinate space; a `math:Atlas` is a covering family of charts. A
`math:MetricTensor` (Riemannian or **Lorentzian**) is a `math:TensorField`; the Lorentzian metric is
the math object a physics slice needs for spacetime, and it stays here on the **math** side of the
boundary.

> **Hard rule — named complements.** A `math:Complement` names its `math:ambientSpace` and its
> `math:complementSemantics` (set-theoretic, orthogonal, complex-linear, topological, or
> quotient/cokernel). "The complement of X" without an ambient space and a complement-semantics is
> ill-formed — the flagship-adjacent KG example's "complex complement" is exactly this hazard, and
> the gate forbids it (`math:UnqualifiedComplement`,
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)). The orthogonal-complement case is
> realized in [`MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md`](MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md).

### Conformal geometry and compactification

Core classes: `math:Compactification`, `math:ConformalCompactification`, and `math:BoundaryAtInfinity`.

Core properties: `math:originalSpace`, `math:compactifyingMap`, `math:compactifiedSpace`,
`math:boundaryAtInfinity`, and `math:conformalFactor`.

A `math:Compactification` embeds an **unbounded** space in a **bounded** (compact) one as a dense
subset, adjoining the points the original space runs off toward as a boundary at infinity. It is not
tied to any single construction — not the one-point (Alexandroff) versus Stone–Čech distinction *per
se* — but the general **structured record** every such construction instantiates, and it names **four
distinct roles**: the original unbounded space (`math:originalSpace`, a `math:TopologicalSpace` or
`math:Manifold`), the embedding (`math:compactifyingMap`, a `math:Function`/`math:CoordinateMap`), the
resulting bounded space (`math:compactifiedSpace`), and the boundary added at infinity
(`math:boundaryAtInfinity`, a `math:BoundaryAtInfinity`). A compactification missing any of the four is
ill-formed (`math:UnderspecifiedCompactification`) — a compactification that does not say what it
embeds, by which map, into what, and with which boundary is not a compactification.

A `math:BoundaryAtInfinity` is the **ideal points** — the conformal boundary — the original space
approaches but never reaches, made a first-class object rather than an informal "edge": for the radial
half-line *r* ∈ [0, ∞) it is the single ideal point *r* = +∞.

A `math:ConformalCompactification` is the **conformal (Penrose-style)** specialization: its
`math:compactifyingMap` is angle-preserving, carrying a `math:conformalFactor` Ω that rescales the
metric so the points at infinity sit at a **finite** conformal boundary. A subclass of
`math:Compactification`, it additionally names its `math:conformalFactor` (a positive
function/quantity — an individual, never a bare literal, so the property is an `owl:ObjectProperty`),
and one that omits it is ill-formed. This is the correct general home for embedding a
`math:LorentzianManifold`'s **radial chart's infinity as a finite boundary**: the conformal factor
sends the unbounded metric distance to a finite rescaled one while preserving the causal (angle)
structure the Lorentzian metric carries. The metric and its rescaling stay **math-side** — a
spacetime's Penrose diagram is a downstream physics reading of this math object, exactly as the
Lorentzian metric above is the math object a physics slice builds spacetime frames on.

> **Hard rule — the four compactification roles.** A `math:Compactification` names its
> `math:originalSpace`, its `math:compactifyingMap`, its `math:compactifiedSpace`, and its
> `math:boundaryAtInfinity`; a `math:ConformalCompactification` additionally names its
> `math:conformalFactor`. "The compactification of X" without all four roles (or a conformal one
> without its rescaling factor) is ill-formed — the gate forbids it
> (`math:UnderspecifiedCompactification`,
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)), mirroring `math:UnderspecifiedManifold`
> and `math:UnderspecifiedChart`.

## The math/physics boundary

Manifolds, charts, tensor fields, and Lorentzian metrics are **mathematics** and stay in this slice.
**Spacetime, physical reference frames, worldlines, proper time, and the SR/GR regimes** are
**physics** — a downstream physics slice authors them, consuming this charter's manifolds and metrics
(the only referenceable frame standard is IVOA STC, an align-by-reference target,
[`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md)). This keeps the boundary as clean as the
`logic:`/`observations` boundaries the manifesto draws.

## A worked example — a chart on a Lorentzian manifold

```ttl
ex:minkowski
    a math:LorentzianManifold ;
    math:manifoldDimension 4 ;
    math:manifoldStructureKind math:lorentzianStructure ;
    math:hasChart ex:inertialChart ;
    math:metricTensor ex:minkowskiMetric .

ex:inertialChart
    a math:Chart ;
    math:chartDomain ex:minkowski ;
    math:coordinateMap ex:inertialCoordinateFn ;
    math:targetCoordinateSpace ex:R4 .
```

The manifold declares dimension and structure kind; the chart names its coordinate map and target;
the Lorentzian metric is a tensor field — all math-side, ready for a physics slice to build spacetime
frames on top.

## Shape and lint gates

Catalogued in [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md): a derivative/limit/series
names what it is of and its variable/mode; continuity/compactness/connectedness are declared, not
assumed; a manifold declares dimension and structure kind; a chart names domain, coordinate map, and
target space; and a complement names its ambient space and complement-semantics.

## Competency questions

1. What does this derivative differentiate, with respect to which variable and at what order?
2. In what mode does this series/sequence converge, and to what?
3. What are the open sets (or basis) of this space, and is the map continuous/compact/connected?
4. What is this manifold's dimension and structure kind, and what charts cover it?
5. For this complement, what is the ambient space and the complement-semantics?
6. For this compactification, what are its original space, compactifying map, compactified space,
   boundary at infinity, and (if conformal) conformal factor?

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

Core classes: `math:Limit`, `math:Derivative`, `math:PartialDerivative`, `math:DifferentialOperator`,
`math:Integral` (from the measure charter), `math:Series`, `math:Sequence`, `math:Convergence`, and
`math:SpecialFunction`.

Core properties: `math:limitOf`, `math:limitPoint`, `math:derivativeOf`, `math:withRespectToVariable`,
`math:derivativeOrder`, `math:seriesTerm`, `math:convergesTo`, and `math:convergenceMode`.

Every operator here is a **binder over the expression AST**: `d/dx`, `∂`, `∫`, `∑`, and `lim` are
`math:BindingExpression`s binding the variable of differentiation, integration, or summation
([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md)). A `math:Derivative` names what it
differentiates, with respect to which variable, at which order; a `math:Limit` names its expression,
its limit point, and — where it matters — the direction/mode; a `math:Series` and `math:Sequence`
carry `math:Convergence` with its mode (pointwise, uniform, in-measure, …). Special functions align
to **DLMF** by equation ID and to OpenMath `calculus1`/`transc1`. This is where a grounding layer
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
a map (preimages of open sets are open), not an assumption; compactness, connectedness, and the
separation axioms are first-class. Homotopy and homology give the algebraic-topology bridge (a
`math:HomologyGroup` is a `math:AbelianGroup`, [`MATHEMATICS-ALGEBRA.md`](MATHEMATICS-ALGEBRA.md)).
Little external ontology exists — the content is in prover libraries (mathlib, Isabelle/AFP), so the
depth is **authored** and **cited**.

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

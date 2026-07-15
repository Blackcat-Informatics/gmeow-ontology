<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — Measure, Integration, and Dimension

> The **measure-and-dimension charter** of the GMEOW Mathematics design set: measurable spaces,
> measures, integration, and — as a cross-cutting invariant — dimensional analysis and the
> quantity/unit/dimension grounding. Measure is the bedrock the probability layer stands on
> ([`MATHEMATICS-PROBABILITY.md`](MATHEMATICS-PROBABILITY.md)); dimension is the invariant that ties
> the units world to every equation and every distribution parameter. Its gates are in
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md) and its anchors (QUDT, OM 2, D-SI, SI
> base dimensions) in [`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's `shapes.ttl`, competency queries, and the
> projection loss ledger.

## Purpose

Two jobs meet here. First, **measure and integration** give probability its foundation: a
probability measure ([`MATHEMATICS-PROBABILITY.md`](MATHEMATICS-PROBABILITY.md)) is exactly a measure
of total mass one, so the probability layer inherits its structure from this charter rather than
re-inventing it. Second, **dimensional analysis** is the grounding layer's most pervasive hard-fail:
every equation is dimensionally homogeneous, every quantity carries a dimension, and a
mis-dimensioned parameter is caught, not silently propagated.

## Measurable spaces, measures, and integration

Core classes: `math:MeasurableSpace`, `math:SigmaAlgebra` (shared with the probability layer),
`math:MeasurableSet`, `math:Measure`, `math:LebesgueMeasure`, `math:CountingMeasure`,
`math:MeasureEvaluation`, `math:MeasurableFunction`, `math:Integral`, and `math:IntegrationOperator`.

Core properties: `math:measurableSpaceOf`, `math:measureOn`, `math:totalMass`, `math:evaluatedMeasure`,
`math:measuredSubset`, `math:measureResult`, `math:integrand`, `math:integrationDomain`,
`math:integrationMeasure`, and `math:withRespectTo`.

A `math:MeasurableSpace` is a set with a σ-algebra; a `math:Measure` assigns non-negative extended-
real mass to its measurable sets (`math:measureOn`, `math:totalMass`). A `math:MeasurableSet` is one
element of that σ-algebra — a single measurable subset, the kind of subset a measure can weigh. An
`math:Integral` is a first-class object — an application of an `math:IntegrationOperator` (a binder over
the integration variable, so `∫` is a `math:BindingExpression`, [`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md))
naming its integrand, domain, and the measure it integrates against (`math:withRespectTo`). Lebesgue
and counting measures unify continuous and discrete cases; expectation
([`MATHEMATICS-PROBABILITY.md`](MATHEMATICS-PROBABILITY.md)) is an integral against a probability
measure.

> **μ(A) as a reified evaluation.** `math:totalMass` is the measure of the *whole* space, μ(X) — a
> single edge on the `math:Measure`. To speak of the mass of a *named* subset A ⊆ X, GMEOW reifies the
> evaluation μ(A) as a `math:MeasureEvaluation`: it names the measure (`math:evaluatedMeasure`), the
> subset (`math:measuredSubset`, a `math:MeasurableSet`), and the resulting mass (`math:measureResult`,
> a finite non-negative number or `math:PositiveInfinity`, **never** `math:NegativeInfinity` — a measure
> is non-negative). Because each evaluation is a first-class object rather than a value dangling off the
> measure, distinct subsets of one measure can be compared side by side — the Lebesgue mass of a two-ball
> μ(B²) against that of a three-ball μ(B³), or one event's probability against another's. An evaluation
> that omits any of the three roles is ill-formed (`math:UnderspecifiedMeasureEvaluation`); a result of
> `math:NegativeInfinity` is a non-negativity violation caught by the `logic:` gate
> `math:MeasureResultNonNegativeConstraint`.

<!-- -->

> **The probability bridge.** A `math:ProbabilityMeasure` **is** a `math:Measure` with
> `math:totalMass` 1 over a probability space; the probability layer specializes this charter, it
> does not duplicate it. Density and mass functions are `math:MeasurableFunction`s;
> `math:ExpectedValue` is an `math:Integral` against the measure.

## Dimension — the cross-cutting invariant

Core classes: `math:Dimension`, `math:BaseDimension`, `math:DerivedDimension`, `math:Dimensionless`,
and `math:DimensionalExpression`.

Core properties: `math:hasDimension`, `math:dimensionVector`, `math:baseDimensionExponent`, and
`math:commensurableWith`.

A `math:Dimension` is a vector over the seven SI base dimensions (length, mass, time, current,
temperature, amount, luminous intensity) via `math:dimensionVector`; a `math:DerivedDimension` (e.g.
velocity = L·T⁻¹, energy = M·L²·T⁻²) is a product recorded by `math:baseDimensionExponent`. Quantities
carry `math:hasDimension`; two quantities are `math:commensurableWith` iff their dimension vectors
are equal. Units (`gmeow:unit`, aligned to **QUDT** / **OM 2**) live *within* a dimension; the SI
truth anchor is the **BIPM D-SI** PID set ([`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md)).
GMEOW's differentiator over QUDT/OM is that dimensional soundness is a **reasoned gate**, not asserted
data.

### Dimensional homogeneity as a gate

Dimension is not a domain silo; it is a hard-fail that fires across the whole layer:

> **Gate — `math:DimensionalInhomogeneity`.** Both sides of an equation share a dimension; the
> operands of an addition share a dimension; a distribution's location parameter shares the random
> variable's dimension and its variance parameter carries the squared dimension
> ([`MATHEMATICS-PROBABILITY.md`](MATHEMATICS-PROBABILITY.md)); a physical law's terms are homogeneous.
> A correlation, a probability, and an angle are `math:Dimensionless`. A dimensionally inhomogeneous
> expression is ill-formed — this is the no-optionality posture applied to dimensions.

This single gate is what lets the miscalibrated-device case (metrology — GUM Type A/B uncertainty
budgets and DCC calibration certificates, alignment targets catalogued in
[`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md) and attached to this charter's dimensioned
quantities) and the relativity case check that their quantities compose lawfully.

## A worked example — an integral against a measure, dimensioned

```ttl
ex:expectedEnergy
    a math:Integral ;
    math:integrand ex:energyDensityFn ;
    math:integrationDomain ex:configurationSpace ;
    math:withRespectTo ex:gibbsMeasure ;
    math:hasDimension ex:energyDimension .

ex:energyDimension
    a math:DerivedDimension ;
    math:dimensionVector "M·L2·T-2" ;
    math:baseDimensionExponent ( ex:massExp1 ex:lengthExp2 ex:timeExpMinus2 ) .
```

The integral carries the dimension of its result; the homogeneity gate checks that the integrand's
dimension times the measure's dimension equals the declared energy dimension.

## Shape and lint gates

Catalogued in [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md): a measure declares its
measurable space and total mass; a probability measure is a measure of mass one; an integral names
integrand/domain/measure; every quantity carries a dimension; and `math:DimensionalInhomogeneity`
fires on any non-homogeneous expression, addition, or parameterization.

## Competency questions

1. What measurable space and total mass does this measure have, and is it a probability measure?
2. What does this integral integrate, over what domain, against what measure?
3. What is the dimension vector of this quantity, and what is it commensurable with?
4. Which expressions or parameterizations fail dimensional homogeneity?
5. Which unit (QUDT/OM/D-SI) realizes this dimension for this quantity?

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — The Probability Layer

> The **probability charter** of the GMEOW Mathematics design set: probability spaces, events,
> measures, random variables, distributions and their mandatory parameterization, the dependency
> models a probabilistic reasoning request points at, and the formal lowering into the `logic:`
> probability-model requirement. It makes precise the manifesto's thesis
> ([`MATHEMATICS.md`](MATHEMATICS.md)) that a probability is not a confidence score. It builds on the
> expression and object layers ([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md)) and feeds
> the statistics layer ([`MATHEMATICS-STATISTICS.md`](MATHEMATICS-STATISTICS.md)); its gates are
> catalogued in [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md) and its lossy lowerings in
> [`MATHEMATICS-PROJECTIONS.md`](MATHEMATICS-PROJECTIONS.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's canonical `module.ttl` axioms and `logic:Constraint` records, competency queries, and the
> projection loss ledger.

## Purpose

The probability layer models probability-domain **objects**. It does **not** redefine probabilistic
reasoning semantics. `logic:` owns `logic:ProbabilityModel`, `logic:probabilityModel`,
`logic:FullIndependence`, `logic:DependencyModel`, `logic:JointOutcome`, `logic:jointProbability`,
and the hard distinction between probability, confidence, solver weight, and evidential support
(`slices/grounding/logic/design/LOGIC-SEMANTICS.md`). The mathematics slice names the probability spaces,
σ-algebras, events, measures, random variables, distributions, parameterizations, and dependency
models those reasoning semantics operate over, and it declares the lowering between the two (§ The
`logic:` seam).

## The probability space

Core classes: `math:ProbabilitySpace`, `math:SampleSpace`, `math:SigmaAlgebra`,
`math:ProbabilityEvent`, and `math:ProbabilityMeasure`.

Core properties: `math:sampleSpace`, `math:eventSigmaAlgebra`, `math:probabilityMeasure`,
`math:eventInSpace`, and `math:probabilityOfEvent`.

A probability space is the Kolmogorov triple made explicit: a sample space Ω
(`math:sampleSpace`), an event σ-algebra ℱ (`math:eventSigmaAlgebra`), and a probability measure
P (`math:probabilityMeasure`). An event is a `math:ProbabilityEvent` situated in a space by
`math:eventInSpace`; a probability *of* an event is a framed value, never a bare literal (§
Probability value representation).

**`ProbabilityEvent` is a set of outcomes, not an occurrent.** In GMEOW's broader vocabulary an
"event" usually means an occurrent (something that happens in time, `logic:Event`). A
`math:ProbabilityEvent` is the measure-theoretic sense — a measurable subset of the sample space —
and the two are held **disjoint** by an OWL axiom and documented as such, so a rain *occurrence* and
the rain *event-set* in a forecast space are never conflated. **`SigmaAlgebra` is the event
algebra; there is no separate `EventSet` class** — a bare "set of events" without the closure
properties of a σ-algebra is the inferior term and is removed under the greenfield rule; membership
of an event in the algebra is `math:eventInSpace` against the space's `math:eventSigmaAlgebra`.

### Symbolic and generative spaces

A σ-algebra is rarely extensionally enumerable, so the space components may be **symbolic or
generative** rather than listed: `math:GeneratedSampleSpace` / `math:SymbolicSampleSpace`,
`math:BorelSigmaAlgebra` / `math:PowerSetSigmaAlgebra` / `math:ProductSigmaAlgebra`, and a
`math:MeasureDefinitionExpression` (an AST from the expression layer) that *defines* the measure
rather than tabulating it.

> **Gate (revised).** A `math:ProbabilitySpace` has a sample-space object, a σ-algebra object, and
> a measure object. Those objects **may be symbolic or generative**, but none may be **absent**. The
> gate requires presence and structural completeness, never impossible enumeration.

## Conditioning and independence

Core classes: `math:ConditionalProbability`, `math:IndependenceAssertion`, and
`math:ConditionalIndependenceAssertion`.

Core properties: `math:conditionalOn`, `math:independenceSubject`, `math:independentOf`,
`math:independentGiven`, `math:mutuallyIndependentSet`, and `math:independenceHoldsInModel`.

Conditional probability names *both* the event and the conditioning event or context
(`math:conditionalOn`) — a conditional with an unnamed conditioning context is ill-formed.

Independence is modeled explicitly and in three distinguishable states, because a reasoning engine
behaves very differently across them and must never infer the second or third from silence:

- **Marginal independence** — a `math:IndependenceAssertion` with `math:independenceSubject` and
  `math:independentOf` (or `math:mutuallyIndependentSet` for a joint set).
- **Conditional independence** — a `math:ConditionalIndependenceAssertion` adding
  `math:independentGiven` naming the conditioning set: *A ⫫ B | C*.
- **Not modeled** — the absence of any assertion, which is a real, distinct state and never read as
  independence.

Independence is a **modeler's claim from a vantage**, not a fact of the graph: an independence
assertion is standpoint-scoped (held by whoever asserted the model) and names the model it holds in
(`math:independenceHoldsInModel`), so "who claimed these independent, and in which model" is always
answerable.

## Random variables, distributions, and moments

Core classes: `math:RandomVariable`, `math:Distribution`, `math:DistributionFamily`,
`math:DistributionParameterization`, `math:DistributionParameter`,
`math:DistributionParameterRole`, `math:DistributionSupport`, `math:Moment`,
`math:ExpectedValue`, `math:Variance`, `math:Covariance`, and `math:Correlation`.

Core properties: `math:randomVariableDomain`, `math:randomVariableCodomain`,
`math:hasDistribution`, `math:distributionFamily`, `math:distributionParameterization`,
`math:hasDistributionParameter`, `math:parameterQuantity`, `math:parameterExpression`,
`math:parameterRole`, `math:hasSupport`, and `math:hasMoment`.

A random variable is a measurable map with a declared domain and codomain
(`math:randomVariableDomain`/`math:randomVariableCodomain`) and a probability-space or distribution
context. A distribution carries a support (`math:hasSupport`) and moments (`math:hasMoment`,
specialized as `math:ExpectedValue`, `math:Variance`, `math:Covariance`, `math:Correlation`);
moments are quantity-valued results, not literals.

**Dimensional constraints are carried, not assumed.** A random variable over height and its
distribution's location parameter share height units; a variance parameter carries squared units; a
correlation is dimensionless. These dimensional relations are stated (via the observations spine's
reference frames and `math:quantityDimension` on parameter roles) so a mis-dimensioned parameter is
a caught error, not a silent one.

### Mandatory, correctly-factored parameterization

Distribution parameterization is **explicit and mandatory**, because many families have several
conventional forms and a bare family label silently hides which one is meant. The parameter
apparatus is factored across three distinct properties — the earlier single `parameterValue`
overload is removed:

- `math:hasDistributionParameter` — Distribution → `math:DistributionParameter`.
- `math:parameterQuantity` — `math:DistributionParameter` → `math:Quantity` (a numeric parameter).
- `math:parameterExpression` — `math:DistributionParameter` → `math:MathematicalExpression` (a
  symbolic parameter).
- `math:parameterRole` — `math:DistributionParameter` → `math:DistributionParameterRole`.

Roles are **scoped by the parameterization**, so "scale" cannot silently mean standard deviation,
variance, rate, or precision across families:

```ttl
ex:normalMeanStddevParameterization
    a math:DistributionParameterization ;
    math:requiresParameterRole ex:normalMeanRole , ex:normalStddevRole .

ex:normalStddevRole
    a math:DistributionParameterRole ;
    math:roleWithinParameterization ex:normalMeanStddevParameterization ;
    math:requiresPositiveValue true ;
    math:quantityDimension ex:sameDimensionAsRandomVariable .

ex:normalDist1
    a math:Distribution ;
    math:distributionFamily math:normalDistributionFamily ;
    math:distributionParameterization ex:normalMeanStddevParameterization ;
    math:hasSupport math:realLineSupport ;
    math:hasDistributionParameter ex:normalMu , ex:normalSigma .

ex:normalMu
    a math:DistributionParameter ;
    math:parameterRole ex:normalMeanRole ;
    math:parameterQuantity ex:muQuantity .

ex:normalSigma
    a math:DistributionParameter ;
    math:parameterRole ex:normalStddevRole ;
    math:parameterQuantity ex:sigmaQuantity .
```

> **Hard-fail rules.**
>
> - A `math:Distribution` without a `math:DistributionFamily` is ill-formed.
> - A parametric `math:Distribution` without a `math:DistributionParameterization` is ill-formed.
> - A parameterization declares its required roles (`math:requiresParameterRole`).
> - A distribution supplies each required role exactly once (`math:parameterRole`), and each
>   parameter's quantity satisfies the role's positivity and dimension constraints.
> - Silent default parameterizations are forbidden.
> - Reparameterization is a **declared transform** (a `math:MathematicalExpression`), not a string
>   rewrite — the normal-by-variance form links to the normal-by-stddev form through an explicit
>   σ = √(σ²) transform, not by re-labelling.

## Probability value representation

A probability value is a quantity/result object, never a bare literal, and a `math:ProbabilityValue`
is **always** in `[0, 1]`. Odds and log-odds are *not* probabilities; they are probability-scale
transforms and are modeled as their own kinds:

```text
math:ProbabilityValue          # always in [0,1]
math:OddsValue                 # (0, ∞)
math:LogOddsValue              # (-∞, ∞)
math:ProbabilityScaleTransform # links a transform value back to its probability
math:transformsToProbabilityValue
```

This keeps the `[0,1]` gate a genuine hard fail while still supporting lawful scales, and it prevents
the downstream bug where a consumer sees `math:ProbabilityValue` and (correctly) assumes a closed
unit interval, only to be handed a log-odds.

```ttl
ex:pRainTomorrow
    a math:Quantity , math:ProbabilityValue ;
    math:quantityValue "0.72"^^xsd:decimal ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:weatherForecastProbabilityFrame ;
    gmeow:isResultOf ex:forecastRun2026_07_03 .

ex:forecastRainEvent
    a math:ProbabilityEvent ;
    math:eventInSpace ex:tomorrowWeatherProbabilitySpace .

ex:forecastProbabilityObservation
    a gmeow:Observation ;
    gmeow:vantage ex:weatherModelV17 ;
    gmeow:observedFeature ex:forecastRainEvent ;
    gmeow:observationResult ex:pRainTomorrow ;
    gmeow:observationType gmeow:observationTypeDerived .
```

> **Gates.**
>
> - A `math:ProbabilityValue` lies in `[0, 1]`; odds/log-odds are `math:OddsValue`/
>   `math:LogOddsValue`, disjoint from `math:ProbabilityValue`.
> - Probability values name their probability frame or model.
> - Probability values are **never** inferred from confidence unless an explicit mapping is
>   declared — the `logic:` probability/confidence boundary is enforced here, not eroded.

## Dependency models and the `logic:` seam

Core classes: `math:FullIndependenceModel`, `math:JointProbabilityTable`,
`math:BayesianNetwork`, `math:FactorGraph`, `math:MarkovKernel`, `math:StochasticProcess`,
`math:PriorDistribution`, `math:LikelihoodFunction`, and `math:PosteriorDistribution`.

Core properties: `math:hasPrior`, `math:hasLikelihood`, `math:hasPosterior`, and
`math:dependencyGraph`.

The probability layer exposes objects that lower into the `logic:` probability-model requirement.
The lowering is a formal, preservation-judged map — not the soft "satisfy or project into" of a
first draft — so the implementation does not invent the seam ad hoc:

| Mathematics object | `logic:` target | Exact? | Required completeness gate |
|---|---|---|---|
| `math:FullIndependenceModel` | `logic:FullIndependence` | yes | all probabilistic facts named |
| `math:JointProbabilityTable` | `logic:DependencyModel` + `logic:JointOutcome` set | yes if exhaustive | outcomes present and mass sums to one |
| `math:BayesianNetwork` | `logic:DependencyModel` (WMC factorization) | conditional | DAG; CPT completeness |
| `math:FactorGraph` | `logic:DependencyModel` (weighted factor product) | conditional | finite factors; normalized or declared unnormalized |
| `math:MarkovKernel` | `logic:GenericDependence` (stochastic transition) | conditional | domain/codomain; kernel totality |
| approximate model | bounded approximate result | no | explicit approximation/loss policy |

The exactness column is a `logic:preservationKind` value
([`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)); a "conditional" row is exact only when
its completeness gate holds, and lossy otherwise with its drops enumerated.

The Bayesian update chain is first-class: `math:PriorDistribution`, `math:LikelihoodFunction`, and
`math:PosteriorDistribution` are distinct objects linked by
`math:hasPrior`/`math:hasLikelihood`/`math:hasPosterior`, so a posterior always answers *from what
prior, under what likelihood, over what data*. Where the posterior is sampled rather than
closed-form, the operational Bayesian objects are modeled too — `math:PosteriorSample`,
`math:MCMCRun` (an `gmeow:Activity`), and the chain diagnostics `math:ChainDiagnostic`,
`math:EffectiveSampleSize`, `math:RHat`, and `math:DivergenceDiagnostic` — so "is this posterior
trustworthy" is answerable, not assumed.

> A probabilistic reasoning request that references probabilistic facts points at an explicit
> probability-model object with a declared `logic:` lowering. If the model is absent, structurally
> incomplete, or its lowering undeclared, the engine reports unsupported / not-evaluated rather than
> assuming independence.

## Shape and lint gates

The probability gates are catalogued with their gate kinds and failure classes in
[`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md). In summary: probability values in
`[0, 1]` (odds/log-odds disjoint); values not inferred from confidence; probability spaces
structurally complete (symbolic allowed); random variables scoped with dimensions; distributions
carry family and parameterization with each required role supplied exactly once under its
constraints; conditional and conditional-independence assertions name their conditioning set;
dependency-model declarations structurally complete; and every reasoning-facing model declaring its
`logic:` lowering.

## Competency questions

The probability layer is accepted only when it can answer these structurally:

1. What probability space does this probability value belong to, and is its σ-algebra extensional or
   symbolic?
2. What event (set of outcomes, not occurrence) is this probability about?
3. What random variable and distribution underlie this statement, and are their dimensions
   consistent?
4. Which distribution parameterization is being used, and which role does each parameter fill?
5. Are two events asserted independent, conditionally independent (given what), or not modeled — and
   who asserted it, in which model?
6. What explicit probability model is available for a given probabilistic reasoning request, and
   what is its `logic:` lowering and preservation?
7. Which claims attempted to use confidence as probability without an explicit declared mapping?

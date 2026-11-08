<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — The Statistics Layer

> The **statistics charter** of the GMEOW Mathematics design set: statistical study objects,
> procedures, model artifacts, and results, held apart from the processes that produce them and the
> claims that assert them. It makes precise the manifesto's thesis
> ([`MATHEMATICS.md`](MATHEMATICS.md)) that a statistical estimate is not a bare number but the
> provenance-heavy result of an inference act. It builds on the probability layer
> ([`MATHEMATICS-PROBABILITY.md`](MATHEMATICS-PROBABILITY.md)) and the object/expression core
> ([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md)); its gates are in
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md) and its Data-Cube lowering in
> [`MATHEMATICS-PROJECTIONS.md`](MATHEMATICS-PROJECTIONS.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's canonical `module.ttl` axioms and `logic:Constraint` records, competency queries, and the
> projection loss ledger.

## Purpose

The statistics layer models statistical study objects, procedures, model artifacts, and results, so
GMEOW can say not only "p = 0.03" but the whole chain that makes that number a *statistical result*:
the estimand and hypothesis, the model or test, the data and sampling frame, the assumptions, the
estimated parameters, the uncertainty measure, the interval policy, the analysis plan, and the
process and vantage that produced it.

The layer keeps three things that novices conflate strictly apart: the **process** (an activity that
ran), the **result object** (an estimate, p-value, posterior), and the **held claim** (an
observation from a vantage). This separation is gated
([`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)).

## Study and data

Core classes: `math:StatisticalPopulation`, `math:SamplingFrame`, `math:Sample`,
`math:ObservationUnit`, `math:StatisticalVariable`, and `math:DatasetMatrix`.

Core properties: `math:population`, `math:samplingFrame`, `math:sampleOf`,
`math:observationUnit`, and `math:statisticalVariable`.

The study spine separates the *target of inference* from the *thing actually measured*: a
`math:StatisticalPopulation` is the population an estimate refers to; a `math:SamplingFrame` is
the operational list the sample is drawn from; a `math:Sample` is drawn (`math:sampleOf`) from the
frame; a `math:ObservationUnit` is what a row denotes; and a `math:DatasetMatrix` is the data
matrix (held by reference, never inlined — [`MATHEMATICS-RUNTIME.md`](MATHEMATICS-RUNTIME.md)).
Keeping population, frame, and sample distinct is what lets the layer evaluate coverage and
selection bias rather than conflating "who we care about" with "who we measured".

### Variables, scales, and data integrity

A `math:StatisticalVariable` is not just a column name; the layer models its role, scale, and
transformations so the canonical form is richer than any Data-Cube export (otherwise the projection
would be *easier* than the source, which is backwards):

- `math:VariableRole` — outcome, predictor, covariate, weight, stratum, cluster, offset.
- `math:MeasurementScale` — nominal, ordinal, interval, ratio, count, compositional.
- `math:UnitPolicy` and `math:MissingValueCode` / `math:MissingnessIndicator` — how values and
  their absence are encoded.
- `math:SamplingWeight`, `math:Stratum`, `math:Cluster`, `math:SurveyDesign` — the design that
  makes a sample analyzable as more than a simple random sample.
- `math:DataTransformation` / `math:DerivedVariable` — a derived column is the declared result of
  a transformation (an expression AST), not an opaque new field.

## Models and estimation

Core classes: `math:StatisticalModel`, `math:ModelFormula`, `math:ModelAssumption`,
`math:Estimand`, `math:Estimator`, `math:Statistic`, `math:Estimate`, `math:FittedModel`, and
`math:InferenceRun`.

Core properties: `math:modelFormula`, `math:modelAssumption`, `math:fittedToData`,
`math:estimatesEstimand`, `math:estimatedParameter`, `math:estimator`, and `math:estimateValue`.

A `math:StatisticalModel` records its model expression as a structured `math:ModelFormula`, a
specialization of `math:MathematicalExpression`. A formula may have a response or be one-sided;
the R tilde representation uses `math:BindingExpression` and indexed `math:ArgumentSlot` operands.
The general formula frame and supplied conformance examples inspect slot structure, not complete
binder admission or preservation of every source role. A formula alone does not supply the full
distributional specification, family/link choices, contrasts, weights, subsets or fitting options.
Assumptions are first-class `math:ModelAssumption` objects (normality, homoscedasticity, independence
of errors). The stated conditions can be cited and, where suitable methods and evidence are
available, assessed or reported violated rather than left implicit. Naming a condition does not
establish its empirical testability or truth.

A model described by a source lift is not evidence of an executed fitting process. Likewise, a
`math:DatasetMatrix` identifier, type, label or ingest provenance does not by itself establish an
authenticated external blob reference, available payload or resolved column schema. Preserve the
actual supplied specification, data reference and source distinctions, and qualify the selected
operation separately; passing a required-link check does not establish numerical fit validity.

**Estimand vs parameter.** The layer distinguishes the target a study seeks to learn from a
model quantity named in an estimate. A model parameter may itself be the specified estimand,
but a coefficient name does not automatically identify a substantive population or causal target.
Supplying both target links does not establish equality or identification between their objects.
Population, outcome, contrast, time, intervention and missing-data policy provide relevant
causal-study framing, not universal mathematical prerequisites or a demonstrated six-role
admission gate for every estimand. Such declarations alone prove neither causal identification
nor valid treatment of missing data or intercurrent events. The vocabulary offers these roles:

```text
math:Estimand
  math:targetPopulation
  math:targetOutcome
  math:targetContrast
  math:targetTime
  math:interventionCondition
  math:missingDataPolicy
```

A `math:Estimator` is the estimation procedure; a `math:Statistic` is a data function defined
without unknown model parameters. The same function can have both roles; distinguish it from
its realized value on particular data. A `math:Estimate` records a result with an estimand
(`math:estimatesEstimand`) or model-parameter target (`math:estimatedParameter`), or both, and a
separately required estimator (`math:estimator`). The inspected target and method requirements do
not establish a universal `math:estimateValue`-presence check or numerical evaluation. A
`math:FittedModel` is the data-bound model (`math:fittedToData`), and a `math:InferenceRun` is the
`gmeow:Activity` that applies a procedure; the procedure is not that activity or a held claim.

The native R source lift can emit estimated-parameter and estimator records without a numerical
coefficient value, a framed study estimand or a completed fitting run. Formula-term labels do not
establish actual design-matrix coefficients, contrast coding, rank or option-sensitive estimator
identity. Preserve the supplied source and result evidence and its limits without inventing values
or treating source-described links as executed estimation.

## Inference and results

Core classes: `math:Hypothesis`, `math:NullHypothesis`, `math:AlternativeHypothesis`,
`math:HypothesisTest`, `math:PValue`, `math:ConfidenceInterval`, `math:CredibleInterval`,
`math:PredictionInterval`, `math:ToleranceInterval`, `math:ConfidenceRegion`,
`math:CredibleRegion`, `math:EffectSize`, `math:Prediction`, `math:Residual`, and
`math:ModelDiagnostic`.

Core properties: `math:nullHypothesis`, `math:alternativeHypothesis`, `math:testStatistic`,
`math:nullDistribution`, `math:tailDefinition`, `math:alternativeSidedness`,
`math:pValueAdjustment`, `math:multipleComparisonFamily`, `math:pValue`, `math:effectSize`,
`math:hasIntervalBound`, `math:confidenceLevel`, `math:credibleMass`, `math:predictionTarget`,
`math:residualOf`, and `math:diagnosticFor`.

A `math:HypothesisTest` binds a `math:NullHypothesis` and a `math:AlternativeHypothesis` and a
test statistic (`math:testStatistic`), and produces a `math:PValue` — a result object with enough
structure to be unambiguous. A p-value carries not only its test but its **null distribution**
(`math:nullDistribution`), its **tail definition** and **sidedness**
(`math:tailDefinition`/`math:alternativeSidedness` — one-sided, two-sided, greater, less, exact,
mid-p), and any **multiplicity adjustment** (`math:pValueAdjustment`/`math:multipleComparisonFamily`),
because a p-value with a null and a statistic but no tail or sidedness is still ambiguous.

Uncertainty is not always a scalar interval: alongside `math:ConfidenceInterval` (with
`math:confidenceLevel`) and `math:CredibleInterval` (with `math:credibleMass`), the layer carries
`math:PredictionInterval`, `math:ToleranceInterval`, and the multivariate `math:ConfidenceRegion`
and `math:CredibleRegion`. A `math:ModelDiagnostic` names the fitted model it evaluates
(`math:diagnosticFor`) and its method (`math:diagnosticMethod`). The canonical required-link
restrictions and closed-world closure require both roles; missing either is
`math:UnanchoredDiagnostic`. These anchors do not establish that a check executed, produced a
numerical result or verified an assumption. Explicitly naming a `math:ModelAssumption` makes the
condition identifiable; it does not make every condition empirically testable from the observed
data.

A raw `math:Residual` uses observed minus predicted. Other residual conventions require their own
source meaning; a residual record and its `math:residualOf` fit link do not supply observation
indices, numerical differences, a variant or a completed diagnostic. Preserve the actual source
convention, values and assessment evidence where supplied without inventing them.

### Model comparison and calibration

Comparing and calibrating models is first-class: `math:ModelComparison`,
`math:InformationCriterion` (AIC/BIC/WAIC/…), `math:CrossValidationResult`, and
`math:CalibrationDiagnostic`, so "which model, and how well calibrated" is answerable rather than
implicit.

Two senses of "calibration" must not be conflated. `math:CalibrationDiagnostic` — designed in this
charter, not yet minted in `module.ttl` — is the **statistical** sense: how well a fitted model's
predicted probabilities match observed frequencies (reliability curves, Brier decomposition, expected
calibration error) over held-out data. `math:StabilityCalibrationRecord`
([`MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md`](MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md)) is the
**topological** sense: the derivation record turning the persistence of a feature into a warranted
`logic:confidence` on a latent-meaning claim, underwritten by the bottleneck stability theorem. The
first calibrates a *model's probabilities against data*; the second calibrates a *credence against a
stability bound*. They share the word and nothing else: one is a `gmeow:Activity`/diagnostic over a
`math:FittedModel`, the other a `math:MathematicalObject` over a `math:PersistenceLifetime`, and a
consumer must select on the specific class, never on the shared token "calibration".

## Design and integrity

Core classes: `math:ExperimentalDesign`, `math:RandomizationScheme`,
`math:MissingnessMechanism`, and `math:AnalysisPlan`.

Core property: `math:analysisPlanVersion`.

A `math:ExperimentalDesign` and its `math:RandomizationScheme` record how units were assigned; a
`math:MissingnessMechanism` records MCAR/MAR/MNAR where an analysis depends on it; and a
`math:AnalysisPlan` (with `math:analysisPlanVersion`) records the pre-specified plan a result
claims to follow — so pre-registered versus post-hoc is data, not lost context.

## Statistical result representation — process, result, and claim

A statistical result is three distinct objects: the **process** that ran, the **result object** it
produced, and the **held claim** from a vantage. None is typed as another — an inference run is a
`gmeow:Activity`, not a `gmeow:Observation`:

```ttl
# process
ex:analysisActivity
    a math:InferenceRun , gmeow:Activity ;
    math:fittedToData ex:trialDatasetV3 ;
    math:usedAnalysisPlan ex:planV2 ;
    gmeow:usedSoftwareCommit ex:analysisPipelineCommitA1B2 .

# result object
ex:treatmentEffectEstimate
    a math:Estimate ;
    math:estimatesEstimand ex:averageTreatmentEffectEstimand ;
    math:estimatedParameter ex:betaTreatment ;
    math:estimateValue ex:treatmentEffectQuantity ;
    math:estimator ex:olsEstimator .

ex:treatmentEffectQuantity
    a math:Quantity ;
    math:quantityValue "1.42"^^xsd:decimal ;
    math:hasDimension math:dimensionless ;
    gmeow:unit ex:outcomeScoreUnit ;
    gmeow:hasReferenceFrame ex:trialOutcomeScale .

# held claim
ex:treatmentEffectObservation
    a gmeow:Observation ;
    gmeow:vantage ex:analysisPipelineCommitA1B2 ;
    gmeow:observedFeature ex:averageTreatmentEffectEstimand ;
    gmeow:observationResult ex:treatmentEffectEstimate ;
    gmeow:wasGeneratedBy ex:analysisActivity .
```

The run is the process (`gmeow:Activity`); the estimate is the structured result linked to its
estimand, parameter, and estimator; the observation is the held claim with a vantage and provenance.

> **Hard rule.** A statistical number without a method, data provenance, and interpretation frame is
> not a statistical result. It is at most an uninterpreted scalar quantity — and it is not an
> observation, an activity, and a result all at once.

## Frequentist and Bayesian parity

Both families are first-class and neither is privileged. The frequentist result kinds — estimator,
sampling distribution, test statistic, null/alternative hypothesis, p-value, confidence interval,
power, Type I/II error rates — and the Bayesian result kinds — prior, likelihood, posterior,
posterior-predictive distribution, credible interval, Bayes factor, posterior probability, and the
operational MCMC diagnostics from the probability layer — coexist.

> **Hard rule.** Do not collapse confidence intervals and credible intervals. They are different
> result kinds with different semantics and required frames — a `math:ConfidenceInterval` carries a
> `math:confidenceLevel` over a sampling procedure; a `math:CredibleInterval` carries a
> `math:credibleMass` over a posterior. They are held disjoint, and a projection that renders one
> as the other records the loss.

## Shape and lint gates

The statistics gates are catalogued with their gate kinds and failure classes in
[`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md): an estimate references its estimand
and/or parameter and its estimator; a fitted model references data and specification; a p-value
references a test, null hypothesis, test statistic, null distribution, and tail/sidedness; intervals
carry bounds and level/mass and are kind-disjoint; effect sizes identify contrast/scale/frame;
diagnostics name their fitted model; and missingness is explicit where an analysis depends on it.

## Competency questions

The statistics layer is accepted only when it can answer these structurally:

1. What population and sample does this estimate refer to, and what estimand does it target versus
   which model parameter does it estimate?
2. Which data, model, estimator, and assumptions produced this result — and by which process?
3. What hypothesis, null distribution, and tail/sidedness produced this p-value, and was it adjusted
   for multiplicity?
4. Is an interval a confidence, credible, prediction, or tolerance interval (or a region), and what
   level or mass does it carry?
5. Which model diagnostics, comparisons, and calibration results evaluate this fitted model?
6. What projection loss occurs when this result is exported to RDF Data Cube?
   (Answered jointly with [`MATHEMATICS-PROJECTIONS.md`](MATHEMATICS-PROJECTIONS.md).)

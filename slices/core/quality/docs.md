<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Quality — data-quality claims as ordinary observations

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/quality` · **tier: core**
> The ISO 19157 dimensions, riding the universal Observation stack — quality is a claim like any other.

Data quality is usually bolted on as a separate report format. GMEOW refuses the bolt-on:
a quality assessment is a *reified observation about an entity*, and so it reuses the
universal Observation stack wholesale — the assessed entity is the `observedFeature`, the
quality result is the `observationResult` (a quantity, an estimate, or a categorical verdict), and the
assessment protocol is the `observationMethod`. The result therefore carries unit,
reference frame, determinacy, and provenance in the same bundle as every other GMEOW
measurement (Principle 11). Explicit quantitative and aggregate subkinds select
stronger result contracts without narrowing ordinary categorical assessments.

The dimensions themselves are ISO 19157's — positional accuracy, temporal accuracy,
thematic accuracy, completeness, logical consistency, topological consistency — plus
lineage. W3C DQV and GeoDCAT-AP are aligned by reference (Principle 5); computing any
quality metric is solver-layer work (Principle 12).

## The core construct

### gmeow:QualityAssessment

A reified assessment of the quality of an entity or dataset — a `logic:SubKind` of
`gmeow:Observation`, so everything the observations slice provides (vantage, method,
result, frames) applies unchanged. The result is typically a scalar quantity (accuracy in
metres, completeness as a normalized dimensionless ratio) or a categorical conformance statement. The
canonical axiom guarantees every assessment assesses *at least one* entity.
Closed-world requirements are authored as `logic:` closure metadata; SHACL is
their generated validation view.
The `assessedEntity` existential does not itself require an explicitly asserted
value in validation; supplied values must be entities.

### Quantitative and aggregate results

Use `gmeow:QuantitativeQualityAssessment` for measured magnitudes. It requires
at least one `observationResult`, and every result is a `math:Quantity`. Each
quantity carries its own `math:hasDimension`; a completeness ratio is dimensionless,
while a position error may be a length. Units and reference frames retain their
ordinary observation semantics. The seven broad quality categories do not acquire
fixed physical dimensions.

Use `gmeow:AggregateQualityAssessment` for estimated aggregate scores. Each required
result is a `math:Estimate` with `math:estimateValue` quantities, declared
`math:Estimator` procedures through `math:estimator`, and a target through
`math:estimatesEstimand` or `math:estimatedParameter`. Every supplied value and
estimator must have the appropriate type; one correct value cannot conceal a
malformed extra value. Quantity dimensions remain the mathematical owner's
invariant. The canonical `gmeow:AggregateQualityResultsConstraint` supplies the
cross-node integrity law and its generated procedural validation view.

The assessment is the held observation, the estimate is its result, the estimator
is a procedure, and the estimand or parameter is its target. These objects are
never collapsed. Multiple estimates can coexist with their own procedures and
targets. No Cartesian pairing between a report's dimension classifications and
its result values is inferred. Split direct and estimated results into separate
assessments of the same entity when their result contracts differ.

Categorical verdicts remain ordinary `QualityAssessment` results. Neither new
subkind classifies every positional, temporal, or completeness assessment as
quantitative. See [dataset-completeness.ttl](examples/dataset-completeness.ttl)
and [aggregate-and-categorical.ttl](examples/aggregate-and-categorical.ttl).

### gmeow:assessedEntity

The entity whose data quality is assessed — the feature of interest of the quality
observation, declared `rdfs:subPropertyOf gmeow:observedFeature`. That subsumption is the
load-bearing move: a generic consumer can query "all observations about Alice" and get
names, coordinates, *and* quality assessments without knowing this slice exists.

### gmeow:qualityDimension

The dimension under which the assessment is made. Non-functional twice over: one report
may evaluate several dimensions, and competing dimension classifications coexist rather
than collapse (Principle 9).

## The dimension vocabulary

### gmeow:QualityDimension

An open value vocabulary of individuals — never subclasses (Principle 9). The seeds are
the ISO 19157 set (`qualityDimensionPositionalAccuracy`, `qualityDimensionTemporalAccuracy`,
`qualityDimensionThematicAccuracy`, `qualityDimensionCompleteness`,
`qualityDimensionLogicalConsistency`, `qualityDimensionTopologicalConsistency`) plus
lineage; a domain-specific "semantic consistency" or "usability" is added by minting a
fresh individual, not a class.

### gmeow:qualityDimensionLineage

The one seed that deserves its own note, because it is a *bridge*, not a duplicate:
structural lineage already lives in the provenance module (`wasGeneratedBy`,
`wasDerivedFrom`, `ImportActivity`). This dimension value marks an assessment that
*evaluates* lineage completeness or correctness — provenance viewed through a quality
lens, with the provenance graph itself untouched (Principle 4: one canonical source).

## Reuse, not redefinition

The module mints no properties for axes that already exist — `observationResult`,
`observationMethod`, and `vantage` come from observations; `confidence` and
`wasGeneratedBy` from provenance; `hasDeterminacy` from the kernel; `unit` from observations;
`hasReferenceFrame` from places; `validFrom`/`validUntil` from temporal. The reuse
declaration in `module.ttl` is documentation of that fact, and the discipline is
constitutional (Principle 4): a quality assessment's confidence is the *same* confidence
every other claim carries, queryable the same way.

## Boundaries

The slice records quality *claims*; it computes nothing. Conformance evaluation,
accuracy statistics, completeness ratios, and the coverage gates that consume these
assessments are solver-layer machinery (Principle 12). Alignment to W3C DQV
(`dqv:QualityMeasurement`, `dqv:Dimension`) and GeoDCAT-AP is by reference and lossy
projection — DQV has no standpoint, no determinacy, and no frame on its measurements,
which is precisely what the Observation stack adds. Depends on logic, math, kernel and observations;
consumed by data-quality claims over any slice's instance data.

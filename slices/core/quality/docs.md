<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Quality — data-quality claims as ordinary observations

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/quality` · **tier: core**
> The ISO 19157 dimensions, riding the universal Observation stack — quality is a claim like any other.

Data quality is usually bolted on as a separate report format. GMEOW refuses the bolt-on:
a quality assessment is a *reified observation about an entity*, and so it reuses the
universal Observation stack wholesale — the assessed entity is the `observedFeature`, the
quality result is the `observationResult` (typically a `math:Quantity`), and the
assessment protocol is the `observationMethod`. The result therefore carries unit,
reference frame, determinacy, and provenance in the same bundle as every other GMEOW
measurement (Principle 11), and the quality layer stays thin: one SubKind, two
properties, one open dimension vocabulary.

The dimensions themselves are ISO 19157's — positional accuracy, temporal accuracy,
thematic accuracy, completeness, logical consistency, topological consistency — plus
lineage. W3C DQV and GeoDCAT-AP are aligned by reference (Principle 5); computing any
quality metric is solver-layer work (Principle 12).

## The core construct

### gmeow:QualityAssessment

A reified assessment of the quality of an entity or dataset — a `gufo:SubKind` of
`gmeow:Observation`, so everything the observations slice provides (vantage, method,
result, frames) applies unchanged. The result is typically a scalar quantity (accuracy in
metres, completeness as a normalized dimensionless ratio) or a categorical conformance statement. The
EL-visible axiom guarantees every assessment assesses *at least one* entity; closed-world
cardinality is SHACL's concern, never OWL's.

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
which is precisely what the Observation stack adds. Depends on kernel and observations;
consumed by data-quality claims over any slice's instance data.

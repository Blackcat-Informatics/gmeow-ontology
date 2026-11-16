BLOCKED at 49e2d94d8

# Review: issue 1893 unit at 49e2d94d8, and the math grounding batch plan

## Scope

- PART 2: worktree `.worktrees/1893-ground-quantitative-quality-assessments`, branch
  `paudley/1893-ground-quantitative-quality-assessments`, head 49e2d94d8, diff `origin/main...HEAD`
  (merge base and origin/main are both 279ce7e40, confirmed against the remote). Five files, all under
  `slices/core/quality`.
- PART 1: `.stage/issue-1893/plan.md` against origin/main 279ce7e40 and issues 1893 to 1897, read in full with
  comments through `gh issue view --json body,comments`.
- Read-only. No gate, producer, reasoning, cargo or make command was run.
- Lock: the first command (`git fetch origin main`) ran under `flock .stage/build.lock` and waited behind a
  nextest run. Plain reads (`git diff`, `grep`, `sed`, `gh issue view`) were then run without the lock, because
  each would have queued behind unrelated builds. No build tool ran outside the lock.

## PART 2: the 1893 unit

### Evidence read

- `.stage/issue-1893/`: `lint.log` (EXIT=0, clean tree at 49e2d94d8), `rapper.log` (four files, EXIT=0),
  `prose.log` (EXIT=0), `msgid.log`, `floors.log`, `floors-docmat-generic.log`, `hand-eval.md`, `checks.sh`.
- Harness: `crates/slicetest/src/exec.rs`, `stores.rs`, `native_query.rs`, `paths.rs`, `dsl.rs`;
  `crates/logic-compile/src/projections/reader_view.rs`, `result_shape.rs`.
- Slice: `module.ttl`, `docs.md`, `manifest.ttl`, both tests, all three examples, the fixture.
- Definitions read: `gmeow:Observation`, `observationResult`, `observationMethod`, `observedFeature`, `vantage`,
  `unit` (observations module); `math:Quantity`, `hasDimension`, `quantityValue`, `lengthDimension`,
  `dimensionless` (math module).

### Worked by hand, then compared with hand-eval.md

Competency questions run over the asserted merged store (`ontology/gmeow.ttl` plus every slice `module.ttl`,
`stores.rs` 284-313) unioned with the data file (`exec.rs` 333-355). No module in that store types an instance
of any of the three assessment classes, so only the data file contributes rows.

| Cell | My result | Can fail | Prefixes | Agrees with hand-eval |
|---|---|---|---|---|
| cqQuantitativeResultCarriesDimensionAndValue | one row (math:dimensionless, "0.985"^^xsd:decimal); expected row present | yes, by a missing row | g, m declared | outcome yes |
| cqCategoricalVerdictIsNotAQuantity | one row (aggregate/conforms) | yes, by a missing row | g, m declared | outcome yes |
| cqMultiDimensionReportKeepsEveryClassification | two rows (positional accuracy, completeness) | yes | g declared | outcome yes |
| cqMultiResultReportKeepsEveryDimensionedQuantity | two rows (lengthDimension, dimensionless) | yes | g, m declared | outcome yes |
| saAggregateConstraintNamesEveryRequiredRelation | ASK true on the committed module | only for the two target relations; see R2 | g, l, m declared | no: the stated failing input does not fail |
| saQualityDimensionIsNotAQuantityDimension | ASK false on the module; true on module plus witness, so the ban trips | yes | g, m declared | yes |

- No new question sets `gmeow:cqReasoning`, so none pairs `gmeow:cqDataFile` with a reasoning lane
  (`exec.rs` 336-346 would refuse it).
- The coder's doubt is resolved from source. Module scope loads `module.ttl` alone (`exec.rs` 630-643) and
  passes it through `with_owl_rdfs_projection`, which is dual-write: `reader_view.rs` line 176 pushes every
  source quad unchanged after any projected form. The raw `logic:integrity`, `logic:and`, `logic:forall`,
  `logic:exists`, `logic:antecedent`, `logic:consequent` and `logic:relation` triples are therefore present,
  with blank identity preserved (one file, one scope). `slices/core/inquiry/tests/structural.ttl` 400-411
  already reads the same alternation path over an integrity tree at module scope on main.
- Fail witness: `tests/counter-examples/` is the directory of all 18 existing `gmeow:saFailWitness` fixtures.
  `module.ttl` 101-102 types `gmeow:qualityDimensionCompleteness` as `gmeow:QualityDimension`; the fixture adds
  `math:hasDimension`, so the ASK holds over module plus witness (`exec.rs` 673-708). It declares no shape, so
  the projection-purity scan (`dev_shapes.rs` 382-405) is not engaged.
- Example obligations by hand: both quantitative results are `math:Quantity` with exactly one
  `math:hasDimension` (math module 12962-12967); at least one result (closure entry, `module.ttl` 38-39); one
  `assessedEntity`, so the functional `observedFeature` holds; one `observationMethod`;
  `observationResult` is declared non-functional (observations module 492-503); the base assessment's two
  verdicts are `logic:Individual`, as on main. I found no class obligation the example violates.

### Frozen ledgers and pinned lists

None needs an entry. `crates/docs/tests/fixtures/reviewed_coverage_golden.json` maps i18n catalogs to
term and predicate pairs; module and catalogs are unchanged. `functional_carrier_ledger.txt` is over module
terms. The slice-spec census (`crates/slicetest/src/repository.rs` 41, 230-237) is computed into the producer
receipt, not committed. No committed file outside `slices/core/quality` names a quality example, question or
assertion except the Rust consumer test and its module selection, which read `module.ttl` only.

### Slice-quality floors (quality slice)

Floors: Gmn1Coverage 1.0, GmnGlyphOptimality 1.0, ShapeMigration 1.0, Documentation 1.0, DocMaturity 0.6667,
TranslationCoverage 0.3333, ProseQuality 0.75. None can fall.

- ProseQuality, TranslationCoverage: inputs are `module.ttl` and `i18n/`, both unchanged.
- ShapeMigration: no `shapes.ttl`, unchanged. GmnGlyphOptimality: reads the lang module, unchanged.
- Documentation: `docs.md` prose grows (6166 to 6365 characters in `floors.log`).
- DocMaturity: identical MISS set before and after in both mirrors; the added fixture, questions and example
  feed existential dimensions only.
- Gmn1Coverage reads `module.ttl` plus `examples/*.ttl` (`crates/slice-quality/src/axes.rs` 1558-1564), so
  the new example is in its denominator. Its quads are default-graph, with IRI and literal terms and no blank
  nodes. `classify_iri` is total and a literal object always classifies (`gmn1_codec.rs` 2217-2256), so every
  new quad is covered and the fraction stays 1.0. This is by reading; the method note calls the axis not
  computable by reading, and I did not run it.

### Required findings

**R1. The new example's length result carries no unit and no reference frame.**
`slices/core/quality/examples/multi-result-multi-dimension.ttl` 28-30: `ex:positionError` is a `math:Quantity`
with `math:lengthDimension` and `math:quantityValue "5.0"`, and nothing says five of what.
- `math:quantityValue` (math module 1303 onward): "A measured quantity also needs its dimension and applicable
  unit or reference frame; a literal by itself supplies none of that context", and its how-to-use says to
  retain "the applicable unit or frame".
- `gmeow:unit` (observations module 716-728) exists "so a framed magnitude is never published with a silent
  unit".
- The slice guide the example is linked from says the result "carries unit, reference frame" (`docs.md` 13)
  and gives "accuracy in metres" (`docs.md` 30).
- Every other measured length in a slice example carries both:
  `slices/grounding/math/examples/projection-requests.ttl` 23-26 (`gmeow:unit <http://qudt.org/vocab/unit/M>`,
  `gmeow:hasReferenceFrame gmeow:referenceFrameSI`) and `external-concept-coverage.ttl` 28-32.
- Correction: give `ex:positionError` a unit and a frame. A unit without a frame is the
  `logic:MeasurementFrameMissing` violation, so both are needed. `gmeow:referenceFrameSI` is declared in the
  places slice, which the quality manifest does not list; check that before choosing the frame term. The
  dimensionless ratio needs neither, as in `dataset-completeness.ttl`.

**R2. `ex:saAggregateConstraintNamesEveryRequiredRelation` does not protect the obligations it names, and the
failing input recorded for it does not fail.**
`slices/core/quality/tests/structural.ttl` 132-145.
- The pattern asks only that some node under the tree has each relation. `math:estimateValue` occurs twice in
  the law: the existence conjunct (`module.ttl` 182) and the antecedent of the typed-value conjunct (185).
  `math:estimator` occurs twice: the existence conjunct (187) and the antecedent of the typed-estimator
  conjunct (190). Both antecedent atoms are reachable through `forall`, `consequent`, `and`, `forall`,
  `antecedent`, `and`.
- `hand-eval.md` item 5 gives "delete the estimator existential from the consequent ... ASK false". The ASK
  stays true, because line 190 still matches.
- Deleting any one of the four conjuncts (value exists, value typed, estimator exists, estimator typed) leaves
  the assertion green. Only the two target relations (192-193), which occur once each, are pinned.
- The rationale says the tree reaches these relations "so each obligation is authored in the canonical core".
  Four of the five obligations can be removed unnoticed.
- Correction: bind the existence atoms as `l:exists` bodies, bind the target pair under one `l:or`, and bind
  the two typing consequents (`rdf:type` with `l:termIri m:Quantity` and `m:Estimator`). Then re-work the
  failing input by hand.

### Optional findings

- **O1.** All four new questions declare `logic:shapeCardinality logic:RowsExact` and omit
  `gmeow:cqExactRows true` (`tests/competency.ttl` 58-141). The harness enforces exactness only from
  `gmeow:cqExactRows` (`dsl.rs` 314, `exec.rs` 492-511); `validate_bindings` enforces only `RowsCount`
  (`result_shape.rs` 517). The questions are containment checks, so an unexpected extra row passes.
  `hand-eval.md` attributes its failures to RowsExact; they fail by a missing row instead. 269 of 299 RowsExact
  shapes in the tree pair the two. Add `gmeow:cqExactRows true` to the four, and to the existing
  `ex:cqAggregateRetainsProcedureValueAndTarget`.
- **O2.** `cqCategoricalVerdictIsNotAQuantity`: its four `FILTER NOT EXISTS` clauses remove nothing in this
  data. In the asserted lane only `ex:categorical` is typed `gmeow:QualityAssessment`, so the question passes
  with the filters deleted. The module-side claim is already held by
  `ex:saCategoricalQualityIsNotUniversallyQuantitative`.
- **O3.** The new example is not bound to any validation run: the slice has no `tests/example-conformance.ttl`,
  and `quality_assessments.rs` has a valid multivalued case for the base class only (line 80). A valid
  two-quantity, two-dimension case there (or a conformance cell) would execute what the example claims.
- **O4.** History wording in the example header, line 10: "which the base class has never required". Prefer
  "does not require".

### Text

Guide paragraph (`docs.md` 64-68) is true of the example, with no history wording and no issue or pull request
number. `prose-check` on `docs.md` is recorded EXIT=0. The commit message carries no process reference.

## PART 1: the plan

Legend: MAIN = delivered on origin/main; UNIT = delivered by the plan's proposed unit; MISSING = in neither.

### Issue 1893 (quality)

| Requirement | Where met | Verdict |
|---|---|---|
| Quantitative contract, results are `math:Quantity` | `slices/core/quality/module.ttl` 10-22, closure 38-39 | MAIN |
| Aggregate contract retains Estimate, values, Estimator, targets | `module.ttl` 24-36, 40-41; law 167-193, a real formula tree | MAIN |
| Categorical verdicts stay valid on the base class | base class has no result restriction (43-59); `tests/structural.ttl` 124-130 | MAIN |
| QualityDimension an open vocabulary; no fixed physical dimension | `module.ttl` 61-72; ban plus witness | MAIN, guard by UNIT |
| Multi-result and multi-dimension stay legal | properties non-functional; Rust case line 80 (base only) | MAIN for base; quantitative case unexecuted (O3) |
| Not conflating assessment with estimate | `math:Estimate logic:disjointWith gmeow:Observation`, math module 8769 | MAIN |
| Reuse substrate, `logic:` constraints, no second shape source | no `shapes.ttl`; module mints no quantity term | MAIN |
| Math references and manifest dependency | `manifest.ttl` 19 | MAIN |
| Term coats and translations | fr and zh carry the three new labels | MAIN (catalog completeness not recounted) |
| Examples | two on main; multi-result example | MAIN and UNIT, subject to R1 |
| Structural and competency coverage | one aggregate question and two class pins on main | UNIT, subject to R2 |
| Produced projections against valid and control cases | `quality_assessments.rs` 57-107, 24 cases plus two missing-dimension checks | MAIN in source; never shown executed. Mistyped dimension and mistyped target controls: MISSING |
| Full gates and source-to-bundle consumer acceptance | none | MISSING |

Closing: no. The issue body makes gates and bundle acceptance mandatory, and its comment keeps it open until
"actual generated/corpus/native/installed acceptance" is shown.

### Issue 1894 (finance)

| Requirement | Where met | Verdict |
|---|---|---|
| Amounts through the quantity substrate, currency a frame, not an SI dimension | observations module 104-112, 1532-1537 (`hasValue math:dimensionless`, `currency` to `ReferenceFrame`) | MAIN |
| Rates | `gmeow:FinancialPeriodRate` 1450, frame constraint 1800, disjoint from `math:ProbabilityValue` 1664 | MAIN |
| Prices | total price roles only; `docs.md` 149-157 states per-unit price is not supplied | MAIN for totals; unit price MISSING |
| Interest and return in math operations | `SimpleInterestCalculation` 1483, `HoldingPeriodReturnCalculation` 1494, signatures 1594, 1605 | MAIN |
| Reuse shared MonetaryAmount | no finance duplicate | MAIN |
| Retire scalar paths with guards | `ex:saNoRetiredMonetaryScalar` (finance `tests/structural.ttl` 385); zero `monetaryValue` in the reviewed-coverage golden | MAIN |
| Manifest dependency | `manifest.ttl` 18 | MAIN |
| Exercise actual consumers, generated constraints, malformed dimensions, frames, computations | `finance_grounding.rs` (three tests), `gmeow finance` command | MAIN in source; execution MISSING |
| Full gates, shipped and installed acceptance | comments record failed hosted runs, with ONNX parity and installed product still open | MISSING |

- The proposed unit (competency questions and signature pins) answers no sentence of this issue, which says
  "source counts alone prove nothing".
- Unit price as a stated boundary: the issue text says "prices" without "unit", so the working assumption does
  not contradict a specific sentence. It does meet "No requirement is waived by this decomposition", so the
  boundary needs the maintainer's own word on the issue.
- Closing: no, not on unit 2.

### Issue 1895 (risk)

| Requirement | Where met | Verdict |
|---|---|---|
| Likelihoods in math with an explicit model, no bare floats | `RiskLikelihoodAssessment` 662-677; `RiskLikelihoodResultsConstraint` 702-719 (formula over `logic:probabilityModel`) | MAIN |
| Prior and Posterior where Bayesian | `examples/bayesian-risk.ttl` | MAIN, example only; no test reads it |
| Quantitative scores in math | `linkStrength` 207 as a framed quantity; `ex:saStrengthHasOneMathematicalOwner` 231 | MAIN |
| Severity and causal modality distinct from probability and confidence | `SeverityLevel` 90 stays a value vocabulary; no disjointness axiom and no test | MAIN by absence of conflation; witness by UNIT |
| Source guards against retired alternatives | `ex:saNoRetiredScalarWeight` 243 | MAIN |
| Manifest dependency | math listed | MAIN |
| Worked examples | three | MAIN |
| Positive and negative production acceptance, full gates | `risk_grounding.rs` in source only | MISSING |

Closing: no. The unit adds witnesses the issue does not ask for; the open sentence is production acceptance.

### Issue 1896 (evidence, trust, confidence)

| Requirement | Where met | Verdict |
|---|---|---|
| Confidence stays on the logic surface, not probability | `logic:confidence` 2908: "is NOT a probability"; math counter-examples `confidence-as-probability.ttl`, `projection-confidence-as-probability.ttl` | MAIN |
| Probabilistic claims require a math probability model | ontology side on main; native query path cannot select a model artifact: `QProbModel`, `crates/logic/src/query_ir.rs` 432-440, has no artifact identity; `dev_logic.rs` 112 | ontology MAIN; native path MISSING |
| Categorical trust and authority distinct | `IntroducerWeight` disjoint from `math:ProbabilityValue`, trust module 714-716, 736 | MAIN; no slice test names it (UNIT) |
| Reuse owners, no manufactured numeric properties | evidence module has zero math references and categorical strengths | MAIN |
| Quantitative claims connect math artifacts; retire scalar paths | trust done. `gmeow:credence` (epistemics 414) is a bare `xsd:decimal` in [0,1]; `gmeow:confidence` (provenance 177) is a bare [0,1] annotation with no stated relation to `logic:confidence` in its module | UNDECIDED: not surfaced by the plan |
| Actual consumers with confidence, probability and model controls | `trust_grounding.rs`; guard test `probabilistic.tests.rs` 135 | partial MAIN; model controls MISSING |
| Distinguish inherited grounding from missing links with evidence | issue comment of 2026-11-03 does this from source | MAIN as an audit |
| Full gates | none | MISSING |

- Working assumption (native model selection split into its own issue): the issue comment places that work
  inside this issue ("The next implementation unit should explicitly select and validate an actual
  mathematical model"), and the body says no requirement is waived. A split is decomposition only if the new
  issue blocks this one; 1896 then stays open until it closes.
- Closing: no, not on unit 4.

### Issue 1897 (model serving)

| Requirement | Where met | Verdict |
|---|---|---|
| Tensors and metrics through math; no parallel tensor vocabulary | model-serving module 422-547 uses `math:WeightTensor`, `TensorComputationGraph`, `LossFunction`, `LearnedModel`, `Dimension` | MAIN |
| Metrics as dimensioned quantities; procedure, result, observation separate | `ModelMetric` 545, `ModelMetricObservation` 547, `ModelMetricResultConstraint` 584 | MAIN |
| No extension-to-extension dependency; manifest lists math | dependencies are core and grounding only | MAIN |
| Generic tensor-shape laws for arbitrary authored RDF | math module 18629-18732: three bounds, a count aggregate, three formula trees, an extent choice | MAIN as source |
| Valid and invalid tensor and metric controls | no shape or axis counter-example in math; model-serving tests never mention math; no model-serving corpus consumer test | MISSING |
| Guard retirement of superseded scalar vocabulary | not located by reading | NOT VERIFIED |
| Execute actual model-serving consumers; native verdicts | none; the datatype-union cardinality failure is owned by issue 1803 and unchecked on current main | MISSING |
| Full gates | none | MISSING |

- The issue comment of 2026-10-21 forbids removing the tensor-shape requirement through a follow-up issue.
  Units 6a and 6b keep it inside 1897, which is consistent.
- Closing: no. Even after unit 6, native verdicts and installed consumers stay open, and the 1803 dependency
  is unsequenced in the plan.

### Judgment of the plan

- The headline holds for canonical source: terms, math dependencies, laws as real formulas and consumer test
  source are on main for all five. "Already mostly on main" is false for acceptance: no issue has execution
  evidence, and each makes that a closing condition.
- The six units are not the right decomposition. Units 2 and 3, and most of unit 4, add slice witnesses that
  the issue text does not request, and close nothing. That is discretionary test growth across four slices and
  four merges. The work that moves these issues is:
  1. one producer and gate pass covering all five clusters, with installed acceptance (one measurement, not
     five);
  2. the native model-selection unit (1896);
  3. tensor-shape and metric controls plus a model-serving consumer (1897), sequenced after the issue 1803
     verdict;
  4. the two small control gaps in 1893.
- "None blocks another" holds for source and not for closure: unit 6 depends on issue 1803.
- Decisions the plan does not surface:
  1. who runs the producer pass, and when, given that all five issues stay open until it runs;
  2. whether `gmeow:credence` and `gmeow:confidence` stay bare scalars or are in scope of 1896;
  3. the status of the hosted failures recorded on 1894 (research-object generation, ONNX parity);
  4. whether the 1803 native cardinality result is a precondition of 1897;
  5. that splitting 1896 leaves it open as a parent.

## Not verified

- Execution of any kind: slice specifications, the Rust consumer tests, `make check`, producer, reasoning,
  installed CLI. By the operator's ruling this is not obtainable in this session.
- That the SPARQL engine evaluates the alternation path as the inquiry precedent implies; read, not run.
- Gmn1Coverage for the quality slice, by reading only.
- Whether a manifest-dependency gate rejects an example term from a slice the manifest does not list (bears
  on the R1 correction).
- Completeness of the fr and zh catalogs beyond the three new labels.
- The relation between `gmeow:confidence` and `logic:confidence` (one reference in the logic module, not
  read).
- Model-serving retirement guards; finance callable-signature placement in `module.ttl` against the
  `mappings/callable-signatures.ttl` convention.
- Current state of the hosted runs cited in the issue 1894 and 1897 comments.

VERDICT: BLOCKED

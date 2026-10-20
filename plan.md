# Signed radial-field value constraints

Implement the full value-validation work in #1657 under Principles 4, 7, 17 and
18. Introduce canonical logic:termIsFiniteNumeric with native finite-numeric
admission and generated SPARQL validation semantics. Preserve arbitrary-precision
integer/decimal values and original RDF identity; refuse text, language literals,
NaN and floating infinities. Add its annotation coat and French/Mandarin catalog
entries together.

Use that predicate in math:ExtendedRealValueConstraint's literal branch. Replace
the insufficient pinned negative-pole measure check with an integrity formula
requiring every result to be PositiveInfinity or a finite non-negative number.
Author explicit conformance fixtures for nonnumeric values, finite negatives,
both infinity poles and positive controls. Do not alter midpoint convexity or
closed-form leaf-linkage policy.

Assess the existing radial scenes' structural, competency and conformance coverage.
The issue explicitly permits the non-flagship surface when sufficient. Retain
the five executable flagship scenarios and state why the two authored scenes are
worked competency examples rather than claim an unimplemented numeric producer.

Test native intrinsic evaluation and generated shape validation against authored
synthetic inputs, including large exact integers/decimals and nonfinite values.
Tests never build a corpus. Run focused Make checks, strict producer-independent
Clippy and normal hooks. Actual whole-corpus acceptance remains a separate
unverified requirement under the CI waiver; do not claim a full gate pass.
Merge through ghprsq and remove the completed worktree/branch.

Implementation is complete for the two value contracts. All 18 selected native
integrity/term-kind/projection controls passed after converting the authored test
data into the validation API's declared N-Triples format. Assertions were retained.
The final shared native-term resolver cleanup passed the final 18-test rerun;
strict producer-independent Clippy passed on the final Rust source. Helper hygiene and corpus purity passed.
Normal hooks passed after removing obsolete task provenance from a touched comment.

The radial scenes retain authored structural, competency and conformance coverage;
they are explicitly documented as worked examples rather than producer-bound
FlagshipScenario registrations, taking the issue's permitted non-flagship option.
Five new isolated counter-examples and the extended positive fixtures are registered
for real corpus conformance. Their production execution and whole-slice acceptance
remain unverified locally, so the issue stays open until actual qualification.

PR #1867 integration: the fetched base has no concurrent source delta and the
predicted merge is clean. The final Rust source passed strict Clippy and all
18 selected tests; helper hygiene, corpus purity and all normal signed hooks
passed. No inline findings or submitted reviews were present at integration;
automatic review was still processing. No independent execution audit is claimed.
CI is waived for this merge. Whole-slice acceptance remains required and unproven.

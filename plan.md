# Intentional finance carrier retirement reconciliation

Refs #1894; Principles 4, 7 and 17.

Hosted production of PR1907 passed producer-build, helpers, lint and rust-prebuild, then failed stage-compile-logic on exactly one completeness discrepancy: retired gmeow:monetaryValue remained in the frozen ledger. Evidence: https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39575061588/job/119149124349 . Current canonical Turtle inputs are unchanged since that run.

Remove only that retired property from the deliberately maintained ledger. Preserve the gate and all other entries. MonetaryAmount retains a decimal all-values restriction, closed required quantityValue path and maximum-one decimal constraint. math:quantityValue is not globally functional and must not gain a global carrier. Correct two obsolete references to a nonexistent pinning test to describe the actual compile_logic integrity owner.

Independent source review confirmed this scope and the stale references. Existing synthetic functional-carrier tests qualify missing/unexpected, duplicate, orphan and reintroduced marker detection. They cannot qualify equality against the full merged corpus. No local corpus production, sync or full gate under the reconfirmed campaign exception. Normal commit/push hooks remain mandatory. Hosted production rerun must establish full corpus equality; issue1894 remains open for all original acceptance.

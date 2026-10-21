# Acceptance index

Source: remove exactly the retired monetaryValue IRI from the global functional-carrier ledger; retain all other entries and integrity checks. Correct obsolete documentation to the actual pipeline integrity owner. No ontology constraints or expectations changed.

PASS: make fmt (terminal exit0); git diff --check; independent source review; exact ledger set-difference/sorted-uniqueness check. Focused functional-carrier synthetic run terminal EXIT0: 8 tests executed, 8 passed, 1040 outside-selection tests skipped. Integrity detection and functional projection controls passed; no full corpus equality claim.

Command: make nextest-synthetic NEXTEST_FILTER='package(gmeow-logic-compile) & test(functional_carrier)' (raw log carrier-tests.log). This uses independent tiny authored graphs and never produces the corpus. It qualifies integrity detection, not full corpus equality.

Hosted evidence: run39575061588 generation(a), job119149124349, reports exactly one missing carrier, monetaryValue. Producer-build/helpers/lint/rust-prebuild passed. Current canonical Turtle inputs have not changed since that run. Full production rerun remains mandatory acceptance but is not a merge prerequisite under the reconfirmed campaign exception. No local corpus/full gate performed. Normal commit hooks completed successfully; normal push hooks completed successfully. Issue1894 remains open.

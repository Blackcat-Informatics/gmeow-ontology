# Retention: `tests/test_cognition.py`

**Category:** Merged-graph guard

## What it tests

Retained guards for the cognition module.

Retained dynamic tests:

- `test_mental_moment_has_exactly_one_gufo_metaclass` — Each new class carries exactly one ontological metaclass.
- `test_cognition_sssom_rows_include_expected_alignments` — The cognition SSSOM ledger contains the expected cross-ontology rows.
- `test_cognition_sssom_includes_corrected_wikidata_qids` — The issue-supplied QIDs were rejected and replaced with verified entities.
- `test_cognition_sssom_includes_opencyc_knows_about` — OpenCyc knowsAbout is present as a relatedMatch anchor.

## Why it cannot be deleted or moved to Rust today

Retained guards for the cognition module.

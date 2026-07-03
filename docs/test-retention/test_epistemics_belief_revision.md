# Retention: `tests/test_epistemics_belief_revision.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Competency test for the doxastic belief-revision pattern.

Retained dynamic tests:

- `test_old_doxastic_tenure_is_closed` — Verify the original doxastic tenure has a single xsd:dateTime end time.
- `test_old_doxastic_tenure_is_suppressed` — Verify the superseded original tenure is marked as not displayable.
- `test_old_doxastic_state_is_retained` — Verify the original belief remains typed and linked to its agent and content.
- `test_new_doxastic_state_is_present` — Verify the revised belief is present as a DoxasticState for the operator.
- `test_new_doxastic_tenure_is_open` — Verify the revised tenure interval has started but has not yet ended.
- `test_qualitative_modality_via_linked_standpoint_claim` — Verify both beliefs link to standpoint claims with the expected modalities.

## Why it cannot move to Rust today

Abox fixture instance checks; python-only file or value inspections.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.

# Retention: `tests/test_email_participant.py`

**Category:** Merged-graph guard

## What it tests

Structural guards for the MessageParticipant relator and EmailAddress facets.

Retained dynamic tests:

- `test_resent_properties_are_multivalued_in_linkml_schema` — Regression guard: non-functional datatype properties must compile to multivalued slots (review feedback).
- `test_fixture_binds_occurrence_correctly` — The coverage fixture shows alice@example.
- `test_fixture_address_decomposition` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.

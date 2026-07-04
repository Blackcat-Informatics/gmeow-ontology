# Retention: `tests/test_disclosure.py`

**Category:** Merged-graph guard

## What it tests

Tests for the consumer projection policy / disclosure control facility.

Retained dynamic tests:

- `test_no_preferred_or_primary_disclosure_term` — No `gmeow:primary*` or `gmeow:preferred*` disclosure term.
- `test_project_when_in_sparql_query` — The schema-org SPARQL projection contains the projectWhen FILTER EXISTS guard.
- `test_public_candidates_query_runnable` — public-candidates.
- `test_privacy_leaks_query_runnable` — privacy-leaks.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.

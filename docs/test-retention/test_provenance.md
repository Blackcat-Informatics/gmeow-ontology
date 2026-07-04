# Retention: `tests/test_provenance.py`

**Category:** Merged-graph guard

## What it tests

Structural guards for the import-provenance / carrier-time slice.

Retained dynamic tests:

- `test_carrier_and_ingestion_props` — Retained dynamic test.
- `test_four_clocks_are_distinct_dated_annotations` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Gmeow:sourceModifiedAt and gmeow:contentDigest are defined in the sources slice, not provenance; cannot be scoped to gmeow:scopeModule for provenance. — gmeow:validFrom, gmeow:validUntil, gmeow:assertedAt, and gmeow:recordedNoLaterThan are all defined in the temporal and sources slices, not provenance.

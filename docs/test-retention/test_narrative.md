# Retention: `tests/test_narrative.py`

**Category:** Merged-graph guard

## What it tests

Narrative reference frame and creative-work sourcing.

Retained dynamic tests:

- `test_narrative_reference_frame_is_not_standpoint_subclass` — gUFO MixIden forbids a sortal from specializing >1 Kind.
- `test_book_release_and_serial_installment_are_creative_works` — Retained dynamic test.
- `test_frame_realm_narrative_and_frame_kind_narrative_exist` — Check that the merged RDF graph declares the narrative frame realm and narrative frame kind individuals.
- `test_reading_order_subclasses_standpoint` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Retained dynamic / SHACL checks -- asserted-TBox invariants whose subjects live in the narrative module have been migrated to slices/extensions/narrative/tests/structural.ttl.

# Retention: `tests/test_creative_works.py`

**Category:** Merged-graph guard

## What it tests

WEMI creative-works spine.

Retained dynamic tests:

- `test_wemi_tiers_subclass_information_object` — Verify each WEMI tier class is a subclass (transitively) of InformationObject.

## Why it cannot be deleted or moved to Rust today

: uses transitive_objects().

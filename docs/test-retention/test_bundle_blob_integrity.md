# Retention: `tests/test_bundle_blob_integrity.py`

**Category:** Python tool algorithm

## What it tests

Bundle blob integrity — the coverage that was MISSING when the pipeline cutover silently dropped the gmeow.gts blob writer.

Retained dynamic tests:

- `test_bundle_carries_the_consumer_archives` — The wheel-mode consumer archives are folded into gmeow.
- `test_no_dangling_guide_blob_references` — Every gmeow:guideBlob digest reference is backed by a blob actually present in the bundle — the docs guide content is embedded, not a dangling pointer.

## Why it cannot be deleted or moved to Rust today

Python-only algorithm or generated-artifact checks with no declarative slice-test equivalent.

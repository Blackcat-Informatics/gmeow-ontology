# Retention: `tests/test_git_merge_driver.py`

**Category:** Static repo guard

## What it tests

Regression tests for the generated bundle merge driver.

Retained dynamic tests:

- `test_bootstrap_configures_ours_merge_driver` — The install bootstrap makes Git's custom ``ours`` driver available locally.
- `test_generated_bundle_merge_keeps_current_side` — Conflicting edits to generated/dist/gmeow.

## Why it cannot be deleted or moved to Rust today

Filesystem, AST, or workflow assertion about the repository itself; not expressible as a module-scoped slice-test cell.

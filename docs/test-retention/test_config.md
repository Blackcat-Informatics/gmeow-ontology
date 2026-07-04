# Retention: `tests/test_config.py`

**Category:** Python tool algorithm

## What it tests

Tests for the license-aware link policy (the core safety mechanism).

Retained dynamic tests:

- `test_policy_for_license` — Retained dynamic test.
- `test_share_alike_never_import_ok` — Retained dynamic test.
- `test_public_domain_not_flagged_by_nd_marker` — Retained dynamic test.
- `test_alignment_targets_policies` — Retained dynamic test.
- `test_gmeow_temp_dir_uses_prefix` — Retained dynamic test.
- `test_sweep_stale_gmeow_temp_dirs_removes_old` — Retained dynamic test.
- `test_sweep_stale_gmeow_temp_dirs_leaves_young` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Python-only algorithm or generated-artifact checks with no declarative slice-test equivalent.

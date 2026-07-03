# Retention: `tests/test_email_versions.py`

**Category:** Merged-graph guard

## What it tests

Email versioning, variant, and patch-diff guards.

Retained dynamic tests:

- `test_fixture_version_memberships_use_roles_not_subclasses` — Retained dynamic test.
- `test_fixture_patch_diff_links_and_digest` — Retained dynamic test.
- `test_fixture_collision_flags_and_fingerprints` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.

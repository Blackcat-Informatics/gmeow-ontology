# Retention: `tests/test_versions.py`

**Category:** Merged-graph guard

## What it tests

Cross-cutting version-set and version-membership guards.

Retained dynamic tests:

- `test_version_label_domain_is_entity` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

VersionLabel is defined in slices/extensions/languages/module.ttl, not in the versions module, so a scopeModule cell would silently miss it): -.

# Retention: `tests/test_slices.py`

**Category:** Python tool algorithm

## What it tests

Slice discovery + manifest loading.

Retained dynamic tests:

- `TestDiscovery.test_repo_exemplar_loads` — Retained dynamic test.
- `TestDiscovery.test_identity_is_manifest_only_not_path` — slices/<group>/ carries no semantics: same name, two groups, two IRIs.
- `TestDiscovery.test_duplicate_iri_rejected` — Retained dynamic test.
- `TestDiscovery.test_missing_tier_rejected` — Retained dynamic test.
- `TestDiscovery.test_empty_root_is_empty` — Retained dynamic test.
- `TestDependencyRule.test_extension_to_core_ok_extension_to_extension_rejected` — Retained dynamic test.
- `TestDependencyRule.test_unknown_dependency_reported` — Retained dynamic test.
- `TestOwnershipGate.test_repo_is_clean` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Python-only algorithm or generated-artifact checks with no declarative slice-test equivalent.

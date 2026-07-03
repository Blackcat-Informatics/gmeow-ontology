# Retention: `tests/test_email_versions.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Email versioning, variant, and patch-diff guards.

Retained dynamic tests:

- `test_fixture_version_memberships_use_roles_not_subclasses`
- `test_fixture_patch_diff_links_and_digest`
- `test_fixture_collision_flags_and_fingerprints`

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; abox fixture instance checks; python-only file or value inspections.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.

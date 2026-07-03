# Retention: `tests/test_email_mailbox.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Structural guards for mailbox hierarchy and provider-derived state terms.

Retained dynamic tests:

- `test_fixture_nested_hierarchy` — The coverage fixture shows a three-level mailbox hierarchy.
- `test_fixture_mailbox_paths` — Derived path strings are present on nested mailboxes.
- `test_fixture_sort_orders` — Sort orders are present on nested mailboxes.
- `test_fixture_destroyed_mailbox_uses_lifecycle` — A destroyed mailbox uses hasDestructionEvent, not a boolean flag.
- `test_fixture_messages_in_nested_mailbox` — Messages reside in the nested projectsFolder.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; abox fixture instance checks; python-only file or value inspections.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.

# Retention: `tests/test_email_mailbox.py`

**Category:** Merged-graph guard

## What it tests

Structural guards for mailbox hierarchy and provider-derived state terms.

Retained dynamic tests:

- `test_fixture_nested_hierarchy` — The coverage fixture shows a three-level mailbox hierarchy.
- `test_fixture_mailbox_paths` — Derived path strings are present on nested mailboxes.
- `test_fixture_sort_orders` — Sort orders are present on nested mailboxes.
- `test_fixture_destroyed_mailbox_uses_lifecycle` — A destroyed mailbox uses hasDestructionEvent, not a boolean flag.
- `test_fixture_messages_in_nested_mailbox` — Messages reside in the nested projectsFolder.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.

# Retention: `test_email_mailbox.py`

**Category:** Domain invariant → slicetest cells; dynamic fixture tests retained in pytest

## What it tests

The structural TBox guards for the email extension have been migrated to
`slices/extensions/email/tests/structural.ttl` (and the cross-slice
EmailAddress stable properties to `slices/core/contacts/tests/structural.ttl`).
This file now only retains genuinely dynamic tests that operate on the concrete
coverage fixture (`tests/fixtures/coverage/email.ttl`) or on generated artifacts.

Remaining pytest functions:

- `test_fixture_destroyed_mailbox_uses_lifecycle`
- `test_fixture_mailbox_paths`
- `test_fixture_messages_in_nested_mailbox`
- `test_fixture_nested_hierarchy`
- `test_fixture_sort_orders`

## Why the remainder cannot move to Rust today

The retained tests are fixture/artifact traversals, not module-graph invariants.
They verify specific ABox triples in the coverage fixture or inspect the
generated LinkML schema. These are appropriate for pytest until an
example-conformance or generated-artifact conformance layer can express them.

## What would be needed to retire this file

Move the fixture-based assertions to slice-resident `example-conformance.ttl`
cells (with fixtures under `slices/extensions/email/tests/conformance-fixtures/`)
and move the LinkML schema check to a generated-schema conformance harness.

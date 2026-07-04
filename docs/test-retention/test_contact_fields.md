# Retention: `tests/test_contact_fields.py`

**Category:** Merged-graph guard

## What it tests

Structural guards for the audited contact-field terms (the small wins).

Retained dynamic tests:

- `test_new_small_terms_exist` — Retained dynamic test.
- `test_membership_relator_completed` — Retained dynamic test.
- `test_no_flat_contact_terms` — nickname / birthDate / jobTitle / url / image / depiction are downcasts or deferred — never canonical flat terms.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.

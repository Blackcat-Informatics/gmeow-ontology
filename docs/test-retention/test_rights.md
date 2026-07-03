# Retention: `tests/test_rights.py`

**Category:** Merged-graph guard

## What it tests

Tests for the rights / IP / trademark / licensing facility.

Retained dynamic tests:

- `test_expanded_action_vocabulary_is_seeded` — The ODRL Common-Vocabulary actions are seeded (maximal, not a thin stub).
- `test_odrl_projection_emits_a_policy_with_rules` — Retained dynamic test.
- `test_odrl_projection_emits_constraint_and_conflict_logic` — Retained dynamic test.
- `test_spdx_projection_emits_listed_license` — Retained dynamic test.
- `test_cc_projection_emits_license_and_attribution` — Retained dynamic test.
- `test_dcterms_projection_emits_flat_rights` — Retained dynamic test.
- `test_schema_projection_emits_rights_cluster` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

The numeric action-count check (not expressible as a module-scoped ASK) and the ODRL / CC REL / schema.org projection round-trips over the coverage fixture.

# Retention: `tests/test_expertise.py`

**Category:** Merged-graph guard

## What it tests

Retained dynamic guards for the expertise module.

Retained dynamic tests:

- `test_proficiency_scale_is_generalised` — ProficiencyScale is a QualityValue and all expected scales exist.
- `test_proficiency_levels_carry_scale` — Each proficiency level individual is linked to its parent scale.
- `test_no_primary_or_preferred_skill_term` — Principle 9: no single slot wins — no primary/preferred skill selector.
- `test_endorsement_uses_attestation` — No new skill-endorsement mechanism beyond the existing Attestation relator.

## Why it cannot be deleted or moved to Rust today

Retained dynamic guards for the expertise module.

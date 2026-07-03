# Retention: `tests/test_registers.py`

**Category:** Merged-graph guard

## What it tests

The registers & personas facility, in the norms slice.

Retained dynamic tests:

- `test_no_primary_persona_machinery` — No primaryPersona / preferredRegister selectors exist.
- `test_divergence_query_surfaces_legal_divergence` — Add a private-only norm: the query reports it (and SHACL still conforms — divergence is not a violation).

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.

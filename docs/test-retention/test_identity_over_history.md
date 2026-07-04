# Retention: `tests/test_identity_over_history.py`

**Category:** Merged-graph guard

## What it tests

Tests for — identity over immutable history (the .mailmap model).

Retained dynamic tests:

- `test_contributor_transition_preserves_both_identities` — Eve and Evan coexist; the historical AuthorIdentity is not erased.
- `test_mailmap_projection_emits_canonical_and_suppressed_lines` — The mailmap profile emits the canonical line plus a suppressed remapping.
- `test_ai_author_is_software_agent_with_statement_metadata` — GitHub-Copilot-Bot is a SoftwareAgent; the authoredBy claim is annotated.
- `test_suppressed_identity_passes_shacl` — A suppressed contributor identity is retained and valid.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.

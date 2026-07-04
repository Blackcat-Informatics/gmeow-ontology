# Retention: `tests/test_constitution.py`

**Category:** Python CLI surface

## What it tests

Constitution-as-code gate tests.

Retained dynamic tests:

- `test_constitution_report_uses_granular_codes` — The canonical report carries per-check codes, not the legacy roll-up.
- `test_real_manifest_passes` — The committed manifest, constitution, and repo agree — zero errors.
- `test_every_principle_has_a_manifest_entry` — Bidirectional sync: heading set == manifest set, titles verbatim.
- `test_principle_18_native_rdf12_stack_enforced` — Principle 18 exists, is titled verbatim, and is gate-enforced.
- `test_honor_system_principles_are_visible_not_silent` — Practice-only principles surface as warnings (today: 1, 6, 15).
- `test_zero_enforcement_is_an_error` — Retained dynamic test.
- `test_stale_artifact_reference_is_an_error` — Retained dynamic test.
- `test_stale_symbol_make_target_and_cli_command_are_errors` — Retained dynamic test.
- `test_orphaned_enforcement_is_an_error` — Retained dynamic test.
- `test_title_drift_is_an_error` — Retained dynamic test.
- `test_undeclared_generator_is_an_error` — A principle citing an enforcement that is not declared in the manifest fails the gate.
- `test_practice_only_principle_warns_not_errors` — Retained dynamic test.
- `test_supersession_matching_pair_passes` — A TTL ``meta:supersededInPartBy`` matched by the MD marker raises no error.
- `test_supersession_markdown_only_is_an_error` — An MD marker with no matching TTL relation fails the gate.
- `test_supersession_ttl_only_is_an_error` — A TTL relation with no matching MD marker fails the gate.
- `test_extends_matching_pair_passes` — A TTL ``meta:extends`` matched by an MD ``Extends Principle N`` marker passes.

## Why it cannot be deleted or moved to Rust today

The CLIs under test are Typer applications; their behavior is exercised through CliRunner and subprocess integration, which is inherently Python-only surface.

# Retention: `tests/test_evidence.py`

**Category:** Python tool algorithm

## What it tests

SHACL guards for the evidence / source-typing module.

Retained dynamic tests:

- `test_infoworld_citation_passes` — InfoWorld = independent secondary significant coverage → supports notability.
- `test_orgbook_citation_passes` — OrgBook = official primary routine filing → factual verification only.
- `test_private_contract_triggers_self_private_warning` — Private contract = self-originated private scan → Warning.
- `test_orgbook_notability_mutation_triggers_violation` — Flip OrgBook supportsNotability to true → Violation (primary ≠ secondary).

## Why it cannot be deleted or moved to Rust today

Fixture-based mutation tests that cannot be expressed as module-scoped SPARQL ASK cells. The five inline-graph run_shacl() tests have been migrated to crates/validate/tests/conformance_evidence.rs.

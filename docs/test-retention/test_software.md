# Retention: `tests/test_software.py`

**Category:** Merged-graph guard

## What it tests

Behavioural guards for the software module.

Retained dynamic tests:

- `test_no_subclass_bridge_between_facets` — The six facet classes are never bridged by rdfs:subClassOf or owl:equivalentClass.
- `test_fixture_has_all_five_facets` — Retained dynamic test.
- `test_fixture_commit_has_content_digest` — Retained dynamic test.
- `test_fixture_ai_contributor_is_first_class` — AI agents are SoftwareAgents with attributed Contribution relators.
- `test_fixture_contribution_reifies_role_and_degree` — Retained dynamic test.
- `test_software_contribution_roles_seeded` — Retained dynamic test.
- `test_software_event_types_seeded` — Retained dynamic test.
- `test_fixture_has_three_commit_dag` — Retained dynamic test.
- `test_fixture_has_commit_ancestor_closure` — Retained dynamic test.
- `test_fixture_has_blobs_and_tree_entries` — Retained dynamic test.
- `test_fixture_has_push_event` — Retained dynamic test.
- `test_fixture_has_merge_event` — Retained dynamic test.
- `test_fixture_has_code_review_event` — Retained dynamic test.
- `test_fixture_has_diff` — Retained dynamic test.
- `test_fixture_repository_has_materialization_depth` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

TBox structural assertions have been migrated to the declarative test-DSL at slices/extensions/software/tests/structural.ttl (21 cells). Only SHACL conformance, ABox fixture checks, and dynamic whole-graph sweeps are retained here.

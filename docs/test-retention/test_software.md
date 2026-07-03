# Retention: `tests/test_software.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Behavioural guards for the software module.

Retained dynamic tests:

- `test_no_subclass_bridge_between_facets` — The six facet classes are never bridged by rdfs:subClassOf or owl:equivalentClass (Principle 9 -- no overtyping).
- `test_fixture_has_all_five_facets`
- `test_fixture_commit_has_content_digest`
- `test_fixture_ai_contributor_is_first_class` — AI agents are SoftwareAgents with attributed Contribution relators (Principle 9 -- co-equal facets, never ground truth).
- `test_fixture_contribution_reifies_role_and_degree`
- `test_software_contribution_roles_seeded`
- `test_software_event_types_seeded`
- `test_fixture_has_three_commit_dag`
- `test_fixture_has_commit_ancestor_closure`
- `test_fixture_has_blobs_and_tree_entries`
- `test_fixture_has_push_event`
- `test_fixture_has_merge_event`
- `test_fixture_has_code_review_event`
- `test_fixture_has_diff`
- `test_fixture_repository_has_materialization_depth`

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; abox fixture instance checks; python-only file or value inspections.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.

# Retention: `tests/test_label_completeness.py`

**Category:** Merged-graph guard

## What it tests

Focused tests for annotation completeness across all GMEOW terms.

Retained dynamic tests:

- `test_merged_ontology_has_no_missing_annotations` — Every GMEOW term in the merged ontology carries the required triple.
- `test_structural_lint_flags_missing_label_definition_and_isdefinedby` — Missing any of the three required annotations is an error.
- `test_structural_lint_covers_individuals` — Individuals are in scope for the annotation-completeness gate.
- `test_structural_lint_covers_annotation_properties` — Annotation properties are in scope for the annotation-completeness gate.
- `test_mapping_dsl_vocabulary_has_no_missing_annotations` — Every vocabulary term in mapping-dsl/vocabulary.
- `test_statement_dsl_vocabulary_has_no_missing_annotations` — Every vocabulary term in dsl/statements/vocabulary.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.

# Retention: `tests/test_notes.py`

**Category:** Merged-graph guard

## What it tests

SHACL + cross-slice structural guards for the notes & annotation building block.

Retained dynamic tests:

- `test_evidence_span_is_information_object` — Retained dynamic test.
- `test_selector_sub_class_of_evidence_span` — Retained dynamic test.
- `test_motivation_values_are_individuals` — Retained dynamic test.
- `test_notes_are_standpoint_indexed` — The standpoint machinery (accordingTo) is available on notes via the statement/provenance layer; the TBox does not forbid it.
- `test_notes_oa_projection_executable` — Retained dynamic test.
- `test_notes_schema_projection_executable` — Retained dynamic test.
- `test_notes_as_projection_executable` — Retained dynamic test.
- `test_notes_markdown_projection_executable` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

: EvidenceSpan subject is in the evidencespan slice, not in notes/module.ttl (cross-slice). - : Selector subject is in the evidencespan slice (cross-slice). - : accordingTo subject is in the standpoint slice (cross-slice). - : the len(...)==10 count assertion is a dynamic whole-graph numeric check; the 10 seed individuals and banned-class guards are covered in structural.ttl cells sa10MotivationSeeds and saMotivationNotClasses. - All projection SPARQL parse tests (generated-artifact, numeric/parse check).

"""Cross-layer consistency gates for the projection stack.

The alignment stack stores the same mappings four ways (SSSOM / EDOAL / FnO /
CONSTRUCT) plus the ontology; the rest of the suite checks each artifact in
isolation, so independent drift between them slipped past CI and was caught only by
AI reviewers. These gates check the artifacts AGAINST each other — they reproduce,
as deterministic checks, the two classes of bug found in review:

* an ``fno:Parameter`` typed inconsistently with its predicate's ``rdfs:range``
  (e.g. ``fno:predicate gmeow:eventTime`` declared with a mismatched ``fno:type``);
* a CONSTRUCT executor emitting a downcast its EDOAL spec never declares.
"""

from __future__ import annotations

from gmeow_tools.projection_lint import (
    fno_reference_integrity,
    fno_type_mismatches,
    projection_spec_drift,
)


def test_fno_param_types_match_predicate_ranges() -> None:
    problems = fno_type_mismatches()
    assert not problems, "FnO type ≠ predicate range:\n" + "\n".join(problems)


def test_edoal_transformations_reference_defined_functions() -> None:
    problems = fno_reference_integrity()
    assert not problems, "EDOAL → undefined FnO function:\n" + "\n".join(problems)


def test_projection_specs_match_executors() -> None:
    problems = projection_spec_drift()
    assert not problems, "Projection spec ↔ executor drift:\n" + "\n".join(problems)

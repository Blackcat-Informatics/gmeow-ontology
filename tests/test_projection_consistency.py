"""Cross-layer consistency gates for the projection stack.

The alignment stack stores the same mappings four ways (SSSOM / EDOAL / FnO /
CONSTRUCT) plus the ontology; the rest of the suite checks each artifact in
isolation, so independent drift between them slipped past CI and was caught only by
AI reviewers. These gates check the artifacts AGAINST each other — they reproduce,
as deterministic checks, the two classes of bug found in review:

* an ``fno:Parameter`` typed inconsistently with its predicate's ``rdfs:range``
  (e.g. ``fno:predicate gmeow:eventTime`` declared with a mismatched ``fno:type``);
* a CONSTRUCT executor emitting a downcast its EDOAL spec never declares.

These gates now run the native ``gmeow_slice.lint_projection`` trio (#854) — the
Rust subsumption of the former Python ``projection_lint`` — over the committed
tree, filtering its findings by ``check`` to keep the per-invariant granularity.
"""

from __future__ import annotations

import gmeow_slice

from gmeow_tools.config import PROJECT_ROOT


def _problems(check: str) -> list[str]:
    """Native projection-lint messages for one ``check`` family."""
    return [
        d["message"]
        for d in gmeow_slice.lint_projection(str(PROJECT_ROOT))
        if d["check"] == check
    ]


def test_fno_param_types_match_predicate_ranges() -> None:
    problems = _problems("fno-type")
    assert not problems, "FnO type ≠ predicate range:\n" + "\n".join(problems)


def test_edoal_transformations_reference_defined_functions() -> None:
    problems = _problems("fno-ref")
    assert not problems, "EDOAL → undefined FnO function:\n" + "\n".join(problems)


def test_projection_specs_match_executors() -> None:
    problems = _problems("spec-drift")
    assert not problems, "Projection spec ↔ executor drift:\n" + "\n".join(problems)

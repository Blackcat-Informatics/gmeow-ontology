# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Conformance tests for the logic: projection compiler (issue #500, Task 5).

Parametrized over every case directory under
``conformance/logic/cases/projections/``.  For each case the test:

1. Parses ``input.logic.ttl`` via :func:`~.logic_frontend.parse_logic_source`
   to obtain a :class:`~.logic_ir.LogicProgram`.
2. Runs all 6 projection back-ends and asserts each output is graph-isomorphic
   (for RDF targets) or byte-equal (for text targets) to the committed golden
   artifact in ``expected/projections/``.
3. Asserts the preservation ledger dict matches the committed
   ``expected/projections/preservation-ledger.json``.
4. Parses ``legacy.ttl`` via :func:`~.logic_adapter.adapt_legacy_source` and
   calls :func:`~.logic_adapter.assert_ir_isomorphic` against the logic: IR.
   Cases that intentionally diverge (e.g. ``confidence-scoped-axiom``, where
   the confidence annotation has no OWL/gUFO equivalent) are listed in
   ``_PARTIAL_LEGACY_CASES`` and checked only for the common (unscoped) axiom
   subset instead.

Golden files are committed under ``expected/projections/`` and were generated
by running the actual projection functions on the input.  The test is the
machine-checked enforcement that the engine never drifts from those artifacts.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from rdflib import Graph

from gmeow_tools.logic_adapter import (
    IRIsomorphismError,
    adapt_legacy_source,
    assert_ir_isomorphic,
)
from gmeow_tools.logic_frontend import parse_logic_source
from gmeow_tools.logic_ir import LogicAxiom, LogicProgram
from gmeow_tools.logic_projections import (
    build_projection_report,
    project_canonical_rdf12,
    project_datalog,
    project_gufo,
    project_n3,
    project_nemo,
    project_owl_dl,
    project_owl_el,
)

# --------------------------------------------------------------------------- #
# Discovery
# --------------------------------------------------------------------------- #

_CONFORMANCE_ROOT = (
    Path(__file__).resolve().parents[1]
    / "conformance"
    / "logic"
    / "cases"
    / "projections"
)

#: Cases where the legacy form intentionally omits contextual scope
#: (confidence, modality annotations) that the logic: form carries.
#: For these, the round-trip gate verifies that every legacy-adapted axiom
#: is present in the logic: program, rather than requiring full isomorphism.
_PARTIAL_LEGACY_CASES: frozenset[str] = frozenset({"confidence-scoped-axiom"})


def _discover_cases() -> list[Path]:
    """Return all case directories that contain an ``input.logic.ttl`` file."""
    if not _CONFORMANCE_ROOT.is_dir():
        return []
    return sorted(
        p
        for p in _CONFORMANCE_ROOT.iterdir()
        if p.is_dir() and (p / "input.logic.ttl").exists()
    )


_ALL_CASES = _discover_cases()
_CASE_IDS = [c.name for c in _ALL_CASES]


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #


def _normalize_source_iri(program: LogicProgram) -> LogicProgram:
    """Return a copy of *program* with ``source_iri`` set to ``None``.

    The round-trip isomorphism gate (:func:`assert_ir_isomorphic`) fails when
    two programs differ only in ``source_iri`` (different file paths for the
    ``logic:`` vs ``legacy.ttl`` files).  Stripping the provenance IRI
    restricts the comparison to the semantic content.
    """
    return LogicProgram(
        axioms=program.axioms,
        rules=program.rules,
        profiles=program.profiles,
        source_iri=None,
    )


def _graphs_isomorphic(g1: Graph, g2: Graph) -> bool:
    """Return True when two rdflib Graphs are blank-node-aware isomorphic."""
    return g1.isomorphic(g2)


def _load_rdf(path: Path) -> Graph:
    """Parse a Turtle file into an rdflib Graph."""
    g = Graph()
    g.parse(str(path), format="turtle")
    return g


def _text_normalize(text: str) -> str:
    """Normalize line-endings for text-format comparison."""
    return text.rstrip("\n") + "\n"


# --------------------------------------------------------------------------- #
# Parametrized conformance test
# --------------------------------------------------------------------------- #


@pytest.mark.parametrize("case_dir", _ALL_CASES, ids=_CASE_IDS)
def test_projection_conformance(case_dir: Path) -> None:
    """Full projection conformance check for one case directory.

    Verifies:

    * All 6 projection back-ends produce output isomorphic / equal to the
      committed golden artifacts.
    * The preservation ledger matches the committed JSON.
    * The legacy round-trip isomorphism gate passes (with appropriate scope
      for partial-legacy cases).
    """
    input_path = case_dir / "input.logic.ttl"
    expected_dir = case_dir / "expected" / "projections"
    case_name = case_dir.name

    # ---- Parse logic: source -----------------------------------------------
    program, _diagnostics = parse_logic_source(input_path)
    assert len(program.axioms) > 0, f"{case_name}: input.logic.ttl produced no axioms"

    # ---- Run all 7 projection back-ends ------------------------------------
    r_dl = project_owl_dl(program)
    r_el = project_owl_el(program)
    r_datalog = project_datalog(program)
    r_n3 = project_n3(program)
    r_gufo = project_gufo(program)
    r_rdf12 = project_canonical_rdf12(program)
    r_nemo = project_nemo(program)

    all_projections = [r_dl, r_el, r_datalog, r_n3, r_gufo, r_rdf12, r_nemo]

    # ---- RDF-target isomorphism checks -------------------------------------
    for result in [r_dl, r_el, r_gufo, r_rdf12]:
        golden_path = expected_dir / f"{result.target}.ttl"
        assert golden_path.exists(), (
            f"{case_name}: missing golden file {golden_path.name}"
        )
        assert result.graph is not None, (
            f"{case_name}: {result.target} produced no graph"
        )
        golden_graph = _load_rdf(golden_path)
        assert _graphs_isomorphic(result.graph, golden_graph), (
            f"{case_name}: {result.target} output is not isomorphic to "
            f"{golden_path.name}"
        )

    # ---- Text-target byte-equality checks ----------------------------------
    datalog_golden = (expected_dir / "datalog.dl").read_text(encoding="utf-8")
    assert _text_normalize(r_datalog.content) == _text_normalize(datalog_golden), (
        f"{case_name}: datalog.dl output does not match golden"
    )

    n3_golden = (expected_dir / "n3.n3").read_text(encoding="utf-8")
    assert _text_normalize(r_n3.content) == _text_normalize(n3_golden), (
        f"{case_name}: n3.n3 output does not match golden"
    )

    nemo_golden_path = expected_dir / "nemo.rls"
    assert nemo_golden_path.exists(), f"{case_name}: missing golden file nemo.rls"
    nemo_golden = nemo_golden_path.read_text(encoding="utf-8")
    assert _text_normalize(r_nemo.content) == _text_normalize(nemo_golden), (
        f"{case_name}: nemo.rls output does not match golden"
    )

    # ---- Preservation ledger -----------------------------------------------
    ledger_path = expected_dir / "preservation-ledger.json"
    assert ledger_path.exists(), f"{case_name}: missing preservation-ledger.json"
    committed_ledger = json.loads(ledger_path.read_text(encoding="utf-8"))
    live_ledger = {
        proj.target: {
            "preservation": proj.preservation.value,
            "complexity": proj.complexity,
            "lossy_drops": list(proj.lossy_drops),
        }
        for proj in all_projections
    }
    assert live_ledger == committed_ledger, (
        f"{case_name}: live preservation ledger differs from committed JSON"
    )

    # ---- Projection report RDF isomorphism ---------------------------------
    report_golden_path = expected_dir / "projection-report.ttl"
    if report_golden_path.exists():
        live_report = build_projection_report(program, all_projections)
        golden_report = _load_rdf(report_golden_path)
        assert _graphs_isomorphic(live_report, golden_report), (
            f"{case_name}: projection-report.ttl is not isomorphic to golden"
        )

    # ---- Legacy round-trip isomorphism gate --------------------------------
    legacy_path = case_dir / "legacy.ttl"
    assert legacy_path.exists(), f"{case_name}: missing legacy.ttl"
    legacy_program, _legacy_diags = adapt_legacy_source(legacy_path)

    norm_logic = _normalize_source_iri(program)
    norm_legacy = _normalize_source_iri(legacy_program)

    if case_name in _PARTIAL_LEGACY_CASES:
        # Cases where legacy intentionally lacks contextual scope
        # (e.g. confidence annotations).  Verify that every legacy-adapted
        # axiom IS present in the logic: program (the legacy is a subset).
        logic_axiom_keys = {
            (a.subject, a.predicate, a.obj, a.obj_is_literal) for a in norm_logic.axioms
        }
        missing: list[LogicAxiom] = []
        for legacy_axiom in norm_legacy.axioms:
            key = (
                legacy_axiom.subject,
                legacy_axiom.predicate,
                legacy_axiom.obj,
                legacy_axiom.obj_is_literal,
            )
            if key not in logic_axiom_keys:
                missing.append(legacy_axiom)
        assert not missing, (
            f"{case_name}: legacy axioms not found in logic: program:\n  "
            + "\n  ".join(str(a) for a in missing)
        )
    else:
        # Full round-trip: the two IRs must be canonically identical.
        try:
            assert_ir_isomorphic(norm_logic, norm_legacy)
        except IRIsomorphismError as exc:
            pytest.fail(
                f"{case_name}: legacy round-trip isomorphism gate FAILED:\n{exc}"
            )

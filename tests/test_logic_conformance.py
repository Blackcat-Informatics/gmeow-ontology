# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Conformance tests for the logic: projection compiler (issue #500, Task 5).

Parametrized over every case directory under
``conformance/logic/cases/projections/``.  For each case the test:

1. Compiles ``input.logic.ttl`` via the Rust :func:`gmeow_logic.compile_logic`
   (the whole frontend → IR → projection pipeline runs in Rust since #664/#727;
   the Python compiler duplicate was deleted in #727).
2. Asserts each artifact is graph-isomorphic (RDF targets) or byte-equal (text
   targets) to the committed golden in ``expected/projections/``.
3. Asserts the Rust-built preservation ledger matches the committed
   ``expected/projections/preservation-ledger.json``.

The legacy round-trip isomorphism gate (the OWL/gUFO adapter) is now covered by
the Rust crate tests (``crates/logic/src/compile/adapter/tests.rs``); the Python
adapter was deleted in #727.

Golden files are committed under ``expected/projections/``.  The test is the
machine-checked enforcement that the engine never drifts from those artifacts.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from rdflib import Graph

from tests._required_native import require_gmeow_logic

gmeow_logic = require_gmeow_logic()

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

#: Projection target short-name → compile_logic dict key.
_TARGET_TO_KEY = {
    "owl-dl": "owl_dl",
    "owl-el": "owl_el",
    "datalog": "datalog",
    "n3": "n3",
    "gufo": "gufo",
    "canonical-rdf12": "canonical_rdf12",
    "nemo": "nemo",
}


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


def _graphs_isomorphic(g1: Graph, g2: Graph) -> bool:
    """Return True when two rdflib Graphs are blank-node-aware isomorphic."""
    return g1.isomorphic(g2)


def _load_rdf(path: Path) -> Graph:
    """Parse a Turtle file into an rdflib Graph."""
    g = Graph()
    g.parse(str(path), format="turtle")
    return g


def _parse_turtle_str(text: str) -> Graph:
    """Parse a Turtle string into an rdflib Graph."""
    g = Graph()
    g.parse(data=text, format="turtle")
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

    * All 7 projection back-ends produce output isomorphic / equal to the
      committed golden artifacts.
    * The preservation ledger matches the committed JSON.
    * The projection report is isomorphic to the committed golden.
    """
    input_path = case_dir / "input.logic.ttl"
    expected_dir = case_dir / "expected" / "projections"
    case_name = case_dir.name

    # ---- Compile logic: source (Rust) --------------------------------------
    source_ttl = input_path.read_text(encoding="utf-8")
    compiled = gmeow_logic.compile_logic(source_ttl)

    # ---- RDF-target isomorphism checks -------------------------------------
    for target in ("owl-dl", "owl-el", "gufo", "canonical-rdf12"):
        golden_path = expected_dir / f"{target}.ttl"
        assert golden_path.exists(), f"{case_name}: missing golden file {target}.ttl"
        live_graph = _parse_turtle_str(str(compiled[_TARGET_TO_KEY[target]]))
        golden_graph = _load_rdf(golden_path)
        assert _graphs_isomorphic(live_graph, golden_graph), (
            f"{case_name}: {target} output is not isomorphic to {target}.ttl"
        )

    # ---- Text-target byte-equality checks ----------------------------------
    datalog_golden = (expected_dir / "datalog.dl").read_text(encoding="utf-8")
    assert _text_normalize(str(compiled["datalog"])) == _text_normalize(
        datalog_golden
    ), f"{case_name}: datalog.dl output does not match golden"

    n3_golden = (expected_dir / "n3.n3").read_text(encoding="utf-8")
    assert _text_normalize(str(compiled["n3"])) == _text_normalize(n3_golden), (
        f"{case_name}: n3.n3 output does not match golden"
    )

    nemo_golden_path = expected_dir / "nemo.rls"
    assert nemo_golden_path.exists(), f"{case_name}: missing golden file nemo.rls"
    nemo_golden = nemo_golden_path.read_text(encoding="utf-8")
    assert _text_normalize(str(compiled["nemo"])) == _text_normalize(nemo_golden), (
        f"{case_name}: nemo.rls output does not match golden"
    )

    # ---- Preservation ledger -----------------------------------------------
    ledger_path = expected_dir / "preservation-ledger.json"
    assert ledger_path.exists(), f"{case_name}: missing preservation-ledger.json"
    committed_ledger = json.loads(ledger_path.read_text(encoding="utf-8"))
    raw_ledger = compiled["preservation_ledger"]
    live_ledger = {
        str(target): {
            "preservation": row["preservation"],
            "complexity": row["complexity"],
            "lossy_drops": list(row["lossy_drops"]),
        }
        for target, row in raw_ledger.items()
    }
    assert live_ledger == committed_ledger, (
        f"{case_name}: live preservation ledger differs from committed JSON"
    )

    # ---- Projection report RDF isomorphism ---------------------------------
    report_golden_path = expected_dir / "projection-report.ttl"
    if report_golden_path.exists():
        live_report = _parse_turtle_str(str(compiled["report"]))
        golden_report = _load_rdf(report_golden_path)
        assert _graphs_isomorphic(live_report, golden_report), (
            f"{case_name}: projection-report.ttl is not isomorphic to golden"
        )

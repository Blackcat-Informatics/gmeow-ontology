# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for ``gmeow logic compile`` CLI and the ``LogicGenerator`` registration.

Module under test: ``logic_compile.py`` (generator) + ``cli_dev.py`` (CLI).

Covers:
* LogicGenerator is discoverable in the registry after load_generators import.
* Generator name / inputs / outputs are correct.
* ``gmeow logic compile --check`` exits 0 on a freshly-generated tree.
* ``gmeow logic compile --mode owl-el`` selects only the EL back-end.
* render → compare round-trip is clean (no drift) on the real source.
* Unknown ``--mode`` exits non-zero with a message.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from gmeow_tools.cli_dev import app as dev_app
from tests._required_native import require_gmeow_logic

if TYPE_CHECKING:
    from gmeow_tools.generator import Generator

# --------------------------------------------------------------------------- #
# Generator registry
# --------------------------------------------------------------------------- #


def test_logic_generator_registered() -> None:
    """LogicGenerator appears in the registry after loading all generators."""
    from gmeow_tools.generator import registry
    from gmeow_tools.load_generators import load_all

    load_all()
    reg = registry()
    assert "logic" in reg, f"'logic' not in registry: {sorted(reg)}"


def _get_logic_gen() -> Generator:
    """Return the registered logic generator instance."""
    from gmeow_tools.generator import registry
    from gmeow_tools.load_generators import load_all

    load_all()
    return registry()["logic"]


def test_logic_generator_name() -> None:
    gen = _get_logic_gen()
    assert gen.name == "logic"


def test_logic_generator_inputs_include_source() -> None:
    from gmeow_tools.logic_compile import LOGIC_SOURCE_FILE

    gen = _get_logic_gen()
    inputs = [str(p) for p in gen.inputs]
    assert any(str(LOGIC_SOURCE_FILE) in s for s in inputs), (
        f"LOGIC_SOURCE_FILE not in inputs: {inputs}"
    )


def test_logic_generator_outputs_all_eight() -> None:
    from gmeow_tools.logic_compile import (
        LOGIC_DATALOG_FILE,
        LOGIC_GUFO_FILE,
        LOGIC_N3_FILE,
        LOGIC_NEMO_FILE,
        LOGIC_OWL_DL_FILE,
        LOGIC_OWL_EL_FILE,
        LOGIC_RDF12_FILE,
        LOGIC_REPORT_FILE,
    )

    gen = _get_logic_gen()
    outputs = list(gen.outputs)
    assert len(outputs) == 8, f"Expected 8 outputs, got {len(outputs)}: {outputs}"
    output_strs = {str(p) for p in outputs}
    for expected in [
        LOGIC_OWL_DL_FILE,
        LOGIC_OWL_EL_FILE,
        LOGIC_DATALOG_FILE,
        LOGIC_N3_FILE,
        LOGIC_GUFO_FILE,
        LOGIC_RDF12_FILE,
        LOGIC_NEMO_FILE,
        LOGIC_REPORT_FILE,
    ]:
        assert str(expected) in output_strs, f"{expected} not in outputs: {output_strs}"


def test_logic_generator_allows_internal_tags() -> None:
    gen = _get_logic_gen()
    assert gen.allows_internal_tags is True


# --------------------------------------------------------------------------- #
# CLI: gmeow logic compile --check
# --------------------------------------------------------------------------- #


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


def test_logic_compile_check_no_drift(runner: CliRunner) -> None:
    """--check exits 0 when committed artifacts match the source."""
    from gmeow_tools.logic_compile import LOGIC_SOURCE_FILE

    if not LOGIC_SOURCE_FILE.exists():
        pytest.skip("logic source file not found in this checkout")

    result = runner.invoke(dev_app, ["logic", "compile", "--check"])
    assert result.exit_code == 0, f"Expected exit 0; got:\n{result.output}"
    assert "no drift" in result.output or "match" in result.output


def test_logic_compile_mode_owl_el(runner: CliRunner, tmp_path: Path) -> None:
    """--mode owl-el writes only the EL back-end (does not raise)."""
    from gmeow_tools.logic_compile import LOGIC_OWL_EL_FILE, LOGIC_SOURCE_FILE

    if not LOGIC_SOURCE_FILE.exists():
        pytest.skip("logic source file not found in this checkout")
    if not LOGIC_OWL_EL_FILE.exists():
        pytest.skip(
            "committed owl-el artifact not found; run gmeow logic compile first"
        )

    result = runner.invoke(dev_app, ["logic", "compile", "--check", "--mode", "owl-el"])
    assert result.exit_code == 0, f"Expected exit 0; got:\n{result.output}"
    assert "owl-el" in result.output


def test_logic_compile_unknown_mode_fails(runner: CliRunner) -> None:
    """An unknown --mode exits non-zero with an error message."""
    result = runner.invoke(dev_app, ["logic", "compile", "--mode", "bad-mode"])
    assert result.exit_code != 0
    assert "unknown --mode" in result.output or "bad-mode" in result.output


# --------------------------------------------------------------------------- #
# gmeow logic query (issue #504, v4 backward goals)
# --------------------------------------------------------------------------- #


def _query_case(name: str) -> Path:
    """Path to a profiles/ backward-goal conformance case (skip if absent)."""
    from gmeow_tools.config import PROJECT_ROOT

    case = Path(PROJECT_ROOT) / "conformance" / "logic" / "cases" / "profiles" / name
    if not case.is_dir():
        pytest.skip(f"conformance case {name} not found in this checkout")
    return case


def test_logic_query_recursive_ancestor(runner: CliRunner) -> None:
    """`logic query` resolves a tabled recursive goal to the transitive closure."""
    require_gmeow_logic()
    case = _query_case("goal-recursive-ancestor")
    result = runner.invoke(
        dev_app,
        [
            "logic",
            "query",
            str(case / "input.nq"),
            str(case / "queries" / "ancestor.logic"),
            "--json",
        ],
    )
    assert result.exit_code == 0, f"Expected exit 0; got:\n{result.output}"
    # Parse the JSON payload rather than asserting on formatting tokens, so the
    # test is coupled to behaviour (status + bindings) not to dict rendering.
    payload = json.loads(result.output)
    assert payload["status"] == "ok"
    # Three ancestors (b, c, d) over the a→b→c→d chain.
    ys = {b["Y"] for b in payload["bindings"]}
    assert ys == {
        "<https://example.org/profiles/goal-recursive-ancestor/b>",
        "<https://example.org/profiles/goal-recursive-ancestor/c>",
        "<https://example.org/profiles/goal-recursive-ancestor/d>",
    }, payload


def test_logic_query_cut_rejected_outside_procedural(runner: CliRunner) -> None:
    """Cut under a non-ProceduralPrologProfile profile hard-fails (AC-2 gate)."""
    require_gmeow_logic()
    case = _query_case("goal-procedural-cut")
    result = runner.invoke(
        dev_app,
        [
            "logic",
            "query",
            str(case / "input.nq"),
            str(case / "queries" / "first.logic"),
            "--profile",
            "PositiveHornProfile",
        ],
    )
    assert result.exit_code != 0
    assert "cut" in result.output.lower()


def test_logic_compile_help(runner: CliRunner) -> None:
    """``gmeow logic compile --help`` exits 0 and describes the command."""
    result = runner.invoke(dev_app, ["logic", "compile", "--help"])
    assert result.exit_code == 0
    assert "compile" in result.output.lower()


# --------------------------------------------------------------------------- #
# Generator round-trip: render → compare → no drift
# --------------------------------------------------------------------------- #


def test_logic_generator_render_compare_round_trip(tmp_path: Path) -> None:
    """Rendering into a temp staging tree and comparing produces no drift."""
    from gmeow_tools.logic_compile import LOGIC_SOURCE_FILE

    if not LOGIC_SOURCE_FILE.exists():
        pytest.skip("logic source file not found in this checkout")

    gen = _get_logic_gen()
    # Render all outputs into tmp_path as the staging root
    gen.render(tmp_path)

    from gmeow_tools.generator import _staging_rel

    drifts: list[str] = []
    for committed in gen.outputs:
        fresh = tmp_path / _staging_rel(committed)
        if fresh.exists():
            drifts.extend(gen.compare(fresh, committed))

    assert not drifts, "Round-trip produced drift:\n" + "\n".join(drifts)

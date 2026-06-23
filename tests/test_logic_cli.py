# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for ``gmeow logic compile`` CLI and ``logic_compile`` library.

Module under test: ``logic_compile.py`` (library) + ``cli_dev.py`` (CLI).

Covers:
* ``gmeow logic compile --check`` exits 0 on a freshly-generated tree.
* ``gmeow logic compile --mode owl-el`` selects only the EL back-end.
* Unknown ``--mode`` exits non-zero with a message.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
import pytest as _pytest
from typer.testing import CliRunner

from gmeow_tools.cli_dev import app as dev_app
from tests._required_native import require_gmeow_logic

pytestmark = _pytest.mark.maintainer

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
# gmeow reason --mode native (issue #665, native Docker-free authority lane)
# --------------------------------------------------------------------------- #


def test_reason_mode_native_exits_clean(runner: CliRunner) -> None:
    """``reason --mode native`` reasons the bundle Docker-free and exits 0.

    The native lane is the Java/Docker-free authority (Principle 18): it runs
    the Rust EL/DL engine in-process — no container, no network — and exits 0
    with the success banner on a consistent bundle.
    """
    require_gmeow_logic()
    from gmeow_tools.config import GTS_SNAPSHOT_FILE

    if not GTS_SNAPSHOT_FILE.exists():
        pytest.skip("GTS snapshot not present in this checkout")

    result = runner.invoke(dev_app, ["reason", "--mode", "native"])
    assert result.exit_code == 0, f"Expected exit 0; got:\n{result.output}"
    assert "native EL/DL reasoning" in result.output


def test_reason_unknown_mode_fails(runner: CliRunner) -> None:
    """An unknown ``--mode`` exits non-zero (only native/docker are valid)."""
    result = runner.invoke(dev_app, ["reason", "--mode", "bogus"])
    assert result.exit_code != 0
    assert "unknown reasoning mode" in result.output or "bogus" in result.output

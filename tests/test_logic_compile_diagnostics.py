# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The logic-compile diagnostics surface (#809, #856).

``gmeow_logic.compile_logic`` builds the parse diagnostics into the canonical
``Finding`` model **in Rust** and returns a live, normalized ``diagnostics_report``
(RUST-FIRST/PYTHON-SURFACE). ``logic_compile.compile_diagnostics_report`` forwards
that native report directly — there is no Python dict→finding re-shaping. The
finding construction itself (``logic-compile.<code>`` codes, severity mapping,
``subject`` → logical location) is unit-tested in Rust
(``crates/logic/src/compile/frontend/tests.rs``); these tests pin the live Python
seam end-to-end.
"""

from __future__ import annotations

import gmeow_logic

from gmeow_tools import logic_compile


def test_compile_logic_returns_a_native_diagnostics_report() -> None:
    """compile_logic exposes a live ``diagnostics_report`` (not a dict list)."""
    source_ttl = logic_compile.LOGIC_SOURCE_FILE.read_text(encoding="utf-8")
    result = gmeow_logic.compile_logic(source_ttl)

    # The legacy ``diagnostics: list[dict]`` channel is gone (#856).
    assert "diagnostics" not in result
    report = result["diagnostics_report"]
    assert report.tool == "logic-compile"
    # The 8 artifacts + the ledger still flow alongside the report.
    for key in (
        "owl_dl",
        "owl_el",
        "datalog",
        "n3",
        "gufo",
        "canonical_rdf12",
        "nemo",
        "report",
        "nemo_rules",
        "preservation_ledger",
    ):
        assert key in result


def test_clean_source_compiles_to_an_ok_report() -> None:
    """The committed logic: source compiles without error findings."""
    report = logic_compile.compile_diagnostics_report()

    assert report.ok
    assert report.error_count == 0
    assert report.tool == "logic-compile"


def test_findings_carry_the_logic_compile_namespace() -> None:
    """Any parse finding is tool-tagged and code-prefixed by the Rust core."""
    report = logic_compile.compile_diagnostics_report()

    for finding in report.findings:
        assert finding["tool"] == "logic-compile"
        assert finding["code"].startswith("logic-compile.")

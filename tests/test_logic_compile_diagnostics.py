# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The logic-compile diagnostics surface (#809).

``gmeow_logic.compile_logic`` produces structured parse diagnostics in Rust;
``logic_compile.to_diagnostics_report`` forwards them, full-fidelity, into the
canonical ``Finding`` model (RUST-FIRST/PYTHON-SURFACE).
"""

from __future__ import annotations

from gmeow_tools import logic_compile


def test_structured_diagnostics_forward_full_fidelity() -> None:
    """Each Rust diagnostic dict becomes one finding; subject → logical."""
    result = {
        "diagnostics": [
            {
                "severity": "warning",
                "code": "unknown-stereotype",
                "message": "term has no recognised stereotype",
                "subject": "https://blackcatinformatics.ca/gmeow/Foo",
            },
            {
                "severity": "note",
                "code": "redundant-axiom",
                "message": "axiom is entailed",
                "subject": "",
            },
        ]
    }

    report = logic_compile.to_diagnostics_report(result)

    assert report.ok  # warnings/notes do not flip the gate
    assert report.finding_count == 2
    first = report.findings[0]
    assert first["severity"] == "warning"
    assert first["code"] == "logic-compile.unknown-stereotype"
    assert first["message"] == "term has no recognised stereotype"
    assert first["locations"][0]["logical"] == (
        "https://blackcatinformatics.ca/gmeow/Foo"
    )
    # An empty subject string carries no logical grouping key.
    second_locations = report.findings[1]["locations"]
    assert all(loc["logical"] is None for loc in second_locations)


def test_no_diagnostics_yields_empty_report() -> None:
    """A clean compile (no parse diagnostics) is an empty, ok report."""
    report = logic_compile.to_diagnostics_report({"diagnostics": []})

    assert report.ok
    assert report.finding_count == 0


def test_clean_source_compiles_to_an_ok_report() -> None:
    """The committed logic: source compiles without error findings."""
    report = logic_compile.compile_diagnostics_report()

    assert report.ok
    assert report.error_count == 0

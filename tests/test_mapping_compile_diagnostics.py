# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The mapping-compile diagnostics surface (#809).

Scoped to the DSL/compile path only — the FnO ``projection_lint`` trio and SSSOM
validation are #848's native-subsumption territory and are deliberately untouched.
"""

from __future__ import annotations

import pytest

from gmeow_tools import mapping_compile
from gmeow_tools.mapping_dsl import CompileError


def test_clean_committed_mappings_compile_to_an_ok_report() -> None:
    """The committed mapping DSL compiles with no error findings."""
    report = mapping_compile.compile_diagnostics_report()

    assert report.ok
    assert report.error_count == 0


def test_dsl_compile_error_becomes_one_finding(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A CompileError on the DSL/artifact path maps to one dsl-error finding."""

    def _boom(*_args: object, **_kwargs: object) -> object:
        raise CompileError("value-class pattern has no value-binding predicate")

    monkeypatch.setattr(mapping_compile, "_artifacts", _boom)

    report = mapping_compile.compile_diagnostics_report()

    assert not report.ok
    assert report.error_count == 1
    item = report.findings[0]
    assert item["code"] == "mapping-compile.dsl-error"
    assert "value-binding predicate" in item["message"]

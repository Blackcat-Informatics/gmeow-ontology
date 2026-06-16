# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Durable parity guard for the Rust gUFO/UFO reasoning invariants (#579).

The golden under ``tests/fixtures/lint-golden/reasoning_invariants.json`` was
captured from the *original* pure-Python ``reasoning_invariants`` over the real
merged graph BEFORE the Rust port. This test asserts the Rust path reproduces it
EXACTLY — so the behavior is pinned independently of the Python check bodies, and
the guard survives Task 5's deletion of any remaining rdflib path.

Two routes are checked:

* the direct ``gmeow_validate.reasoning_invariants`` extension API over the real
  source paths, and
* the ``gmeow_tools.validate.reasoning_lint`` wrapper over the merged rdflib
  graph (the production ``validate_all`` path),

so neither the FFI boundary nor the Python adapter can drift from the golden.
"""

from __future__ import annotations

import json
from pathlib import Path

import gmeow_validate

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import iter_source_files, load_merged_graph
from gmeow_tools.validate import reasoning_lint

_GOLDEN = (
    Path(__file__).parent / "fixtures" / "lint-golden" / "reasoning_invariants.json"
)


def _golden() -> list[str]:
    payload = json.loads(_GOLDEN.read_text(encoding="utf-8"))
    assert isinstance(payload, list)
    return payload


def _source_paths() -> list[str]:
    return [str(p) for p in iter_source_files()]


def test_reasoning_invariants_rust_matches_golden() -> None:
    report = gmeow_validate.reasoning_invariants(_source_paths(), str(NAMESPACE))
    assert sorted(report["errors"]) == _golden()
    assert list(report["warnings"]) == []


def test_reasoning_lint_wrapper_matches_golden() -> None:
    result = reasoning_lint(load_merged_graph())
    assert sorted(result.errors) == _golden()
    assert result.warnings == []

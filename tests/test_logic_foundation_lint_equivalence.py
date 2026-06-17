# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""End-to-end regression gate for the foundation lowering (issues #503 / #636).

The foundation lowering — the OntoUML-discipline ``logic:violation`` verdicts, the
cross-world rigidity closure, and the anti-rigidity witness policy — is now
evaluated entirely by the native Rust evaluator ``gmeow_logic.foundation`` (issue
#636).  The Python oracle (``logic_foundation.py``) and its direct-import unit
tests (``test_logic_foundation``/``test_logic_rigidity``/``test_logic_witness_policy``)
have been retired; the lowering's correctness is now gated end-to-end through the
runner against the committed conformance goldens.

This module keeps the two runner-/graph-level acceptance anchors that do NOT depend
on the retired oracle:

* :func:`test_real_ontology_is_clean_over_all_lints` — AC#2 regression: the real
  merged gmeow ontology stays clean under every reasoning lint, so the lowering work
  never perturbed the production ontology.
* :func:`test_foundation_conformance_cases_are_green` — AC#1/#2 end-to-end: every
  ``conformance/logic/cases/foundation/`` case runs clean against its goldens
  (materialized.nq, verdicts.json, certification.json, projections/, and the
  content-addressed ``expected/explanation/*.md`` cited-IRI skeletons) through
  :func:`gmeow_tools.logic_runner.run` / :func:`~.logic_runner.diff_case` — the same
  gate ``gmeow-dev conformance`` (20/20) enforces.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.logic_runner import diff_case, run

# Graph-accepting shim: serialize the merged graph and route it through the
# graph-free production reasoning invariants (#579).
from tests._graph_nt import reasoning_invariants

# --------------------------------------------------------------------------- #
# AC#2 — regression: the real ontology stays clean under every reasoning lint.
# --------------------------------------------------------------------------- #


def test_real_ontology_is_clean_over_all_lints() -> None:
    """AC#2: the real merged gmeow ontology is clean under EVERY reasoning lint.

    Self-contained AC anchor (also asserted by
    :func:`tests.test_reasoning_lint.test_real_ontology_is_clean`): the foundation
    lowering must not have perturbed the real ontology, and ``reasoning_invariants``
    runs the full stereotype set (all four lowering lints plus the coequal-facet /
    frame-completeness invariants).
    """
    assert reasoning_invariants(load_merged_graph()) == []


# --------------------------------------------------------------------------- #
# AC#1/#2 — end-to-end: the foundation conformance cases stay green.
# --------------------------------------------------------------------------- #


def _foundation_cases_root() -> Path:
    """The ``conformance/logic/cases/foundation/`` directory under the worktree."""
    return (
        Path(__file__).resolve().parents[1]
        / "conformance"
        / "logic"
        / "cases"
        / "foundation"
    )


def test_foundation_conformance_cases_are_green() -> None:
    """AC#1/#2: the six foundation conformance cases run clean against their goldens.

    Invokes the runner over each ``conformance/logic/cases/foundation/`` case and
    asserts :func:`gmeow_tools.logic_runner.diff_case` reports no differences — the
    same gate ``gmeow-dev conformance`` (20/20) enforces.  The lowering is evaluated
    natively by ``gmeow_logic.foundation`` (issue #636); this gate proves the runner
    wiring + native provenance reproduce the committed goldens exactly.  A case
    directory needs both ``input.logic.ttl`` and ``profile.json``; bare marker dirs
    (``.gitkeep``) are skipped.
    """
    root = _foundation_cases_root()
    assert root.is_dir(), f"foundation cases dir missing: {root}"
    case_dirs = sorted(
        d
        for d in root.iterdir()
        if d.is_dir()
        and (d / "input.logic.ttl").exists()
        and (d / "profile.json").exists()
    )
    # The six foundation cases (one per lowered discipline + the cross-world case).
    pytest.importorskip(
        "gmeow_logic",
        reason="gmeow_logic native extension not installed — run 'make logic-py' first",
    )
    assert len(case_dirs) == 6, [d.name for d in case_dirs]
    for case_dir in case_dirs:
        outputs = run(case_dir)
        result = diff_case(outputs)
        assert result.passed, f"{case_dir.name}: {result.diffs}"

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Registered generator: logic: source → IR → 8 committed generated artifacts.

``gmeow logic compile`` (or ``gmeow regenerate logic``) renders, from the
canonical ``logic:`` vocabulary source at ``slices/core/logic/module.ttl``:

* ``generated/owl/gmeow-dl.ttl``            — OWL 2 DL projection
* ``generated/owl/gmeow-el.ttl``            — OWL 2 EL projection
* ``generated/datalog/gmeow.dl``            — Datalog projection
* ``generated/n3/gmeow.n3``                 — N3 rules projection
* ``generated/foundation/gufo.ttl``         — gUFO bridge projection
* ``generated/logic/gmeow.logic.rdf12.ttl`` — canonical RDF 1.2 artifact
* ``generated/logic/gmeow.rls``             — Nemo (.rls) projection
* ``generated/logic/projection-report.ttl`` — preservation ledger (loss ledger)

Every artifact is drift-gated via the registered generator framework
(``gmeow regenerate`` / ``make check-generated``).  The overclaim gate runs
before any write reaches disk (CONSTITUTION Principle 7).

This is the CLI face of the #500 logic compiler (Task 4); committed outputs are
kept aligned with the same registered-generator discipline as the native
statements stage.
"""

from __future__ import annotations

import logging
from pathlib import Path

import gmeow_logic

from gmeow_tools import diagnostics
from gmeow_tools.config import GENERATED_DIR, PROJECT_ROOT, SLICES_DIR

log = logging.getLogger(__name__)

#: Canonical tool/code namespace for this surface's diagnostics (#809).
TOOL = "logic-compile"

# --------------------------------------------------------------------------- #
# Canonical input + output paths
# --------------------------------------------------------------------------- #

#: The single authoritative logic: source for the GMEOW vocabulary.
LOGIC_SOURCE_FILE = SLICES_DIR / "core" / "logic" / "module.ttl"

#: The 8 committed outputs, in declaration order.
LOGIC_OWL_DL_FILE = GENERATED_DIR / "owl" / "gmeow-dl.ttl"
LOGIC_OWL_EL_FILE = GENERATED_DIR / "owl" / "gmeow-el.ttl"
LOGIC_DATALOG_FILE = GENERATED_DIR / "datalog" / "gmeow.dl"
LOGIC_N3_FILE = GENERATED_DIR / "n3" / "gmeow.n3"
LOGIC_GUFO_FILE = GENERATED_DIR / "foundation" / "gufo.ttl"
LOGIC_RDF12_FILE = GENERATED_DIR / "logic" / "gmeow.logic.rdf12.ttl"
LOGIC_NEMO_FILE = GENERATED_DIR / "logic" / "gmeow.rls"
LOGIC_REPORT_FILE = GENERATED_DIR / "logic" / "projection-report.ttl"


def _rel_str(path: Path) -> str:
    try:
        return str(path.relative_to(PROJECT_ROOT))
    except ValueError:
        return str(path)


# --------------------------------------------------------------------------- #
# Canonical diagnostics surface (#809)
# --------------------------------------------------------------------------- #


def compile_diagnostics_report(
    *,
    tool: str = TOOL,
) -> diagnostics.DiagnosticsReport:
    """Compile the logic: source and return its native diagnostics report.

    The dev-gate surface entry point (folded into the feedback bundle, #809). The
    parse diagnostics are built into the canonical ``Finding`` model **in Rust**
    (``.goals`` RUST-FIRST/PYTHON-SURFACE, #856): ``gmeow_logic.compile_logic``
    returns a live, normalized ``diagnostics_report`` and this surface forwards it
    directly — no Python re-shaping. A hard compile failure (the Rust overclaim /
    rule-safety gate raising ``ValueError``) is itself surfaced as a single
    ``logic-compile.failed`` error finding so the failure reaches SARIF/JSON/HTML
    instead of terminating on stderr.
    """
    source_ttl = LOGIC_SOURCE_FILE.read_text(encoding="utf-8")
    try:
        result = gmeow_logic.compile_logic(source_ttl)
    except ValueError as exc:
        return diagnostics.report_from_findings(
            tool=tool,
            findings=[
                diagnostics.finding(
                    severity="error",
                    code=f"{tool}.failed",
                    message=f"logic: compile failed: {exc}",
                    tool=tool,
                    path=_rel_str(LOGIC_SOURCE_FILE),
                )
            ],
        )
    return result["diagnostics_report"]


# --------------------------------------------------------------------------- #
# Registered generator
# --------------------------------------------------------------------------- #

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Registered generator: logic: source → IR → 7 committed generated artifacts.

``gmeow logic compile`` (or ``gmeow regenerate logic``) renders, from the
canonical ``logic:`` vocabulary source at ``slices/core/logic/module.ttl``:

* ``generated/owl/gmeow-dl.ttl``            — OWL 2 DL projection
* ``generated/owl/gmeow-el.ttl``            — OWL 2 EL projection
* ``generated/datalog/gmeow.dl``            — Datalog projection
* ``generated/n3/gmeow.n3``                 — N3 rules projection
* ``generated/foundation/gufo.ttl``         — gUFO bridge projection
* ``generated/logic/gmeow.logic.rdf12.ttl`` — canonical RDF 1.2 artifact
* ``generated/logic/projection-report.ttl`` — preservation ledger

Every artifact is drift-gated via the registered generator framework
(``gmeow regenerate`` / ``make check-generated``).  The overclaim gate runs
before any write reaches disk (CONSTITUTION Principle 7).

This mirrors :mod:`gmeow_tools.statement_compile` (the EXEMPLAR) and is the
CLI face of the #500 logic compiler (Task 4).
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path

from gmeow_tools.config import GENERATED_DIR, PROJECT_ROOT, SLICES_DIR
from gmeow_tools.generator import Generator, rdf_compare, register
from gmeow_tools.logic_frontend import parse_logic_source
from gmeow_tools.logic_projections import (
    OverclaimError,
    build_projection_report,
    project_canonical_rdf12,
    project_datalog,
    project_gufo,
    project_n3,
    project_owl_dl,
    project_owl_el,
)

# --------------------------------------------------------------------------- #
# Canonical input + output paths
# --------------------------------------------------------------------------- #

#: The single authoritative logic: source for the GMEOW vocabulary.
LOGIC_SOURCE_FILE = SLICES_DIR / "core" / "logic" / "module.ttl"

#: The 7 committed outputs, in declaration order.
LOGIC_OWL_DL_FILE = GENERATED_DIR / "owl" / "gmeow-dl.ttl"
LOGIC_OWL_EL_FILE = GENERATED_DIR / "owl" / "gmeow-el.ttl"
LOGIC_DATALOG_FILE = GENERATED_DIR / "datalog" / "gmeow.dl"
LOGIC_N3_FILE = GENERATED_DIR / "n3" / "gmeow.n3"
LOGIC_GUFO_FILE = GENERATED_DIR / "foundation" / "gufo.ttl"
LOGIC_RDF12_FILE = GENERATED_DIR / "logic" / "gmeow.logic.rdf12.ttl"
LOGIC_REPORT_FILE = GENERATED_DIR / "logic" / "projection-report.ttl"


def _rel_str(path: Path) -> str:
    try:
        return str(path.relative_to(PROJECT_ROOT))
    except ValueError:
        return str(path)


# --------------------------------------------------------------------------- #
# Registered generator
# --------------------------------------------------------------------------- #


@register
class LogicGenerator(Generator):
    """Compile slices/core/logic/module.ttl → 7 generated logic artifacts."""

    name: str = "logic"
    #: Internal x-gmeow-* tags are canonical; they survive the leak gate (#287).
    allows_internal_tags: bool = True

    @property
    def inputs(self) -> Sequence[Path]:
        """The logic: source vocabulary file is the sole authoring input."""
        return [LOGIC_SOURCE_FILE]

    @property
    def outputs(self) -> Sequence[Path]:
        """All 7 committed artifacts owned by this generator."""
        return [
            LOGIC_OWL_DL_FILE,
            LOGIC_OWL_EL_FILE,
            LOGIC_DATALOG_FILE,
            LOGIC_N3_FILE,
            LOGIC_GUFO_FILE,
            LOGIC_RDF12_FILE,
            LOGIC_REPORT_FILE,
        ]

    def render(self, staging: Path) -> None:
        """Parse the logic: source and write all 7 artifacts into *staging*.

        The overclaim gate (:func:`~.logic_projections.assert_no_overclaim`) is
        invoked inside each projection function — any overclaim raises
        :class:`~.logic_projections.OverclaimError` before a single byte is
        written to the committed tree (CONSTITUTION Principle 7).
        """
        from gmeow_tools.mapping_dsl import CompileError

        # --- Parse -------------------------------------------------------
        try:
            program, diagnostics = parse_logic_source(LOGIC_SOURCE_FILE)
        except Exception as exc:
            raise CompileError(f"logic: source parse failed: {exc}") from exc

        # Log any recoverable diagnostics (they do not block the build).
        import logging

        log = logging.getLogger(__name__)
        for diag in diagnostics:
            log.warning(
                "logic: parse diagnostic [%s] %s: %s",
                diag.severity,
                diag.code,
                diag.message,
            )

        # --- Output paths inside the staging tree ------------------------
        def _staged(committed: Path) -> Path:
            return staging / committed.relative_to(PROJECT_ROOT)

        owl_dl_path = _staged(LOGIC_OWL_DL_FILE)
        owl_el_path = _staged(LOGIC_OWL_EL_FILE)
        datalog_path = _staged(LOGIC_DATALOG_FILE)
        n3_path = _staged(LOGIC_N3_FILE)
        gufo_path = _staged(LOGIC_GUFO_FILE)
        rdf12_path = _staged(LOGIC_RDF12_FILE)
        report_path = _staged(LOGIC_REPORT_FILE)

        # --- Run all 6 back-ends (overclaim gate fires inside each) ------
        try:
            r_dl = project_owl_dl(program, path=owl_dl_path)
            r_el = project_owl_el(program, path=owl_el_path)
            r_dl_r = project_datalog(program, path=datalog_path)
            r_n3 = project_n3(program, path=n3_path)
            r_gufo = project_gufo(program, path=gufo_path)
            r_rdf12 = project_canonical_rdf12(program, path=rdf12_path)
        except OverclaimError as exc:
            raise CompileError(f"logic: overclaim gate blocked emit: {exc}") from exc

        # --- Projection report (also runs the per-projection overclaim gate) ---
        try:
            build_projection_report(
                program,
                [r_dl, r_el, r_dl_r, r_n3, r_gufo, r_rdf12],
                path=report_path,
            )
        except OverclaimError as exc:
            raise CompileError(f"logic: overclaim in report: {exc}") from exc

    def compare(self, fresh: Path, committed: Path) -> list[str]:
        """Graph-isomorphism for RDF/Turtle; byte-normalized compare for Datalog."""
        if committed == LOGIC_DATALOG_FILE:
            # Plain text: normalize line endings then compare
            if not committed.exists():
                return [f"{_rel_str(committed)} (missing committed file)"]
            if not fresh.exists():
                return [f"{_rel_str(committed)} (not produced in staging)"]
            fresh_text = fresh.read_text(encoding="utf-8").rstrip("\n") + "\n"
            committed_text = committed.read_text(encoding="utf-8").rstrip("\n") + "\n"
            if fresh_text != committed_text:
                return [_rel_str(committed)]
            return []

        if committed == LOGIC_N3_FILE:
            # N3 text: normalize line endings
            if not committed.exists():
                return [f"{_rel_str(committed)} (missing committed file)"]
            if not fresh.exists():
                return [f"{_rel_str(committed)} (not produced in staging)"]
            fresh_text = fresh.read_text(encoding="utf-8").rstrip("\n") + "\n"
            committed_text = committed.read_text(encoding="utf-8").rstrip("\n") + "\n"
            if fresh_text != committed_text:
                return [_rel_str(committed)]
            return []

        # All other outputs are RDF/Turtle — use graph isomorphism.
        return rdf_compare(fresh, committed)

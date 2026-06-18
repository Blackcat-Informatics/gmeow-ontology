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

This mirrors :mod:`gmeow_tools.statement_compile` (the EXEMPLAR) and is the
CLI face of the #500 logic compiler (Task 4).
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path

import gmeow_logic

from gmeow_tools.config import GENERATED_DIR, PROJECT_ROOT, SLICES_DIR
from gmeow_tools.generator import Generator, rdf_compare, register

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
# Registered generator
# --------------------------------------------------------------------------- #


@register
class LogicGenerator(Generator):
    """Compile slices/core/logic/module.ttl → 8 generated logic artifacts."""

    name: str = "logic"
    #: Internal x-gmeow-* tags are canonical; they survive the leak gate (#287).
    allows_internal_tags: bool = True

    @property
    def inputs(self) -> Sequence[Path]:
        """The logic: source vocabulary file is the sole authoring input."""
        return [LOGIC_SOURCE_FILE]

    @property
    def outputs(self) -> Sequence[Path]:
        """All 8 committed artifacts owned by this generator."""
        return [
            LOGIC_OWL_DL_FILE,
            LOGIC_OWL_EL_FILE,
            LOGIC_DATALOG_FILE,
            LOGIC_N3_FILE,
            LOGIC_GUFO_FILE,
            LOGIC_RDF12_FILE,
            LOGIC_NEMO_FILE,
            LOGIC_REPORT_FILE,
        ]

    def render(self, staging: Path) -> None:
        """Compile the logic: source (in Rust) and write all 8 artifacts.

        The whole frontend → IR → 7-projections + report pipeline now runs in the
        native ``gmeow_logic.compile_logic`` Rust compiler (#664).  The overclaim
        gate (CONSTITUTION Principle 7) and the Nemo rule-safety check fire inside
        the Rust ``compile_program`` — either raises ``ValueError`` before a single
        byte is written to the committed tree.
        """
        from gmeow_tools.mapping_dsl import CompileError

        # --- Compile (Rust): source Turtle → the 8 artifact strings ------
        source_ttl = LOGIC_SOURCE_FILE.read_text(encoding="utf-8")
        try:
            artifacts = gmeow_logic.compile_logic(source_ttl)
        except ValueError as exc:
            raise CompileError(f"logic: compile failed: {exc}") from exc

        # --- Map artifact name → committed path, write into staging ------
        def _staged(committed: Path) -> Path:
            return staging / committed.relative_to(PROJECT_ROOT)

        outputs: dict[str, Path] = {
            "owl_dl": LOGIC_OWL_DL_FILE,
            "owl_el": LOGIC_OWL_EL_FILE,
            "datalog": LOGIC_DATALOG_FILE,
            "n3": LOGIC_N3_FILE,
            "gufo": LOGIC_GUFO_FILE,
            "canonical_rdf12": LOGIC_RDF12_FILE,
            "nemo": LOGIC_NEMO_FILE,
            "report": LOGIC_REPORT_FILE,
        }
        for key, committed in outputs.items():
            staged = _staged(committed)
            staged.parent.mkdir(parents=True, exist_ok=True)
            staged.write_text(artifacts[key], encoding="utf-8")

    def compare(self, fresh: Path, committed: Path) -> list[str]:
        """Graph-isomorphism for RDF/Turtle; byte-normalized compare for text."""
        # Plain-text targets (Datalog, N3, Nemo): normalize line endings, then compare.
        if committed in {LOGIC_DATALOG_FILE, LOGIC_N3_FILE, LOGIC_NEMO_FILE}:
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

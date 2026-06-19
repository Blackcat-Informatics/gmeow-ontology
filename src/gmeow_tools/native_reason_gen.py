# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Registered generator: native EL/DL reasoning → 3 committed artifacts.

``gmeow regenerate native-reasoning`` runs the Java/Docker-free native EL/DL
reasoning lane (``gmeow_logic.reason_native``, Principle 17/18) over the
committed GTS bundle and renders, from that single native result:

* ``generated/logic/inferred-closure.rdf12.ttl``      — told-vs-inferred closure
* ``generated/logic/reasoning-explanations.rdf12.ttl`` — per-axiom proof skeletons
* ``generated/logic/dl-el-crosscheck-report.ttl``     — native↔oracle ledger
  (REPORT-ONLY; the ELK/HermiT oracle comparison + divergence enforcement are
  deferred to the ``classic-cross-check`` lane in #666 so this generator — and
  therefore ``regenerate`` / ``make check`` — stays Java/Docker-free).

Every artifact is drift-gated via the registered generator framework
(``gmeow regenerate`` / ``make check-generated``) using RDF-graph isomorphism,
so an isomorphic re-serialization is itself drift (CONSTITUTION Principle 7).

This mirrors :mod:`gmeow_tools.logic_compile` (the EXEMPLAR) and is the
generator face of the #665 native reasoning authority (Task 5).
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path

import gmeow_logic
import pyoxigraph

from gmeow_tools import reason
from gmeow_tools.config import GENERATED_DIR, GTS_SNAPSHOT_FILE, PROJECT_ROOT
from gmeow_tools.generator import Generator, register

# --------------------------------------------------------------------------- #
# Committed output paths
# --------------------------------------------------------------------------- #

#: The native told-vs-inferred closure (RDF 1.2, per-triple derivation).
NATIVE_CLOSURE_FILE = GENERATED_DIR / "logic" / "inferred-closure.rdf12.ttl"
#: Per-axiom proof skeletons (RDF 1.2).
NATIVE_EXPLANATIONS_FILE = GENERATED_DIR / "logic" / "reasoning-explanations.rdf12.ttl"
#: The report-only native↔oracle DL/EL crosscheck ledger (#666 enforces).
NATIVE_LEDGER_FILE = GENERATED_DIR / "logic" / "dl-el-crosscheck-report.ttl"


def _rel(path: Path) -> str:
    try:
        return str(path.relative_to(PROJECT_ROOT))
    except ValueError:
        return str(path)


def _canonical_quads(path: Path) -> list[str]:
    """Parse an RDF 1.2 Turtle file and return its canonical quad strings.

    Uses pyoxigraph because the artifacts carry RDF 1.2 triple terms
    (``<< … >>``), which rdflib's Turtle parser (and therefore the framework's
    rdflib-based ``rdf_compare``) cannot read. The RDFC-1.0 canonicalization
    gives blank-node-stable output, so an isomorphic re-serialization compares
    equal — a foreign serialization of an isomorphic graph is still drift
    (CONSTITUTION Principle 7), exactly like ``rdf_compare`` but star-aware.
    """
    dataset = pyoxigraph.Dataset()
    for quad in pyoxigraph.parse(path.read_bytes(), format=pyoxigraph.RdfFormat.TURTLE):
        dataset.add(pyoxigraph.Quad(quad.subject, quad.predicate, quad.object))
    dataset.canonicalize(pyoxigraph.CanonicalizationAlgorithm.RDFC_1_0)
    return sorted(str(quad) for quad in dataset)


# --------------------------------------------------------------------------- #
# Registered generator
# --------------------------------------------------------------------------- #


@register
class NativeReasoningGenerator(Generator):
    """Render the 3 native EL/DL reasoning artifacts from the GTS bundle."""

    name: str = "native-reasoning"

    @property
    def inputs(self) -> Sequence[Path]:
        """The committed GTS snapshot is the sole reasoning input."""
        return [GTS_SNAPSHOT_FILE]

    @property
    def outputs(self) -> Sequence[Path]:
        """The 3 committed artifacts owned by this generator, in order."""
        return [
            NATIVE_CLOSURE_FILE,
            NATIVE_EXPLANATIONS_FILE,
            NATIVE_LEDGER_FILE,
        ]

    def render(self, staging: Path) -> None:
        """Reason the bundle (native, Rust) and write all 3 artifacts.

        The whole closure / explanations / ledger pipeline runs from the single
        ``gmeow_logic.reason_native`` Rust result. No Docker, no Java: the
        ELK/HermiT oracle comparison is deferred to ``classic-cross-check``
        (#666), so the ledger is built from native results only.
        """
        result = gmeow_logic.reason_native(GTS_SNAPSHOT_FILE.read_bytes())

        artifacts: dict[Path, str] = {
            NATIVE_CLOSURE_FILE: reason.build_inferred_closure_ttl(result),
            NATIVE_EXPLANATIONS_FILE: reason.build_explanations_ttl(result),
            NATIVE_LEDGER_FILE: reason.build_dl_el_ledger_ttl(result),
        }
        for committed, content in artifacts.items():
            staged = staging / committed.relative_to(PROJECT_ROOT)
            staged.parent.mkdir(parents=True, exist_ok=True)
            staged.write_text(content, encoding="utf-8")

    def compare(self, fresh: Path, committed: Path) -> list[str]:
        """RDF 1.2 graph-isomorphism drift compare (pyoxigraph, star-aware).

        All 3 outputs carry RDF 1.2 triple terms, so the rdflib-based
        ``rdf_compare`` cannot parse them; canonicalized quad-set equality via
        pyoxigraph is the order- and blank-node-independent equivalent.
        """
        rel = _rel(committed)
        if not committed.exists():
            return [f"{rel} (missing committed file)"]
        if not fresh.exists():
            return [f"{rel} (not produced in staging)"]
        try:
            if _canonical_quads(fresh) != _canonical_quads(committed):
                return [rel]
        except (ValueError, SyntaxError) as exc:
            return [f"{rel} (parse error: {exc})"]
        return []

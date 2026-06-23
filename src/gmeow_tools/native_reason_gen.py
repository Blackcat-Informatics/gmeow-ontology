# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Native EL/DL reasoning artifact paths + the RDF-1.2 drift comparator.

The native EL/DL reasoning lane (``gmeow_logic.reason_native``, Principle
17/18) produces three committed artifacts:

* ``generated/logic/inferred-closure.rdf12.ttl``      — told-vs-inferred closure
* ``generated/logic/reasoning-explanations.rdf12.ttl`` — per-axiom proof skeletons
* ``generated/logic/dl-el-crosscheck-report.ttl``     — native↔oracle ledger

#861 P7 retired the Python build orchestrator: the Rust ``stage-reason`` leaf of
the ``gmeow-pipeline`` executor renders and drift-gates these artifacts. What
survives here are the committed-path constants and :func:`_canonical_quads`, the
star-aware RDF-1.2 canonical comparator the GTS composer still consumes.
"""

from __future__ import annotations

from pathlib import Path

import gmeow_rdf

from gmeow_tools.config import GENERATED_DIR

# --------------------------------------------------------------------------- #
# Committed output paths
# --------------------------------------------------------------------------- #

#: The native told-vs-inferred closure (RDF 1.2, per-triple derivation).
NATIVE_CLOSURE_FILE = GENERATED_DIR / "logic" / "inferred-closure.rdf12.ttl"
#: Per-axiom proof skeletons (RDF 1.2).
NATIVE_EXPLANATIONS_FILE = GENERATED_DIR / "logic" / "reasoning-explanations.rdf12.ttl"
#: The report-only native↔oracle DL/EL crosscheck ledger (#666 enforces).
NATIVE_LEDGER_FILE = GENERATED_DIR / "logic" / "dl-el-crosscheck-report.ttl"


def _canonical_quads(path: Path) -> list[str]:
    """Parse an RDF 1.2 Turtle file and return its canonical quad strings.

    Uses gmeow_rdf because the artifacts carry RDF 1.2 triple terms
    (``<< … >>``), which rdflib's Turtle parser (and therefore the framework's
    rdflib-based ``rdf_compare``) cannot read. The RDFC-1.0 canonicalization
    gives blank-node-stable output, so an isomorphic re-serialization compares
    equal — a foreign serialization of an isomorphic graph is still drift
    (CONSTITUTION Principle 7), exactly like ``rdf_compare`` but star-aware.
    """
    dataset = gmeow_rdf.Dataset()
    for quad in gmeow_rdf.parse(path.read_bytes(), format=gmeow_rdf.RdfFormat.TURTLE):
        dataset.add(gmeow_rdf.Quad(quad.subject, quad.predicate, quad.object))
    dataset.canonicalize(gmeow_rdf.CanonicalizationAlgorithm.RDFC_1_0)
    return sorted(str(quad) for quad in dataset)

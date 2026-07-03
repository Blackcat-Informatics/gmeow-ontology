# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The norms extension + rights graft (#351 / #352, EPIC #348) — RETAINED tests.

All asserted-TBox invariants and competency questions have been migrated to the
slice-resident declarative test-DSL:

  - slices/extensions/norms/tests/structural.ttl
  - slices/extensions/norms/tests/competency.ttl
  - slices/core/rights/tests/structural.ttl (Permission/Prohibition/Duty classhood)

What remains here is the single cross-slice file-load check that cannot be
expressed as a module-scoped SPARQL ASK cell:

  - test_graft_axioms_live_extension_side_only: loads slices/core/rights/module.ttl
    as a separate graph and asserts that no norms-extension IRIs appear there.
"""

from __future__ import annotations

from pathlib import Path

from purrdf.compat.rdflib import Graph, Namespace

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)


# --------------------------------------------------------------------------- #
# The rights graft (#352) — cross-slice file-load check (not migratable to DSL)
# --------------------------------------------------------------------------- #


def test_graft_axioms_live_extension_side_only() -> None:
    """Zero core churn: the core rights module contains no reference to any
    norms-extension IRI — the graft is asserted in the norms module."""
    core_rights = Graph()
    core_rights.parse(
        Path(__file__).parent.parent / "slices" / "core" / "rights" / "module.ttl",
        format="turtle",
    )
    norms_terms = [GM.Norm, GM.deonticModality, GM.normIssuer, GM.normBearer]
    for term in norms_terms:
        assert not list(core_rights.triples((term, None, None))), f"{term} as subject"
        assert not list(core_rights.triples((None, term, None))), f"{term} as predicate"
        assert not list(core_rights.triples((None, None, term))), f"{term} as object"

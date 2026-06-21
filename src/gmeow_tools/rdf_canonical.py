# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Native RDFC-1.0 graph comparison — the rdflib-free ``isomorphic`` replacement.

``rdflib.compare.isomorphic`` is the only blank-node-aware graph-equality primitive
the rdflib API offers, but it runs in Python and **cannot read the RDF 1.2
``<< … >>`` triple terms** the statement layer emits. This module routes graph
equality through the in-repo ``gmeow_rdf`` kernel instead (issue #630): each graph
is serialized to N-Triples, parsed by the native kernel, and RDFC-1.0
canonicalized. RDFC-1.0 makes isomorphic graphs byte-identical, so equal canonical
quad-sets ⇔ graph isomorphism — the exact verdict ``isomorphic`` returned, but
computed natively (Rust) and RDF-1.2-safe.

The ``classic-cross-check`` Jena/Docker oracle lane keeps rdflib by design (it is
the independent oracle); everything else compares natively through here.
"""

from __future__ import annotations

from rdflib import Graph


def _canonical_quads(nt: str) -> list[str]:
    """RDFC-1.0-canonicalize an N-Triples document to a sorted N-Quads line list."""
    import gmeow_rdf

    dataset = gmeow_rdf.Dataset()
    for quad in gmeow_rdf.parse(
        nt.encode("utf-8"), format=gmeow_rdf.RdfFormat.N_TRIPLES
    ):
        dataset.add(
            gmeow_rdf.Quad(quad.subject, quad.predicate, quad.object, quad.graph_name)
        )
    dataset.canonicalize(gmeow_rdf.CanonicalizationAlgorithm.RDFC_1_0)
    return sorted(str(quad) for quad in dataset)


def graphs_isomorphic(a: Graph, b: Graph) -> bool:
    """Return whether two rdflib graphs are isomorphic, computed natively.

    The drop-in native replacement for ``rdflib.compare.isomorphic(a, b)`` (#630):
    both graphs are RDFC-1.0 canonicalized via ``gmeow_rdf`` and their canonical
    quad-sets compared. Equivalent to ``isomorphic`` for RDF 1.1 graphs and
    additionally correct for RDF 1.2 triple-term content.
    """
    return _canonical_quads(a.serialize(format="nt")) == _canonical_quads(
        b.serialize(format="nt")
    )

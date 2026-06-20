# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""rdflib I/O adapter for the native OWL 2 RL closure (issue #630).

The production reasoning core (:mod:`gmeow_tools.native_rl`) is rdflib-free: it
takes a serialized graph and returns the RL closure as an N-Triples string,
round-tripping through the in-repo ``gmeow_rdf`` kernel only. This module is the
**caller-boundary adapter** for the rdflib-native consumers — the
competency/observation test suites and the ``rl_agreement`` classic-cross-check
oracle, which build, mutate and SPARQL-query rdflib graphs and compare against
``owlrl`` (itself rdflib-based). It serializes the caller's rdflib graph to
N-Triples, runs the native closure, and folds every closed triple back into the
graph in place — the exact ``owlrl.DeductiveClosure(...).expand(graph)`` contract.
"""

from __future__ import annotations

from rdflib import BNode, Graph, Literal, URIRef
from rdflib.term import Node

from gmeow_tools.native_rl import native_rl_closure as _native_rl_closure_nt

#: XSD string datatype IRI — rdflib renders a plain ``xsd:string`` literal without
#: a datatype, so a ``"v"^^<xsd:string>`` from the engine collapses to a plain one.
_XSD_STRING = "http://www.w3.org/2001/XMLSchema#string"


def native_rl_closure(graph: Graph) -> Graph:
    """Expand ``graph`` under OWL 2 RL in place — the native ``owlrl.expand`` twin.

    Computes the RL deductive closure via the rdflib-free native core
    (:func:`gmeow_tools.native_rl.native_rl_closure`), then adds every closed
    triple to ``graph`` in place and returns it (so both the in-place-mutation and
    returned-graph call styles work). Blank nodes round-trip as blank nodes;
    literals keep their datatype/language; named graphs (if the caller used a
    ``Dataset``) are NOT supported here — the suites use a single default graph,
    which closes in one world.

    Args:
        graph: The rdflib graph to close (mutated in place).

    Returns:
        The same ``graph`` object, now carrying the RL closure.
    """
    import gmeow_rdf

    nt = graph.serialize(format="nt")
    closure_nt = _native_rl_closure_nt(nt)
    if not closure_nt.strip():
        return graph

    bnodes: dict[str, BNode] = {}

    def _to_rdflib(term: object) -> Node:
        if isinstance(term, gmeow_rdf.NamedNode):
            return URIRef(term.value)
        if isinstance(term, gmeow_rdf.BlankNode):
            return bnodes.setdefault(term.value, BNode())
        if isinstance(term, gmeow_rdf.Literal):
            lang = term.language
            if lang is not None:
                return Literal(term.value, lang=lang)
            dt = term.datatype.value
            if dt == _XSD_STRING:
                return Literal(term.value)
            return Literal(term.value, datatype=URIRef(dt))
        raise TypeError(f"unexpected gmeow_rdf term in RL closure: {term!r}")

    for quad in gmeow_rdf.parse(
        closure_nt.encode("utf-8"), format=gmeow_rdf.RdfFormat.N_TRIPLES
    ):
        s = _to_rdflib(quad.subject)
        p = _to_rdflib(quad.predicate)
        o = _to_rdflib(quad.object)
        graph.add((s, p, o))
    return graph

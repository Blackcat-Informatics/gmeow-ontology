# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""rdflib I/O adapter for the native OWL 2 RL closure (issue #630).

The production reasoning core (:mod:`gmeow_tools.native_rl`) is rdflib-free. This
module is the **caller-boundary adapter** for the rdflib-native consumers — the
competency/observation test suites and the ``rl_agreement`` classic-cross-check
oracle, which build, mutate and SPARQL-query rdflib graphs and compare against
``owlrl`` (itself rdflib-based).

Single FFI boundary (issue #630)
--------------------------------
The closure is computed by **one** native call, ``gmeow_logic.rl_closure_quads``,
which parses the serialized graph, runs the OWL 2 RL chase, and returns the full
closure as live ``gmeow_rdf.Quad`` objects — there is no intermediate N-Triples
string the Python side renders and re-parses (the old Rust→Python→Rust seam is
gone). The adapter serializes the caller's rdflib graph once, hands the bytes to
Rust, and folds the returned quads back into the graph in place — the exact
``owlrl.DeductiveClosure(...).expand(graph)`` contract.
"""

from __future__ import annotations

from rdflib import BNode, Graph, Literal, URIRef
from rdflib.term import Node

#: XSD string datatype IRI — rdflib renders a plain ``xsd:string`` literal without
#: a datatype, so a ``"v"^^<xsd:string>`` from the engine collapses to a plain one.
_XSD_STRING = "http://www.w3.org/2001/XMLSchema#string"


def native_rl_closure(graph: Graph) -> Graph:
    """Expand ``graph`` under OWL 2 RL in place — the native ``owlrl.expand`` twin.

    Computes the RL deductive closure via a single native call
    (``gmeow_logic.rl_closure_quads``), then adds every closed triple to ``graph``
    in place and returns it (so both the in-place-mutation and returned-graph call
    styles work). Blank nodes round-trip as blank nodes; literals keep their
    datatype/language; named graphs (if the caller used a ``Dataset``) are NOT
    supported here — the suites use a single default graph, which closes in one
    world.

    Args:
        graph: The rdflib graph to close (mutated in place).

    Returns:
        The same ``graph`` object, now carrying the RL closure.
    """
    import gmeow_logic
    import gmeow_rdf

    nt = graph.serialize(format="nt")
    quads = gmeow_logic.rl_closure_quads(nt)
    if not quads:
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

    for quad in quads:
        s = _to_rdflib(quad.subject)
        p = _to_rdflib(quad.predicate)
        o = _to_rdflib(quad.object)
        graph.add((s, p, o))
    return graph

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Native OWL 2 RL deductive closure — the Docker-free primary entailment authority.

This is the drop-in replacement for the ``owlrl`` deductive-closure baseline
(``owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)``) the conformance
suites used to call (issue #666, Task 5). ``owlrl`` is relocated to the
classic-cross-check lane as the agreement *oracle* — it is no longer required for
normal use; the primary path is native and Java/Docker-free.

Architecture
------------
The closure is computed **RDF-1.2-first** by the native Rust engine
(``gmeow_logic.rl_closure``): every triple is encoded into a generic 4-ary
``triple(?s, ?p, ?o, ?w)`` Datalog relation (predicate-as-DATA, the world axis
preserved) and the fixed OWL 2 RL/RDF rule set runs through the Nemo chase. The
per-property ternary ``materialize`` seam cannot express RL's property-quantifying
meta-rules (``prp-dom``/``prp-rng``/``prp-trp``/``prp-inv``/``prp-spo*`` etc.),
so the generic-triple encoding in ``crates/logic/src/reason/rl.rs`` is used.

This module owns only the rdflib I/O seam: serialize the caller's graph to
N-Triples, hand it to Rust, and fold the derived triples back into the graph
in place (exactly the ``owlrl.expand`` contract).
"""

from __future__ import annotations

from rdflib import BNode, Graph, Literal, URIRef
from rdflib.term import Node

#: The sentinel world IRI the Rust engine encodes a default-graph triple under
#: (mirrors ``crate::reason::rl::DEFAULT_WORLD``). The conversion suites build an
#: rdflib *default* graph, so the whole closure runs in this single world; the
#: world component is dropped when folding back into the (un-named) graph.
DEFAULT_WORLD = "https://blackcatinformatics.ca/gmeow/graph/rl-default"

#: Skolem IRI prefix the engine mints for a blank node (mirrors
#: ``crate::encode::skolem_iri``). A derived triple whose subject/object is one of
#: these is a blank node in the source graph; we map it back to a fresh BNode so
#: the rdflib graph never grows a spurious ``http(s)://…/skolem/…`` IRI resource.
_SKOLEM_PREFIX = "https://blackcatinformatics.ca/gmeow/skolem/"

#: XSD string datatype IRI — rdflib renders a plain ``xsd:string`` literal without
#: a datatype, so a ``"v"^^<xsd:string>`` from the engine collapses to a plain one.
_XSD_STRING = "http://www.w3.org/2001/XMLSchema#string"


def _parse_object(obj_nt: str, skolems: dict[str, BNode]) -> Node:
    """Parse an engine object term (N3 display form) back into an rdflib node.

    The engine emits IRIs as ``<iri>``, blank nodes as the skolem IRI form, and
    literals as ``"v"`` / ``"v"@lang`` / ``"v"^^<dt>``.
    """
    if obj_nt.startswith("<") and obj_nt.endswith(">"):
        inner = obj_nt[1:-1]
        if inner.startswith(_SKOLEM_PREFIX):
            return skolems.setdefault(inner, BNode())
        return URIRef(inner)
    if obj_nt.startswith('"'):
        # Find the closing quote (engine display form never escapes it mid-value
        # in a way that survives to here for the suites' fixtures; honour escapes).
        body = obj_nt[1:]
        idx = 0
        out: list[str] = []
        while idx < len(body):
            ch = body[idx]
            if ch == "\\" and idx + 1 < len(body):
                nxt = body[idx + 1]
                escapes = {"\\": "\\", '"': '"', "n": "\n", "r": "\r", "t": "\t"}
                out.append(escapes.get(nxt, nxt))
                idx += 2
                continue
            if ch == '"':
                break
            out.append(ch)
            idx += 1
        value = "".join(out)
        suffix = body[idx + 1 :]
        if suffix.startswith("@"):
            return Literal(value, lang=suffix[1:])
        if suffix.startswith("^^<") and suffix.endswith(">"):
            dt = suffix[3:-1]
            if dt == _XSD_STRING:
                return Literal(value)
            return Literal(value, datatype=URIRef(dt))
        return Literal(value)
    # Bare IRI (subjects/predicates arrive bare; defensive for objects too).
    if obj_nt.startswith(_SKOLEM_PREFIX):
        return skolems.setdefault(obj_nt, BNode())
    return URIRef(obj_nt)


def _parse_subject(value: str, skolems: dict[str, BNode]) -> Node:
    """Parse an engine subject/predicate IRI (bare) back into an rdflib node."""
    if value.startswith(_SKOLEM_PREFIX):
        return skolems.setdefault(value, BNode())
    return URIRef(value)


def native_rl_closure(graph: Graph) -> Graph:
    """Expand ``graph`` under OWL 2 RL in place — the native ``owlrl.expand`` twin.

    Computes the RL deductive closure natively (Rust, Docker/Java-free), then adds
    every derived triple to ``graph`` in place and returns it (so both the
    in-place-mutation and returned-graph call styles work). Blank nodes round-trip
    as blank nodes; literals keep their datatype/language; named graphs (if the
    caller used a ``Dataset``) are NOT supported here — the suites use a single
    default graph, which closes in one world.

    Args:
        graph: The rdflib graph to close (mutated in place).

    Returns:
        The same ``graph`` object, now carrying the RL closure.
    """
    import gmeow_logic

    nt = graph.serialize(format="nt")
    rows = gmeow_logic.rl_closure(nt)

    skolems: dict[str, BNode] = {}
    for subject, predicate, obj_nt, _world, _is_edb in rows:
        s = _parse_subject(subject, skolems)
        p = _parse_subject(predicate, skolems)
        o = _parse_object(obj_nt, skolems)
        graph.add((s, p, o))
    return graph

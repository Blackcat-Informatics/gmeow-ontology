# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Rust-backed up-projection surface (consumer RDF -> GMEOW).

The lawful up-projection authority lives in
``crates/pipeline/src/put_executor.rs`` — the native ``execute_put_legs`` runs
every lawful alignment rule as a SPARQL ``CONSTRUCT`` (the "put leg") through the
native engine. This module is deliberately only a Python surface: it locates the
same repo or bundle inputs the public CLI already uses, serializes an
rdflib-compatible source graph to N-Triples, calls ``gmeow_native.pipeline``, and
adapts the native ``LiftedReport`` to the small dataclass API used by the CLI and
tests.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from types import ModuleType
from typing import cast

from gmeow_rdf.compat.rdflib import BNode, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.term import Node

from gmeow_tools.language_tags import retag_graph_to_internal
from gmeow_tools.up_projection_audit import (
    _ontology_nt,
    _projection_ttls,
    _sssom_texts,
)

GM = Namespace("https://blackcatinformatics.ca/gmeow/")


@dataclass
class UpProjection:
    """The result of an up-projection: graph plus native accounting.

    The lawful put executor reports ``lifted`` facts, ``claimed`` reified claim
    cells, and the ``gap_terms`` it could not lift; the native ``residue`` ledger
    records the dropped heuristic categories (context-descent, tag-resolution,
    ambiguity) honestly rather than carrying inert counters for them.
    """

    graph: Graph
    lifted: int
    gap_terms: dict[str, int]
    claimed: int = 0
    residue: list[str] = field(default_factory=list)


class UnsupportedNativeRdfTermError(TypeError):
    """Raised when the native RDF parser returns a term this adapter cannot map."""

    def __init__(self, value: object) -> None:
        """Initialize with the unsupported native term value."""
        super().__init__(f"unsupported native RDF term: {value!r}")


def _pipeline() -> ModuleType:
    from gmeow_native import pipeline

    return cast(ModuleType, pipeline)


def up_project(source: Graph) -> UpProjection:
    """Lift a consumer graph up to GMEOW through the lawful native put executor.

    Runs every lawful put leg (rename, inverse, and lossy reified claim) as a
    native SPARQL ``CONSTRUCT``; the Rust kernel derives the authoritative lawful
    rule set from the current bundled/repo inputs on every call.
    """
    source_nt = source.serialize(format="nt", encoding="utf-8").decode("utf-8")
    raw = _pipeline().execute_put_legs(
        source_nt,
        list(_sssom_texts()),
        list(_projection_ttls()),
        _ontology_nt(),
    )
    graph = _graph_from_native_nt(raw["graph_nt"])
    retag_graph_to_internal(graph)
    return UpProjection(
        graph=graph,
        lifted=raw["lifted"],
        gap_terms=dict(raw["gap_terms"]),
        claimed=raw["claimed"],
        residue=list(raw["residue"]),
    )


def _graph_from_native_nt(graph_nt: str | bytes) -> Graph:
    """Parse native N-Triples without rdflib parse-scope blank-node rewriting."""
    import gmeow_rdf

    data = graph_nt.encode("utf-8") if isinstance(graph_nt, str) else graph_nt
    graph = Graph()
    if not data.strip():
        return graph
    for quad in gmeow_rdf.parse(data, format=gmeow_rdf.RdfFormat.N_TRIPLES):
        graph.add(
            (
                _rdflib_term(quad.subject),
                _rdflib_term(quad.predicate),
                _rdflib_term(quad.object),
            )
        )
    return graph


def _rdflib_term(value: object) -> Node:
    import gmeow_rdf

    if isinstance(value, gmeow_rdf.NamedNode):
        return URIRef(value.value)
    if isinstance(value, gmeow_rdf.BlankNode):
        return BNode(value.value)
    if isinstance(value, gmeow_rdf.Literal):
        if value.language is not None:
            return Literal(value.value, lang=value.language)
        datatype = value.datatype.value
        if datatype == "http://www.w3.org/2001/XMLSchema#string":
            return Literal(value.value)
        return Literal(value.value, datatype=URIRef(datatype))
    raise UnsupportedNativeRdfTermError(value)

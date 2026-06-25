# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Rust-backed up-projection surface (consumer RDF -> GMEOW, #942).

The up-projection authority lives in ``crates/pipeline/src/up_projection.rs``.
This module is deliberately only a Python surface: it locates the same repo or
bundle inputs the public CLI already uses, serializes rdflib-compatible graphs
to N-Triples, calls ``gmeow_native.pipeline``, and adapts the native report to
the small dataclass API used by the CLI and tests.
"""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass, field
from decimal import Decimal, InvalidOperation
from functools import lru_cache
from types import ModuleType
from typing import cast

from gmeow_rdf.compat.rdflib import BNode, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.term import Node

from gmeow_tools.language_tags import retag_graph_to_internal
from gmeow_tools.up_projection_audit import _ontology_nt, _projection_ttls, _sssom_texts

GM = Namespace("https://blackcatinformatics.ca/gmeow/")

_SKOS = "http://www.w3.org/2004/02/skos/core#"
_RDFS_LABEL = "http://www.w3.org/2000/01/rdf-schema#label"

_ADOPTED_PREDICATES: frozenset[str] = frozenset(
    {_SKOS + "exactMatch", _SKOS + "closeMatch"}
)
_STATEMENT_METADATA_TERMS: frozenset[str] = frozenset(
    {
        str(GM.StatementMetadata),
        str(GM.qSubject),
        str(GM.qPredicate),
        str(GM.qObject),
        str(GM.qObjectLiteral),
    }
)
_NORMALIZED_PREDICATES: dict[str, str] = {
    _SKOS + "prefLabel": _RDFS_LABEL,
    _SKOS + "altLabel": _RDFS_LABEL,
}


@dataclass
class LiftMap:
    """Native lift-map snapshot adapted to the historical Python shape."""

    rules: dict[str, str] = field(default_factory=dict)
    ambiguous: dict[str, set[str]] = field(default_factory=dict)
    inverse_rules: dict[str, str] = field(default_factory=dict)
    claim_rules: dict[str, tuple[str, str]] = field(default_factory=dict)
    object_properties: set[str] = field(default_factory=set)
    value_rules: dict[tuple[str, str], tuple[str, str]] = field(default_factory=dict)


@dataclass
class UpProjection:
    """The result of an up-projection: graph plus native accounting."""

    graph: Graph
    lifted: int
    gap_terms: dict[str, int]
    ambiguous_terms: dict[str, int] = field(default_factory=dict)
    claimed: int = 0
    claim_terms: dict[str, int] = field(default_factory=dict)
    context_resolved: int = 0
    context_terms: dict[str, int] = field(default_factory=dict)
    tag_resolved: int = 0
    tag_resolved_terms: dict[str, int] = field(default_factory=dict)
    minted: int = 0


class UnsupportedLiftMapError(ValueError):
    """Raised when a caller passes a lift map other than the native authority."""

    def __init__(self) -> None:
        """Initialize with the stable public adapter error message."""
        super().__init__("unsupported custom LiftMap")


class UnsupportedNativeRdfTermError(TypeError):
    """Raised when the native RDF parser returns a term this adapter cannot map."""

    def __init__(self, value: object) -> None:
        """Initialize with the unsupported native term value."""
        super().__init__(f"unsupported native RDF term: {value!r}")


def _pipeline() -> ModuleType:
    from gmeow_native import pipeline

    return cast(ModuleType, pipeline)


def _decimal_confidence(conf: str) -> Decimal | None:
    """Return a valid xsd:decimal confidence in [0,1], rejecting exponent forms."""
    if not conf or "e" in conf or "E" in conf:
        return None
    try:
        value = Decimal(conf)
    except InvalidOperation:
        return None
    if not value.is_finite() or value < 0 or value > 1:
        return None
    return value


@lru_cache(maxsize=1)
def _build_lift_map_cached() -> LiftMap:
    """Build the up-projection lift map through the Rust authority."""
    raw = _pipeline().up_projection_build_lift_map(
        list(_sssom_texts()), list(_projection_ttls()), _ontology_nt()
    )
    value_rules = {
        (row["source_predicate"], row["source_value"]): (
            row["gmeow_predicate"],
            row["gmeow_value"],
        )
        for row in raw["value_rules"]
    }
    return LiftMap(
        rules=dict(raw["rules"]),
        ambiguous={k: set(v) for k, v in raw["ambiguous"].items()},
        inverse_rules=dict(raw["inverse_rules"]),
        claim_rules={k: tuple(v) for k, v in raw["claim_rules"].items()},
        object_properties=set(raw["object_properties"]),
        value_rules=value_rules,
    )


def build_lift_map() -> LiftMap:
    """Build the up-projection lift map through the Rust authority."""
    return deepcopy(_build_lift_map_cached())


def up_project(source: Graph, lift: LiftMap | None = None) -> UpProjection:
    """Lift a consumer graph up to GMEOW through the native per-term floor.

    ``lift`` is accepted for the existing call shape; the Rust kernel derives the
    authoritative lift map from the current bundled/repo inputs on every call.
    """
    if lift is not None and lift != build_lift_map():
        raise UnsupportedLiftMapError
    return _native_project(source, descend=False)


def _native_project(source: Graph, *, descend: bool) -> UpProjection:
    source_nt = source.serialize(format="nt", encoding="utf-8").decode("utf-8")
    raw = _pipeline().up_projection_project_nt(
        source_nt,
        list(_sssom_texts()),
        list(_projection_ttls()),
        _ontology_nt(),
        descend,
    )
    graph = _graph_from_native_nt(raw["graph_nt"])
    retag_graph_to_internal(graph)
    return UpProjection(
        graph=graph,
        lifted=raw["lifted"],
        gap_terms=dict(raw["gap_terms"]),
        ambiguous_terms=dict(raw["ambiguous_terms"]),
        claimed=raw["claimed"],
        claim_terms=dict(raw["claim_terms"]),
        context_resolved=raw["context_resolved"],
        context_terms=dict(raw["context_terms"]),
        tag_resolved=raw["tag_resolved"],
        tag_resolved_terms=dict(raw["tag_resolved_terms"]),
        minted=raw["minted"],
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

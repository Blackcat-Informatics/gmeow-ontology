# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Ontology-derived suppression vocabulary for projection/transpile paths."""

from __future__ import annotations

from dataclasses import dataclass
from functools import lru_cache

from gmeow_rdf.compat.rdflib import RDFS, Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import Namespace

from gmeow_tools.config import PREFIXES
from gmeow_tools.graph import load_merged_graph

GM = Namespace(PREFIXES["gmeow"])


@dataclass(frozen=True, slots=True)
class SuppressionVocab:
    """Ontology-derived suppression knowledge for guards and saturation (#282)."""

    bearer_props: tuple[URIRef, ...]
    appellation_domain_props: frozenset[URIRef]
    appellation_classes: frozenset[URIRef]
    coarsen_guarded: frozenset[URIRef]


def _subclass_closure(graph: Graph, root: URIRef) -> frozenset[URIRef]:
    """``root`` plus every class transitively ``rdfs:subClassOf`` it."""
    closure: set[URIRef] = {root}
    grew = True
    while grew:
        grew = False
        for sub, sup in graph.subject_objects(RDFS.subClassOf):
            if sup in closure and isinstance(sub, URIRef) and sub not in closure:
                closure.add(sub)
                grew = True
    return frozenset(closure)


def suppression_vocab(onto: Graph) -> SuppressionVocab:
    """Derive the suppression vocabulary from the merged ontology."""
    appellation = GM.Appellation
    guarded = GM.coarsenGuarded
    classes = _subclass_closure(onto, appellation)
    bearer: set[URIRef] = set()
    domain_props: set[URIRef] = set()
    for prop, rng in onto.subject_objects(RDFS.range):
        if rng in classes and isinstance(prop, URIRef):
            bearer.add(prop)
    for prop, dom in onto.subject_objects(RDFS.domain):
        if dom in classes and isinstance(prop, URIRef):
            domain_props.add(prop)
    coarsen = {
        s
        for s, o in onto.subject_objects(guarded)
        if isinstance(s, URIRef) and str(o) == "true"
    }
    return SuppressionVocab(
        bearer_props=tuple(sorted(bearer)),
        appellation_domain_props=frozenset(domain_props),
        appellation_classes=classes,
        coarsen_guarded=frozenset(coarsen),
    )


@lru_cache(maxsize=1)
def default_suppression_vocab() -> SuppressionVocab:
    """The suppression vocabulary over the merged ontology."""
    return suppression_vocab(load_merged_graph(include_imports=False))

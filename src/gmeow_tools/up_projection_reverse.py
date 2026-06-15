# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Reverse-projection up-lift — mint STRUCTURED GMEOW from flat consumer vocab.

The per-term lift reverses a 1:1 alignment, but a flat consumer predicate often
denotes a *structured* gmeow shape that the down-projection consumes: a
``foaf:familyName`` is one part of a ``gmeow:PersonName``; a ``gedcom:spouseIn``
is membership in a ``gmeow:CoupleRelationship``; a ``doap:maintainer`` is a
``gmeow:Contribution`` with a maintainer role. The flat rule lifts these to a
bare leaf the down-cell cannot re-consume, so they never round-trip.

This module mints that structure with hand-authored reverse ``CONSTRUCT``
queries — the *best-faithful contextual lift* (#451 Principle 3/4) the flat rule
cannot express. The queries run against the source and produce pure GMEOW.
Minting is **deterministic** (``MD5`` of the bound nodes), so reruns are
byte-identical and shared structures coincide — one ``PersonName`` per person,
one ``CoupleRelationship`` per family — rather than fragmenting.
"""

from __future__ import annotations

import pyoxigraph
from rdflib import Graph

from gmeow_tools.config import NAMESPACE

#: Deterministic-mint base — a genid IRI namespace; the value is an MD5 of the
#: bound source nodes, so the same input always mints the same node.
_GENID = NAMESPACE + ".well-known/genid/up-"

_PREFIXES = f"""
PREFIX gmeow: <{NAMESPACE}>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX foaf: <http://xmlns.com/foaf/0.1/>
PREFIX vcard: <http://www.w3.org/2006/vcard/ns#>
PREFIX schema: <https://schema.org/>
PREFIX gedcom: <http://www.w3.org/2000/10/swap/pim/gedcom#>
"""

#: Flat name-part predicates → the gmeow:NamePartType they denote. Each mints a
#: shared ``gmeow:PersonName`` (one per person) carrying the typed part.
_NAME_PARTS: tuple[tuple[str, str], ...] = (
    ("foaf:givenName", "namePartGiven"),
    ("schema:givenName", "namePartGiven"),
    ("vcard:given-name", "namePartGiven"),
    ("foaf:familyName", "namePartSurname"),
    ("schema:familyName", "namePartSurname"),
    ("vcard:family-name", "namePartSurname"),
)

#: Flat full-name predicates → ``gmeow:fullName`` on the person's shared PersonName.
_FULL_NAMES: tuple[str, ...] = ("foaf:name", "schema:name", "vcard:fn")

#: Flat nickname predicates → a SEPARATE nickname-purpose PersonName (the model
#: the projection reads: hasName/namePurpose=Nickname/fullName), not a name-part.
_NICKNAMES: tuple[str, ...] = ("foaf:nick", "vcard:nickname")


def _name_part_query(source_pred: str, part_type: str) -> str:
    """A CONSTRUCT minting a typed name-part inside the person's shared PersonName."""
    return f"""{_PREFIXES}
CONSTRUCT {{
  ?p gmeow:hasName ?app .
  ?app rdf:type gmeow:PersonName .
  ?app gmeow:hasNamePart ?part .
  ?part gmeow:namePartType gmeow:{part_type} .
  ?part gmeow:partText ?v .
}}
WHERE {{
  ?p {source_pred} ?v .
  FILTER(isLiteral(?v))
  BIND(IRI(CONCAT("{_GENID}name-", MD5(STR(?p)))) AS ?app)
  BIND(IRI(CONCAT("{_GENID}part-{part_type}-",
       MD5(CONCAT(STR(?p), "|", STR(?v))))) AS ?part)
}}"""


def _full_name_query(source_pred: str) -> str:
    """A CONSTRUCT adding ``gmeow:fullName`` to the person's shared PersonName."""
    return f"""{_PREFIXES}
CONSTRUCT {{
  ?p gmeow:hasName ?app .
  ?app rdf:type gmeow:PersonName .
  ?app gmeow:fullName ?v .
}}
WHERE {{
  ?p {source_pred} ?v .
  FILTER(isLiteral(?v))
  BIND(IRI(CONCAT("{_GENID}name-", MD5(STR(?p)))) AS ?app)
}}"""


def _nickname_query(source_pred: str) -> str:
    """A CONSTRUCT minting a nickname-purpose PersonName (distinct from the main)."""
    return f"""{_PREFIXES}
CONSTRUCT {{
  ?p gmeow:hasName ?nn .
  ?nn rdf:type gmeow:PersonName .
  ?nn gmeow:namePurpose gmeow:namePurposeNickname .
  ?nn gmeow:fullName ?v .
}}
WHERE {{
  ?p {source_pred} ?v .
  FILTER(isLiteral(?v))
  BIND(IRI(CONCAT("{_GENID}nick-", MD5(CONCAT(STR(?p), "|", STR(?v))))) AS ?nn)
}}"""


def _reverse_queries() -> list[str]:
    """All authored reverse-projection CONSTRUCT queries."""
    return (
        [_name_part_query(sp, pt) for sp, pt in _NAME_PARTS]
        + [_full_name_query(sp) for sp in _FULL_NAMES]
        + [_nickname_query(sp) for sp in _NICKNAMES]
    )


def apply_reverse(source: Graph) -> Graph:
    """Run every reverse-projection query over ``source`` → pure-GMEOW structure.

    The result is added to the up-projection as bare facts (a documentary
    reverse-projection is faithful, not inferred). Idempotent and deterministic.
    """
    store = pyoxigraph.Store()
    store.bulk_load(
        source.serialize(format="nt").encode(),
        format=pyoxigraph.RdfFormat.N_TRIPLES,
    )
    out = Graph()
    for query in _reverse_queries():
        result = store.query(query)
        nt = pyoxigraph.serialize(result, format=pyoxigraph.RdfFormat.N_TRIPLES)
        if nt:
            out.parse(data=nt.decode("utf-8"), format="nt")
    return out

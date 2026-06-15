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


#: Contact endpoints: source predicate → the gmeow ContactPoint type. The source
#: IRI (mailto:/tel:) IS reused as the contact node, typed and linked.
_CONTACTS: tuple[tuple[str, str], ...] = (
    ("vcard:hasEmail", "EmailAddress"),
    ("foaf:mbox", "EmailAddress"),
    ("vcard:hasTelephone", "TelephoneNumber"),
    ("foaf:phone", "TelephoneNumber"),
)


def _contact_query(source_pred: str, cp_type: str) -> str:
    """A CONSTRUCT typing the source endpoint IRI as a gmeow ContactPoint."""
    return f"""{_PREFIXES}
CONSTRUCT {{
  ?p gmeow:hasContactPoint ?cp .
  ?cp rdf:type gmeow:{cp_type} .
}}
WHERE {{
  ?p {source_pred} ?cp .
  FILTER(isIRI(?cp))
}}"""


#: Genealogy reverse-projection: the GEDCOM Family structure mints gmeow kinship
#: relators (the proper model — gedcom:sex already lifts to sexAssignedAtBirth via
#: value inversion; husband/wife stay REFUSED — gendered spousal roles are never
#: derived, SEX != GENDER, Principle 9).
_GENEALOGY: tuple[str, ...] = (
    # a couple: both partners of a Family form one CoupleRelationship (within it),
    # so the down-projection mints gedcom:Marriage + spouseIn for each partner.
    f"""{_PREFIXES}
CONSTRUCT {{
  ?cr rdf:type gmeow:CoupleRelationship .
  ?cr gmeow:hasPartner ?h .
  ?cr gmeow:hasPartner ?w .
  ?cr gmeow:withinFamily ?fam .
}}
WHERE {{
  ?fam gedcom:husband ?h . ?fam gedcom:wife ?w .
  BIND(IRI(CONCAT("{_GENID}couple-", MD5(STR(?fam)))) AS ?cr)
}}""",
    # a child's membership: one ParentChildRelationship within the family, so the
    # down-projection emits gedcom:childIn.
    f"""{_PREFIXES}
CONSTRUCT {{
  ?pcr rdf:type gmeow:ParentChildRelationship .
  ?pcr gmeow:relationshipChild ?c .
  ?pcr gmeow:withinFamily ?fam .
}}
WHERE {{
  {{ ?c gedcom:childIn ?fam }} UNION {{ ?fam gedcom:child ?c }}
  BIND(IRI(CONCAT("{_GENID}pcr-", MD5(CONCAT(STR(?fam), "|", STR(?c))))) AS ?pcr)
}}""",
    # flat parent→child: each parent of the family has each child (gedcom:child).
    f"""{_PREFIXES}
CONSTRUCT {{ ?parent gmeow:hasChild ?c . }}
WHERE {{
  {{ ?c gedcom:childIn ?fam }} UNION {{ ?fam gedcom:child ?c }}
  {{ ?fam gedcom:husband ?parent }} UNION {{ ?fam gedcom:wife ?parent }}
}}""",
)


def _reverse_queries() -> list[str]:
    """All authored reverse-projection CONSTRUCT queries."""
    return (
        [_name_part_query(sp, pt) for sp, pt in _NAME_PARTS]
        + [_full_name_query(sp) for sp in _FULL_NAMES]
        + [_nickname_query(sp) for sp in _NICKNAMES]
        + [_contact_query(sp, ct) for sp, ct in _CONTACTS]
        + list(_GENEALOGY)
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

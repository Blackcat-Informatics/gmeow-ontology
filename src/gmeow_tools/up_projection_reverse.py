# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
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

import gmeow_rdf
from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools.config import NAMESPACE

#: Deterministic-mint base — a genid IRI namespace; the value is an MD5 of the
#: bound source nodes, so the same input always mints the same node.
_GENID = NAMESPACE + ".well-known/genid/up-"

_PREFIXES = f"""
PREFIX gmeow: <{NAMESPACE}>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX foaf: <http://xmlns.com/foaf/0.1/>
PREFIX vcard: <http://www.w3.org/2006/vcard/ns#>
PREFIX schema: <https://schema.org/>
PREFIX gedcom: <http://www.w3.org/2000/10/swap/pim/gedcom#>
PREFIX doap: <http://usefulinc.com/ns/doap#>
PREFIX sioc: <http://rdfs.org/sioc/ns#>
PREFIX time: <http://www.w3.org/2006/time#>
PREFIX dcterms: <http://purl.org/dc/terms/>
PREFIX bf: <http://id.loc.gov/ontologies/bibframe/>
"""

#: Flat type rewrites (source class → gmeow class) the per-term lift misses
#: because the down-cell is multi-leg. Faithful round trip: the source asserted
#: the vocab term; the gmeow form is the draft's interpretation, re-emitted down.
_TYPE_REWRITES: tuple[tuple[str, str], ...] = (
    ("sioc:Post", "gmeow:EmailMessage"),
    ("sioc:Thread", "gmeow:Thread"),
    ("sioc:Container", "gmeow:Thread"),
    # bibframe:Work IS the WEMI Work — the down-cell projects gmeow:Work → bf:Work
    ("bf:Work", "gmeow:Work"),
)

#: Flat predicate rewrites (source predicate → gmeow predicate).
_PRED_REWRITES: tuple[tuple[str, str], ...] = (
    ("sioc:has_container", "gmeow:partOfThread"),
    ("sioc:reply_of", "gmeow:inReplyTo"),
    ("sioc:has_creator", "gmeow:from"),
    ("sioc:topic", "gmeow:isAbout"),
    ("sioc:link", "gmeow:sourceLocation"),
    ("time:hasTime", "gmeow:eventTime"),
    ("doap:repository", "gmeow:hasRepository"),
    ("doap:browse", "gmeow:webUrl"),
    ("dcterms:rights", "gmeow:copyrightNotice"),
    # a bf:title node IS a gmeow title relator (the down-cell re-types it bf:Title)
    ("bf:title", "gmeow:hasTitle"),
)

#: The object of these source predicates is typed as a gmeow class (so the
#: down-cell that requires the type fires) — e.g. a doap:repository node IS a
#: gmeow:Repository, which doap:browse → gmeow:webUrl then needs.
_OBJECT_TYPES: tuple[tuple[str, str], ...] = (
    ("doap:repository", "gmeow:Repository"),
    # the depiction object is the depicting MediaObject (gmeow:depicts' domain)
    ("foaf:depiction", "gmeow:MediaObject"),
)


def _object_type_query(src_pred: str, gmeow_type: str) -> str:
    return f"""{_PREFIXES}
CONSTRUCT {{ ?o rdf:type {gmeow_type} . }} WHERE {{ ?s {src_pred} ?o . }}"""


#: Inverse predicate rewrites: source ``s P o`` → ``o gmeow:Q s`` (the down-cell
#: emits the source as the inverse of the gmeow edge).
_INVERSE_REWRITES: tuple[tuple[str, str], ...] = (
    ("sioc:container_of", "gmeow:partOfThread"),
    ("sioc:has_reply", "gmeow:inReplyTo"),
    # foaf:depiction (agent → image) is the inverse of gmeow:depicts (image → agent)
    ("foaf:depiction", "gmeow:depicts"),
    # foaf:publications (agent → work) is the inverse of gmeow:hasAuthor (work → agent)
    ("foaf:publications", "gmeow:hasAuthor"),
)


def _type_rewrite_query(src_type: str, gmeow_type: str) -> str:
    return f"""{_PREFIXES}
CONSTRUCT {{ ?s rdf:type {gmeow_type} . }} WHERE {{ ?s rdf:type {src_type} . }}"""


def _pred_rewrite_query(src_pred: str, gmeow_pred: str) -> str:
    return f"""{_PREFIXES}
CONSTRUCT {{ ?s {gmeow_pred} ?o . }} WHERE {{ ?s {src_pred} ?o . }}"""


def _inverse_rewrite_query(src_pred: str, gmeow_pred: str) -> str:
    return f"""{_PREFIXES}
CONSTRUCT {{ ?o {gmeow_pred} ?s . }} WHERE {{ ?s {src_pred} ?o . }}"""


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
    ("vcard:hasInstantMessage", "InstantMessageAddress"),
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


#: Software contribution roles: source predicate → the gmeow contribution-role
#: individual. A flat maintainer/developer edge mints a gmeow:Contribution relator
#: (the down-projection reads {target, contributor, role}).
_CONTRIBUTIONS: tuple[tuple[str, str, str], ...] = (
    ("doap:maintainer", "maint", "roleSoftwareMaintainer"),
    ("doap:developer", "dev", "roleSoftwareDeveloper"),
)


#: Job-title sources → a Membership relator whose Role's label carries the title
#: (the down-projection reads Membership/hasRole/role-label → schema:jobTitle etc.).
_JOB_TITLE_QUERY = f"""{_PREFIXES}
CONSTRUCT {{
  ?m rdf:type gmeow:Membership .
  ?m gmeow:membershipMember ?person .
  ?m gmeow:hasRole ?role .
  ?role rdfs:label ?title .
}}
WHERE {{
  {{ {{ ?person schema:jobTitle ?title }} UNION {{ ?person foaf:title ?title }} }}
  FILTER(isLiteral(?title))
  BIND(IRI(CONCAT("{_GENID}membership-",
       MD5(CONCAT(STR(?person), "|", STR(?title))))) AS ?m)
  BIND(IRI(CONCAT("{_GENID}role-",
       MD5(CONCAT(STR(?person), "|", STR(?title))))) AS ?role)
}}"""


def _contribution_query(source_pred: str, slug: str, role: str) -> str:
    """A CONSTRUCT minting a gmeow:Contribution relator from a flat role edge."""
    return f"""{_PREFIXES}
CONSTRUCT {{
  ?contrib rdf:type gmeow:Contribution .
  ?contrib gmeow:contributionTarget ?proj .
  ?contrib gmeow:contributor ?agent .
  ?contrib gmeow:contributionRole gmeow:{role} .
}}
WHERE {{
  ?proj {source_pred} ?agent .
  FILTER(isIRI(?agent))
  BIND(IRI(CONCAT("{_GENID}contrib-{slug}-",
       MD5(CONCAT(STR(?proj), "|", STR(?agent))))) AS ?contrib)
}}"""


def _reverse_queries() -> list[str]:
    """All authored reverse-projection CONSTRUCT queries."""
    return (
        [_name_part_query(sp, pt) for sp, pt in _NAME_PARTS]
        + [_full_name_query(sp) for sp in _FULL_NAMES]
        + [_nickname_query(sp) for sp in _NICKNAMES]
        + [_contact_query(sp, ct) for sp, ct in _CONTACTS]
        + list(_GENEALOGY)
        + [_contribution_query(sp, sl, r) for sp, sl, r in _CONTRIBUTIONS]
        + [_type_rewrite_query(s, g) for s, g in _TYPE_REWRITES]
        + [_pred_rewrite_query(s, g) for s, g in _PRED_REWRITES]
        + [_inverse_rewrite_query(s, g) for s, g in _INVERSE_REWRITES]
        + [_object_type_query(s, g) for s, g in _OBJECT_TYPES]
        + [_JOB_TITLE_QUERY]
    )


def apply_reverse(source: Graph) -> Graph:
    """Run every reverse-projection query over ``source`` → pure-GMEOW structure.

    The result is added to the up-projection as bare facts (a documentary
    reverse-projection is faithful, not inferred). Any blank node this emits (a
    place node, a time entity passed through from the source) is given a stable
    label by the transform's RDFC-1.0 canonicalization, so reruns are identical.
    """
    store = gmeow_rdf.Store()
    store.bulk_load(
        source.serialize(format="nt").encode(),
        format=gmeow_rdf.RdfFormat.N_TRIPLES,
    )
    out = Graph()
    for query in _reverse_queries():
        result = store.query(query)
        # every _reverse_queries() entry is a CONSTRUCT, so the result is always a
        # triple stream — narrow the union for the serializer (and the type checker).
        assert isinstance(result, gmeow_rdf.QueryTriples)
        nt = gmeow_rdf.serialize(result, format=gmeow_rdf.RdfFormat.N_TRIPLES)
        if nt:
            out.parse(data=nt.decode("utf-8"), format="nt")
    return out

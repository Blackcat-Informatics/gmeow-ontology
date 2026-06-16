# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the clean-reversal up-projection (consumer RDF → GMEOW, #451)."""

from __future__ import annotations

from rdflib import RDF, XSD, BNode, Graph, Literal, URIRef

from gmeow_tools import sparql
from gmeow_tools.config import FIXTURES_DIR
from gmeow_tools.projections import project_graph
from gmeow_tools.up_projection import (
    _ADOPTED_PREDICATES,
    _NORMALIZED_PREDICATES,
    build_lift_map,
    up_project,
)

#: Lift targets that are deliberately NOT gmeow-namespaced: the SKOS coreference
#: predicates GMEOW adopts and uses directly (skos:exactMatch/closeMatch) and the
#: canonical label predicate every external label normalizes to (rdfs:label). Pure
#: GMEOW is "gmeow terms PLUS this declared adopted vocabulary", never a guess.
_ADOPTED_LIFT_TARGETS = set(_ADOPTED_PREDICATES) | set(_NORMALIZED_PREDICATES.values())

GM = "https://blackcatinformatics.ca/gmeow/"
SCHEMA = "https://schema.org/"
DC = "http://purl.org/dc/elements/1.1/"
RDFS_LABEL = "http://www.w3.org/2000/01/rdf-schema#label"


def test_lift_map_is_unambiguous() -> None:
    """Every lift rule maps a target to exactly one gmeow term; many-to-one
    down-images with no ranking winner are held out as ambiguous, never guessed."""
    lift = build_lift_map()
    assert len(lift.rules) > 200, "expected a substantial clean-reversal map"
    # rules and ambiguous are disjoint and each rule is a single gmeow IRI
    assert not (set(lift.rules) & set(lift.ambiguous))
    assert all(
        v.startswith(GM) or v in _ADOPTED_LIFT_TARGETS for v in lift.rules.values()
    )
    # dc:date is the down-image of six peer gmeow date properties, no identity
    # winner → genuinely ambiguous
    assert DC + "date" in lift.ambiguous
    assert DC + "date" not in lift.rules
    # a genuinely 1:1 term IS a rule
    assert lift.rules.get(SCHEMA + "conformsTo") == GM + "conformsTo"


SKOS = "http://www.w3.org/2004/02/skos/core#"
WD = "http://www.wikidata.org/entity/"


def test_skos_concept_lifts_to_asserted_tag_with_label() -> None:
    """A source skos:Concept IS a gmeow:Tag — asserted, not a refutable claim —
    and its skos:prefLabel normalizes to rdfs:label (the canonical label). The
    tag's scheme membership asserts via the inScheme identity."""
    src = Graph()
    c = URIRef(WD + "Q59")
    src.add((c, RDF.type, URIRef(SKOS + "Concept")))
    src.add((c, URIRef(SKOS + "prefLabel"), Literal("PHP", lang="en")))
    src.add((c, URIRef(SKOS + "inScheme"), URIRef("https://ex/scheme")))
    out = up_project(src).graph
    # asserted Tag fact, not buried in a StatementMetadata claim reifier
    assert (c, RDF.type, URIRef(GM + "Tag")) in out
    assert not list(out.subjects(RDF.type, URIRef(GM + "StatementMetadata")))
    # prefLabel → rdfs:label (value preserved); scheme membership asserted
    labels = [str(o) for o in out.objects(c, URIRef(RDFS_LABEL))]
    assert labels == ["PHP"]
    assert (c, URIRef(GM + "tagInScheme"), URIRef("https://ex/scheme")) in out


def test_qid_bridge_links_keyword_string_to_anchored_tag() -> None:
    """The implicit QID bridge: a keyword/programmingLanguage string that matches
    (case-folded) a QID-anchored tag's label becomes gmeow:hasTag to that entity —
    the curated coreference the data entailed but did not state."""
    src = Graph()
    php = URIRef(WD + "Q59")
    src.add((php, RDF.type, URIRef(SKOS + "Concept")))
    src.add((php, URIRef(SKOS + "prefLabel"), Literal("PHP", lang="en")))
    proj = URIRef("https://ex/proj")
    src.add((proj, URIRef("https://schema.org/programmingLanguage"), Literal("php")))
    src.add((proj, URIRef("https://schema.org/keywords"), Literal("not-a-concept")))
    result = up_project(src)
    # case-folded "php" → the anchored wd:Q59 tag
    assert (proj, URIRef(GM + "hasTag"), php) in result.graph
    assert result.tag_resolved == 1  # the unmatched keyword adds nothing
    # a tag with NO QID anchor is never a bridge target (no guessing)
    src2 = Graph()
    local = URIRef("https://ex/local-tag")
    src2.add((local, RDF.type, URIRef(SKOS + "Concept")))
    src2.add((local, URIRef(SKOS + "prefLabel"), Literal("PHP", lang="en")))
    p2 = URIRef("https://ex/p2")
    src2.add((p2, URIRef("https://schema.org/keywords"), Literal("php")))
    assert up_project(src2).tag_resolved == 0


def test_knows_about_lifts_entity_as_fact_text_as_claim() -> None:
    """schema:knowsAbout exactMatch gmeow:knowsAbout: an IRI subject lifts to an
    ASSERTED knowsAbout fact (the concept/QID preserved); a TEXT value lifts to a
    claim, never an ill-typed object-property-with-literal edge (the polymorphic
    Text|Thing guard)."""
    schema_ka = URIRef(SCHEMA + "knowsAbout")
    ka = URIRef(GM + "knowsAbout")
    src = Graph()
    agent = URIRef("https://ex/agent")
    topic = URIRef(WD + "Q28865")  # Python — an entity
    src.add((agent, schema_ka, topic))
    src.add((agent, schema_ka, Literal("STL Metaprogramming")))  # free text
    out = up_project(src).graph
    # the entity edge asserts as a fact
    assert (agent, ka, topic) in out
    # the literal does NOT assert (would be ill-typed); it is claimed instead
    assert (agent, ka, Literal("STL Metaprogramming")) not in out
    sm = URIRef(GM + "StatementMetadata")
    claims = [
        r
        for r in out.subjects(RDF.type, sm)
        if (r, URIRef(GM + "qPredicate"), ka) in out
    ]
    assert claims, "the text knowsAbout must survive as a claim, not vanish"
    assert any(
        (r, URIRef(GM + "qObjectLiteral"), Literal("STL Metaprogramming")) in out
        for r in claims
    )


def test_reverse_projection_mints_structured_name() -> None:
    """A flat foaf:familyName/givenName lifts to a STRUCTURED gmeow:PersonName with
    typed name-parts (the contextual lift the flat rule can't express). Both parts
    hang off ONE shared PersonName (deterministic mint keyed on the person)."""
    foaf = "http://xmlns.com/foaf/0.1/"
    p = URIRef("https://ex/p")
    src = Graph()
    src.add((p, URIRef(foaf + "givenName"), Literal("Ada")))
    src.add((p, URIRef(foaf + "familyName"), Literal("Lovelace")))
    out = up_project(src).graph
    apps = set(out.objects(p, URIRef(GM + "hasName")))
    assert len(apps) == 1, "given + family must share ONE PersonName"
    app = next(iter(apps))
    assert (app, RDF.type, URIRef(GM + "PersonName")) in out
    parts = set(out.objects(app, URIRef(GM + "hasNamePart")))
    types = {
        o for part in parts for o in out.objects(part, URIRef(GM + "namePartType"))
    }
    assert URIRef(GM + "namePartGiven") in types
    assert URIRef(GM + "namePartSurname") in types


def test_value_mapped_inversion_lifts_documentary_literal() -> None:
    """A whenValue down-cell read backwards: a documentary value literal lifts to
    its gmeow value individual as a FACT (gedcom:sex "M" → sexAssignedAtBirth
    saabMale). An ambiguous literal (GEDCOM "U", several sources) does not lift."""
    gedcom = "http://www.w3.org/2000/10/swap/pim/gedcom#"
    p = URIRef("https://ex/person")
    src = Graph()
    src.add((p, URIRef(gedcom + "sex"), Literal("M")))
    src.add((p, URIRef(gedcom + "sex"), Literal("U")))  # ambiguous → no lift
    out = up_project(src).graph
    assert (p, URIRef(GM + "sexAssignedAtBirth"), URIRef(GM + "saabMale")) in out
    # "U" is irreversible (saabUnknown vs intersex degrade) → never guessed
    assert len(list(out.triples((p, URIRef(GM + "sexAssignedAtBirth"), None)))) == 1


def test_identity_outranks_projection_collision() -> None:
    """Preferred-up-target disambiguation (#451 stage 3): an exactMatch/equivalent
    identity wins over a structural projection of a NARROWER gmeow term to the
    same external target, so the target resolves cleanly instead of being held
    out as ambiguous."""
    lift = build_lift_map()
    # schema:name ≡ gmeow:name (equivalentProperty); gmeow:fullName / hasPlaceName
    # also project DOWN to schema:name but are narrower — identity wins
    assert lift.rules.get(SCHEMA + "name") == GM + "name"
    assert SCHEMA + "name" not in lift.ambiguous
    # prov:Activity ≡ gmeow:Activity (equivalentClass); gmeow:BuildActivity is a
    # subclass that also projects to prov:Activity — identity wins
    assert lift.rules.get("http://www.w3.org/ns/prov#Activity") == GM + "Activity"
    # a peer set with NO identity winner stays ambiguous (no fabrication): every
    # rival of dc:date is a structural projection, none an identity
    assert DC + "date" in lift.ambiguous
    assert len(lift.ambiguous[DC + "date"]) == 6


def test_up_project_recovers_identity_resolved_term() -> None:
    """A round trip through an identity-resolved target recovers the gmeow term:
    schema:name lifts back to gmeow:name (not the narrower gmeow:fullName)."""
    src = Graph()
    person = URIRef("https://ex.org/ada")
    src.add((person, URIRef(SCHEMA + "name"), Literal("Ada Lovelace")))
    up = up_project(src)
    assert (person, URIRef(GM + "name"), Literal("Ada Lovelace")) in up.graph
    assert (person, URIRef(GM + "fullName"), Literal("Ada Lovelace")) not in up.graph
    assert "schema:name" not in up.ambiguous_terms


def test_up_project_retags_public_bcp47_to_canonical_internal() -> None:
    """The up-projection is the inverse of ``fnComposeBcp`` (#451): a consumer's
    public BCP-47 tag (``@en``/``@fr``) is lifted back to the canonical internal
    ``@x-gmeow-*`` form, so the pure-GMEOW draft is genuinely canonical and the
    canonical transpile tiers stop leaking public tags."""
    src = Graph()
    person = URIRef("https://ex.org/ada")
    src.add((person, URIRef(SCHEMA + "name"), Literal("Ada Lovelace", lang="en")))
    src.add((person, URIRef(SCHEMA + "conformsTo"), Literal("v1", lang="fr")))
    up = up_project(src)
    tags = {
        o.language for _s, _p, o in up.graph if isinstance(o, Literal) and o.language
    }
    assert tags == {"x-gmeow-english", "x-gmeow-french"}
    # no public tag survives into canonical GMEOW
    assert "en" not in tags and "fr" not in tags


def _down(fixture: str, profile: str) -> Graph:
    data = Graph().parse(FIXTURES_DIR / fixture, format="turtle")
    store = sparql.store_with(include_imports=True, extra_triples=data)
    return project_graph(profile, store)


def test_up_project_round_trip_recovers_clean_terms() -> None:
    """down(GMEOW) → up(consumer) recovers the clean-reversible gmeow terms."""
    down = _down("clean-wins.ttl", "schema-org")
    up = up_project(down)
    ex = "https://example.org/clean/"
    # clean 1:1 terms survive the round trip
    assert (
        URIRef(ex + "doc"),
        URIRef(GM + "conformsTo"),
        URIRef("https://example.org/spec/v1"),
    ) in up.graph
    assert (
        URIRef(ex + "doc"),
        URIRef(GM + "hasEditor"),
        URIRef(ex + "editor"),
    ) in up.graph
    assert (URIRef(ex + "data"), RDF.type, URIRef(GM + "Dataset")) in up.graph
    assert up.lifted > 0


def test_up_project_output_is_pure_gmeow() -> None:
    """The lift output contains only GMEOW predicates/types — never consumer terms."""
    up = up_project(_down("clean-wins.ttl", "schema-org"))
    for _s, p, o in up.graph:
        assert str(p).startswith(GM) or p == RDF.type, f"non-gmeow predicate {p}"
        if p == RDF.type and isinstance(o, URIRef):
            assert str(o).startswith(GM), f"non-gmeow type {o}"


def test_up_project_recovers_inverse_path_terms_swapped() -> None:
    """An inverted down-projection (edoalPath anchored on the atom's object)
    round-trips back to the original gmeow edge with subject↔object restored.

    ``gmeow:alumniOf`` (alum→school) projects down to ``schema:alumni``
    (school→alum, inverted); the up-lift must swap the endpoints back so the
    recovered edge points alum→school, not school→alum.
    """
    lift = build_lift_map()
    assert SCHEMA + "alumni" in lift.inverse_rules
    assert lift.inverse_rules[SCHEMA + "alumni"] == GM + "alumniOf"

    up = up_project(_down("gap-clusters.ttl", "schema-org"), lift)
    gap = "https://example.org/gap/"
    # recovered in the ORIGINAL direction: ada (alum) → cambridge (school)
    assert (
        URIRef(gap + "ada"),
        URIRef(GM + "alumniOf"),
        URIRef(gap + "cambridge"),
    ) in up.graph
    # and NOT the inverted school→alum direction the consumer graph carried
    assert (
        URIRef(gap + "cambridge"),
        URIRef(GM + "alumniOf"),
        URIRef(gap + "ada"),
    ) not in up.graph

    # subOrganization is the down-image of the same gmeow term; it also swaps
    org = up_project(_down("organizations.ttl", "schema-org"), lift)
    ex = "https://example.org/organizations/"
    assert (
        URIRef(ex + "archives-dept"),
        URIRef(GM + "subOrganizationOf"),
        URIRef(ex + "meridian-institute"),
    ) in org.graph

    # a blank-node object swaps too — it is a legal RDF subject after the swap
    bn = BNode()
    src = Graph()
    src.add((URIRef(ex + "meridian-institute"), URIRef(SCHEMA + "alumni"), bn))
    bnode_up = up_project(src, lift)
    assert (
        bn,
        URIRef(GM + "alumniOf"),
        URIRef(ex + "meridian-institute"),
    ) in bnode_up.graph


def test_up_project_inverse_literal_object_is_skipped_not_a_gap() -> None:
    """An inverse-rule predicate with a LITERAL object cannot swap (a literal is
    not a legal subject); it is skipped silently, never counted as a gap — a lift
    rule exists for it, so reporting a gap would be dishonest accounting."""
    lift = build_lift_map()
    src = Graph()
    # schema:alumni carrying a stray literal object (malformed consumer data)
    src.add(
        (
            URIRef("https://example.org/organizations/x"),
            URIRef(SCHEMA + "alumni"),
            Literal("not a resource"),
        )
    )
    up = up_project(src, lift)
    assert up.lifted == 0
    assert "schema:alumni" not in up.gap_terms
    assert "schema:alumni" not in up.ambiguous_terms
    assert len(up.graph) == 0


def test_up_project_does_not_guess_ambiguous_or_structural() -> None:
    """An ambiguous many-to-one term with no identity winner is reported, never
    lifted — the no-fabrication discipline. schema:alternateName reverses to
    gmeow:hasName / hasPlaceName (peers, no exactMatch), so it stays held out."""
    lift = build_lift_map()
    assert lift.ambiguous.get(SCHEMA + "alternateName") == {
        GM + "hasName",
        GM + "hasPlaceName",
    }
    up = up_project(_down("names.ttl", "schema-org"), lift)
    # reported as ambiguous, and neither rival is invented as its lift
    assert "schema:alternateName" in up.ambiguous_terms
    assert SCHEMA + "alternateName" not in lift.rules


def test_lift_map_claims_are_distinct_and_confident() -> None:
    """Claim rules (closeMatch + generalizing <=) carry a single gmeow term and a
    [0,1] confidence (or none, for a generalizing cell that supplied none), and
    never overlap the fact / inverse / ambiguous layers (fact coverage wins)."""
    lift = build_lift_map()
    assert len(lift.claim_rules) > 100, "expected a substantial claim layer"
    assert not (set(lift.claim_rules) & set(lift.rules))
    assert not (set(lift.claim_rules) & set(lift.inverse_rules))
    assert not (set(lift.claim_rules) & set(lift.ambiguous))
    for gmeow, conf in lift.claim_rules.values():
        assert gmeow.startswith(GM)
        assert conf == "" or 0.0 <= float(conf) <= 1.0
    # schema:sender is a closeMatch of gmeow:from (not an equivalence)
    assert lift.claim_rules.get(SCHEMA + "sender") == (GM + "from", "0.9")


def test_relation_aware_exact_is_fact_generalizing_is_claim() -> None:
    """The EDOAL relation qualifier rules fact-vs-claim: an `=` structural cell
    reverses as a fact, a `<=` (dumb-down) cell *infers* specificity so it lifts
    as a claim, never asserted as fact. A many-to-one `<=` collapse stays
    ambiguous (no source signal picks which narrow term)."""
    lift = build_lift_map()
    # schema:creator is a `<=` generalizing collapse → claim (gmeow:hasAuthor),
    # NOT a fact rule (reversing it infers the author role)
    assert lift.claim_rules.get(SCHEMA + "creator") == (GM + "hasAuthor", "0.9")
    assert SCHEMA + "creator" not in lift.rules
    # schema:conformsTo is an `=` exact identity → a fact rule
    assert lift.rules.get(SCHEMA + "conformsTo") == GM + "conformsTo"
    assert SCHEMA + "conformsTo" not in lift.claim_rules
    # a coarse Dublin-Core term whose `<=` sources are several narrow gmeow
    # terms is held out — never a phantom "ambiguous identity" and never guessed
    assert DC + "date" in lift.ambiguous


def test_up_project_generalizing_claim_without_confidence_is_valid() -> None:
    """A `<=` cell that supplied no confidence still lifts as a claim — carrying
    gmeow:mappedFrom but no gmeow:confidence — and stays SHACL-valid."""
    from pyshacl import validate as shacl_validate

    from gmeow_tools.config import STATEMENT_DSL_SHAPES_FILE

    lift = build_lift_map()
    # doap:license generalizes from gmeow:projectLicense with no curated confidence
    gmeow, conf = lift.claim_rules["http://usefulinc.com/ns/doap#license"]
    assert gmeow == GM + "projectLicense"
    assert conf == ""  # no confidence on the cell
    src = Graph()
    src.add(
        (
            URIRef("https://ex.org/proj"),
            URIRef("http://usefulinc.com/ns/doap#license"),
            URIRef("https://ex.org/mit"),
        )
    )
    up = up_project(src, lift)
    assert up.claimed == 1
    cell = next(up.graph.subjects(RDF.type, URIRef(GM + "StatementMetadata")))
    # mappedFrom is present, confidence is absent (omitted, not an empty literal)
    props = {
        up.graph.value(a, URIRef(GM + "annProperty"))
        for a in up.graph.objects(cell, URIRef(GM + "annotation"))
    }
    assert URIRef(GM + "mappedFrom") in props
    assert URIRef(GM + "confidence") not in props
    shapes = Graph().parse(STATEMENT_DSL_SHAPES_FILE, format="turtle")
    conforms, _g, report = shacl_validate(
        up.graph, shacl_graph=shapes, advanced=True, inference="none"
    )
    assert conforms, report


def test_up_project_emits_provenance_stamped_claim_not_a_bare_fact() -> None:
    """A closeMatch term lifts to a gmeow:StatementMetadata claim quoting the
    inferred triple, stamped with confidence + mappedFrom — and the bare gmeow
    triple is NEVER asserted directly (closeMatch is close, not equal)."""
    msg, alice = URIRef("https://ex.org/msg"), URIRef("https://ex.org/alice")
    src = Graph()
    src.add((msg, URIRef(SCHEMA + "sender"), alice))
    up = up_project(src)
    assert up.claimed == 1
    assert "schema:sender" in up.claim_terms

    # the inferred edge is NOT asserted as a plain fact
    assert (msg, URIRef(GM + "from"), alice) not in up.graph

    # exactly one StatementMetadata cell, quoting (msg gmeow:from alice)
    cells = list(up.graph.subjects(RDF.type, URIRef(GM + "StatementMetadata")))
    assert len(cells) == 1
    cell = cells[0]
    assert up.graph.value(cell, URIRef(GM + "qSubject")) == msg
    assert up.graph.value(cell, URIRef(GM + "qPredicate")) == URIRef(GM + "from")
    assert up.graph.value(cell, URIRef(GM + "qObject")) == alice
    # stamped with the curated confidence and the source term it was mapped from
    anns = {
        (
            up.graph.value(a, URIRef(GM + "annProperty")),
            up.graph.value(a, URIRef(GM + "annValue")),
        )
        for a in up.graph.objects(cell, URIRef(GM + "annotation"))
    }
    assert (URIRef(GM + "confidence"), Literal("0.9", datatype=XSD.decimal)) in anns
    assert (URIRef(GM + "mappedFrom"), URIRef(SCHEMA + "sender")) in anns


def test_up_project_claim_literal_object_uses_qobjectliteral() -> None:
    """A closeMatch whose object is a literal quotes it via gmeow:qObjectLiteral,
    so the StatementMetadata stays shape-valid (qObject is IRI-only)."""
    lift = build_lift_map()
    # find a closeMatch claim target that takes a literal value in real data
    target = SCHEMA + "softwareVersion"  # gmeow:modelVersionTag, datatype-valued
    assert target in lift.claim_rules
    src = Graph()
    src.add((URIRef("https://ex.org/app"), URIRef(target), Literal("1.2.3")))
    up = up_project(src, lift)
    assert up.claimed == 1
    cell = next(up.graph.subjects(RDF.type, URIRef(GM + "StatementMetadata")))
    assert up.graph.value(cell, URIRef(GM + "qObjectLiteral")) == Literal("1.2.3")
    assert up.graph.value(cell, URIRef(GM + "qObject")) is None


def test_up_project_claims_are_shacl_valid() -> None:
    """Every emitted claim satisfies the StatementMetadata SHACL shape — the
    output is well-formed GMEOW, not just GMEOW-namespaced.

    Validated against the merged shapes graph with the statement-DSL shapes as
    the base, exactly the loader path that previously fused the two colliding
    ``gmeow:AnnotationShape`` definitions (#478). After the rename the merged
    graph no longer confuses the statement-DSL annotation shape with the Web
    Annotation shape.
    """
    from gmeow_tools.config import STATEMENT_DSL_SHAPES_FILE
    from gmeow_tools.validate import run_shacl

    src = Graph()
    a = URIRef("https://ex.org/a")
    ver = URIRef(SCHEMA + "softwareVersion")
    src.add((URIRef("https://ex.org/msg"), URIRef(SCHEMA + "sender"), a))
    src.add((URIRef("https://ex.org/app"), ver, Literal("9")))
    up = up_project(src)
    assert up.claimed == 2
    result = run_shacl(up.graph, shapes_path=STATEMENT_DSL_SHAPES_FILE)
    assert result.ok, "\n".join(result.errors)


def test_up_project_claim_skips_blank_node_endpoints() -> None:
    """A closeMatch triple with a blank-node subject/object is unquotable under
    the StatementMetadata shape (IRI endpoints only), so it is skipped — a rule
    exists, so it is NOT counted as a gap."""
    lift = build_lift_map()
    src = Graph()
    # blank-node subject under a closeMatch predicate
    src.add((BNode(), URIRef(SCHEMA + "sender"), URIRef("https://ex.org/a")))
    up = up_project(src, lift)
    assert up.claimed == 0
    assert len(up.graph) == 0
    assert "schema:sender" not in up.gap_terms
    assert "schema:sender" not in up.ambiguous_terms


def test_up_project_class_claim_skips_blank_node_subject() -> None:
    """A class closeMatch on a blank-node subject is unquotable (qSubject is
    IRI-only), so it is skipped — a rule exists, so it is NOT a gap (parity with
    the property-claim blank-node skip)."""
    lift = build_lift_map()
    # find a closeMatch claim target that is a class (lifts via rdf:type)
    target = SCHEMA + "Offer"  # gmeow:Offering, a class closeMatch
    assert target in lift.claim_rules
    src = Graph()
    src.add((BNode(), RDF.type, URIRef(target)))
    up = up_project(src, lift)
    assert up.claimed == 0
    assert len(up.graph) == 0
    assert "schema:Offer" not in up.gap_terms
    assert "schema:Offer" not in up.ambiguous_terms


def test_decimal_confidence_rejects_non_decimal_lexical_forms() -> None:
    """Confidence must be a finite [0,1] value in xsd:decimal lexical form — the
    raw string is emitted as an xsd:decimal literal, so exponent / NaN / out-of-
    range forms (valid float, invalid decimal) are rejected, not silently kept."""
    from decimal import Decimal

    from gmeow_tools.up_projection import _decimal_confidence

    assert _decimal_confidence("0.9") == Decimal("0.9")
    assert _decimal_confidence("1") == Decimal("1")
    assert _decimal_confidence("0") == Decimal("0")
    for bad in ("1e-1", "1E-1", "NaN", "Infinity", "-0.1", "1.5", "abc", ""):
        assert _decimal_confidence(bad) is None, bad


def test_up_project_empty_graph_raises() -> None:
    """up_project fails fast on an empty source (its contract is non-empty input)."""
    import pytest

    with pytest.raises(ValueError, match="empty"):
        up_project(Graph())

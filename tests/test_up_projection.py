# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the clean-reversal up-projection (consumer RDF → GMEOW, #451)."""

from __future__ import annotations

from rdflib import RDF, XSD, BNode, Graph, Literal, URIRef

from gmeow_tools import sparql
from gmeow_tools.config import FIXTURES_DIR
from gmeow_tools.projections import project_graph
from gmeow_tools.up_projection import build_lift_map, up_project

GM = "https://blackcatinformatics.ca/gmeow/"
SCHEMA = "https://schema.org/"
DC = "http://purl.org/dc/elements/1.1/"


def test_lift_map_is_unambiguous() -> None:
    """Every lift rule maps a target to exactly one gmeow term; many-to-one
    down-images with no ranking winner are held out as ambiguous, never guessed."""
    lift = build_lift_map()
    assert len(lift.rules) > 200, "expected a substantial clean-reversal map"
    # rules and ambiguous are disjoint and each rule is a single gmeow IRI
    assert not (set(lift.rules) & set(lift.ambiguous))
    assert all(v.startswith(GM) for v in lift.rules.values())
    # dc:date is the down-image of six peer gmeow date properties, no identity
    # winner → genuinely ambiguous
    assert DC + "date" in lift.ambiguous
    assert DC + "date" not in lift.rules
    # a genuinely 1:1 term IS a rule
    assert lift.rules.get(SCHEMA + "conformsTo") == GM + "conformsTo"


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


def test_lift_map_closematch_claims_are_distinct_and_confident() -> None:
    """closeMatch claim rules carry a single gmeow term + a [0,1] confidence and
    never overlap the clean / inverse / ambiguous layers (clean coverage wins)."""
    lift = build_lift_map()
    assert len(lift.claim_rules) > 100, "expected a substantial closeMatch layer"
    assert not (set(lift.claim_rules) & set(lift.rules))
    assert not (set(lift.claim_rules) & set(lift.inverse_rules))
    assert not (set(lift.claim_rules) & set(lift.ambiguous))
    for gmeow, conf in lift.claim_rules.values():
        assert gmeow.startswith(GM)
        assert 0.0 <= float(conf) <= 1.0
    # schema:sender is a closeMatch of gmeow:from (not an equivalence)
    assert lift.claim_rules.get(SCHEMA + "sender") == (GM + "from", "0.9")


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

    Validated against the statement-DSL shapes *in isolation* (as the DSL
    validator does), not the merged shapes graph: the merged graph fuses the two
    same-IRI ``gmeow:AnnotationShape`` definitions (statement-DSL's annProperty
    one and the web-annotation one), a collision that never occurs in production.
    """
    from pyshacl import validate as shacl_validate

    from gmeow_tools.config import STATEMENT_DSL_SHAPES_FILE

    src = Graph()
    a = URIRef("https://ex.org/a")
    ver = URIRef(SCHEMA + "softwareVersion")
    src.add((URIRef("https://ex.org/msg"), URIRef(SCHEMA + "sender"), a))
    src.add((URIRef("https://ex.org/app"), ver, Literal("9")))
    up = up_project(src)
    assert up.claimed == 2
    shapes = Graph().parse(STATEMENT_DSL_SHAPES_FILE, format="turtle")
    conforms, _report_graph, report_text = shacl_validate(
        up.graph, shacl_graph=shapes, advanced=True, inference="none"
    )
    assert conforms, report_text


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

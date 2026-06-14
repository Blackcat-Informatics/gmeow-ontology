# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the clean-reversal up-projection (consumer RDF → GMEOW, #451)."""

from __future__ import annotations

from rdflib import RDF, BNode, Graph, Literal, URIRef

from gmeow_tools import sparql
from gmeow_tools.config import FIXTURES_DIR
from gmeow_tools.projections import project_graph
from gmeow_tools.up_projection import build_lift_map, up_project

GM = "https://blackcatinformatics.ca/gmeow/"
SCHEMA = "https://schema.org/"


def test_lift_map_is_unambiguous() -> None:
    """Every lift rule maps a target to exactly one gmeow term; many-to-one
    down-images are held out as ambiguous, never guessed."""
    lift = build_lift_map()
    assert len(lift.rules) > 200, "expected a substantial clean-reversal map"
    # rules and ambiguous are disjoint and each rule is a single gmeow IRI
    assert not (set(lift.rules) & set(lift.ambiguous))
    assert all(v.startswith(GM) for v in lift.rules.values())
    # schema:name is the down-image of several gmeow name terms → ambiguous
    assert SCHEMA + "name" in lift.ambiguous
    assert SCHEMA + "name" not in lift.rules
    # a genuinely 1:1 term IS a rule
    assert lift.rules.get(SCHEMA + "conformsTo") == GM + "conformsTo"


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
    """Ambiguous (schema:name) and structural-only (minted) terms are reported,
    never lifted — the no-fabrication discipline."""
    up = up_project(_down("names.ttl", "schema-org"))
    # schema:name is ambiguous; it must NOT appear as a gmeow lift, only reported
    assert "schema:name" in up.ambiguous_terms
    assert not any(str(p) == GM + "name" for _s, p, _o in up.graph), (
        "must not invent a gmeow:name lift for the ambiguous schema:name"
    )


def test_up_project_empty_graph_raises() -> None:
    """up_project fails fast on an empty source (its contract is non-empty input)."""
    import pytest

    with pytest.raises(ValueError, match="empty"):
        up_project(Graph())

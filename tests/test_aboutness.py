# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The universal aboutness vocabulary (#349, EPIC #348).

The mention/use distinction (describes vs enacts) is the fourth domain-free
epistemic axis alongside granularity, determinacy, and sensitivity. This module
pins that as a structural invariant: AboutnessMode is a universal gUFO
QualityValue, hasAboutness is a domain-free non-functional AnnotationProperty
(the accordingTo pattern — statement-layer cells stay DL-clean, P3)
orthogonal to every other kernel axis, and the two seeds span the mention/use
space with no privileged winner (Principle 9).
"""

from __future__ import annotations

from itertools import combinations

from rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef
from rdflib.query import ResultRow

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_aboutness_class_structure() -> None:
    g = _graph()
    assert (GM.AboutnessMode, RDF.type, OWL.Class) in g
    assert (GM.AboutnessMode, RDF.type, GUFO.AbstractIndividualType) in g
    assert (GM.AboutnessMode, RDFS.subClassOf, GUFO.QualityValue) in g


def test_has_aboutness_property_structure() -> None:
    """An AnnotationProperty, unlike the other three axes' ObjectProperties:
    aboutness is routinely asserted ABOUT statements through the statement
    layer, and the annotation form keeps the OWL downcast DL-clean (the
    accordingTo pattern, Principle 3 — design adopted from wip-aboutness-349).
    """
    g = _graph()
    assert (GM.hasAboutness, RDF.type, OWL.AnnotationProperty) in g
    assert (GM.hasAboutness, RDF.type, OWL.ObjectProperty) not in g
    assert (GM.hasAboutness, RDFS.range, GM.AboutnessMode) in g
    # Domain-free (universal, like hasGranularity / hasDeterminacy / hasSensitivity).
    assert g.value(GM.hasAboutness, RDFS.domain) is None
    # NOT functional: multi-vantage classifications coexist (Principle 9).
    assert (GM.hasAboutness, RDF.type, OWL.FunctionalProperty) not in g


def test_value_vocab_spans_two_seeds() -> None:
    g = _graph()
    members = set(g.subjects(RDF.type, GM.AboutnessMode))
    assert members == {GM.aboutnessDescribes, GM.aboutnessEnacts}


def test_aboutness_orthogonal_to_other_axes() -> None:
    """hasAboutness ⟂ every other kernel axis: no inferential bridge (Principle 9).

    Granularity is resolution, determinacy is ontic, sensitivity is privacy,
    confidence is epistemic, standpointModality is doxastic — aboutness is
    rhetorical. None may subsume or equate another.
    """
    g = _graph()
    axes = [
        GM.hasAboutness,
        GM.hasGranularity,
        GM.hasDeterminacy,
        GM.hasSensitivity,
        GM.hasDisclosurePolicy,
        GM.confidence,
    ]
    for a, b in combinations(axes, 2):
        assert (a, RDFS.subPropertyOf, b) not in g
        assert (b, RDFS.subPropertyOf, a) not in g
        assert (a, OWL.equivalentProperty, b) not in g
        assert (b, OWL.equivalentProperty, a) not in g


def test_no_aboutness_truth_bridge() -> None:
    """Enactment never implies assertion: no axiom links aboutness to
    veridicality or standpoint modality (the licensed-falsehood boundary is a
    documented bridge, not an entailment)."""
    g = _graph()
    for seed in (GM.aboutnessDescribes, GM.aboutnessEnacts):
        # Seeds are plain vocabulary individuals — exactly one class membership.
        types = set(g.objects(seed, RDF.type))
        assert types == {GM.AboutnessMode}


def test_competency_aboutness_modes_query() -> None:
    """The aboutness-modes competency query returns exactly the two seeds."""
    from gmeow_tools.config import COMPETENCY_DIR

    query = (COMPETENCY_DIR / "aboutness-modes.rq").read_text(encoding="utf-8")
    modes: set[object] = set()
    for row in _graph().query(query):
        assert isinstance(row, ResultRow)
        modes.add(row[0])
    assert modes == {GM.aboutnessDescribes, GM.aboutnessEnacts}


def test_wellformed_aboutness_fixture_conforms() -> None:
    """A carrier can describe one thing while enacting another — both cells valid."""
    from pathlib import Path

    from tests._graph_nt import run_shacl

    g = Graph()
    g.parse(
        Path(__file__).parent / "fixtures" / "shapes" / "aboutness-wellformed.ttl",
        format="turtle",
    )
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_malformed_aboutness_fixture_is_flagged() -> None:
    """hasAboutness must target a vocabulary IRI, never a free literal."""
    from pathlib import Path

    from tests._graph_nt import run_shacl

    g = Graph()
    g.parse(
        Path(__file__).parent / "fixtures" / "shapes" / "aboutness-malformed.ttl",
        format="turtle",
    )
    result = run_shacl(g)
    assert not result.ok
    assert "not a free literal" in "\n".join(result.errors)


def test_every_term_labeled_and_defined() -> None:
    g = _graph()
    skos_def = URIRef("http://www.w3.org/2004/02/skos/core#definition")
    for term in (
        GM.AboutnessMode,
        GM.hasAboutness,
        GM.aboutnessDescribes,
        GM.aboutnessEnacts,
    ):
        assert g.value(term, RDFS.label) is not None, f"{term} missing label"
        assert g.value(term, skos_def) is not None, f"{term} missing definition"

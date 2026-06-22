"""The universal determinacy vocabulary (#71).

Ontic indeterminacy (crisp, vague, fuzzy, probabilistic, disputed) is held
distinct from epistemic confidence (Principle 9). This module pins that as a
structural invariant: Determinacy is a universal logic:QualityValue, hasDeterminacy
is a domain-free non-functional ObjectProperty orthogonal to confidence, and the
five seeds span the determinacy space with no privileged winner.
"""

from __future__ import annotations

from itertools import combinations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
LOGIC = "https://blackcatinformatics.ca/logic/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_determinacy_class_structure() -> None:
    g = _graph()
    assert (GM.Determinacy, RDF.type, OWL.Class) in g
    assert (
        GM.Determinacy,
        RDF.type,
        URIRef(LOGIC + "AbstractIndividualType"),
    ) in g
    assert (
        GM.Determinacy,
        RDFS.subClassOf,
        URIRef(LOGIC + "QualityValue"),
    ) in g


def test_has_determinacy_property_structure() -> None:
    g = _graph()
    assert (GM.hasDeterminacy, RDF.type, OWL.ObjectProperty) in g
    assert (GM.hasDeterminacy, RDFS.range, GM.Determinacy) in g
    # Domain-free (universal, like hasGranularity).
    assert g.value(GM.hasDeterminacy, RDFS.domain) is None
    # NOT functional: multi-source claims coexist (Principle 9).
    assert (GM.hasDeterminacy, RDF.type, OWL.FunctionalProperty) not in g


def test_determinacy_model_preserved() -> None:
    """The frame-level default remains functional on ReferenceFrame (Principle 11)."""
    g = _graph()
    assert (GM.determinacyModel, RDF.type, OWL.ObjectProperty) in g
    assert (GM.determinacyModel, RDF.type, OWL.FunctionalProperty) in g
    assert (GM.determinacyModel, RDFS.domain, GM.ReferenceFrame) in g
    assert (GM.determinacyModel, RDFS.range, GM.Determinacy) in g


def test_value_vocab_spans_five_seeds() -> None:
    g = _graph()
    members = set(g.subjects(RDF.type, GM.Determinacy))
    assert members == {
        GM.determinacyCrisp,
        GM.determinacyVague,
        GM.determinacyFuzzy,
        GM.determinacyProbabilistic,
        GM.determinacyDisputed,
    }


def test_determinacy_confidence_orthogonal() -> None:
    """hasDeterminacy ⟂ confidence: no inferential bridge (Principle 9)."""
    g = _graph()
    axes = [GM.hasDeterminacy, GM.confidence]
    for a, b in combinations(axes, 2):
        assert (a, RDFS.subPropertyOf, b) not in g
        assert (b, RDFS.subPropertyOf, a) not in g
        assert (a, OWL.equivalentProperty, b) not in g
        assert (b, OWL.equivalentProperty, a) not in g


def test_has_determinacy_determinacy_model_distinct() -> None:
    """The universal facet and the frame default are unrelated properties."""
    g = _graph()
    assert (GM.hasDeterminacy, RDFS.subPropertyOf, GM.determinacyModel) not in g
    assert (GM.determinacyModel, RDFS.subPropertyOf, GM.hasDeterminacy) not in g
    assert (GM.hasDeterminacy, OWL.equivalentProperty, GM.determinacyModel) not in g


def test_no_preferred_or_primary_term_is_declared() -> None:
    """No GMEOW vocabulary term is a preferred/primary selector (Principle 9)."""
    g = _graph()
    offenders = []
    for s in set(g.subjects()):
        if not isinstance(s, URIRef) or not str(s).startswith(GMEOW):
            continue
        local = str(s)[len(GMEOW) :].lower()
        if "/" not in local and local.startswith(("primary", "preferred")):
            offenders.append(str(s))
    assert offenders == [], f"preferred/primary terms must not exist: {offenders}"

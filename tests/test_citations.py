"""Citation & Credit module structural guards (issue #211).

Pins the CitationAct relator, CitationIntent and ContributionDegree value vocabularies,
Selector, and SourceRole. Verifies gUFO grounding, property existence, and SHACL
well-formedness.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# =========================================================================== #
# Class hierarchy
# =========================================================================== #


def test_citation_act_is_relator_kind() -> None:
    g = _graph()
    assert (GMEOW.CitationAct, RDF.type, OWL.Class) in g
    assert (GMEOW.CitationAct, RDF.type, GUFO.Kind) in g
    assert (GMEOW.CitationAct, RDFS.subClassOf, GUFO.Relator) in g


def test_selector_is_kind() -> None:
    g = _graph()
    assert (GMEOW.Selector, RDF.type, OWL.Class) in g
    assert (GMEOW.Selector, RDF.type, GUFO.Kind) in g
    assert (GMEOW.Selector, RDFS.subClassOf, GMEOW.EvidenceSpan) in g


def test_source_role_is_role_mixin() -> None:
    g = _graph()
    assert (GMEOW.SourceRole, RDF.type, OWL.Class) in g
    assert (GMEOW.SourceRole, RDF.type, GUFO.RoleMixin) in g


def test_citation_intent_is_quality_value() -> None:
    g = _graph()
    assert (GMEOW.CitationIntent, RDFS.subClassOf, GUFO.QualityValue) in g


def test_contribution_degree_is_quality_value() -> None:
    g = _graph()
    assert (GMEOW.ContributionDegree, RDFS.subClassOf, GUFO.QualityValue) in g


# =========================================================================== #
# Properties
# =========================================================================== #


def test_citation_act_properties_exist() -> None:
    g = _graph()
    for prop in (
        GMEOW.citingEntity,
        GMEOW.citedEntity,
        GMEOW.citationIntent,
    ):
        assert (prop, RDF.type, OWL.ObjectProperty) in g
        assert (prop, RDF.type, OWL.FunctionalProperty) in g
    assert (GMEOW.viaSelector, RDF.type, OWL.ObjectProperty) in g
    assert (GMEOW.cites, RDF.type, OWL.ObjectProperty) in g


def test_citation_intent_seeds_exist() -> None:
    g = _graph()
    for ind in (
        GMEOW.intentCitesAsDataSource,
        GMEOW.intentUsesMethodIn,
        GMEOW.intentExtends,
        GMEOW.intentIsInspiredBy,
        GMEOW.intentConformsTo,
        GMEOW.intentDerivedFrom,
        GMEOW.intentDocuments,
        GMEOW.intentSupports,
        GMEOW.intentDisagreesWith,
        GMEOW.intentBridgedByReference,
    ):
        assert (ind, RDF.type, GMEOW.CitationIntent) in g


def test_selector_properties_exist() -> None:
    g = _graph()
    for prop in (
        GMEOW.selectorPage,
        GMEOW.selectorTextPosition,
        GMEOW.selectorTextQuote,
        GMEOW.selectorLocator,
    ):
        assert (prop, RDF.type, OWL.DatatypeProperty) in g


def test_contribution_degree_seeds_exist() -> None:
    g = _graph()
    for ind in (
        GMEOW.degreeLead,
        GMEOW.degreeEqual,
        GMEOW.degreeSupporting,
    ):
        assert (ind, RDF.type, GMEOW.ContributionDegree) in g


# =========================================================================== #
# SHACL well-formedness
# =========================================================================== #


def test_citation_act_shacl_passes() -> None:
    """A well-formed CitationAct relator passes SHACL."""
    g = Graph()
    g.add((EX.citation, RDF.type, GMEOW.CitationAct))
    g.add((EX.citation, GMEOW.citingEntity, EX.claim))
    g.add((EX.citation, GMEOW.citedEntity, EX.work))
    g.add((EX.citation, GMEOW.citationIntent, GMEOW.intentCitesAsDataSource))
    g.add((EX.claim, RDF.type, GMEOW.Entity))
    g.add((EX.work, RDF.type, GMEOW.Work))
    g.add((EX.work, RDFS.label, Literal("Test Work")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_citation_act_missing_intent_fails_shacl() -> None:
    """A CitationAct without citationIntent violates SHACL."""
    g = Graph()
    g.add((EX.citation, RDF.type, GMEOW.CitationAct))
    g.add((EX.citation, GMEOW.citingEntity, EX.claim))
    g.add((EX.citation, GMEOW.citedEntity, EX.work))
    g.add((EX.claim, RDF.type, GMEOW.Entity))
    g.add((EX.work, RDF.type, GMEOW.Work))
    g.add((EX.work, RDFS.label, Literal("Test Work")))

    result = run_shacl(g)
    assert not result.ok
    assert any("citation intent" in e.lower() for e in result.errors)


def test_contribution_with_degree_shacl_passes() -> None:
    """A Contribution with an optional degree passes SHACL."""
    g = Graph()
    g.add((EX.contribution, RDF.type, GMEOW.Contribution))
    g.add((EX.contribution, GMEOW.contributor, EX.alice))
    g.add((EX.contribution, GMEOW.contributionTarget, EX.work))
    g.add((EX.contribution, GMEOW.contributionRole, GMEOW.roleAuthor))
    g.add((EX.contribution, GMEOW.contributionDegree, GMEOW.degreeLead))
    g.add((EX.alice, RDF.type, GMEOW.Agent))
    g.add((EX.work, RDF.type, GMEOW.Work))
    g.add((EX.work, RDFS.label, Literal("Test Work")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


# =========================================================================== #
# Self-description loader
# =========================================================================== #


def test_self_description_loader() -> None:
    from gmeow_tools.self_desc import load_self_description

    meta = load_self_description()
    assert meta.title.startswith("GMEOW")
    assert meta.version == "0.1.0"
    assert meta.release_date == "2026-06-03"
    assert meta.doi == "10.XXXXX/gmeow"
    assert meta.depositor_name == "Blackcat Informatics Inc."
    assert meta.depositor_email == "doi@blackcatinformatics.ca"
    assert meta.registrant == "Blackcat Informatics Inc."
    assert meta.license_uri == "https://creativecommons.org/licenses/by/4.0/"
    assert meta.homepage == "https://blackcatinformatics.ca/gmeow"

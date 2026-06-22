"""Citation & Credit module structural guards (issue #211).

Pins the CitationAct relator, CitationIntent and ContributionDegree value vocabularies,
Selector, and SourceRole. Verifies gUFO grounding, property existence, and SHACL
well-formedness.
"""

from __future__ import annotations

import re

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test/")
SELF = Namespace("https://blackcatinformatics.ca/gmeow/self#")


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
    # Concept DOI lives on the Work; version DOI (optional) on the Manifestation.
    assert meta.concept_doi == "10.67342/26w4o"
    assert meta.version_doi is None  # concept-only until a version DOI is minted
    assert meta.doi == "10.67342/26w4o"  # preferred citable = version or concept
    assert meta.version_iri == "https://blackcatinformatics.ca/gmeow/0.1.0"
    assert meta.depositor_name == "Blackcat Informatics® Inc."
    assert meta.depositor_email == "root@blackcatinformatics.ca"
    assert meta.registrant == "Blackcat Informatics® Inc."
    assert meta.license_uri == "https://creativecommons.org/licenses/by/4.0/"
    assert meta.homepage == "https://blackcatinformatics.ca/gmeow"


def test_self_description_models_project_repository_and_brand_assets() -> None:
    from gmeow_tools.self_desc import SELF_DESC_FILE

    g = Graph()
    g.parse(SELF_DESC_FILE, format="turtle")

    ontology = URIRef("https://blackcatinformatics.ca/gmeow")

    assert (SELF.project, RDF.type, GMEOW.SoftwareProject) in g
    assert (SELF.project, GMEOW.hasRepository, SELF.repository) in g
    assert (SELF.project, GMEOW.maintenanceStatus, GMEOW.statusActive) in g
    assert (SELF.project, GMEOW.projectLicense, SELF["license-agpl-3"]) in g
    assert (SELF["license-agpl-3"], RDF.type, GMEOW.License) in g
    assert (
        SELF["license-agpl-3"],
        GMEOW.licensor,
        URIRef("https://blackcatinformatics.ca/#bii"),
    ) in g
    assert (SELF.repository, RDF.type, GMEOW.Repository) in g
    assert (SELF.repository, GMEOW.repositoryType, GMEOW.repoTypeGit) in g

    assert (SELF.project, GMEOW.hasLogo, SELF["logo-svg"]) in g
    assert (ontology, GMEOW.hasLogo, SELF["logo-svg"]) in g
    assert (SELF["logo-svg"], RDF.type, GMEOW.MediaObject) in g
    assert (SELF["logo-svg"], GMEOW.mediaType, Literal("image/svg+xml")) in g
    assert (SELF["logo-svg"], GMEOW.depicts, ontology) not in g

    assert (SELF["social-preview-png"], RDF.type, GMEOW.MediaObject) in g
    assert (
        SELF["social-preview-png"],
        GMEOW.wasDerivedFrom,
        SELF["social-preview-svg"],
    ) in g


# =========================================================================== #
# Canonical description — standardized across all surfaces (single source)
# =========================================================================== #


def _norm_ws(text: str) -> str:
    """Collapse all whitespace runs so cross-format copies compare equal."""
    return " ".join(text.split())


def test_canonical_description_is_standardized() -> None:
    """One abstract, identical across self-desc / ontology header / CITATION.cff.

    Also asserts the stated vocabulary count matches the real count. The slice
    count is deliberately NOT stated in prose — it would drift as slices are
    added; the manifest tier is the sole source of slice truth.
    """
    import yaml
    from gmeow_rdf.compat.rdflib.namespace import DCTERMS

    from gmeow_tools.config import (
        ALIGNMENT_TARGETS,
        ONTOLOGY_FILE,
        PROJECT_ROOT,
    )
    from gmeow_tools.self_desc import load_self_description

    canonical = load_self_description().description
    assert canonical, "self-description carries no description"

    # ontology header dcterms:description == canonical (the serialization-facing copy)
    onto = Graph()
    onto.parse(ONTOLOGY_FILE, format="turtle")
    onto_desc = str(
        next(
            onto.objects(
                URIRef("https://blackcatinformatics.ca/gmeow"), DCTERMS.description
            )
        )
    )
    assert _norm_ws(onto_desc) == _norm_ws(canonical)

    # CITATION.cff abstract == canonical (the human-citation copy)
    cff = yaml.safe_load((PROJECT_ROOT / "CITATION.cff").read_text(encoding="utf-8"))
    assert _norm_ws(cff["abstract"]) == _norm_ws(canonical)

    # The stated vocabulary count must equal the real count — and the slice count
    # must NOT be stated in prose (it would drift; manifests are the truth).
    n_align = len(ALIGNMENT_TARGETS)
    assert f"{n_align} external vocabularies" in canonical
    assert "self-contained slices" in canonical
    assert not re.search(r"\d+\s+self-contained slices", canonical), (
        "slice count must not be hard-coded in the canonical description"
    )

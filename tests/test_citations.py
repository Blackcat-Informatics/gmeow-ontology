"""Citation & Credit module retained guards (issue #211).

The asserted-TBox structural invariants (CitationAct, Selector, SourceRole,
CitationIntent, ContributionDegree class hierarchy; citingEntity/citedEntity/
citationIntent/viaSelector/cites property shapes; CitationIntent and
ContributionDegree seed individuals; Selector datatype properties) have been
migrated to the declarative slicetest DSL in
slices/core/citations/tests/structural.ttl (#867).

SHACL ExampleConformance checks (test_citation_act_shacl_passes,
test_citation_act_missing_intent_fails_shacl,
test_contribution_with_degree_shacl_passes) have been migrated to the Rust
conformance suite in crates/validate/tests/conformance_citations.rs (#867).

Retained here: self-description loader and the whole-ontology
canonical-description standardisation sweep.
"""

from __future__ import annotations

import re

from gmeow_rdf.compat.rdflib import RDF, Graph, Literal, Namespace, URIRef

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test/")
SELF = Namespace("https://blackcatinformatics.ca/gmeow/self#")


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

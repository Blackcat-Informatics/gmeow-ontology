"""Tests for the CrossRef DOI deposit-XML generator and doi-lint."""

from __future__ import annotations

import dataclasses
from xml.etree import ElementTree as ET

from gmeow_tools.config import ALIGNMENT_TARGETS, ONTOLOGY_IRI
from gmeow_tools.crossref import (
    AI_NS,
    CR_NS,
    REL_NS,
    build_deposit_xml,
    lint_deposit,
)
from gmeow_tools.self_desc import SelfDescription, full_doi, load_self_description


def _parse(xml: str) -> ET.Element:
    return ET.fromstring(xml)


def _with_version_doi(version_doi: str = "10.67342/v010") -> SelfDescription:
    """The real self-description, but with a minted version DOI for two-record tests."""
    return dataclasses.replace(load_self_description(), version_doi=version_doi)


def _doi_data_for(root: ET.Element, doi: str) -> ET.Element:
    for doi_data in root.iter(f"{{{CR_NS}}}doi_data"):
        if doi_data.findtext(f"{{{CR_NS}}}doi") == doi:
            return doi_data
    raise AssertionError(f"missing doi_data for {doi}")


def test_deposit_is_well_formed() -> None:
    root = _parse(build_deposit_xml(timestamp="20260603120000"))
    assert root.tag == f"{{{CR_NS}}}doi_batch"
    assert root.attrib["version"] == "5.4.0"


def test_deposit_carries_doi_and_resource() -> None:
    root = _parse(build_deposit_xml(timestamp="20260603120000"))
    doi = root.find(f".//{{{CR_NS}}}doi")
    resource = root.find(f".//{{{CR_NS}}}resource")
    assert doi is not None and doi.text == full_doi()
    assert resource is not None and resource.text == ONTOLOGY_IRI


def test_deposit_batch_id_and_timestamp_deterministic() -> None:
    xml = build_deposit_xml(timestamp="20260603120000", batch_id="gmeow-test")
    root = _parse(xml)
    assert root.find(f".//{{{CR_NS}}}doi_batch_id").text == "gmeow-test"  # type: ignore[union-attr]
    assert root.find(f".//{{{CR_NS}}}timestamp").text == "20260603120000"  # type: ignore[union-attr]


def test_default_batch_stamp_is_a_live_submission_timestamp() -> None:
    """Transient submission doc: the default timestamp is live (14-digit)."""
    root = _parse(build_deposit_xml())
    ts = root.find(f".//{{{CR_NS}}}timestamp").text  # type: ignore[union-attr]
    assert ts is not None and ts.isdigit() and len(ts) == 14
    batch = root.find(f".//{{{CR_NS}}}doi_batch_id").text  # type: ignore[union-attr]
    assert batch is not None and batch.startswith("gmeow-0.1.0-")


def test_deposit_has_depositor_and_dataset() -> None:
    root = _parse(build_deposit_xml(timestamp="20260603120000"))
    assert root.find(f".//{{{CR_NS}}}depositor_name") is not None
    assert root.find(f".//{{{CR_NS}}}registrant") is not None
    dataset = root.find(f".//{{{CR_NS}}}dataset")
    assert dataset is not None and dataset.attrib["dataset_type"] == "record"


def test_publication_date_split() -> None:
    root = _parse(build_deposit_xml(timestamp="20260603120000"))
    assert root.find(f".//{{{CR_NS}}}year").text == "2026"  # type: ignore[union-attr]
    assert root.find(f".//{{{CR_NS}}}month").text == "06"  # type: ignore[union-attr]
    assert root.find(f".//{{{CR_NS}}}day").text == "03"  # type: ignore[union-attr]


def test_concept_only_has_single_record_and_no_version_link() -> None:
    """With no version DOI, exactly one dataset and no concept↔version link.

    hasFormat relations are themselves intra_work_relations, so the absence is
    checked specifically for the isVersionOf / hasVersion edge.
    """
    root = _parse(build_deposit_xml())
    datasets = root.findall(f".//{{{CR_NS}}}dataset")
    assert len(datasets) == 1
    version_links = {
        r.attrib["relationship-type"]
        for r in root.findall(f".//{{{REL_NS}}}intra_work_relation")
    } & {"isVersionOf", "hasVersion"}
    assert not version_links


def test_inter_work_relations_project_alignment_graph() -> None:
    """The curated alignment targets become inter_work_relation entries."""
    root = _parse(build_deposit_xml())
    relations = root.findall(f".//{{{REL_NS}}}inter_work_relation")
    assert relations, "expected alignment-derived inter_work_relation entries"
    types = {r.attrib["relationship-type"] for r in relations}
    assert "references" in types
    assert "isDerivedFrom" in types  # upper ontologies (e.g. gUFO / BFO)


def test_version_doi_emits_two_records_with_intra_relations() -> None:
    meta = _with_version_doi()
    root = _parse(build_deposit_xml(meta=meta))
    datasets = root.findall(f".//{{{CR_NS}}}dataset")
    assert len(datasets) == 2

    intra = {
        (r.attrib["relationship-type"], r.text)
        for r in root.findall(f".//{{{REL_NS}}}intra_work_relation")
    }
    assert ("hasVersion", meta.version_doi) in intra
    assert ("isVersionOf", meta.concept_doi) in intra

    # Each DOI resolves to its own resource (concept IRI vs versionIRI).
    pairs = {
        (
            dd.findtext(f"{{{CR_NS}}}doi"),
            dd.findtext(f"{{{CR_NS}}}resource"),
        )
        for dd in root.iter(f"{{{CR_NS}}}doi_data")
    }
    assert (meta.concept_doi, ONTOLOGY_IRI) in pairs
    assert (meta.version_doi, meta.version_iri) in pairs

    # hasFormat relations are version-scoped: the version record points at its
    # own immutable release snapshot, not the mutable always-latest Work formats.
    has_format = {
        r.text
        for r in root.findall(f".//{{{REL_NS}}}intra_work_relation")
        if r.attrib["relationship-type"] == "hasFormat"
    }
    assert f"{meta.version_iri}.ttl" in has_format  # version record
    assert f"{ONTOLOGY_IRI}.ttl" in has_format  # concept record


def test_deposit_carries_access_indicators_license() -> None:
    """The CC license is deposited via the AccessIndicators ai:program."""
    root = _parse(build_deposit_xml())
    license_refs = root.findall(f".//{{{AI_NS}}}license_ref")
    applies_to = {node.attrib["applies_to"] for node in license_refs}
    assert applies_to == {"tdm", "vor"}
    assert {node.text for node in license_refs} == {load_self_description().license_uri}
    assert root.find(f".//{{{AI_NS}}}free_to_read") is not None


def test_deposit_carries_registrant_wikidata_institution_id() -> None:
    """BII's QID is Crossref-valid institution metadata."""
    root = _parse(build_deposit_xml())
    ids = {
        (node.attrib["type"], node.text)
        for node in root.findall(f".//{{{CR_NS}}}institution_id")
    }
    assert ("wikidata", "https://www.wikidata.org/entity/Q140285712") in ids


def test_deposit_carries_person_contributor_with_orcid() -> None:
    """A person author is emitted as person_name with given/surname + ORCID."""
    root = _parse(build_deposit_xml())
    orcids = [e.text for e in root.iter(f"{{{CR_NS}}}ORCID")]
    assert any(o and o.startswith("https://orcid.org/") for o in orcids)
    assert root.find(f".//{{{CR_NS}}}organization") is not None  # org too


def test_deposit_carries_format_and_serialization_relations() -> None:
    """<format> is present and every serialization has a hasFormat relation."""
    root = _parse(build_deposit_xml())
    assert root.find(f".//{{{CR_NS}}}format") is not None
    has_format = [
        r.text
        for r in root.findall(f".//{{{REL_NS}}}intra_work_relation")
        if r.attrib["relationship-type"] == "hasFormat"
    ]
    assert f"{ONTOLOGY_IRI}.ttl" in has_format
    assert f"{ONTOLOGY_IRI}.gts" in has_format


def test_deposit_carries_text_mining_urls() -> None:
    """TDM URLs expose every machine-readable ontology serialization."""
    root = _parse(build_deposit_xml())
    doi_data = _doi_data_for(root, full_doi())
    collection = doi_data.find(f"{{{CR_NS}}}collection[@property='text-mining']")
    assert collection is not None
    resources = {
        (node.text, node.attrib["mime_type"], node.attrib["content_version"])
        for node in collection.findall(f".//{{{CR_NS}}}resource")
    }
    assert (f"{ONTOLOGY_IRI}.ttl", "text/turtle", "vor") in resources
    assert (f"{ONTOLOGY_IRI}.rdf", "application/rdf+xml", "vor") in resources
    assert (f"{ONTOLOGY_IRI}.nt", "application/n-triples", "vor") in resources
    assert (f"{ONTOLOGY_IRI}.jsonld", "application/ld+json", "vor") in resources
    assert (
        f"{ONTOLOGY_IRI}.gts",
        "application/cbor-seq",
        "vor",
    ) in resources


def test_version_doi_carries_version_scoped_text_mining_urls() -> None:
    """Version DOI TDM URLs point at immutable version serializations."""
    meta = _with_version_doi()
    root = _parse(build_deposit_xml(meta=meta))
    doi_data = _doi_data_for(root, meta.version_doi or "")
    collection = doi_data.find(f"{{{CR_NS}}}collection[@property='text-mining']")
    assert collection is not None
    resources = {node.text for node in collection.findall(f".//{{{CR_NS}}}resource")}
    assert f"{meta.version_iri}.ttl" in resources
    assert f"{ONTOLOGY_IRI}.ttl" in {
        node.text
        for node in _doi_data_for(root, meta.concept_doi).findall(
            f".//{{{CR_NS}}}resource"
        )
    }


def test_citation_list_projects_alignment_targets() -> None:
    """The alignment registry becomes Crossref references."""
    root = _parse(build_deposit_xml())
    citations = root.findall(f".//{{{CR_NS}}}citation_list/{{{CR_NS}}}citation")
    assert len(citations) == len(ALIGNMENT_TARGETS)
    by_key = {node.attrib["key"]: node for node in citations}
    assert {"ref-bfo", "ref-gufo", "ref-schema"}.issubset(by_key)
    gufo = by_key["ref-gufo"]
    assert gufo.attrib["type"] == "web_resource"
    assert gufo.findtext(f"{{{CR_NS}}}article_title") == "gUFO"
    assert (
        gufo.findtext(f"{{{CR_NS}}}unstructured_citation")
        == "gUFO. http://purl.org/nemo/gufo#."
    )


def test_deposit_omits_unsupported_crossref_fields() -> None:
    """Do not spoof unavailable metadata just to raise participation metrics."""
    xml = build_deposit_xml()
    root = _parse(xml)
    assert root.find(f".//{{{CR_NS}}}subject") is None
    assert root.find(f".//{{{CR_NS}}}keywords") is None
    assert 'property="crawler-based"' not in xml
    assert "similarity-check" not in xml
    assert "Crossmark" not in xml
    assert "fundref" not in xml


def test_lint_passes_on_real_self_description() -> None:
    assert lint_deposit() == []


def test_lint_flags_placeholder_doi() -> None:
    bad = dataclasses.replace(load_self_description(), concept_doi="10.XXXXX/gmeow")
    problems = lint_deposit(bad)
    assert any("placeholder" in p for p in problems)

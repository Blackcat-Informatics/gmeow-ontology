"""Tests for the CrossRef DOI deposit-XML generator and doi-lint."""

from __future__ import annotations

import dataclasses
from xml.etree import ElementTree as ET

from gmeow_tools.config import ONTOLOGY_IRI
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


def test_deposit_carries_access_indicators_license() -> None:
    """The CC license is deposited via the AccessIndicators ai:program."""
    root = _parse(build_deposit_xml())
    license_ref = root.find(f".//{{{AI_NS}}}license_ref")
    assert license_ref is not None
    assert license_ref.text == load_self_description().license_uri
    assert license_ref.attrib["applies_to"] == "vor"
    assert root.find(f".//{{{AI_NS}}}free_to_read") is not None


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


def test_lint_passes_on_real_self_description() -> None:
    assert lint_deposit() == []


def test_lint_flags_placeholder_doi() -> None:
    bad = dataclasses.replace(load_self_description(), concept_doi="10.XXXXX/gmeow")
    problems = lint_deposit(bad)
    assert any("placeholder" in p for p in problems)

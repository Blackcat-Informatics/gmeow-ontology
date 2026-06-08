"""Tests for the CrossRef DOI deposit-XML generator."""

from __future__ import annotations

from xml.etree import ElementTree as ET

from gmeow_tools.config import ONTOLOGY_IRI
from gmeow_tools.crossref import CR_NS, build_deposit_xml
from gmeow_tools.self_desc import full_doi


def _parse(xml: str) -> ET.Element:
    return ET.fromstring(xml)


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


def test_deposit_has_depositor_and_dataset() -> None:
    root = _parse(build_deposit_xml(timestamp="20260603120000"))
    assert root.find(f".//{{{CR_NS}}}depositor_name") is not None
    assert root.find(f".//{{{CR_NS}}}registrant") is not None
    dataset = root.find(f".//{{{CR_NS}}}dataset")
    assert dataset is not None and dataset.attrib["dataset_type"] == "record"


def test_publication_date_split() -> None:
    root = _parse(
        build_deposit_xml(timestamp="20260603120000", release_date="2026-06-03")
    )
    assert root.find(f".//{{{CR_NS}}}year").text == "2026"  # type: ignore[union-attr]
    assert root.find(f".//{{{CR_NS}}}month").text == "06"  # type: ignore[union-attr]
    assert root.find(f".//{{{CR_NS}}}day").text == "03"  # type: ignore[union-attr]

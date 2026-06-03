"""Generate a CrossRef deposit document for the GMEOW DOI.

Blackcat Informatics mints GMEOW's DOI as a CrossRef member (its own prefix),
not via Zenodo. CrossRef registration is by depositing XML conforming to the
CrossRef deposit schema (5.4.0); an ontology is deposited as a ``<database>`` /
``<dataset>`` record whose DOI resolves to the ontology landing page.

This module builds that deposit XML from the ontology metadata in ``config``.
The output is well-formed and follows the schema's element model; validate it
against the official XSD (and test on the CrossRef *test* system) before a
production deposit. The DOI prefix and depositor email in ``config`` are
placeholders until CrossRef membership is finalized.
"""

from __future__ import annotations

import datetime
from pathlib import Path
from xml.etree import ElementTree as ET

from gmeow_tools.config import (
    CROSSREF_DEPOSITOR_EMAIL,
    CROSSREF_DEPOSITOR_NAME,
    CROSSREF_REGISTRANT,
    DIST_DIR,
    ONTOLOGY_IRI,
    RELEASE_DATE,
    TITLE,
    VERSION,
    full_doi,
)

#: CrossRef deposit schema namespace (version 5.4.0).
CR_NS = "http://www.crossref.org/schema/5.4.0"
_XSI_NS = "http://www.w3.org/2001/XMLSchema-instance"
_SCHEMA_LOCATION = f"{CR_NS} https://www.crossref.org/schemas/crossref5.4.0.xsd"


def _child(
    parent: ET.Element, tag: str, text: str | None = None, **attrs: str
) -> ET.Element:
    """Append a namespaced child element with optional text and attributes."""
    element = ET.SubElement(parent, f"{{{CR_NS}}}{tag}", attrs)
    if text is not None:
        element.text = text
    return element


def build_deposit_xml(
    *,
    doi: str | None = None,
    timestamp: str | None = None,
    batch_id: str | None = None,
    release_date: str = RELEASE_DATE,
) -> str:
    """Build the CrossRef deposit XML for the GMEOW DOI.

    Args:
        doi: The DOI to register (defaults to ``config.full_doi()``).
        timestamp: CrossRef batch timestamp (``YYYYMMDDHHMMSS``); defaults to the
            current UTC time. CrossRef uses it to order competing submissions.
        batch_id: Unique submission id (defaults to ``gmeow-{version}-{timestamp}``).
        release_date: ISO-8601 publication date for the dataset record.

    Returns:
        The deposit document as an XML string (with declaration).
    """
    doi = doi or full_doi()
    if timestamp is None:
        timestamp = datetime.datetime.now(datetime.UTC).strftime("%Y%m%d%H%M%S")
    batch_id = batch_id or f"gmeow-{VERSION}-{timestamp}"
    year, month, day = release_date.split("-")

    ET.register_namespace("", CR_NS)
    ET.register_namespace("xsi", _XSI_NS)
    root = ET.Element(
        f"{{{CR_NS}}}doi_batch",
        {
            f"{{{_XSI_NS}}}schemaLocation": _SCHEMA_LOCATION,
            "version": "5.4.0",
        },
    )

    head = _child(root, "head")
    _child(head, "doi_batch_id", batch_id)
    _child(head, "timestamp", timestamp)
    depositor = _child(head, "depositor")
    _child(depositor, "depositor_name", CROSSREF_DEPOSITOR_NAME)
    _child(depositor, "email_address", CROSSREF_DEPOSITOR_EMAIL)
    _child(head, "registrant", CROSSREF_REGISTRANT)

    body = _child(root, "body")
    database = _child(body, "database")
    db_metadata = _child(database, "database_metadata", language="en")
    _child(_child(db_metadata, "titles"), "title", TITLE)

    dataset = _child(database, "dataset", dataset_type="record")
    contributors = _child(dataset, "contributors")
    _child(
        contributors,
        "organization",
        CROSSREF_REGISTRANT,
        sequence="first",
        contributor_role="author",
    )
    _child(_child(dataset, "titles"), "title", f"{TITLE} (version {VERSION})")
    publication_date = _child(
        _child(dataset, "database_date"), "publication_date", media_type="online"
    )
    _child(publication_date, "year", year)
    _child(publication_date, "month", month)
    _child(publication_date, "day", day)
    doi_data = _child(dataset, "doi_data")
    _child(doi_data, "doi", doi)
    _child(doi_data, "resource", ONTOLOGY_IRI)

    ET.indent(root)
    return ET.tostring(root, encoding="unicode", xml_declaration=True)


def write_deposit(path: Path | None = None, **kwargs: str) -> Path:
    """Write the CrossRef deposit XML to ``dist/``.

    Args:
        path: Output path (defaults to ``dist/crossref-deposit.xml``).
        **kwargs: Forwarded to :func:`build_deposit_xml`.

    Returns:
        The path written.
    """
    out = path or (DIST_DIR / "crossref-deposit.xml")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(build_deposit_xml(**kwargs) + "\n", encoding="utf-8")
    return out

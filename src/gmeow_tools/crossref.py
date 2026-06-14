"""Generate a CrossRef deposit document for the GMEOW DOI.

Blackcat Informatics mints GMEOW's DOI as a CrossRef member (its own prefix),
not via Zenodo. CrossRef registration is by depositing XML conforming to the
CrossRef deposit schema (5.4.0); an ontology is deposited as a ``<database>`` /
``<dataset>`` record whose DOI resolves to the ontology landing page.

This module builds that deposit XML from the ontology self-description in
``metadata/gmeow-self.ttl``. The output is well-formed and follows the schema's
element model; validate it against the official XSD (and test on the CrossRef
*test* system) before a production deposit.
"""

from __future__ import annotations

import datetime
from pathlib import Path
from xml.etree import ElementTree as ET

from gmeow_tools.config import DIST_DIR, ONTOLOGY_IRI
from gmeow_tools.self_desc import SelfDescription, load_self_description

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
    meta: SelfDescription | None = None,
    doi: str | None = None,
    timestamp: str | None = None,
    batch_id: str | None = None,
    release_date: str | None = None,
) -> str:
    """Build the CrossRef deposit XML for the GMEOW DOI.

    Args:
        meta: Preloaded self-description metadata. Defaults to the checkout file.
        doi: The DOI to register (defaults to the DOI from self-description).
        timestamp: CrossRef batch timestamp (``YYYYMMDDHHMMSS``); defaults to the
            current UTC time. CrossRef uses it to order competing submissions.
        batch_id: Unique submission id (defaults to ``gmeow-{version}-{timestamp}``).
        release_date: ISO-8601 publication date for the dataset record.

    Returns:
        The deposit document as an XML string (with declaration).
    """
    description = meta or load_self_description()
    doi = doi or description.doi
    if timestamp is None:
        timestamp = datetime.datetime.now(datetime.UTC).strftime("%Y%m%d%H%M%S")
    batch_id = batch_id or f"gmeow-{description.version}-{timestamp}"
    release_date = release_date or description.release_date
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
    _child(depositor, "depositor_name", description.depositor_name)
    _child(depositor, "email_address", description.depositor_email)
    _child(head, "registrant", description.registrant)

    body = _child(root, "body")
    database = _child(body, "database")
    db_metadata = _child(database, "database_metadata", language="en")
    _child(_child(db_metadata, "titles"), "title", description.title)

    dataset = _child(database, "dataset", dataset_type="record")
    contributors = _child(dataset, "contributors")
    _child(
        contributors,
        "organization",
        description.registrant,
        sequence="first",
        contributor_role="author",
    )
    _child(
        _child(dataset, "titles"),
        "title",
        f"{description.title} (version {description.version})",
    )
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


def write_deposit(
    path: Path | None = None,
    *,
    meta: SelfDescription | None = None,
    **kwargs: str,
) -> Path:
    """Write the CrossRef deposit XML to ``dist/``.

    Args:
        path: Output path (defaults to ``dist/crossref-deposit.xml``).
        meta: Preloaded self-description metadata.
        **kwargs: Forwarded to :func:`build_deposit_xml`.

    Returns:
        The path written.
    """
    out = path or (DIST_DIR / "crossref-deposit.xml")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(build_deposit_xml(meta=meta, **kwargs) + "\n", encoding="utf-8")
    return out

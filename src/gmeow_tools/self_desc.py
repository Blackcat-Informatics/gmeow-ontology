"""Load GMEOW self-description metadata from metadata/gmeow-self.ttl.

This module replaces hard-coded citation/DOI metadata in config.py with a
loader that parses the canonical RDF self-description.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

from rdflib import Graph, Literal, Namespace, URIRef
from rdflib.namespace import DCTERMS, RDFS

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
FOAF = Namespace("http://xmlns.com/foaf/0.1/")

SELF_DESC_FILE = Path(__file__).resolve().parents[2] / "metadata" / "gmeow-self.ttl"

#: Minimal DOI regex — 10.{registrant}/{suffix}
_DOI_RE = re.compile(r"10\.[^/\s]+/\S+")
#: Minimal email regex for sanity-checking.
_EMAIL_RE = re.compile(r"^[^@\s]+@[^@\s]+\.[^@\s]+$")


@dataclass(frozen=True, slots=True)
class SelfDescription:
    """GMEOW self-description metadata extracted from gmeow-self.ttl."""

    title: str
    version: str
    release_date: str
    doi: str
    depositor_name: str
    depositor_email: str
    registrant: str
    license_uri: str
    homepage: str


def load_self_description(path: Path | None = None) -> SelfDescription:
    """Parse ``metadata/gmeow-self.ttl`` and return structured metadata.

    Args:
        path: Override path to the self-description file.

    Returns:
        A :class:`SelfDescription` dataclass with all metadata fields.

    Raises:
        ValueError: If required metadata is missing or malformed.
    """
    g = Graph()
    g.parse(path or SELF_DESC_FILE, format="turtle")

    work = URIRef("https://blackcatinformatics.ca/gmeow")

    # In FRBR terms the Work is the abstract entity; the Manifestation is a
    # specific version/edition.  The self-description Turtle asserts a
    # separate Manifestation URI (not the Work URI) that carries the version
    # fingerprint and publication date.  We discover it dynamically by looking
    # for any URI subject with gmeow:versionFingerprint.
    manifestation = None
    for subj in g.subjects(GMEOW.versionFingerprint, None):
        if isinstance(subj, URIRef):
            manifestation = subj
            break
    if manifestation is None:
        raise ValueError(
            "No manifestation with gmeow:versionFingerprint found in self-description"
        )

    def _lit(subject: URIRef, predicate: URIRef) -> str:
        for obj in g.objects(subject, predicate):
            if isinstance(obj, Literal):
                return str(obj)
        raise ValueError(f"No literal found for {subject} {predicate}")

    def _obj(subject: URIRef, predicate: URIRef) -> URIRef | None:
        for obj in g.objects(subject, predicate):
            if isinstance(obj, URIRef):
                return obj
        return None

    title = _lit(work, RDFS.label)
    version = _lit(manifestation, GMEOW.versionFingerprint)
    release_date = _lit(manifestation, GMEOW.datePublished)
    doi = _lit(manifestation, DCTERMS.identifier)
    license_uri = str(_obj(work, DCTERMS.license) or "")
    homepage = str(_obj(work, FOAF.homepage) or "")

    # Validate extracted fields.
    if not _DOI_RE.match(doi):
        raise ValueError(f"Invalid DOI format in self-description: {doi!r}")
    try:
        datetime.strptime(release_date, "%Y-%m-%d")
    except ValueError as exc:
        raise ValueError(
            f"Invalid release_date format in self-description "
            f"(expected YYYY-MM-DD): {release_date!r}"
        ) from exc

    publisher = _obj(manifestation, DCTERMS.publisher)
    if publisher is None:
        raise ValueError(
            f"No dcterms:publisher found for manifestation {manifestation}; "
            "publisher metadata is required for CrossRef deposits and other outputs."
        )

    depositor_name = ""
    depositor_email = ""
    for obj in g.objects(publisher, FOAF.name):
        if isinstance(obj, Literal):
            depositor_name = str(obj)
            break
    for obj in g.objects(publisher, FOAF.mbox):
        if isinstance(obj, URIRef):
            depositor_email = str(obj).removeprefix("mailto:")
            break
    registrant = depositor_name

    if not depositor_name or not depositor_email:
        raise ValueError(
            f"Publisher {publisher} must have foaf:name and foaf:mbox; "
            "depositor metadata is required for CrossRef deposits."
        )

    if not _EMAIL_RE.match(depositor_email):
        raise ValueError(
            f"Invalid depositor_email format in self-description: {depositor_email!r}"
        )

    return SelfDescription(
        title=title,
        version=version,
        release_date=release_date,
        doi=doi,
        depositor_name=depositor_name,
        depositor_email=depositor_email,
        registrant=registrant,
        license_uri=license_uri,
        homepage=homepage,
    )


def full_doi() -> str:
    """Return the full GMEOW DOI (``{prefix}/{suffix}``)."""
    return load_self_description().doi

"""Load GMEOW self-description metadata from metadata/gmeow-self.ttl.

This module replaces hard-coded citation/DOI metadata in config.py with a
loader that parses the canonical RDF self-description.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from rdflib import Graph, Literal, Namespace, URIRef
from rdflib.namespace import DCTERMS, RDFS

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
FOAF = Namespace("http://xmlns.com/foaf/0.1/")

SELF_DESC_FILE = Path(__file__).resolve().parents[2] / "metadata" / "gmeow-self.ttl"


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
    """
    g = Graph()
    g.parse(path or SELF_DESC_FILE, format="turtle")

    work = URIRef("https://blackcatinformatics.ca/gmeow")
    manifestation = URIRef("https://blackcatinformatics.ca/gmeow/0.1.0")

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

    publisher = _obj(manifestation, DCTERMS.publisher)
    if publisher:
        depositor_name = ""
        depositor_email = ""
        for obj in g.objects(publisher, FOAF.name):
            if isinstance(obj, Literal):
                depositor_name = str(obj)
                break
        for obj in g.objects(publisher, FOAF.mbox):
            if isinstance(obj, URIRef):
                depositor_email = str(obj).replace("mailto:", "")
                break
        registrant = depositor_name
    else:
        depositor_name = ""
        depositor_email = ""
        registrant = ""

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

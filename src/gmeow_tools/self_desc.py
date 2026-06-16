"""Load GMEOW self-description metadata from metadata/gmeow-self.ttl.

This module replaces hard-coded citation/DOI metadata in config.py with a
loader that parses the canonical RDF self-description.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

from rdflib import RDF, Graph, Literal, Namespace, URIRef
from rdflib.namespace import DCTERMS, RDFS, SKOS

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
FOAF = Namespace("http://xmlns.com/foaf/0.1/")

#: ORCID is the recognised person-authority scheme for Crossref contributors.
_ORCID_PREFIX = "https://orcid.org/"

SELF_DESC_FILE = Path(__file__).resolve().parents[2] / "metadata" / "gmeow-self.ttl"

#: Minimal DOI regex — 10.{registrant}/{suffix}
_DOI_RE = re.compile(r"10\.[^/\s]+/\S+")
#: Minimal email regex for sanity-checking.
_EMAIL_RE = re.compile(r"^[^@\s]+@[^@\s]+\.[^@\s]+$")


@dataclass(frozen=True, slots=True)
class Contributor:
    """A credited author of the work, projected to a Crossref contributor.

    ``kind`` is ``"organization"`` or ``"person"``; ``orcid`` is the ORCID URL for
    persons (``None`` otherwise). ``sequence`` is the Crossref order (``"first"``
    for the lead, ``"additional"`` after).
    """

    kind: str
    name: str
    orcid: str | None
    sequence: str
    role: str = "author"

    @property
    def given_name(self) -> str:
        """The given name(s) of a person — everything but the final token."""
        parts = self.name.rsplit(" ", 1)
        return parts[0] if len(parts) == 2 else ""

    @property
    def surname(self) -> str:
        """The surname of a person — the final whitespace-delimited token."""
        return self.name.rsplit(" ", 1)[-1]


@dataclass(frozen=True, slots=True)
class SelfDescription:
    """GMEOW self-description metadata extracted from gmeow-self.ttl.

    Two DOIs are modelled, mirroring the FRBR spine: the **concept DOI** denotes
    the Work (always-latest citation anchor), the optional **version DOI** denotes
    the Manifestation (this immutable release, resolving to its ``owl:versionIRI``).
    Concept-only — ``version_doi is None`` — is a first-class supported state, not
    a placeholder (per the single-anchor strategy, see ``docs/dois.md``).
    """

    title: str
    version: str
    release_date: str
    concept_doi: str
    version_doi: str | None
    version_iri: str
    depositor_name: str
    depositor_email: str
    registrant: str
    license_uri: str
    homepage: str
    description: str
    repo_url: str
    contributors: tuple[Contributor, ...]

    @property
    def doi(self) -> str:
        """The preferred citable DOI: the version DOI if minted, else concept."""
        return self.version_doi or self.concept_doi


def _agent_name(g: Graph, agent: URIRef) -> str:
    """The display name of a contributor agent (foaf:name / gmeow:name / label)."""
    for prop in (FOAF.name, GMEOW.name, RDFS.label):
        for obj in g.objects(agent, prop):
            if isinstance(obj, Literal):
                return str(obj)
    return ""


def _load_contributors(g: Graph, work: URIRef) -> tuple[Contributor, ...]:
    """Author contributions to the work, ordered organizations-first.

    Reads the credit graph (``gmeow:Contribution`` with ``contributionRole
    roleAuthor`` targeting the work) rather than a positional convention. The
    first contributor overall carries Crossref ``sequence="first"``; the rest
    ``"additional"``. Persons carry their ORCID (``gmeow:authorityLink`` into
    ``orcid.org``).
    """
    orgs: list[Contributor] = []
    persons: list[Contributor] = []
    for contrib in g.subjects(RDF.type, GMEOW.Contribution):
        if (contrib, GMEOW.contributionTarget, work) not in g:
            continue
        if (contrib, GMEOW.contributionRole, GMEOW.roleAuthor) not in g:
            continue
        agent = next(
            (o for o in g.objects(contrib, GMEOW.contributor) if isinstance(o, URIRef)),
            None,
        )
        if agent is None:
            continue
        name = _agent_name(g, agent)
        if not name:
            continue
        types = set(g.objects(agent, RDF.type))
        if FOAF.Organization in types or GMEOW.Organization in types:
            orgs.append(Contributor("organization", name, None, "first"))
        elif GMEOW.Person in types:
            orcid = next(
                (
                    str(o)
                    for o in g.objects(agent, GMEOW.authorityLink)
                    if isinstance(o, URIRef) and str(o).startswith(_ORCID_PREFIX)
                ),
                None,
            )
            persons.append(Contributor("person", name, orcid, "additional"))

    ordered = sorted(orgs, key=lambda c: c.name) + sorted(persons, key=lambda c: c.name)
    return tuple(
        Contributor(c.kind, c.name, c.orcid, "first" if i == 0 else "additional")
        for i, c in enumerate(ordered)
    )


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
    return load_self_description_from_graph(g)


def load_self_description_from_graph(g: Graph) -> SelfDescription:
    """Extract structured self-description metadata from an RDF graph.

    Args:
        g: Graph containing the GMEOW self-description triples.

    Returns:
        A :class:`SelfDescription` dataclass with all metadata fields.

    Raises:
        ValueError: If required metadata is missing or malformed.
    """
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

    def _opt_lit(subject: URIRef, predicate: URIRef) -> str | None:
        for obj in g.objects(subject, predicate):
            if isinstance(obj, Literal):
                return str(obj)
        return None

    title = _lit(work, RDFS.label)
    version = _lit(manifestation, GMEOW.versionFingerprint)
    release_date = _lit(manifestation, GMEOW.datePublished)
    # Concept DOI lives on the Work (always-latest anchor); the optional version
    # DOI lives on the Manifestation (this release). The Work←Manifestation edge
    # is the WEMI realizes/embodies chain, so the relationship is read by role,
    # never inferred positionally.
    concept_doi = _lit(work, DCTERMS.identifier)
    version_doi = _opt_lit(manifestation, DCTERMS.identifier)
    version_iri = str(manifestation)
    license_uri = str(_obj(work, DCTERMS.license) or "")
    homepage = str(_obj(work, FOAF.homepage) or "")

    # Validate extracted fields. The concept DOI is required; the version DOI is
    # optional but, when present, must be well-formed (and must not reuse the
    # concept DOI — Crossref forbids two DOIs on the same resource).
    if not _DOI_RE.match(concept_doi):
        raise ValueError(
            f"Invalid concept DOI format in self-description: {concept_doi!r}"
        )
    if version_doi is not None:
        if not _DOI_RE.match(version_doi):
            raise ValueError(
                f"Invalid version DOI format in self-description: {version_doi!r}"
            )
        if version_doi == concept_doi:
            raise ValueError(
                "Version DOI must differ from the concept DOI "
                f"(both are {concept_doi!r}); they resolve to distinct resources."
            )
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

    description = _opt_lit(work, SKOS.definition) or ""

    repo_url = ""
    for obj in g.objects(None, GMEOW.webUrl):
        if isinstance(obj, URIRef | Literal):
            repo_url = str(obj)
            break

    contributors = _load_contributors(g, work)
    if not contributors:
        raise ValueError(
            "No author Contribution found targeting the work; at least one "
            "contributor is required for CrossRef deposits."
        )

    return SelfDescription(
        title=title,
        version=version,
        release_date=release_date,
        concept_doi=concept_doi,
        version_doi=version_doi,
        version_iri=version_iri,
        depositor_name=depositor_name,
        depositor_email=depositor_email,
        registrant=registrant,
        license_uri=license_uri,
        homepage=homepage,
        description=description,
        repo_url=repo_url,
        contributors=contributors,
    )


def full_doi() -> str:
    """Return the preferred citable GMEOW DOI (version DOI if minted, else concept)."""
    return load_self_description().doi

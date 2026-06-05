"""Validation-time snapshots of target-vocabulary *axioms* (domain/range/inverse).

The SSSOM alignment-direction linter (:mod:`gmeow_tools.alignment_lint`) needs the
*target* terms' own structural axioms — ``rdfs:domain``/``rdfs:range`` (or
schema.org's ``schema:domainIncludes``/``rangeIncludes``), ``owl:inverseOf`` (or
``schema:inverseOf``), and property-character types — to tell whether a GMEOW
mapping points at the right term *or its inverse*. Those axioms live in external
vocabularies (schema.org, ORG, FOAF, vCard, PROV-O, OWL-Time, GeoSPARQL).

GMEOW is published CC BY 4.0, so we **reference, not copy**: target axioms are a
*validation-time* concern and must never enter the published artifact. Two rails
enforce that:

* Snapshots are vendored under :data:`~gmeow_tools.config.TARGET_SNAPSHOT_DIR`
  (``imports/targets/``) — a SUBDIR of ``imports/``, invisible to
  ``iter_import_files()`` (which globs ``imports/*.ttl`` non-recursively).
* :func:`refresh_snapshot` refuses to vendor a ``REFERENCE_ONLY`` target (the same
  license gate as :func:`gmeow_tools.extract.guard_importable`). schema.org
  (CC-BY-SA) is therefore never written to disk; its axioms are fetched **live**
  only, under the ``network`` test mark / ``--network`` CLI flag.

The fetch keeps only the minimal structural axiom set (no labels, definitions, or
prose) so even a transient in-memory copy of a reference-only vocabulary is facts,
not republication.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import httpx
from rdflib import RDF, RDFS, Graph, URIRef
from rdflib.namespace import OWL

from gmeow_tools.config import (
    ALIGNMENT_TARGETS,
    PREFIXES,
    TARGET_SNAPSHOT_DIR,
    LinkPolicy,
)
from gmeow_tools.graph import bind_prefixes

_SCHEMA = PREFIXES["schema"]

#: schema.org uses its own soft-typing predicates instead of rdfs:/owl:.
SCHEMA_DOMAIN_INCLUDES = URIRef(_SCHEMA + "domainIncludes")
SCHEMA_RANGE_INCLUDES = URIRef(_SCHEMA + "rangeIncludes")
SCHEMA_INVERSE_OF = URIRef(_SCHEMA + "inverseOf")

#: Predicates worth snapshotting — the structural axioms the linter reads.
_AXIOM_PREDICATES: frozenset[URIRef] = frozenset(
    {
        RDFS.domain,
        RDFS.range,
        OWL.inverseOf,
        SCHEMA_DOMAIN_INCLUDES,
        SCHEMA_RANGE_INCLUDES,
        SCHEMA_INVERSE_OF,
    }
)

#: ``rdf:type`` objects that mark a term as a property and/or carry its character.
_PROPERTY_TYPES: frozenset[URIRef] = frozenset(
    {
        RDF.Property,
        OWL.ObjectProperty,
        OWL.DatatypeProperty,
        OWL.FunctionalProperty,
        OWL.InverseFunctionalProperty,
        OWL.TransitiveProperty,
        OWL.SymmetricProperty,
        OWL.AsymmetricProperty,
    }
)


@dataclass(frozen=True, slots=True)
class TargetSource:
    """Where to fetch a target vocabulary's canonical machine-readable document."""

    prefix: str
    fetch_url: str
    fetch_format: str  # an rdflib parse format ("turtle" | "xml" | "json-ld")


#: Canonical source documents per target prefix. The prefix keys index both
#: :data:`PREFIXES` (for the namespace) and :data:`ALIGNMENT_TARGETS` (for the
#: license/policy). schema.org is listed so the live (network) path can fetch it,
#: but it is REFERENCE_ONLY so :func:`refresh_snapshot` refuses to vendor it.
TARGET_SOURCES: dict[str, TargetSource] = {
    "org": TargetSource("org", "https://www.w3.org/ns/org.ttl", "turtle"),
    "foaf": TargetSource("foaf", "http://xmlns.com/foaf/spec/index.rdf", "xml"),
    "vcard": TargetSource("vcard", "https://www.w3.org/2006/vcard/ns.ttl", "turtle"),
    "prov": TargetSource("prov", "https://www.w3.org/ns/prov-o.ttl", "turtle"),
    "time": TargetSource("time", "https://www.w3.org/2006/time.ttl", "turtle"),
    "geo": TargetSource(
        "geo",
        "https://opengeospatial.github.io/ogc-geosparql/geosparql11/geo.ttl",
        "turtle",
    ),
    "schema": TargetSource(
        "schema",
        "https://schema.org/version/latest/schemaorg-current-https.ttl",
        "turtle",
    ),
}

_USER_AGENT = "gmeow-tools/0.1 (ontology alignment-direction validator)"


def _minimal_axiom_graph(source: Graph, namespace: str) -> Graph:
    """Filter a parsed vocabulary down to the linter-relevant axioms.

    Keeps only triples whose subject is in ``namespace`` and whose predicate is a
    structural axiom (domain/range/inverse) or an ``rdf:type`` naming a property
    kind. Drops labels, definitions, and all prose — the snapshot is facts only.

    Args:
        source: The fully-parsed target vocabulary.
        namespace: The target's IRI namespace prefix (only subjects under it kept).

    Returns:
        A new, prefix-bound graph containing the minimal axiom subset.
    """
    out = Graph()
    bind_prefixes(out)
    for subject, predicate, obj in source:
        if not isinstance(subject, URIRef) or not str(subject).startswith(namespace):
            continue
        if predicate in _AXIOM_PREDICATES or (
            predicate == RDF.type and obj in _PROPERTY_TYPES
        ):
            out.add((subject, predicate, obj))
    return out


def fetch_target_axioms(prefix: str, *, timeout: float = 60.0) -> Graph:
    """Fetch a target vocabulary and return its minimal axiom graph.

    Args:
        prefix: A key into :data:`TARGET_SOURCES`.
        timeout: HTTP timeout in seconds.

    Returns:
        The filtered, in-memory axiom graph (never written to disk here).

    Raises:
        KeyError: If ``prefix`` is not a known target source.
        httpx.HTTPError: On a network/HTTP failure (callers gate/skip on this).
    """
    source = TARGET_SOURCES[prefix]
    namespace = PREFIXES[prefix]
    response = httpx.get(
        source.fetch_url,
        timeout=timeout,
        follow_redirects=True,
        headers={"User-Agent": _USER_AGENT},
    )
    response.raise_for_status()
    parsed = Graph().parse(data=response.text, format=source.fetch_format)
    return _minimal_axiom_graph(parsed, namespace)


def refresh_snapshot(
    prefix: str, *, snapshot_dir: Path = TARGET_SNAPSHOT_DIR, timeout: float = 60.0
) -> Path:
    """Fetch, filter, and vendor a target's axiom snapshot to ``imports/targets/``.

    Refuses ``REFERENCE_ONLY`` targets: their axioms must never be committed into
    the CC BY 4.0 artifact (they are fetched live at lint time instead).

    Args:
        prefix: A key into :data:`TARGET_SOURCES` / :data:`ALIGNMENT_TARGETS`.
        snapshot_dir: Destination directory (default :data:`TARGET_SNAPSHOT_DIR`).
        timeout: HTTP timeout in seconds.

    Returns:
        The path to the written snapshot.

    Raises:
        LicensePolicyError: If the target is unknown or ``REFERENCE_ONLY``.
        httpx.HTTPError: On a network/HTTP failure.
    """
    # Imported here to avoid a module-level cycle (extract imports config too).
    from gmeow_tools.extract import LicensePolicyError

    target = ALIGNMENT_TARGETS.get(prefix)
    if target is None:
        raise LicensePolicyError(
            f"unknown alignment target {prefix!r}; refusing to vendor a snapshot"
        )
    if target.policy is not LinkPolicy.IMPORT_OK:
        raise LicensePolicyError(
            f"refusing to vendor {target.name} ({target.license}): "
            f"{target.policy.value}. Its axioms are fetched live at lint time "
            f"(--network) — never committed into CC BY 4.0 GMEOW."
        )
    graph = fetch_target_axioms(prefix, timeout=timeout)
    snapshot_dir.mkdir(parents=True, exist_ok=True)
    out_path = snapshot_dir / f"{prefix}.ttl"
    graph.serialize(destination=out_path, format="turtle")
    return out_path


def load_target_snapshot(
    prefix: str, *, snapshot_dir: Path = TARGET_SNAPSHOT_DIR
) -> Graph | None:
    """Load a vendored target axiom snapshot, or ``None`` if absent.

    Args:
        prefix: The target prefix (snapshot file is ``<prefix>.ttl``).
        snapshot_dir: Directory holding the vendored snapshots.

    Returns:
        The parsed snapshot graph, or ``None`` when no snapshot file exists (the
        caller then degrades to an INFO/skip finding rather than failing).
    """
    path = snapshot_dir / f"{prefix}.ttl"
    if not path.exists():
        return None
    return Graph().parse(path, format="turtle")

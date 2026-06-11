"""SSSOM mapping management → alignment axioms + VoID linksets.

Cross-ontology alignments live as SSSOM TSV tables (one row per mapping, with
predicate, justification and confidence). This module converts them into:

* an **alignment graph** of OWL/SKOS axioms (``owl:equivalentClass``,
  ``skos:exactMatch`` …) — these are *links* (they reference external IRIs and
  copy nothing), so they are emitted for every target regardless of license;
* **VoID linksets** grouping the links by target dataset and predicate, which
  the LOD-Cloud submission requires.

The license-aware *copy* refusal lives in ``extract.py`` (which would copy
axioms in); linking is always permitted.
"""

from __future__ import annotations

import csv
import hashlib
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

from rdflib import RDF, RDFS, Graph, Literal, URIRef
from rdflib.namespace import OWL, VOID

from gmeow_tools.config import (
    MAPPINGS_DIR,
    PREFIXES,
    VOID_DATASET_IRI,
)
from gmeow_tools.graph import bind_prefixes, iter_module_files
from gmeow_tools.wikidata import local_name, local_name_wdt

#: Required SSSOM columns this tool consumes.
_REQUIRED_COLUMNS = ("subject_id", "predicate_id", "object_id")


@dataclass(frozen=True, slots=True)
class Mapping:
    """One SSSOM mapping row (links a GMEOW term to an external term)."""

    subject_id: str
    predicate_id: str
    object_id: str
    justification: str
    confidence: str
    object_label: str
    source: Path


class MappingError(ValueError):
    """Raised on a malformed mapping (bad CURIE, missing column, …)."""


def expand_curie(curie: str) -> URIRef:
    """Expand a ``prefix:local`` CURIE using the canonical prefix registry.

    Args:
        curie: A compact IRI such as ``"foaf:Person"``.

    Returns:
        The full IRI as an rdflib :class:`~rdflib.URIRef`.

    Raises:
        MappingError: If the CURIE is malformed or its prefix is unknown.
    """
    if ":" not in curie:
        raise MappingError(f"not a CURIE: {curie!r}")
    prefix, local = curie.split(":", 1)
    namespace = PREFIXES.get(prefix)
    if namespace is None:
        raise MappingError(f"unknown prefix {prefix!r} in {curie!r}")
    return URIRef(namespace + local)


def _iter_data_rows(path: Path) -> list[dict[str, str]]:
    """Yield SSSOM data rows from a TSV, skipping the YAML metadata header."""
    lines = [
        line
        for line in path.read_text(encoding="utf-8").splitlines()
        if not line.startswith("#")
    ]
    reader = csv.DictReader(lines, delimiter="\t")
    if reader.fieldnames is None:
        raise MappingError(f"no header row in {path}")
    missing = [c for c in _REQUIRED_COLUMNS if c not in reader.fieldnames]
    if missing:
        raise MappingError(f"{path} missing SSSOM columns: {missing}")
    return list(reader)


def load_mappings(mappings_dir: Path = MAPPINGS_DIR) -> list[Mapping]:
    """Load all SSSOM mappings from ``*.sssom.tsv`` files in a directory."""
    mappings: list[Mapping] = []
    for path in sorted(mappings_dir.glob("*.sssom.tsv")):
        for row in _iter_data_rows(path):
            mappings.append(
                Mapping(
                    subject_id=row["subject_id"],
                    predicate_id=row["predicate_id"],
                    object_id=row["object_id"],
                    justification=row.get("mapping_justification", ""),
                    confidence=row.get("confidence", ""),
                    object_label=row.get("object_label", ""),
                    source=path,
                )
            )
    return mappings


def build_alignment_graph(mappings: list[Mapping]) -> Graph:
    """Build a graph of alignment axioms from mappings (links only)."""
    graph = Graph()
    bind_prefixes(graph)
    for mapping in mappings:
        graph.add(
            (
                expand_curie(mapping.subject_id),
                expand_curie(mapping.predicate_id),
                expand_curie(mapping.object_id),
            )
        )
    return graph


def object_namespace(object_iri: URIRef) -> str:
    """Return the namespace of an IRI (split on the last ``#`` or ``/``)."""
    iri = str(object_iri)
    for sep in ("#", "/"):
        if sep in iri:
            return iri.rsplit(sep, 1)[0] + sep
    return iri


def build_linksets(mappings: list[Mapping]) -> Graph:
    """Build VoID linksets grouping mappings by target namespace + predicate.

    Args:
        mappings: The loaded mappings.

    Returns:
        A graph of ``void:Linkset`` descriptions (one per target/predicate
        pair), each carrying its link predicate, target, and triple count.
    """
    graph = Graph()
    bind_prefixes(graph)
    dataset = URIRef(VOID_DATASET_IRI)
    buckets: dict[tuple[str, str], int] = defaultdict(int)
    for mapping in mappings:
        target_ns = object_namespace(expand_curie(mapping.object_id))
        buckets[(target_ns, mapping.predicate_id)] += 1

    for (target_ns, predicate_id), count in sorted(buckets.items()):
        predicate = expand_curie(predicate_id)
        slug = predicate_id.replace(":", "_")
        target_slug = target_ns.rstrip("#/").rsplit("/", 1)[-1] or "target"
        # Collision-safe slug: include a short hash of the full namespace so
        # distinct namespaces (e.g. .../ns#, .../ns#) do not collide.
        ns_hash = hashlib.sha256(target_ns.encode()).hexdigest()[:6]
        linkset = URIRef(f"{VOID_DATASET_IRI}-linkset-{target_slug}-{ns_hash}-{slug}")
        graph.add((linkset, RDF.type, VOID.Linkset))
        graph.add((linkset, VOID.subjectsTarget, dataset))
        graph.add((linkset, VOID.objectsTarget, URIRef(target_ns)))
        graph.add((linkset, VOID.linkPredicate, predicate))
        graph.add((linkset, VOID.triples, Literal(count)))
        graph.add(
            (
                linkset,
                RDFS.label,
                Literal(f"GMEOW {predicate_id} links to {target_ns} ({count})"),
            )
        )
    return graph


def collect_wikidata_ids(mappings: list[Mapping]) -> list[str]:
    """Return the Wikidata ids targeted by the mappings (for validation)."""
    ids: list[str] = []
    for mapping in mappings:
        name = local_name(str(expand_curie(mapping.object_id)))
        if name is not None:
            ids.append(name)
        name_wdt = local_name_wdt(str(expand_curie(mapping.object_id)))
        if name_wdt is not None:
            ids.append(name_wdt)
    return ids


def group_mappings_by_source(mappings: list[Mapping]) -> dict[str, list[Mapping]]:
    """Group mappings by the source SSSOM file name (used as a domain key)."""
    groups: dict[str, list[Mapping]] = defaultdict(list)
    for mapping in mappings:
        key = mapping.source.stem
        groups[key].append(mapping)
    return dict(groups)


def collect_ontology_terms(
    modules_dir: Path | None = None,
) -> dict[str, set[str]]:
    """Scan ontology modules for declared classes, properties, and individuals.

    Defaults to the canonical module enumeration (flat modules + slice
    modules, #287); pass a directory to scan an explicit tree instead.

    Returns a dict of {term_type: {curie, ...}} where term_type is one of
    ``classes``, ``properties``, ``individuals``.
    """
    terms: dict[str, set[str]] = {
        "classes": set(),
        "properties": set(),
        "individuals": set(),
    }
    paths = sorted(modules_dir.glob("*.ttl")) if modules_dir else iter_module_files()
    for path in paths:
        graph = Graph()
        graph.parse(path, format="turtle")
        for s in graph.subjects(RDF.type, OWL.Class):
            if isinstance(s, URIRef):
                terms["classes"].add(str(s))
        for s in graph.subjects(RDF.type, OWL.ObjectProperty):
            if isinstance(s, URIRef):
                terms["properties"].add(str(s))
        for s in graph.subjects(RDF.type, OWL.DatatypeProperty):
            if isinstance(s, URIRef):
                terms["properties"].add(str(s))
        for s in graph.subjects(RDF.type, OWL.NamedIndividual):
            if isinstance(s, URIRef):
                terms["individuals"].add(str(s))
    return terms

"""Coverage harness: does GMEOW cover the entity slice?

Loads the vendored fixture graphs (public ``bii``/``paudley`` site data), collects
every class (``rdf:type`` object) and predicate IRI they use, and classifies each
against the GMEOW alignment set:

* **covered** — the IRI is a GMEOW term, an IRI GMEOW aligns to (from the SSSOM
  mappings), or a namespace GMEOW explicitly reuses (SKOS);
* **gap** — the IRI is used in the slice but GMEOW does not yet align to it.

RDF/RDFS/OWL/XSD plumbing is ignored. The report's gap list is the project's
progress ledger across the slice series — it is informational, not a failure
(each slice is intentionally partial).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from rdflib import RDF, Graph, URIRef

from gmeow_tools.config import FIXTURES_DIR, NAMESPACE, PREFIXES
from gmeow_tools.mappings import build_alignment_graph, load_mappings

#: Namespaces that are pure RDF plumbing — not counted for or against coverage.
_IGNORED = (
    PREFIXES["rdf"],
    PREFIXES["rdfs"],
    PREFIXES["owl"],
    PREFIXES["xsd"],
)
#: Namespaces GMEOW explicitly reuses wholesale (recommended value vocabularies).
_RECOMMENDED = (PREFIXES["skos"],)


@dataclass(slots=True)
class CoverageReport:
    """Coverage outcome over the fixture graphs."""

    covered_classes: set[str] = field(default_factory=set)
    gap_classes: set[str] = field(default_factory=set)
    covered_predicates: set[str] = field(default_factory=set)
    gap_predicates: set[str] = field(default_factory=set)

    @property
    def class_coverage(self) -> float:
        """Fraction of used classes that are covered (0..1)."""
        total = len(self.covered_classes) + len(self.gap_classes)
        return len(self.covered_classes) / total if total else 1.0

    @property
    def predicate_coverage(self) -> float:
        """Fraction of used predicates that are covered (0..1)."""
        total = len(self.covered_predicates) + len(self.gap_predicates)
        return len(self.covered_predicates) / total if total else 1.0


def covered_iris() -> set[str]:
    """Return the set of external IRIs GMEOW aligns to (from the mappings).

    Returns:
        Every non-GMEOW IRI mentioned as a subject or object in the alignment
        graph (i.e. every external term GMEOW links to).
    """
    graph = build_alignment_graph(load_mappings())
    iris: set[str] = set()
    for subject, _predicate, obj in graph:
        for node in (subject, obj):
            if isinstance(node, URIRef):
                iris.add(str(node))
    return iris


def _is_ignored(iri: str) -> bool:
    return any(iri.startswith(ns) for ns in _IGNORED)


def _is_covered(iri: str, aligned: set[str]) -> bool:
    if iri.startswith(NAMESPACE):
        return True
    if any(iri.startswith(ns) for ns in _RECOMMENDED):
        return True
    return iri in aligned


def load_fixtures(fixtures_dir: Path = FIXTURES_DIR) -> Graph:
    """Parse and merge all vendored coverage fixtures into one graph.

    Recurses into ``external/`` so the real-world site snapshots (the bii/paudley
    parity targets) are part of the measurement — coverage is *about* them. The
    snapshots are exempt from GMEOW's authoring policies, but they are still the
    target the harness scores against.
    """
    graph = Graph()
    for path in sorted(fixtures_dir.rglob("*.ttl")):
        graph.parse(path, format="turtle")
    return graph


def analyze(data: Graph, aligned: set[str] | None = None) -> CoverageReport:
    """Classify the classes and predicates used in a data graph.

    Args:
        data: The merged fixture graph.
        aligned: The set of aligned external IRIs (computed if not supplied).

    Returns:
        The coverage report.
    """
    if aligned is None:
        aligned = covered_iris()
    report = CoverageReport()

    for cls in set(data.objects(None, RDF.type)):
        if not isinstance(cls, URIRef):
            continue
        iri = str(cls)
        if not iri.startswith("http") or _is_ignored(iri):
            continue
        if _is_covered(iri, aligned):
            report.covered_classes.add(iri)
        else:
            report.gap_classes.add(iri)

    for predicate in set(data.predicates()):
        iri = str(predicate)
        if _is_ignored(iri):
            continue
        target = (
            report.covered_predicates
            if _is_covered(iri, aligned)
            else report.gap_predicates
        )
        target.add(iri)

    return report


def run_coverage(fixtures_dir: Path = FIXTURES_DIR) -> CoverageReport:
    """Run the coverage analysis over the vendored fixtures."""
    return analyze(load_fixtures(fixtures_dir))

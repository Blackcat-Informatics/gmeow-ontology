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

import gmeow_validate
from rdflib import URIRef

from gmeow_tools.config import FIXTURES_DIR, NAMESPACE
from gmeow_tools.mappings import build_alignment_graph, load_mappings


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


def fixture_paths(fixtures_dir: Path = FIXTURES_DIR) -> list[Path]:
    """Discover every vendored coverage fixture, sorted.

    Recurses into ``external/`` so the real-world site snapshots (the bii/paudley
    parity targets) are part of the measurement — coverage is *about* them. The
    snapshots are exempt from GMEOW's authoring policies, but they are still the
    target the harness scores against.
    """
    return sorted(fixtures_dir.rglob("*.ttl"))


def analyze(
    paths: list[Path],
    aligned: set[str] | None = None,
) -> CoverageReport:
    """Classify the classes and predicates used across the fixture graphs.

    A thin seam over the Rust ``gmeow_validate.coverage_analyze`` engine (#579):
    the graph-walk classification runs on oxigraph; only the SSSOM ``aligned``
    set and the ``CoverageReport`` assembly stay in Python.

    Args:
        paths: The discovered fixture file paths.
        aligned: The set of aligned external IRIs (computed if not supplied).

    Returns:
        The coverage report.
    """
    if aligned is None:
        aligned = covered_iris()
    result = gmeow_validate.coverage_analyze(
        [str(p) for p in paths],
        sorted(aligned),
        str(NAMESPACE),
    )
    return CoverageReport(
        covered_classes=set(result["covered_classes"]),
        gap_classes=set(result["gap_classes"]),
        covered_predicates=set(result["covered_predicates"]),
        gap_predicates=set(result["gap_predicates"]),
    )


def run_coverage(fixtures_dir: Path = FIXTURES_DIR) -> CoverageReport:
    """Run the coverage analysis over the vendored fixtures."""
    return analyze(fixture_paths(fixtures_dir))

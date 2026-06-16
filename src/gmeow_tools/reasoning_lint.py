"""UFO/OntoUML anti-pattern checks over the meta-grounded ontology.

Every GMEOW class records two orthogonal gUFO facets: its **nature** — what an
instance *is* — via ``rdfs:subClassOf`` into the gUFO individual taxonomy
(``gufo:FunctionalComplex``, ``gufo:Relator``, ``gufo:Event`` …); and its
**stereotype** — the type's identity/rigidity status — via ``rdf:type`` into the
gUFO ``gufo:EndurantType`` / ``gufo:EventType`` / ``gufo:SituationType`` taxonomy
(OWL 2 punning; see ``imports/gufo.ttl`` ~line 1427). This module checks the
structural discipline the stereotype facet licenses, exactly the role
:mod:`gmeow_tools.statement_lint` plays for the statement layer and
:mod:`gmeow_tools.projection_lint` for the projection stack.

The reasoning logic itself lives in the Rust ``gmeow_validate`` extension (#579):
the transitive ``rdfs:subClassOf`` closure, the ``owl:AllDisjointClasses`` /
``owl:members`` RDF-Collection walk, and the subPropertyOf/equivalentProperty
property-bridge DFS all run over an oxigraph store, with byte-exact parity to the
former pure-Python checks. Each function below is a thin shim that serializes the
passed graph to N-Triples and routes it through the matching Rust entry point.

The checks (each cites the OntoUML anti-pattern it guards; catalogue:
https://ontouml.readthedocs.io/en/latest/anti-patterns/):

* :func:`exactly_one_stereotype` — every GMEOW class carries exactly one gUFO
  meta-class (the precondition for every check below).
* :func:`identity_overlap` — **MixIden**: a sortal inherits identity from exactly
  one ``gufo:Kind``; no ``Kind`` specializes another ``Kind``.
* :func:`anti_rigidity_discipline` — **MixRig / FreeRole**: an anti-rigid sortal
  (``Role`` / ``Phase``) specializes a rigid sortal, and no rigid type specializes
  an anti-rigid one.
* :func:`relator_mediation` — **RelComp**: a concrete ``gufo:Relator`` mediates at
  least two relata.
* :func:`coequal_facet_orthogonality` — **Principle 9 (#281)**: co-equal facet
  axes stay orthogonal.
* :func:`frame_declaration_completeness` — **Principle 11 (#283)**: frame-pointing
  property carrier classes declare ``gmeow:requiresFrame``.
"""

from __future__ import annotations

import tempfile
from collections.abc import Callable
from contextlib import suppress
from pathlib import Path

import gmeow_validate
from rdflib import Graph

from gmeow_tools.config import NAMESPACE


def _graph_source_paths(graph: Graph) -> tuple[list[str], Callable[[], None]]:
    """Serialize *graph* to a temporary N-Triples file for the Rust checks.

    The Rust reasoning engine builds its own oxigraph store from file paths, so
    an in-memory rdflib graph (the merged ontology, or a test's hand-built
    graph) is written to one N-Triples temp file. Returns the source-path list
    plus a cleanup callback the caller invokes when done.

    N-Triples is chosen so any graph round-trips losslessly through oxigraph's
    Turtle-family parser without prefix bookkeeping.
    """
    with tempfile.NamedTemporaryFile(
        "wb", suffix=".nt", prefix="gmeow-reasoning-", delete=False
    ) as handle:
        graph.serialize(destination=handle, format="nt", encoding="utf-8")
        path = Path(handle.name)

    def _cleanup() -> None:
        with suppress(OSError):
            path.unlink(missing_ok=True)

    return [str(path)], _cleanup


def _run(
    check: Callable[[list[str], str], dict[str, list[str]]], graph: Graph
) -> list[str]:
    """Serialize *graph* and route it through one Rust reasoning *check*."""
    source_paths, cleanup = _graph_source_paths(graph)
    try:
        report = check(source_paths, str(NAMESPACE))
    finally:
        cleanup()
    return list(report["errors"])


def exactly_one_stereotype(graph: Graph) -> list[str]:
    """Every GMEOW class must be punned with exactly one gUFO meta-class."""
    return _run(gmeow_validate.reasoning_exactly_one_stereotype, graph)


def identity_overlap(graph: Graph) -> list[str]:
    """MixIden: a sortal inherits identity from exactly one Kind; no Kind ⊑ Kind."""
    return _run(gmeow_validate.reasoning_identity_overlap, graph)


def anti_rigidity_discipline(graph: Graph) -> list[str]:
    """MixRig / FreeRole: anti-rigid sortals need a rigid super; rigid avoid them."""
    return _run(gmeow_validate.reasoning_anti_rigidity_discipline, graph)


def relator_mediation(graph: Graph) -> list[str]:
    """RelComp: every concrete gufo:Relator mediates at least two relata."""
    return _run(gmeow_validate.reasoning_relator_mediation, graph)


def coequal_facet_orthogonality(graph: Graph) -> list[str]:
    """Principle 9 by annotation (#281): annotated axes stay orthogonal."""
    return _run(gmeow_validate.reasoning_coequal_facet_orthogonality, graph)


def frame_declaration_completeness(graph: Graph) -> list[str]:
    """Principle 11 by annotation (#283): the "did you forget the frame?" guard."""
    return _run(gmeow_validate.reasoning_frame_declaration_completeness, graph)


def reasoning_invariants(graph: Graph) -> list[str]:
    """Run every UFO anti-pattern check; an empty list means the graph is clean."""
    return _run(gmeow_validate.reasoning_invariants, graph)

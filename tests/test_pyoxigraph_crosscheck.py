"""Independent in-process cross-check: pyoxigraph parses and normalizes RDF 1.2.

CONSTITUTION Principle 7 (verified by construction) + Principle 4 (Jena remains
canonical). This module proves that pyoxigraph — a pure in-process SPARQL
engine — can parse the committed RDF 1.2 triple-term Turtle and reproduce the
same OWL axiom-annotation normal form that the Jena-backed pipeline produces.

The Jena-backed ``regenerate`` (statements), ``check-generated``, and docker-marked
tests in ``test_statements.py`` remain authoritative; this is an additive,
read-only verification path.
"""

from __future__ import annotations

from pathlib import Path

import pyoxigraph
import pytest
from rdflib import Graph
from rdflib.compare import graph_diff, isomorphic

from gmeow_tools.config import STATEMENT_OWL_FILE, STATEMENT_RDF12_FILE
from gmeow_tools.rdf12 import normalize_rdf12_to_owl as normalize_rdf12_to_owl_jena
from gmeow_tools.rdf12_pyoxigraph import (
    NORMALIZE_QUERY,
    project_owl_to_rdf12,
)
from gmeow_tools.rdf12_pyoxigraph import (
    normalize_rdf12_to_owl as normalize_rdf12_to_owl_pyoxigraph,
)


def _load_quads(path: Path) -> set[str]:
    """Load a Turtle file into pyoxigraph and return a set of N-Quad strings."""
    store = pyoxigraph.Store()
    store.load(path=str(path), format=pyoxigraph.RdfFormat.TURTLE)
    return {str(q) for q in store.quads_for_pattern(None, None, None, None)}


def test_pyoxigraph_parses_rdf12_triple_terms() -> None:
    """pyoxigraph must load the committed RDF 1.2 artifact without error."""
    store = pyoxigraph.Store()
    try:
        store.load(
            path=str(STATEMENT_RDF12_FILE),
            format=pyoxigraph.RdfFormat.TURTLE,
        )
    except Exception as exc:
        pytest.fail(f"pyoxigraph failed to parse {STATEMENT_RDF12_FILE.name}: {exc}")
    quads = list(store.quads_for_pattern(None, None, None, None))
    assert len(quads) > 0, "expected non-empty graph after parsing RDF 1.2"


def test_pyoxigraph_executes_normalize_construct() -> None:
    """pyoxigraph must run the rdf12-to-owl CONSTRUCT query without error."""
    store = pyoxigraph.Store()
    store.load(path=str(STATEMENT_RDF12_FILE), format=pyoxigraph.RdfFormat.TURTLE)
    query_text = NORMALIZE_QUERY.read_text(encoding="utf-8")
    try:
        results = store.query(query_text)
    except Exception as exc:
        pytest.fail(f"pyoxigraph failed to execute {NORMALIZE_QUERY.name}: {exc}")
    assert isinstance(results, pyoxigraph.QueryTriples)
    triples = list(results)
    assert len(triples) > 0, "expected non-empty CONSTRUCT result"


@pytest.mark.docker
def test_pyoxigraph_normalization_matches_jena() -> None:
    """pyoxigraph-normalized RDF 1.2 must be isomorphic to Jena-normalized form."""
    jena_graph = normalize_rdf12_to_owl_jena(STATEMENT_RDF12_FILE)
    pyoxi_graph = normalize_rdf12_to_owl_pyoxigraph(STATEMENT_RDF12_FILE)

    if not isomorphic(jena_graph, pyoxi_graph):
        _, only_jena, only_pyoxi = graph_diff(jena_graph, pyoxi_graph)
        msg_lines = [
            f"pyoxigraph normalization diverges from Jena "
            f"({len(only_jena)} only-Jena, {len(only_pyoxi)} only-pyoxigraph "
            f"triples):"
        ]
        for triple in sorted(only_jena, key=str)[:10]:
            msg_lines.append(f"  Jena-only: {triple}")
        for triple in sorted(only_pyoxi, key=str)[:10]:
            msg_lines.append(f"  pyoxigraph-only: {triple}")
        pytest.fail("\n".join(msg_lines))


def test_pyoxigraph_projection_matches_jena() -> None:
    """pyoxigraph-projected OWL → RDF 1.2 must match the committed Jena artifact.

    Loads both the pyoxigraph-projected output and the committed
    ``statements/gmeow.rdf12.ttl`` into pyoxigraph and compares at the quad
    level (semantic, not byte, comparison).
    """
    from tempfile import TemporaryDirectory

    with TemporaryDirectory() as tmp:
        rdf12_tmp = Path(tmp) / "gmeow.rdf12.ttl"
        project_owl_to_rdf12(STATEMENT_OWL_FILE, rdf12_tmp)

        projected_quads = _load_quads(rdf12_tmp)
        committed_quads = _load_quads(STATEMENT_RDF12_FILE)

        only_projected = projected_quads - committed_quads
        only_committed = committed_quads - projected_quads

        if only_projected or only_committed:
            msg_lines = [
                f"pyoxigraph projection diverges from committed Jena artifact "
                f"({len(only_projected)} only-projected, "
                f"{len(only_committed)} only-committed quads):"
            ]
            for q in sorted(only_projected)[:10]:
                msg_lines.append(f"  projected-only: {q}")
            for q in sorted(only_committed)[:10]:
                msg_lines.append(f"  committed-only: {q}")
            pytest.fail("\n".join(msg_lines))


def test_pyoxigraph_round_trip_is_lossless() -> None:
    """OWL → RDF 1.2 (pyoxigraph) → OWL (pyoxigraph) must be isomorphic to origin."""
    from tempfile import TemporaryDirectory

    origin = Graph()
    origin.parse(STATEMENT_OWL_FILE, format="turtle")

    with TemporaryDirectory() as tmp:
        root = Path(tmp)
        rdf12_tmp = root / "gmeow.rdf12.ttl"
        project_owl_to_rdf12(STATEMENT_OWL_FILE, rdf12_tmp)
        normalized = normalize_rdf12_to_owl_pyoxigraph(rdf12_tmp)

    if not isomorphic(origin, normalized):
        _, only_origin, only_normalized = graph_diff(origin, normalized)
        msg_lines = [
            f"pyoxigraph round-trip is lossy "
            f"({len(only_origin)} only-origin, "
            f"{len(only_normalized)} only-normalized triples):"
        ]
        for triple in sorted(only_origin, key=str)[:10]:
            msg_lines.append(f"  origin-only: {triple}")
        for triple in sorted(only_normalized, key=str)[:10]:
            msg_lines.append(f"  normalized-only: {triple}")
        pytest.fail("\n".join(msg_lines))

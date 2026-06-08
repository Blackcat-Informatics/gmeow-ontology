"""Independent in-process cross-check: pyoxigraph parses and normalizes RDF 1.2.

CONSTITUTION Principle 7 (verified by construction) + Principle 4 (Jena remains
canonical). This module proves that pyoxigraph — a pure in-process SPARQL
engine — can parse the committed RDF 1.2 triple-term Turtle and reproduce the
same OWL axiom-annotation normal form that the Jena-backed pipeline produces.

The Jena-backed ``compile-statements``, ``statements-check``, and docker-marked
tests in ``test_statements.py`` remain authoritative; this is an additive,
read-only verification path.
"""

from __future__ import annotations

from io import BytesIO
from pathlib import Path

import pyoxigraph
import pytest
from rdflib import Graph
from rdflib.compare import graph_diff, isomorphic

from gmeow_tools.config import STATEMENT_RDF12_FILE
from gmeow_tools.rdf12 import normalize_rdf12_to_owl

QUERIES_DIR = Path(__file__).resolve().parents[1] / "queries"
NORMALIZE_QUERY = QUERIES_DIR / "rdf12-to-owl.rq"


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


def test_pyoxigraph_normalization_matches_jena() -> None:
    """pyoxigraph-normalized RDF 1.2 must be isomorphic to Jena-normalized form."""
    jena_graph = normalize_rdf12_to_owl(STATEMENT_RDF12_FILE)

    store = pyoxigraph.Store()
    store.load(path=str(STATEMENT_RDF12_FILE), format=pyoxigraph.RdfFormat.TURTLE)
    query_text = NORMALIZE_QUERY.read_text(encoding="utf-8")
    results = store.query(query_text)
    assert isinstance(results, pyoxigraph.QueryTriples)

    output = BytesIO()
    pyoxigraph.serialize(results, output, format=pyoxigraph.RdfFormat.TURTLE)
    ttl_bytes = output.getvalue()

    pyoxi_graph = Graph()
    pyoxi_graph.parse(data=ttl_bytes, format="turtle")

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

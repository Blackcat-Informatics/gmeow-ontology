"""Cross-check the statement DSL → RDF 1.2 + OWL downcast (pyoxigraph).

A non-authoritative read-only mirror of :mod:`gmeow_tools.statement_compile` that
uses pyoxigraph for the RDF 1.2 projection and normalization instead of Apache
Jena. This proves the round-trip is engine-independent (CONSTITUTION Principle 7).

Jena remains the canonical artifact writer (Principle 4). This module never
writes to the committed artifact paths.
"""

from __future__ import annotations

import tempfile
from pathlib import Path

from rdflib import Graph
from rdflib.compare import graph_diff, isomorphic

from gmeow_tools.config import (
    PROJECT_ROOT,
    STATEMENT_OWL_FILE,
    STATEMENT_RDF12_FILE,
)
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.mapping_dsl import CompileError
from gmeow_tools.rdf12_pyoxigraph import normalize_rdf12_to_owl, project_owl_to_rdf12
from gmeow_tools.statement_compile import (
    _OWL_BANNER,
    StatementReport,
    _rel_str,
    _write_ttl,
    emit_owl,
)
from gmeow_tools.statement_dsl import load_statement_dsl
from gmeow_tools.statement_lint import statement_invariants


def _drift_pyoxigraph(owl_graph: Graph, fresh_rdf12: Path) -> list[str]:
    """Compare freshly-rendered artifacts against the committed ones (pyoxigraph)."""
    drifted: list[str] = []
    if not STATEMENT_OWL_FILE.exists():
        drifted.append(f"{_rel_str(STATEMENT_OWL_FILE)} (missing committed file)")
    elif not isomorphic(Graph().parse(STATEMENT_OWL_FILE, format="turtle"), owl_graph):
        drifted.append(_rel_str(STATEMENT_OWL_FILE))
    if not STATEMENT_RDF12_FILE.exists():
        drifted.append(f"{_rel_str(STATEMENT_RDF12_FILE)} (missing committed file)")
    elif not isomorphic(
        normalize_rdf12_to_owl(STATEMENT_RDF12_FILE),
        normalize_rdf12_to_owl(fresh_rdf12),
    ):
        drifted.append(_rel_str(STATEMENT_RDF12_FILE))
    return drifted


def assert_lossless_pyoxigraph(owl_graph: Graph, rdf12_path: Path) -> list[str]:
    """Prove the RDF 1.2 form round-trips to the OWL form via pyoxigraph."""
    normalized = normalize_rdf12_to_owl(rdf12_path)
    if isomorphic(owl_graph, normalized):
        return []
    _, only_owl, only_rdf12 = graph_diff(owl_graph, normalized)
    problems: list[str] = []
    for triple in sorted(only_owl, key=str):
        problems.append(f"OWL form has, RDF 1.2 lost: {triple}")
    for triple in sorted(only_rdf12, key=str):
        problems.append(f"RDF 1.2 form has, OWL lacks: {triple}")
    return problems


def compile_statements_pyoxigraph() -> StatementReport:
    """Cross-check statement-dsl/ → RDF 1.2 + OWL downcast (pyoxigraph, read-only).

    Identical logic to :func:`gmeow_tools.statement_compile.compile_statements`,
    but the RDF 1.2 projection and normalization use pyoxigraph instead of Jena.
    Always runs in drift-check mode; never writes to the committed artifact paths.

    Raises:
        CompileError: On an invariant violation or a lossy round-trip.
    """
    dsl = load_statement_dsl()
    onto = load_merged_graph(include_imports=False)
    problems = statement_invariants(dsl, onto)
    if problems:
        raise CompileError(
            "statement DSL violates invariants:\n  " + "\n  ".join(problems)
        )

    owl = emit_owl(dsl)
    with tempfile.TemporaryDirectory(dir=PROJECT_ROOT) as tmp:
        root = Path(tmp)
        owl_tmp = root / "gmeow-statements.owl.ttl"
        _write_ttl(owl, owl_tmp, _OWL_BANNER)
        rdf12_tmp = root / "gmeow.rdf12.ttl"
        project_owl_to_rdf12(owl_tmp, rdf12_tmp)

        lossy = assert_lossless_pyoxigraph(owl, rdf12_tmp)
        if lossy:
            raise CompileError(
                "RDF 1.2 / OWL round-trip is lossy (emit blocked):\n  "
                + "\n  ".join(lossy)
            )
        return StatementReport(drifted=_drift_pyoxigraph(owl, rdf12_tmp))

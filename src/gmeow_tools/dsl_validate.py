"""RDF-native SHACL validation for the GMEOW mapping and statement DSL sources.

Runs ``gmeow_shacl`` over the merged DSL graph *before* the Python graph-walkers
(:mod:`gmeow_tools.mapping_dsl`, :mod:`gmeow_tools.statement_dsl`) proceed
into dataclass parsing. Violations are surfaced as structured, per-node
diagnostics (focus node, path, message, source file) so malformed DSL cells
fail with an RDF-native conformance report rather than a bare Python
:exc:`CompileError`.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, URIRef
from rdflib.term import Node

from gmeow_tools import shacl_engine
from gmeow_tools.config import (
    MAPPING_DSL_SHAPES_FILE,
    STATEMENT_DSL_SHAPES_FILE,
)
from gmeow_tools.shacl_engine import ShaclResult


def _format_violations(
    results: list[ShaclResult], node_to_file: dict[Node, Path]
) -> list[str]:
    """Extract structured SHACL results into readable, enriched lines.

    Each line carries ``focus=<node> | path=<path> | msg=<message>``
    and ``source=<file>`` when the focus node is a named IRI that was
    tracked to its originating Turtle file.
    """
    violations: list[str] = []
    for result in results:
        focus = result.get("focus")
        path = result.get("path")
        message = result.get("message")
        parts: list[str] = []
        if focus is not None:
            parts.append(f"focus={shacl_engine.term_to_str(focus)}")
        if path is not None:
            parts.append(f"path={shacl_engine.term_to_str(path)}")
        if message is not None:
            parts.append(f"msg={message}")
        # Source provenance only applies to named-IRI focus nodes (gmeow_shacl
        # renders them as <iri>); blank nodes carry no file mapping.
        if focus is not None and focus.startswith("<") and focus.endswith(">"):
            src = node_to_file.get(URIRef(focus[1:-1]))
            if src is not None:
                parts.append(f"source={src}")
        violations.append(" | ".join(parts))
    return violations


def _validate_dsl(
    graph: Graph,
    shapes_path: Path,
    node_to_file: dict[Node, Path],
) -> list[str]:
    """Validate ``graph`` against the DSL shapes at ``shapes_path`` (gmeow_shacl).

    Returns a list of formatted violation strings (empty == conformant).

    Raises:
        FileNotFoundError: If the shapes file is missing.
        ValueError: Propagated from ``gmeow_shacl`` on a parse/validate error
            (hard-fail, never a silent conforms — P11/§11).
    """
    if not shapes_path.exists():
        raise FileNotFoundError(f"DSL SHACL shapes not found: {shapes_path}")

    shapes_ttl = shapes_path.read_text(encoding="utf-8")
    report = shacl_engine.validate_graph(graph, shapes_ttl)
    if report["conforms"]:
        return []

    violations = _format_violations(report["results"], node_to_file)
    # Defensive: a non-conforming report with no parseable results must still
    # surface (gmeow_shacl reports conforms == results-empty, so unreachable).
    if not violations:
        violations.append("SHACL validation failed: non-conforming with no results")
    return violations


def validate_mapping_dsl(graph: Graph, node_to_file: dict[Node, Path]) -> list[str]:
    """Validate a merged mapping DSL graph.

    Args:
        graph: The merged mapping DSL graph.
        node_to_file: Mapping from named IRIs to the source ``.ttl`` file
            they were first seen in.

    Returns:
        A list of formatted violation strings; empty when conformant.
    """
    return _validate_dsl(graph, MAPPING_DSL_SHAPES_FILE, node_to_file)


def validate_statement_dsl(graph: Graph, node_to_file: dict[Node, Path]) -> list[str]:
    """Validate a merged statement DSL graph.

    Args:
        graph: The merged statement DSL graph.
        node_to_file: Mapping from named IRIs to the source ``.ttl`` file
            they were first seen in.

    Returns:
        A list of formatted violation strings; empty when conformant.
    """
    return _validate_dsl(graph, STATEMENT_DSL_SHAPES_FILE, node_to_file)

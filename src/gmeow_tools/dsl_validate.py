"""RDF-native SHACL validation for the GMEOW mapping and statement DSL sources.

Runs pyshacl over the merged DSL graph *before* the Python graph-walkers
(:mod:`gmeow_tools.mapping_dsl`, :mod:`gmeow_tools.statement_dsl`) proceed
into dataclass parsing. Violations are surfaced as structured, per-node
diagnostics (focus node, path, message, source file) so malformed DSL cells
fail with an RDF-native conformance report rather than a bare Python
:exc:`CompileError`.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import RDF, Graph, URIRef
from rdflib.namespace import SH
from rdflib.term import Node

from gmeow_tools.config import (
    MAPPING_DSL_SHAPES_FILE,
    STATEMENT_DSL_SHAPES_FILE,
)
from gmeow_tools.graph import bind_prefixes


def _format_violations(
    report_graph: Graph, node_to_file: dict[Node, Path]
) -> list[str]:
    """Extract SHACL validation results into readable, enriched lines.

    Each line carries ``focus=<node> | path=<path> | msg=<message>``
    and ``source=<file>`` when the focus node is a named IRI that was
    tracked to its originating Turtle file.
    """
    violations: list[str] = []
    for result in report_graph.subjects(RDF.type, SH.ValidationResult):
        focus = report_graph.value(result, SH.focusNode)
        path = report_graph.value(result, SH.resultPath)
        message = report_graph.value(result, SH.resultMessage)
        parts: list[str] = []
        if focus is not None:
            parts.append(f"focus={focus}")
        if path is not None:
            parts.append(f"path={path}")
        if message is not None:
            parts.append(f"msg={message}")
        if isinstance(focus, URIRef):
            src = node_to_file.get(focus)
            if src is not None:
                parts.append(f"source={src}")
        violations.append(" | ".join(parts))
    return violations


def _validate_dsl(
    graph: Graph,
    shapes_path: Path,
    node_to_file: dict[Node, Path],
) -> list[str]:
    """Run pyshacl against ``graph`` using the shapes at ``shapes_path``.

    Returns a list of formatted violation strings (empty == conformant).
    """
    if not shapes_path.exists():
        raise FileNotFoundError(f"DSL SHACL shapes not found: {shapes_path}")

    from pyshacl import validate as shacl_validate

    bind_prefixes(graph)
    shapes_graph = Graph().parse(shapes_path, format="turtle")
    conforms, report_graph, report_text = shacl_validate(
        graph,
        shacl_graph=shapes_graph,
        advanced=True,
        inference="none",
        abort_on_first=False,
        meta_shacl=False,
    )
    if conforms:
        return []

    violations = _format_violations(report_graph, node_to_file)
    # Defensive: if pyshacl reports non-conformance but we could not parse
    # any structured results, surface the raw report text.
    if not violations:
        violations.append(f"SHACL validation failed:\n{report_text.strip()}")
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

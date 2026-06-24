"""RDF-native SHACL validation for the GMEOW mapping and statement DSL sources.

Runs ``gmeow_shacl`` over the merged DSL graph before the mapping parser or the
native statement compiler proceed. Violations are surfaced as structured, per-node
diagnostics (focus node, path, message, source file) so malformed DSL cells
fail with an RDF-native conformance report rather than a bare Python
:exc:`CompileError`.

The merged N-Triples and the focus→file provenance map are both built in Rust
(``gmeow_validate.dsl_merge_with_provenance``): each ``.ttl`` file is parsed in
order and every named subject is mapped to the FIRST file it appears in. This
module therefore constructs no Python graph object at all — the validation path
is graph-free end to end (#579).
"""

from __future__ import annotations

from pathlib import Path

import gmeow_validate

from gmeow_tools import shacl_engine
from gmeow_tools.config import (
    MAPPING_DSL_SHAPES_FILE,
    STATEMENT_DSL_SHAPES_FILE,
    TEST_DSL_SHAPES_FILE,
)
from gmeow_tools.shacl_engine import ShaclResult


def _format_violations(
    results: list[ShaclResult], focus_to_file: dict[str, str]
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
            src = focus_to_file.get(focus[1:-1])
            if src is not None:
                parts.append(f"source={src}")
        violations.append(" | ".join(parts))
    return violations


def _validate_dsl(dsl_paths: list[str], shapes_path: Path) -> list[str]:
    """Validate the merged DSL ``dsl_paths`` against the shapes at ``shapes_path``.

    Builds the merged N-Triples and the focus→file map in Rust, validates via
    ``gmeow_shacl``, and enriches each violation with ``source=`` from the map.
    Returns a list of formatted violation strings (empty == conformant).

    Raises:
        FileNotFoundError: If the shapes file is missing.
        ValueError: Propagated from the Rust merge (a DSL file that fails to
            parse) or from ``gmeow_shacl`` (a parse/validate error) — hard-fail,
            never a silent conforms (P11/§11).
    """
    if not shapes_path.exists():
        raise FileNotFoundError(f"DSL SHACL shapes not found: {shapes_path}")

    data_nt, focus_pairs = gmeow_validate.dsl_merge_with_provenance(dsl_paths)
    focus_to_file: dict[str, str] = dict(focus_pairs)

    shapes_ttl = shapes_path.read_text(encoding="utf-8")
    report = shacl_engine.validate_nt(data_nt, shapes_ttl)
    if report["conforms"]:
        return []

    violations = _format_violations(report["results"], focus_to_file)
    # Defensive: a non-conforming report with no parseable results must still
    # surface (gmeow_shacl reports conforms == results-empty, so unreachable).
    if not violations:
        violations.append("SHACL validation failed: non-conforming with no results")
    return violations


def validate_mapping_dsl(dsl_paths: list[str]) -> list[str]:
    """Validate the merged mapping DSL sources.

    Args:
        dsl_paths: The mapping DSL ``.ttl`` source paths to merge and validate.

    Returns:
        A list of formatted violation strings; empty when conformant.
    """
    return _validate_dsl(dsl_paths, MAPPING_DSL_SHAPES_FILE)


def validate_statement_dsl(dsl_paths: list[str]) -> list[str]:
    """Validate the merged statement DSL sources.

    Args:
        dsl_paths: The statement DSL ``.ttl`` source paths to merge and validate.

    Returns:
        A list of formatted violation strings; empty when conformant.
    """
    return _validate_dsl(dsl_paths, STATEMENT_DSL_SHAPES_FILE)


def validate_test_dsl(dsl_paths: list[str]) -> list[str]:
    """Validate the merged test DSL sources.

    Args:
        dsl_paths: The test DSL ``.ttl`` source paths to merge and validate
            (the vocabulary plus every slice-resident ``tests/*.ttl`` fixture).

    Returns:
        A list of formatted violation strings; empty when conformant.
    """
    return _validate_dsl(dsl_paths, TEST_DSL_SHAPES_FILE)

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Test-only rdflib→N-Triples seam for the graph-free validation path.

The production validation path (``gmeow_tools.validate``) no longer accepts
rdflib graphs — it takes FILE PATHS or N-Triples strings and builds its oxigraph
store in Rust. Tests, however, still hand-build small synthetic rdflib graphs to
seed a single anti-pattern or a malformed-annotation case. rdflib is removed only
from the validation-path source files, NOT from the test suite, so this helper
lives here: it serializes a synthetic graph to N-Triples (or a temp ``.nt`` file)
and calls the graph-free production functions.

Keeping the serialization here — rather than in the source files — is the whole
point of #579: the seam that converts a Python graph into the Rust validators'
input is a *test* concern now, exercised only by hand-built fixtures.
"""

from __future__ import annotations

import tempfile
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from pathlib import Path

import gmeow_validate
from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools.config import NAMESPACE, SHAPES_FILE
from gmeow_tools.validate import (
    ValidationResult,
)
from gmeow_tools.validate import (
    guide_anchor_lint as _guide_anchor_lint,
)
from gmeow_tools.validate import (
    run_shacl as _run_shacl,
)
from gmeow_tools.validate import (
    structural_lint as _structural_lint,
)


def graph_to_nt(graph: Graph) -> str:
    """Serialize a synthetic rdflib graph to N-Triples (UTF-8)."""
    return graph.serialize(format="nt", encoding="utf-8").decode("utf-8")


@contextmanager
def graph_as_paths(graph: Graph) -> Iterator[list[str]]:
    """Yield a one-element source-path list for *graph* (a temp ``.nt`` file).

    The path-based Rust lints (`structural_lint`, `term_naming_lint`,
    `guide_anchor_lint`) build their store from file paths; a synthetic graph is
    written to one N-Triples temp file. The file is removed on exit.
    """
    with tempfile.NamedTemporaryFile(
        "w", suffix=".nt", prefix="gmeow-test-", delete=False, encoding="utf-8"
    ) as handle:
        handle.write(graph_to_nt(graph))
        path = Path(handle.name)
    try:
        yield [str(path)]
    finally:
        path.unlink(missing_ok=True)


# --------------------------------------------------------------------------- #
# Graph-accepting wrappers over the graph-free production functions.
# --------------------------------------------------------------------------- #


def run_shacl(graph: Graph, *, shapes_path: Path = SHAPES_FILE) -> ValidationResult:
    """Validate a synthetic rdflib *graph* against the SHACL shapes."""
    return _run_shacl(graph_to_nt(graph), shapes_path=shapes_path)


def structural_lint(graph: Graph) -> ValidationResult:
    """Run the structural lint over a synthetic rdflib *graph*."""
    with graph_as_paths(graph) as paths:
        return _structural_lint(paths)


def guide_anchor_lint(graph: Graph, root: Path | None = None) -> ValidationResult:
    """Run the guide-anchor lint over a synthetic rdflib *graph*."""
    with graph_as_paths(graph) as paths:
        return _guide_anchor_lint(paths, root=root)


# Reasoning per-check shims: serialize the graph and route each anti-pattern
# check straight through the native ``gmeow_validate.reasoning_*_nt`` engine. The
# Rust check returns ``{"errors": [...], ...}``; tests want the bare error list.
# The production ``validate_all`` path calls the same native entry points from
# Rust over file paths — this seam only adapts a hand-built synthetic graph.


def _reasoning_check(
    check: Callable[[str, str], dict[str, list[str]]], graph: Graph
) -> list[str]:
    """Serialize *graph* to N-Triples and route it through one native *check*."""
    report = check(graph_to_nt(graph), str(NAMESPACE))
    return list(report["errors"])


def anti_rigidity_discipline(graph: Graph) -> list[str]:
    return _reasoning_check(gmeow_validate.reasoning_anti_rigidity_discipline_nt, graph)


def coequal_facet_orthogonality(graph: Graph) -> list[str]:
    return _reasoning_check(
        gmeow_validate.reasoning_coequal_facet_orthogonality_nt, graph
    )

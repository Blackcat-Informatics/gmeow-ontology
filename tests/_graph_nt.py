# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Test-only rdflib→N-Triples seam for the graph-free validation path (#579).

The production validation path (``gmeow_tools.validate`` / ``reasoning_lint``)
no longer accepts rdflib graphs — it takes FILE PATHS or N-Triples strings and
builds its oxigraph store in Rust. Tests, however, still hand-build small
synthetic rdflib graphs to seed a single anti-pattern or a malformed-annotation
case. rdflib is removed only from the five validation-path source files, NOT from
the test suite, so this helper lives here: it serializes a synthetic graph to
N-Triples (or a temp ``.nt`` file) and calls the graph-free production functions.

Keeping the serialization here — rather than in the source files — is the whole
point of #579: the seam that converts a Python graph into the Rust validators'
input is a *test* concern now, exercised only by hand-built fixtures.
"""

from __future__ import annotations

import tempfile
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools import reasoning_lint as _reasoning_lint
from gmeow_tools.config import SHAPES_FILE
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
from gmeow_tools.validate import (
    term_naming_lint as _term_naming_lint,
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


def term_naming_lint(graph: Graph) -> ValidationResult:
    """Run the term-naming lint over a synthetic rdflib *graph*."""
    with graph_as_paths(graph) as paths:
        return _term_naming_lint(paths)


def reasoning_lint(graph: Graph) -> ValidationResult:
    """Run the reasoning lint (validate wrapper) over a synthetic rdflib *graph*."""
    from gmeow_tools.validate import reasoning_lint as _rl

    with graph_as_paths(graph) as paths:
        return _rl(paths)


def guide_anchor_lint(graph: Graph, root: Path | None = None) -> ValidationResult:
    """Run the guide-anchor lint over a synthetic rdflib *graph*."""
    with graph_as_paths(graph) as paths:
        return _guide_anchor_lint(paths, root=root)


# Reasoning per-check shims: serialize the graph and route through reasoning_lint
# (which now accepts N-Triples text directly). The production checks are typed
# ``-> list[str]``; the explicit annotation here keeps these wrappers strict.


def exactly_one_stereotype(graph: Graph) -> list[str]:
    result: list[str] = _reasoning_lint.exactly_one_stereotype(graph_to_nt(graph))
    return result


def identity_overlap(graph: Graph) -> list[str]:
    result: list[str] = _reasoning_lint.identity_overlap(graph_to_nt(graph))
    return result


def anti_rigidity_discipline(graph: Graph) -> list[str]:
    result: list[str] = _reasoning_lint.anti_rigidity_discipline(graph_to_nt(graph))
    return result


def relator_mediation(graph: Graph) -> list[str]:
    result: list[str] = _reasoning_lint.relator_mediation(graph_to_nt(graph))
    return result


def coequal_facet_orthogonality(graph: Graph) -> list[str]:
    result: list[str] = _reasoning_lint.coequal_facet_orthogonality(graph_to_nt(graph))
    return result


def frame_declaration_completeness(graph: Graph) -> list[str]:
    nt = graph_to_nt(graph)
    result: list[str] = _reasoning_lint.frame_declaration_completeness(nt)
    return result


def reasoning_invariants(graph: Graph) -> list[str]:
    result: list[str] = _reasoning_lint.reasoning_invariants(graph_to_nt(graph))
    return result

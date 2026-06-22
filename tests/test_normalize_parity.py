"""Task 9 gate: the native canonical Turtle normalizer preserves graph identity.

The Rust serializer (`gmeow_rdf.canonicalize_turtle`, over the gmeow-rdf IR) is
NOT byte-identical to rdflib `longturtle` — its gate is RDFC-1.0 ISOMORPHISM:
re-serializing a source must yield an isomorphic graph, and re-running must be a
no-op (idempotent). Verified across every authored ontology source.
"""

from __future__ import annotations

from pathlib import Path

import gmeow_rdf
import pytest
from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools.config import PREFIXES
from gmeow_tools.graph import iter_source_files
from gmeow_tools.rdf_canonical import graphs_isomorphic

_PREFIXES = sorted(PREFIXES.items())
_SOURCES = sorted(iter_source_files(include_imports=False))


def _canon(data: bytes) -> bytes:
    return bytes(gmeow_rdf.canonicalize_turtle(data, _PREFIXES))


@pytest.mark.parametrize("path", _SOURCES, ids=lambda p: str(p))
def test_normalize_preserves_isomorphism_and_is_idempotent(path: Path) -> None:
    original = path.read_bytes()
    once = _canon(original)
    # 1. The normalized graph is isomorphic to the source (no triples gained/lost).
    assert graphs_isomorphic(
        Graph().parse(data=original, format="turtle"),
        Graph().parse(data=once, format="turtle"),
    ), f"{path}: normalization changed the graph"
    # 2. Re-normalizing is a byte-for-byte no-op.
    assert _canon(once) == once, f"{path}: normalization is not idempotent"


def test_corpus_nonempty() -> None:
    # Guard against the sweep silently covering nothing.
    assert len(_SOURCES) > 10

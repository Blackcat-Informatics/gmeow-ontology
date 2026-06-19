# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Byte-parity regression test for the Rust GTS producer (#645).

Builds the full GMEOW offline snapshot through both the new Rust producer
(``compile_gts``) and the legacy Python producer (``_compile_gts_legacy``)
using the ``identity`` transform so the comparison isolates producer logic
from cross-library gzip encoding differences. If the bytes differ, the test
falls back to semantic parity via canonical N-Quads and reports the first
differing byte offset.

The production ``build_snapshot_bytes()`` path uses ``["gzip"]``; that is
covered by a separate semantic round-trip assertion.
"""

from __future__ import annotations

from typing import Any

import pytest

from gmeow_tools.config import (
    GTS_GRAPH_IMPORTS,
    GTS_GRAPH_METADATA,
    SLICES_DIR,
    STATEMENT_RDF12_FILE,
)
from gmeow_tools.graph import iter_source_files, load_merged_graph
from gmeow_tools.gts_gen import (
    _doc_blobs,
    _imports_graph,
    _metadata_graph,
    _ontology_doc_blobs,
    _project_doc_blobs,
    _transform_blobs,
    build_snapshot_bytes,
)
from gmeow_tools.gts_producer import _compile_gts_legacy, compile_gts
from gmeow_tools.i18n_catalog import merge_terms
from gmeow_tools.mappings import build_alignment_graph, load_mappings
from gmeow_tools.validate import guide_anchor_lint


def _build_snapshot_inputs() -> tuple[object, list[tuple[bytes, str, str]]]:
    """Return the same (multilingual_graph, doc_blobs) tuple used by the generator."""
    graph = load_merged_graph(include_imports=False)
    lint = guide_anchor_lint([str(p) for p in iter_source_files(include_imports=False)])
    if lint.errors:
        details = "; ".join(lint.errors[:5])
        raise ValueError(
            f"docs-in-sync invariant violated (#325): {len(lint.errors)} "
            f"guide anchor error(s) — {details}"
        )

    po_paths = sorted(p for p in SLICES_DIR.glob("*/*/i18n/*.po") if p.stem != "en")
    multilingual_graph = merge_terms(graph, po_paths)

    blobs = (
        _doc_blobs(multilingual_graph)
        + _project_doc_blobs()
        + _ontology_doc_blobs()
        + _transform_blobs(multilingual_graph)
    )
    return multilingual_graph, blobs


def _producer_kwargs(
    multilingual_graph: object,
    blobs: list[tuple[bytes, str, str]],
    transform: list[str],
) -> dict[str, Any]:
    """Keyword arguments shared by both producer entry points."""
    return {
        "graph": multilingual_graph,
        "rdf12_path": STATEMENT_RDF12_FILE,
        "alignment_graph": build_alignment_graph(load_mappings()),
        "extra_named_graphs": [
            (_imports_graph(), GTS_GRAPH_IMPORTS, "imports"),
            (_metadata_graph(), GTS_GRAPH_METADATA, "metadata"),
        ],
        "doc_blobs": blobs,
        "transform": transform,
    }


def _first_diff(a: bytes, b: bytes) -> int:
    """Return the first byte offset where *a* and *b* differ."""
    limit = min(len(a), len(b))
    for i in range(limit):
        if a[i] != b[i]:
            return i
    return limit


@pytest.mark.ci_only
def test_rust_and_legacy_producers_are_byte_identical() -> None:
    """Full-snapshot byte parity between Rust and legacy Python producers.

    Uses ``identity`` compression so the test exercises term/quad/reifier
    canonicalization, blob ordering, and frame construction without depending
    on cross-library gzip implementation details.
    """
    multilingual_graph, doc_blobs = _build_snapshot_inputs()

    kwargs = _producer_kwargs(multilingual_graph, doc_blobs, ["identity"])
    rust_bytes = compile_gts(**kwargs)
    py_bytes = _compile_gts_legacy(**kwargs)

    if rust_bytes == py_bytes:
        return

    first = _first_diff(rust_bytes, py_bytes)
    print(f"First byte difference at offset {first}")

    from gts import read, to_nquads

    rust_graph = read(rust_bytes)
    py_graph = read(py_bytes)
    rust_nq = sorted(to_nquads(rust_graph).splitlines())
    py_nq = sorted(to_nquads(py_graph).splitlines())

    if rust_nq != py_nq:
        only_rust = set(rust_nq) - set(py_nq)
        only_py = set(py_nq) - set(rust_nq)
        pytest.fail(
            f"Semantic drift: {len(only_rust)} quads only in Rust, "
            f"{len(only_py)} quads only in Python; "
            f"first byte diff at offset {first}"
        )

    pytest.fail(
        f"Semantic parity holds but bytes differ at offset {first}; "
        "likely CBOR encoding or library ordering drift"
    )


@pytest.mark.ci_only
def test_production_snapshot_semantic_round_trip() -> None:
    """The production ``["gzip"]`` snapshot folds to the same graph as identity."""
    multilingual_graph, doc_blobs = _build_snapshot_inputs()

    rust_gzip = build_snapshot_bytes()
    rust_identity = compile_gts(
        **_producer_kwargs(multilingual_graph, doc_blobs, ["identity"])
    )

    from gts import read, to_nquads

    gzip_nq = sorted(to_nquads(read(rust_gzip)).splitlines())
    identity_nq = sorted(to_nquads(read(rust_identity)).splitlines())

    if gzip_nq == identity_nq:
        return

    only_gzip = set(gzip_nq) - set(identity_nq)
    only_identity = set(identity_nq) - set(gzip_nq)
    pytest.fail(
        f"Production gzip snapshot drifts from identity snapshot: "
        f"{len(only_gzip)} quads only in gzip, "
        f"{len(only_identity)} quads only in identity"
    )

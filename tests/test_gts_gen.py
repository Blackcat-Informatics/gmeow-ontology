# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the ``gts`` snapshot generator — the narrow waist's producer (#267).

The snapshot is only usable as a drift-gated artifact if its bytes are a pure
function of the sources: rdflib blank-node labels are per-process UUIDs and
iteration order is hash-seed-dependent, so determinism is the load-bearing
property here, tested in-process AND across processes with different hash
seeds.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import pytest
from rdflib import RDF, Graph, Literal, URIRef
from rdflib.compare import to_canonical_graph

from gmeow_tools.config import (
    GTS_GRAPH_ALIGNMENTS,
    GTS_GRAPH_STATEMENTS,
    GTS_SNAPSHOT_FILE,
    PROJECT_ROOT,
)
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.gts_producer import compile_gts
from gmeow_tools.mappings import build_alignment_graph, load_mappings
from gts import read
from gts.model import TermKind

EX = "https://example.org/"


def _small_graph() -> Graph:
    g = Graph()
    g.add((URIRef(EX + "cat"), RDF.type, URIRef(EX + "Animal")))
    g.add((URIRef(EX + "cat"), URIRef(EX + "label"), Literal("Cat", lang="en")))
    return g


def _build_snapshot() -> bytes:
    from gmeow_tools.gts_gen import build_snapshot_bytes

    return build_snapshot_bytes()


def test_double_build_is_byte_identical() -> None:
    """Two in-process builds emit identical bytes."""
    assert _build_snapshot() == _build_snapshot()


def test_cross_hash_seed_builds_are_byte_identical(tmp_path: Path) -> None:
    """Builds under different PYTHONHASHSEED values emit identical bytes.

    The teeth of the determinism claim: set/dict iteration order and rdflib
    BNode labels vary across processes; the canonicalization + content-sorted
    interning must erase all of it.
    """
    script = (
        "from gmeow_tools.config import STATEMENT_RDF12_FILE\n"
        "from gmeow_tools.graph import load_merged_graph\n"
        "from gmeow_tools.gts_producer import compile_gts\n"
        "from gmeow_tools.mappings import build_alignment_graph, load_mappings\n"
        "import sys\n"
        "data = compile_gts(load_merged_graph(include_imports=False),"
        " STATEMENT_RDF12_FILE,"
        " alignment_graph=build_alignment_graph(load_mappings()))\n"
        "sys.stdout.buffer.write(data)\n"
    )
    outputs = []
    for seed in ("0", "424242"):
        result = subprocess.run(
            [sys.executable, "-c", script],
            capture_output=True,
            check=True,
            env={**os.environ, "PYTHONHASHSEED": seed},
            cwd=PROJECT_ROOT,
        )
        outputs.append(result.stdout)
    assert outputs[0] == outputs[1], "snapshot bytes depend on the hash seed"


def test_committed_snapshot_matches_a_fresh_build() -> None:
    """The committed artifact reproduces from sources (the drift gate's claim)."""
    assert GTS_SNAPSHOT_FILE.exists(), "run `gmeow regenerate gts`"
    assert _build_snapshot() == GTS_SNAPSHOT_FILE.read_bytes()


def test_snapshot_partitions_sources_into_named_graphs() -> None:
    """Base graph → default; rdf12 base quads → statements; SSSOM → alignments."""
    g = read(GTS_SNAPSHOT_FILE.read_bytes())
    assert g.diagnostics == []
    graph_names = {g.terms[gid].value for _, _, _, gid in g.quads if gid is not None}
    assert graph_names == {GTS_GRAPH_STATEMENTS, GTS_GRAPH_ALIGNMENTS}

    default_quads = sum(1 for q in g.quads if q[3] is None)
    merged_size = len(to_canonical_graph(load_merged_graph(include_imports=False)))
    from gmeow_tools.config import SLICES_DIR

    guide_links = sum(
        1
        for m in SLICES_DIR.glob("*/*/manifest.ttl")
        if (m.parent / "docs.md").exists()
    )
    # The default graph carries the merged sources PLUS one gmeow:guideBlob
    # linkage per embedded slice guide (#325).
    assert default_quads == merged_size + guide_links

    alignments = [
        q
        for q in g.quads
        if q[3] is not None and g.terms[q[3]].value == GTS_GRAPH_ALIGNMENTS
    ]
    assert len(alignments) == len(build_alignment_graph(load_mappings()))

    # the statement layer rides with its reifier machinery intact
    assert g.reifiers and g.annotations


def test_snapshot_term_ids_are_content_sorted() -> None:
    """IRIs precede bnodes precede literals; each kind lexicographic — the
    pure-function-of-content id assignment."""
    g = read(GTS_SNAPSHOT_FILE.read_bytes())
    kinds = [int(t.kind) for t in g.terms]
    assert kinds == sorted(kinds)
    iris = [t.value or "" for t in g.terms if t.kind is TermKind.IRI]
    assert iris == sorted(iris)


def test_conflicting_reifier_rebind_is_an_error(tmp_path: Path) -> None:
    """The canonical producer refuses ambiguous input instead of order-
    defined first-wins (the READER tolerates foreign files; we don't ship
    ambiguity)."""
    rdf12 = tmp_path / "conflict.ttl"
    rdf12.write_text(
        f"@prefix ex: <{EX}> .\n"
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n"
        "ex:r1 rdf:reifies <<( ex:a ex:p ex:b )>> .\n"
        "ex:r1 rdf:reifies <<( ex:a ex:p ex:c )>> .\n",
        encoding="utf-8",
    )
    with pytest.raises(ValueError, match="conflicting reifier"):
        compile_gts(_small_graph(), rdf12)


def test_compare_labels_encoding_only_vs_semantic_drift(tmp_path: Path) -> None:
    """The generator's compare distinguishes codec skew from source change."""
    from gmeow_tools.generator import registry
    from gmeow_tools.gts_gen import GtsSnapshotGenerator  # noqa: F401  (register)

    gen = registry()["gts"]

    committed = tmp_path / "committed.gts"
    committed.write_bytes(compile_gts(_small_graph()))

    # identical fold, different bytes (identity vs zstd) → encoding-only
    fresh = tmp_path / "fresh.gts"
    fresh.write_bytes(compile_gts(_small_graph(), transform=["identity"]))
    [diag] = gen.compare(fresh, committed)
    assert "encoding-only" in diag

    # different content → semantic
    other = _small_graph()
    other.add((URIRef(EX + "dog"), RDF.type, URIRef(EX + "Animal")))
    fresh.write_bytes(compile_gts(other))
    [diag] = gen.compare(fresh, committed)
    assert "semantic" in diag

    # equal bytes → clean
    fresh.write_bytes(committed.read_bytes())
    assert gen.compare(fresh, committed) == []

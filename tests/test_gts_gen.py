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
from collections.abc import Mapping
from pathlib import Path

import pytest
from rdflib import RDF, Graph, Literal, URIRef
from rdflib.compare import to_canonical_graph

from gmeow_tools.config import (
    GTS_GRAPH_ALIGNMENTS,
    GTS_GRAPH_IMPORTS,
    GTS_GRAPH_METADATA,
    GTS_GRAPH_STATEMENTS,
    GTS_SNAPSHOT_FILE,
    PROJECT_ROOT,
)
from gmeow_tools.graph import iter_import_files, load_merged_graph
from gmeow_tools.gts_producer import compile_gts
from gmeow_tools.mappings import build_alignment_graph, load_mappings
from gts import read
from gts.model import TermKind
from gts.wire import iter_items, unwrap_header

EX = "https://example.org/"


def _small_graph() -> Graph:
    g = Graph()
    g.add((URIRef(EX + "cat"), RDF.type, URIRef(EX + "Animal")))
    g.add((URIRef(EX + "cat"), URIRef(EX + "label"), Literal("Cat", lang="en")))
    return g


def _build_snapshot() -> bytes:
    from gmeow_tools.gts_gen import build_snapshot_bytes

    return build_snapshot_bytes()


@pytest.fixture(scope="module")
def fresh_snapshot() -> bytes:
    """One full source-built snapshot for read-only drift/content assertions."""
    return _build_snapshot()


def _frame_codecs(data: bytes) -> list[tuple[str, list[str]]]:
    items, torn = iter_items(data)
    assert torn is None
    header = unwrap_header(items[0][1])
    raw_catalog = header.get("cat")
    assert isinstance(raw_catalog, Mapping)
    catalog: dict[int, str] = {}
    for cid, entry in raw_catalog.items():
        assert isinstance(cid, int)
        assert isinstance(entry, Mapping)
        name = entry.get("name")
        assert isinstance(name, str)
        catalog[cid] = name
    frames: list[tuple[str, list[str]]] = []
    for _offset, item in items[1:]:
        assert isinstance(item, dict)
        frames.append(
            (str(item["t"]), [str(catalog[cid]) for cid in item.get("x", [])])
        )
    return frames


@pytest.mark.ci_only
def test_double_build_is_byte_identical(fresh_snapshot: bytes) -> None:
    """Two in-process builds emit identical bytes."""
    assert fresh_snapshot == _build_snapshot()


@pytest.mark.ci_only
def test_cross_hash_seed_builds_are_byte_identical(tmp_path: Path) -> None:
    """Builds under different PYTHONHASHSEED values emit identical bytes.

    The teeth of the determinism claim: set/dict iteration order and rdflib
    BNode labels vary across processes; the canonicalization + content-sorted
    interning must erase all of it.
    """
    script = (
        "from gmeow_tools.gts_gen import build_snapshot_bytes\n"
        "import sys\n"
        "sys.stdout.buffer.write(build_snapshot_bytes())\n"
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


@pytest.mark.ci_only
def test_committed_snapshot_matches_a_fresh_build(fresh_snapshot: bytes) -> None:
    """The committed artifact reproduces from sources (the drift gate's claim)."""
    assert GTS_SNAPSHOT_FILE.exists(), "run `gmeow regenerate gts`"
    assert fresh_snapshot == GTS_SNAPSHOT_FILE.read_bytes()


def test_committed_snapshot_uses_rsyncable_frames() -> None:
    """The committed bundle's large frames are delta-friendly."""
    frames = _frame_codecs(GTS_SNAPSHOT_FILE.read_bytes())
    assert ("snapshot", ["zstd-rsyncable"]) in frames
    assert any(
        frame == "blob" and codecs == ["zstd-rsyncable"] for frame, codecs in frames
    )


def test_snapshot_partitions_sources_into_named_graphs() -> None:
    """Default graph is authored GMEOW; imports/metadata are named graphs."""
    g = read(GTS_SNAPSHOT_FILE.read_bytes())
    assert g.diagnostics == []
    graph_names = {g.terms[gid].value for _, _, _, gid in g.quads if gid is not None}
    assert graph_names == {
        GTS_GRAPH_STATEMENTS,
        GTS_GRAPH_ALIGNMENTS,
        GTS_GRAPH_IMPORTS,
        GTS_GRAPH_METADATA,
    }

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

    imports = [
        q
        for q in g.quads
        if q[3] is not None and g.terms[q[3]].value == GTS_GRAPH_IMPORTS
    ]
    import_graph = Graph()
    for source in iter_import_files():
        import_graph.parse(source, format="turtle")
    assert len(imports) == len(to_canonical_graph(import_graph))

    metadata = [
        q
        for q in g.quads
        if q[3] is not None and g.terms[q[3]].value == GTS_GRAPH_METADATA
    ]
    assert metadata

    # the statement layer rides with its reifier machinery intact
    assert g.reifiers and g.annotations


def test_default_graph_loader_excludes_import_subjects() -> None:
    """Term-facing consumers read authored GMEOW, not the gUFO import closure."""
    from gmeow_tools.describe import load_graph_from_gts

    graph = load_graph_from_gts(GTS_SNAPSHOT_FILE)
    assert any(
        str(s).startswith("https://blackcatinformatics.ca/gmeow/") for s, _, _ in graph
    )
    assert not any(str(s).startswith("http://purl.org/nemo/gufo#") for s, _, _ in graph)


def test_self_description_metadata_is_named_graph() -> None:
    """CrossRef metadata remains bundled without polluting the default graph."""
    from gmeow_tools.describe import load_graph_from_gts
    from gmeow_tools.self_desc import load_self_description_from_graph

    metadata = load_graph_from_gts(GTS_SNAPSHOT_FILE, graph_names={GTS_GRAPH_METADATA})
    assert load_self_description_from_graph(metadata).doi

    default = load_graph_from_gts(GTS_SNAPSHOT_FILE)
    with pytest.raises(ValueError, match="self-description"):
        load_self_description_from_graph(default)


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

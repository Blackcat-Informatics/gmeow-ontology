# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
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
from gts import read
from gts.model import TermKind
from gts.wire import iter_items, unwrap_header
from rdflib import RDF, Graph, Literal, URIRef
from rdflib.compare import to_canonical_graph
from rdflib.namespace import RDFS

from gmeow_tools.config import (
    GTS_GRAPH_ALIGNMENTS,
    GTS_GRAPH_IMPORTS,
    GTS_GRAPH_METADATA,
    GTS_GRAPH_STATEMENTS,
    GTS_GRAPH_VERIFY,
    GTS_SNAPSHOT_FILE,
    NAMESPACE,
    PROJECT_ROOT,
    SLICES_DIR,
)
from gmeow_tools.graph import iter_import_files, load_merged_graph
from gmeow_tools.gts_producer import compile_gts
from gmeow_tools.gts_views import load_fold
from gmeow_tools.i18n_catalog import load_po_catalog
from gmeow_tools.mappings import build_alignment_graph, load_mappings

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
def test_committed_snapshot_reproduces_from_sources(fresh_snapshot: bytes) -> None:
    """The committed artifact reproduces from sources — SEMANTICALLY.

    Byte-exactness is intentionally not asserted: the compressed bytes depend
    on the zstd/libzstd build (CI's manylinux wheel vs a local one), so the
    contract is the folded graph, not the exact bytes (see
    GtsSnapshotGenerator.compare).
    """
    from gts import read, to_nquads

    assert GTS_SNAPSHOT_FILE.exists(), "run `gmeow regenerate gts`"
    fresh_fold = sorted(to_nquads(read(fresh_snapshot)).splitlines())
    committed_fold = sorted(
        to_nquads(read(GTS_SNAPSHOT_FILE.read_bytes())).splitlines()
    )
    assert fresh_fold == committed_fold


def test_committed_snapshot_uses_deterministic_gzip_frames() -> None:
    """The committed bundle avoids zstd byte drift across CI/local codecs."""
    data = GTS_SNAPSHOT_FILE.read_bytes()
    frames = _frame_codecs(data)
    target_frames = [
        codecs for frame, codecs in frames if frame in {"snapshot", "blob"}
    ]
    assert target_frames
    assert all(codecs == ["gzip"] for codecs in target_frames)

    items, torn = iter_items(data)
    assert torn is None
    header = unwrap_header(items[0][1])
    raw_catalog = header.get("cat")
    assert isinstance(raw_catalog, Mapping)
    gzip_ids = {
        cid
        for cid, entry in raw_catalog.items()
        if isinstance(cid, int)
        and isinstance(entry, Mapping)
        and entry.get("name") == "gzip"
    }
    assert len(gzip_ids) == 1
    gzip_id = next(iter(gzip_ids))
    for _offset, item in items[1:]:
        assert isinstance(item, dict)
        if item.get("x") == [gzip_id]:
            payload = item.get("d")
            assert isinstance(payload, bytes)
            assert payload[4:8] == b"\x00\x00\x00\x00"


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
        GTS_GRAPH_VERIFY,
    }

    default_quads = sum(1 for q in g.quads if q[3] is None)
    merged_size = len(to_canonical_graph(load_merged_graph(include_imports=False)))

    guide_links = sum(
        1
        for m in SLICES_DIR.glob("*/*/manifest.ttl")
        if (m.parent / "docs.md").exists()
    )
    translation_quads = sum(
        len(load_po_catalog(p))
        for p in sorted(SLICES_DIR.glob("*/*/i18n/*.po"))
        if p.stem != "en"
    )
    # The default graph carries the merged sources PLUS one gmeow:guideBlob
    # linkage per embedded slice guide (#325) PLUS merged non-English PO
    # translations (#572).
    assert default_quads == merged_size + guide_links + translation_quads

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


def test_snapshot_includes_translated_literals() -> None:
    """Non-English PO catalogs are merged into the default graph before encoding."""
    fold = load_fold(GTS_SNAPSHOT_FILE)
    term_tid = fold.tid_of_iri(NAMESPACE + "EntityExistence")
    assert term_tid is not None
    label_langs = {
        fold.lang(o_tid)
        for o_tid in fold.objects(term_tid, str(RDFS.label))
        if fold.is_literal(o_tid)
    }
    assert "x-gmeow-french" in label_langs


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


def test_compare_ignores_encoding_only_drift(tmp_path: Path) -> None:
    """compare() flags semantic source change but tolerates codec/library skew."""
    from gmeow_tools.generator import registry
    from gmeow_tools.gts_gen import GtsSnapshotGenerator  # noqa: F401  (register)

    gen = registry()["gts"]

    committed = tmp_path / "committed.gts"
    committed.write_bytes(compile_gts(_small_graph()))

    # identical fold, different bytes (identity vs zstd) → NOT drift
    fresh = tmp_path / "fresh.gts"
    fresh.write_bytes(compile_gts(_small_graph(), transform=["identity"]))
    assert gen.compare(fresh, committed) == []

    # different content → semantic drift
    other = _small_graph()
    other.add((URIRef(EX + "dog"), RDF.type, URIRef(EX + "Animal")))
    fresh.write_bytes(compile_gts(other))
    [diag] = gen.compare(fresh, committed)
    assert "semantic" in diag

    # equal bytes → clean
    fresh.write_bytes(committed.read_bytes())
    assert gen.compare(fresh, committed) == []


def test_build_verify_attestation_graph_marks_pass_and_fail() -> None:
    """A QualityAssessment per query records pass/fail from the verify report."""
    from gmeow_tools import diagnostics
    from gmeow_tools.gts_gen import build_verify_attestation_graph

    report = diagnostics.report("verify")
    report.add(
        diagnostics.finding(
            severity="error",
            code="verify.bad-one",
            message="row returned",
            tool="verify",
        )
    )
    names = ["queries/verify/bad-one.rq", "queries/verify/good-one.rq"]
    graph = build_verify_attestation_graph(names, report)

    result = URIRef(NAMESPACE + "observationResult")
    bad = URIRef(NAMESPACE + "verify-attestation/bad-one")
    good = URIRef(NAMESPACE + "verify-attestation/good-one")
    assert (bad, result, Literal(False)) in graph
    assert (good, result, Literal(True)) in graph
    # The activity + its agent (reused provenance vocab) are present.
    activity = URIRef(NAMESPACE + "activity/native-verify")
    assert (
        activity,
        URIRef(NAMESPACE + "wasAssociatedWith"),
        URIRef(NAMESPACE + "agent/native-verify"),
    ) in graph


def test_snapshot_carries_verify_attestation() -> None:
    """The committed bundle's gmeow:graph/verify carries the attestations."""
    g = read(GTS_SNAPSHOT_FILE.read_bytes())
    verify_quads = [
        q
        for q in g.quads
        if q[3] is not None and g.terms[q[3]].value == GTS_GRAPH_VERIFY
    ]
    assert verify_quads, "expected a gmeow:graph/verify named graph in the bundle"
    subjects = {g.terms[q[0]].value for q in verify_quads}
    assert any(s is not None and "verify-attestation/" in s for s in subjects)

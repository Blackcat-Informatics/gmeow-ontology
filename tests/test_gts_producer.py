# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the RDF → GTS producer and the gts → {sqlite,duckdb} shims (#271)."""

from __future__ import annotations

import sqlite3
from collections.abc import Mapping
from pathlib import Path

from gts import read, to_nquads
from gts.wire import iter_items, unwrap_header
from rdflib import BNode, Dataset, Graph, Literal, URIRef
from rdflib.namespace import RDFS, XSD

from gmeow_tools.gts_db import to_duckdb, to_sqlite
from gmeow_tools.gts_producer import compile_gts, gts_from_graph

EX = "https://example.org/"


def _sample_graph() -> Graph:
    g = Graph()
    cat = URIRef(EX + "Cat")
    g.add((cat, RDFS.label, Literal("Cat", lang="en")))
    g.add((cat, URIRef(EX + "legs"), Literal("4", datatype=XSD.integer)))
    g.add((cat, RDFS.comment, Literal("a plain comment")))
    b = BNode()
    g.add((cat, URIRef(EX + "sample"), b))
    g.add((b, RDFS.label, Literal("a sample", lang="en")))
    return g


def _reparse(nq: str, *, dataset: bool = False) -> Graph:
    fmt = "nquads" if dataset else "nt"
    target: Graph = Dataset() if dataset else Graph()
    target.parse(data=nq, format=fmt)
    return target


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


def _reps(folded: object) -> dict[str, bytes]:
    """Map each blob's ``rep`` to its decoded payload in a folded bundle."""
    out: dict[str, bytes] = {}
    for digest, meta in folded.blob_meta.items():  # type: ignore[attr-defined]
        rep = meta.get("rep")
        payload = folded.blobs.get(digest)  # type: ignore[attr-defined]
        if rep is not None and payload is not None:
            out[str(rep)] = payload
    return out


def test_report_blobs_are_additive_and_do_not_perturb_the_graph() -> None:
    """Embedding report blobs leaves the snapshot graph byte-identical (#654)."""
    source = _sample_graph()
    sarif = b'{"version":"2.1.0"}'
    rdf = b"<https://ex/f> <https://ex/p> <https://ex/o> <https://ex/g> .\n"

    base = compile_gts(source)
    with_report = compile_gts(
        source,
        report_blobs=[
            (sarif, "application/sarif+json", "gmeow:report/sarif"),
            (rdf, "application/n-quads", "gmeow:report/rdf"),
        ],
    )

    # The folded graph is identical with and without the report — purely additive.
    assert to_nquads(read(base)) == to_nquads(read(with_report))

    # The report payloads are retrievable by rep from the embedded bundle.
    reps = _reps(read(with_report))
    assert reps["gmeow:report/sarif"] == sarif
    assert reps["gmeow:report/rdf"] == rdf
    # The base bundle carries no report blobs.
    assert "gmeow:report/sarif" not in _reps(read(base))


def test_s3_slice_artifacts_recoverable_repo_free() -> None:
    """The S3 self-describing bundle: per-slice ontology artifacts are recoverable
    from the serialized bundle ALONE (no slices/ tree), by role + logical path +
    content digest, with EXACT bytes — and unknown manifest triples plus literal
    lang/datatype identity survive the round-trip (#820 S3, gap G4).
    """
    from blake3 import blake3

    # A manifest carrying an UNKNOWN (non-well-known) triple plus a lang-tagged and
    # a typed literal, to prove identity survival through the bundle round-trip.
    manifest_bytes = (
        b"@prefix ex: <https://example.org/> .\n"
        b"@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n"
        b"<https://example.org/slice/sample> "
        b'ex:unknownProp "weird value"@x-gmeow-english ;\n'
        b'    ex:answer "42"^^xsd:integer .\n'
    )
    module_bytes = b"@prefix ex: <https://example.org/> .\nex:Cat a ex:Class .\n"
    shapes_bytes = b"@prefix sh: <http://www.w3.org/ns/shacl#> .\n# shapes\n"

    rows = [
        (
            "https://example.org/slice/sample",
            "Sample slice",
            "Module",
            "slices/core/sample/module.ttl",
            module_bytes,
        ),
        (
            "https://example.org/slice/sample",
            "Sample slice",
            "Shapes",
            "slices/core/sample/shapes.ttl",
            shapes_bytes,
        ),
        (
            "https://example.org/slice/sample",
            "Sample slice",
            "Manifest",
            "slices/core/sample/manifest.ttl",
            manifest_bytes,
        ),
    ]

    data = compile_gts(_sample_graph(), slice_artifacts=rows)

    # Repo-free: parse ONLY the serialized bytes; we never touch the source rows
    # for recovery, only to assert exact equality.
    folded = read(data)
    reps = _reps(folded)

    for _slice_iri, _name, role, logical_path, content in rows:
        rep = f"slice-artifact:{role}:{logical_path}"
        assert rep in reps, f"artifact {logical_path!r} recoverable by role + path"
        recovered = reps[rep]
        # Exact bytes.
        assert recovered == content, f"exact bytes for {logical_path!r}"
        # Digest matches: the bundle blob meta carries a blake3 content digest.
        digest = next(
            meta["digest"]
            for meta in folded.blob_meta.values()
            if meta.get("rep") == rep
        )
        assert digest == "blake3:" + blake3(content).hexdigest()

    # Unknown manifest triples + literal lang/datatype identity survive: reparse
    # the recovered manifest and check the exact terms came through unchanged.
    recovered_manifest = reps["slice-artifact:Manifest:slices/core/sample/manifest.ttl"]
    g = Graph()
    g.parse(data=recovered_manifest.decode("utf-8"), format="turtle")
    subj = URIRef("https://example.org/slice/sample")
    unknown = list(g.objects(subj, URIRef("https://example.org/unknownProp")))
    assert len(unknown) == 1
    assert isinstance(unknown[0], Literal)
    assert str(unknown[0]) == "weird value"
    assert unknown[0].language == "x-gmeow-english"
    answer = list(g.objects(subj, URIRef("https://example.org/answer")))
    assert len(answer) == 1
    assert isinstance(answer[0], Literal)
    assert answer[0].datatype == XSD.integer
    assert str(answer[0]) == "42"

    # The snapshot graph identity is unperturbed by the S3 artifact blobs (purely
    # additive, exactly like report/doc blobs).
    assert to_nquads(read(data)) == to_nquads(read(compile_gts(_sample_graph())))


def test_snapshot_content_id_is_stable_and_blob_independent() -> None:
    """The self-attestation content id is a pure function of the snapshot (#654)."""
    from gmeow_tools.gts_producer import _Builder

    def _named_graph() -> Graph:
        # No blank nodes: real usage canonicalizes first, so a bnode-free graph
        # keeps the content id stable without to_canonical_graph here.
        g = Graph()
        g.add((URIRef(EX + "Cat"), RDFS.label, Literal("Cat", lang="en")))
        return g

    def _cid() -> str:
        builder = _Builder()
        builder.add_graph(_named_graph())
        return builder.snapshot_content_id()

    cid = _cid()
    assert cid.startswith("blake3:")
    assert cid == _cid()


def test_signed_bundle_carries_the_report_under_the_signature() -> None:
    """A signed feedback bundle still carries the report (tamper-evident, #654)."""
    import base64

    import gts

    signer = gts.Signer.generate("gmeow-feedback-test")
    armor = base64.b64encode(signer.public_raw).decode("ascii")
    sarif = b'{"version":"2.1.0"}'

    signed = compile_gts(
        _sample_graph(),
        report_blobs=[(sarif, "application/sarif+json", "gmeow:report/sarif")],
        signer=signer,
        public_key_armor=armor,
    )

    folded = read(signed)
    assert [d.code for d in folded.diagnostics] == []
    assert _reps(folded)["gmeow:report/sarif"] == sarif


def test_producer_round_trip_isomorphic() -> None:
    """RDF → GTS → fold → N-Quads → RDF reproduces an isomorphic graph."""
    source = _sample_graph()
    data = gts_from_graph(source)
    folded = read(data)
    assert [d.code for d in folded.diagnostics] == []
    back = _reparse(to_nquads(folded))
    assert source.isomorphic(back)


def test_producer_default_compresses() -> None:
    """The default snapshot uses zstd (a transformed frame) and still folds clean."""
    data = gts_from_graph(_sample_graph())
    # the self-describe magic + a snapshot frame; folds without diagnostics
    assert read(data).diagnostics == []


def test_large_frames_use_zstd_rsyncable() -> None:
    """Large GTS frames use rsyncable zstd blocks for git-friendly deltas."""
    large_blob = b"0123456789abcdef" * 5000
    data = compile_gts(
        _sample_graph(),
        doc_blobs=[(large_blob, "text/plain", "test:large")],
    )
    frames = _frame_codecs(data)
    assert ("blob", ["zstd-rsyncable"]) in frames

    snapshot_codecs = [
        codecs for frame_type, codecs in frames if frame_type == "snapshot"
    ]
    assert snapshot_codecs == [["zstd"]]


def test_rsyncable_threshold_only_rewrites_default_zstd() -> None:
    """The threshold switches default zstd frames without overriding explicit codecs."""
    rsyncable = compile_gts(_sample_graph(), rsyncable_threshold=1)
    frames = _frame_codecs(rsyncable)
    assert frames[-1] == ("snapshot", ["zstd-rsyncable"])

    explicit = compile_gts(
        _sample_graph(), transform=["identity"], rsyncable_threshold=1
    )
    frames = _frame_codecs(explicit)
    assert frames[-1] == ("snapshot", ["identity"])


def test_producer_named_graphs() -> None:
    """A Dataset round-trips its named-graph quads."""
    ds = Dataset()
    g1 = ds.graph(URIRef(EX + "g1"))
    g1.add((URIRef(EX + "s"), URIRef(EX + "p"), URIRef(EX + "o")))
    data = gts_from_graph(ds)
    folded = read(data)
    assert len(folded.quads) == 1
    gname = folded.quads[0][3]
    assert gname is not None
    assert folded.term(gname).value == EX + "g1"


def test_to_sqlite(tmp_path: Path) -> None:
    """gts → sqlite loads the dictionary-encoded tables with the right cardinalities."""
    folded = read(gts_from_graph(_sample_graph()))
    db = to_sqlite(folded, tmp_path / "out.db")
    conn = sqlite3.connect(db)
    try:
        n_terms = (conn.execute("SELECT count(*) FROM terms").fetchone() or (0,))[0]
        n_quads = (conn.execute("SELECT count(*) FROM quads").fetchone() or (0,))[0]
        # a resolving join works: every quad subject resolves to a term row
        joined = (
            conn.execute(
                "SELECT count(*) FROM quads q JOIN terms t ON q.s = t.id"
            ).fetchone()
            or (0,)
        )[0]
    finally:
        conn.close()
    assert n_terms == len(folded.terms)
    assert n_quads == len(folded.quads)
    assert joined == len(folded.quads)


def test_to_duckdb(tmp_path: Path) -> None:
    """gts → duckdb loads and a resolving join returns the source labels."""
    import duckdb

    folded = read(gts_from_graph(_sample_graph()))
    db = to_duckdb(folded, tmp_path / "out.duckdb")
    conn = duckdb.connect(str(db))
    try:
        n_quads = (conn.execute("SELECT count(*) FROM quads").fetchone() or (0,))[0]
        labels = conn.execute(
            "SELECT t.lex FROM quads q "
            "JOIN terms p ON q.p = p.id "
            "JOIN terms t ON q.o = t.id "
            "WHERE p.lex = ? AND t.lang = 'en'",
            [str(RDFS.label)],
        ).fetchall()
    finally:
        conn.close()
    assert n_quads == len(folded.quads)
    assert ("Cat",) in labels


def test_producer_default_graph_is_unnamed() -> None:
    """Default-graph triples export with a None graph name (not the default id)."""
    ds = Dataset()
    # default_graph.add (not the deprecated 3-tuple Dataset.add)
    ds.default_graph.add((URIRef(EX + "s"), URIRef(EX + "p"), URIRef(EX + "o")))
    folded = read(gts_from_graph(ds))
    assert len(folded.quads) == 1
    assert folded.quads[0][3] is None  # default graph, not a spurious named graph


def test_rdf12_producer_reifier_and_annotation(tmp_path: Path) -> None:
    """The RDF 1.2 path (gmeow_rdf) ingests reifier triple-terms + annotations."""
    from gmeow_tools.gts_producer import gts_from_rdf12

    ttl = (
        "@prefix g: <https://example.org/> .\n"
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n"
        "g:alice g:knows g:bob .\n"
        "g:r1 rdf:reifies <<( g:alice g:knows g:bob )>> ; g:confidence 0.9 .\n"
    )
    src = tmp_path / "stmt.ttl"
    src.write_text(ttl, encoding="utf-8")
    g = read(gts_from_rdf12(src))
    assert [d.code for d in g.diagnostics] == []
    assert len(g.reifiers) == 1  # reifier bound to the quoted triple
    assert len(g.annotations) == 1  # the g:confidence statement metadata
    _reifier, pred_id, value_id = g.annotations[0]
    assert g.term(pred_id).value == "https://example.org/confidence"
    assert g.term(value_id).value == "0.9"


def test_compile_gts_missing_rdf12_raises(tmp_path: Path) -> None:
    """compile_gts errors on an explicitly-provided but missing RDF 1.2 path."""
    import pytest

    from gmeow_tools.gts_producer import compile_gts

    with pytest.raises(FileNotFoundError):
        compile_gts(_sample_graph(), tmp_path / "does-not-exist.ttl")


def test_to_nquads_lang_map_remaps_tags() -> None:
    """The renderer's lang_map remaps tags on OUTPUT; the graph is untouched."""
    folded = read(gts_from_graph(_sample_graph()))
    mapped = to_nquads(folded, {"en": "en-CA"})
    assert "@en-CA" in mapped
    assert "@en ." not in mapped and "@en\n" not in mapped
    # unmapped pass-through + the stored graph keeps its original tags
    assert "@en" in to_nquads(folded, {"fr": "fr-CA"})
    assert any(t.lang == "en" for t in folded.terms)

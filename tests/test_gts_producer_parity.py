# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Parity gate for the native ``RDF → GTS`` producer cutover (#819 Task 8).

The byte-emitting core of ``gmeow_tools.gts_producer`` moved from Python to Rust.
The two encoders are NOT byte-identical: the snapshot frame's CBOR PAYLOAD is
byte-identical, but the zstd codecs differ (Python ``zstandard`` vs the Rust
``zstd`` crate — the documented codec-skew). The gate is therefore SEMANTIC-FOLD
equivalence, proven two ways for each case:

1. the UNCOMPRESSED snapshot payload is byte-identical (the strongest claim —
   identical terms/quads/reifies/annot in identical order), and
2. the folded graph (``gts.read`` → ``to_nquads``) is identical.

The pre-cutover Python encoder is captured verbatim in ``tests/_gts_producer_legacy``
and used as the oracle.
"""

from __future__ import annotations

import importlib.util
import io
from pathlib import Path

import cbor2
import zstandard
from gts import Signer, read, to_nquads
from rdflib import BNode, Dataset, Graph, Literal, URIRef
from rdflib.namespace import RDFS, XSD

from gmeow_tools import gts_producer as native

# Load the frozen pre-#819 Python encoder as the parity oracle.
_LEGACY_PATH = Path(__file__).with_name("_gts_producer_legacy.py")
_spec = importlib.util.spec_from_file_location("_gts_producer_legacy", _LEGACY_PATH)
assert _spec is not None and _spec.loader is not None
legacy = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(legacy)

EX = "https://example.org/"


def _iter_frames(data: bytes) -> list[object]:
    buf = io.BytesIO(data)
    decoder = cbor2.CBORDecoder(buf)
    out: list[object] = []
    while buf.tell() < len(data):
        out.append(decoder.decode())
    return out


def _snapshot_payload(data: bytes) -> bytes:
    """The decompressed snapshot-frame CBOR payload bytes."""
    for frame in _iter_frames(data):
        value = frame.value if isinstance(frame, cbor2.CBORTag) else frame
        if isinstance(value, dict) and value.get("t") == "snapshot":
            raw: bytes = value["d"]
            if "x" in value:  # transformed (zstd / zstd-rsyncable) — decompress
                decompressor = zstandard.ZstdDecompressor()
                raw = bytes(decompressor.decompress(raw, max_output_size=1 << 24))
            return raw
    raise AssertionError("no snapshot frame found")


def _frame_types(data: bytes) -> list[str]:
    """The ``t`` type of each non-header frame, in order."""
    out: list[str] = []
    for frame in _iter_frames(data):
        value = frame.value if isinstance(frame, cbor2.CBORTag) else frame
        if isinstance(value, dict) and "t" in value:
            out.append(str(value["t"]))
    return out


def _blob_digests(data: bytes) -> set[str]:
    """The set of blob-frame content digests (rep+digest), codec-independent."""
    out: set[str] = set()
    for frame in _iter_frames(data):
        value = frame.value if isinstance(frame, cbor2.CBORTag) else frame
        if isinstance(value, dict) and value.get("t") == "blob":
            pub = value.get("pub", {})
            if isinstance(pub, dict):
                out.add(f"{pub.get('rep')}::{pub.get('digest')}")
    return out


def _fold(data: bytes) -> list[str]:
    return sorted(to_nquads(read(data)).splitlines())


def _assert_fold_equivalent(native_bytes: bytes, legacy_bytes: bytes) -> None:
    # The strongest claim: the uncompressed snapshot payloads are byte-identical.
    assert _snapshot_payload(native_bytes) == _snapshot_payload(legacy_bytes)
    # And the folded graphs are identical.
    assert _fold(native_bytes) == _fold(legacy_bytes)
    # Blob frames (doc/report) carry by content address, codec-independent.
    assert _blob_digests(native_bytes) == _blob_digests(legacy_bytes)


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


def test_plain_graph_parity() -> None:
    g = _sample_graph()
    _assert_fold_equivalent(native.gts_from_graph(g), legacy.gts_from_graph(g))


def test_dataset_named_graph_parity() -> None:
    ds = Dataset()
    g = ds.graph(URIRef(EX + "g1"))
    g.add((URIRef(EX + "s"), URIRef(EX + "p"), Literal("v", lang="en")))
    ds.add((URIRef(EX + "s2"), URIRef(EX + "p2"), URIRef(EX + "o2")))
    _assert_fold_equivalent(native.gts_from_graph(ds), legacy.gts_from_graph(ds))


def test_compile_gts_with_rdf12_parity(tmp_path: Path) -> None:
    g = _sample_graph()
    rdf12 = tmp_path / "stmt.ttl"
    rdf12.write_text(
        "<https://example.org/r> "
        "<http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> "
        "<<( <https://example.org/Cat> "
        "<http://www.w3.org/2000/01/rdf-schema#label> "
        '"Cat"@en )>> .\n'
        "<https://example.org/r> <https://example.org/confidence> "
        '"0.9"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n',
        encoding="utf-8",
    )
    _assert_fold_equivalent(
        native.compile_gts(g, rdf12),
        legacy.compile_gts(g, rdf12),
    )


def test_compile_gts_full_surface_parity(tmp_path: Path) -> None:
    g = _sample_graph()
    align = Graph()
    align.add((URIRef(EX + "Cat"), URIRef(EX + "exactMatch"), URIRef("http://wd/Q146")))
    extra = Graph()
    extra.add((URIRef(EX + "Cat"), URIRef(EX + "note"), Literal("extra")))
    doc_blobs = [(b"# guide\n", "text/markdown", "gmeow:doc/guide")]
    report_blobs = [(b'{"ok":true}', "application/json", "gmeow:report/findings")]
    kwargs: dict[str, object] = {
        "alignment_graph": align,
        "extra_named_graphs": [(extra, "https://example.org/graph/extra", "extra")],
        "transform": ["zstd"],
        "doc_blobs": doc_blobs,
        "report_blobs": report_blobs,
    }
    _assert_fold_equivalent(
        native.compile_gts(g, **kwargs),  # type: ignore[arg-type]
        legacy.compile_gts(g, **kwargs),
    )


def test_signed_bundle_parity() -> None:
    # A fixed Ed25519 signer (deterministic raw key) so the signature bytes are
    # reproducible; the public "armor" is an opaque transport-key string here.
    signer = Signer.generate("test-kid")
    public_key_armor = "-----BEGIN PGP PUBLIC KEY BLOCK-----\nfake\n-----END-----\n"
    g = _sample_graph()
    native_bytes = native.compile_gts(
        g, signer=signer, public_key_armor=public_key_armor
    )
    legacy_bytes = legacy.compile_gts(
        g, signer=signer, public_key_armor=public_key_armor
    )
    _assert_fold_equivalent(native_bytes, legacy_bytes)
    # The signed bundle carries a transport-key meta frame; both encoders emit it.
    # (The frame ids — and thus the signatures — differ from legacy because the
    # zstd codec differs; verify the native signatures against the signer's key
    # instead of asserting cross-encoder byte-identity.)
    assert _frame_types(native_bytes).count("meta") == 1
    assert _frame_types(legacy_bytes).count("meta") == 1
    _assert_signatures_verify(native_bytes, signer)


def _assert_signatures_verify(data: bytes, signer: Signer) -> None:
    """Every signed frame's COSE_Sign1 verifies against ``signer``'s public key.

    Reproduces ``gts.crypto`` verification minimally: the detached payload is the
    frame ``id``, wrapped in the COSE ``Signature1`` structure. This proves the
    native raw-Ed25519 signing path (``Writer::sign_with``) matches
    ``gts.crypto.sign_id`` semantically, independent of the zstd codec skew.
    """
    public_key = signer.key.public_key()
    signed = 0
    for frame in _iter_frames(data):
        value = frame.value if isinstance(frame, cbor2.CBORTag) else frame
        if not isinstance(value, dict) or "sig" not in value:
            continue
        signed += 1
        cose = cbor2.loads(value["sig"])
        protected, _unprotected, _payload, signature = cose.value
        sig_structure = cbor2.dumps(
            ["Signature1", protected, b"", value["id"]], canonical=True
        )
        public_key.verify(signature, sig_structure)  # raises on a bad signature
    assert signed >= 1, "signed bundle must carry at least one signed frame"


def test_signer_xor_armor_rejected() -> None:
    import pytest

    g = _sample_graph()
    with pytest.raises(ValueError):
        native.compile_gts(g, signer=Signer.generate("k"))
    with pytest.raises(ValueError):
        native.compile_gts(g, public_key_armor="x")


def test_snapshot_content_id_parity() -> None:
    g = _sample_graph()
    nb = native._Builder()
    nb.add_graph(g)
    lb = legacy._Builder()
    lb.add_graph(g)
    assert nb.snapshot_content_id() == lb.snapshot_content_id()

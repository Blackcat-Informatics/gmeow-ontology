# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Conformance vectors for the GTS reference implementation (§18 of GTS-SPEC.md).

Each test builds a GTS file with the :class:`Writer` (or hand-crafts edge-case
frames) and asserts the folded :class:`Graph`, its diagnostics, and the
``gts → nquads`` output.
"""

from __future__ import annotations

import cbor2

from gmeow_tools.gts import Term, TermKind, Writer, read, to_nquads
from gmeow_tools.gts.codec import Codec
from gmeow_tools.gts.model import RDF_LANG_STRING, XSD_STRING
from gmeow_tools.gts.wire import canonical, content_id, header_id

CAT = "https://example.org/Cat"
LABEL = "http://www.w3.org/2000/01/rdf-schema#label"


def _diag_codes(graph: object) -> list[str]:
    return [d.code for d in graph.diagnostics]  # type: ignore[attr-defined]


# -- Vector 1: minimal valid file --------------------------------------------


def test_vector_01_minimal() -> None:
    w = Writer(profile="dist")
    w.add_terms(
        [
            Term(TermKind.IRI, CAT),
            Term(TermKind.IRI, LABEL),
            Term(TermKind.LITERAL, "Cat", lang="en"),
        ]
    )
    w.add_quads([(0, 1, 2, None)])
    g = read(w.to_bytes())
    assert _diag_codes(g) == []
    assert g.quads == [(0, 1, 2, None)]
    assert g.term(0).value == CAT
    assert to_nquads(g) == f'<{CAT}> <{LABEL}> "Cat"@en .\n'


# -- Vector 2: zstd-transformed frame ----------------------------------------


def test_vector_02_zstd_frame() -> None:
    w = Writer()
    w.add_terms(
        [Term(TermKind.IRI, CAT), Term(TermKind.IRI, LABEL), Term(TermKind.IRI, CAT)]
    )
    w.add_quads([(0, 1, 2, None)], transform=["zstd"])
    g = read(w.to_bytes())
    assert _diag_codes(g) == []
    assert g.quads == [(0, 1, 2, None)]


def test_gzip_frame() -> None:
    w = Writer()
    w.add_terms(
        [Term(TermKind.IRI, CAT), Term(TermKind.IRI, LABEL), Term(TermKind.IRI, CAT)]
    )
    w.add_quads([(0, 1, 2, None)], transform=["gzip"])
    g = read(w.to_bytes())
    assert _diag_codes(g) == []
    assert g.quads == [(0, 1, 2, None)]


# -- Vector 3: unknown codec -> opaque ---------------------------------------


def test_vector_03_unknown_codec() -> None:
    w = Writer(catalog={0: Codec("identity", "encode"), 9: Codec("brotli", "compress")})
    frame = {"t": "quads", "x": [9], "d": b"\x00\x01\x02", "prev": w.head}
    frame["id"] = content_id(frame)
    g = read(w.to_bytes() + canonical(frame))
    assert "UnknownCodec" in _diag_codes(g)
    assert g.opaque and g.opaque[0].reason == "unknown-codec"


def test_encrypt_codec_missing_key() -> None:
    w = Writer(
        catalog={0: Codec("identity", "encode"), 7: Codec("cose-encrypt", "encrypt")}
    )
    frame = {"t": "annot", "x": [7], "d": b"sealed", "prev": w.head}
    frame["id"] = content_id(frame)
    g = read(w.to_bytes() + canonical(frame))
    assert "MissingKey" in _diag_codes(g)
    assert g.opaque[0].reason == "missing-key"


# -- Vector 4: damaged frame (self-id mismatch) ------------------------------


def test_vector_04_damaged_frame() -> None:
    w = Writer()
    frame = {"t": "meta", "d": {"k": 1}, "prev": w.head, "id": b"\x00" * 32}
    g = read(w.to_bytes() + canonical(frame))
    assert "DamagedFrame" in _diag_codes(g)
    assert g.opaque and g.opaque[0].reason == "damaged"


# -- Vector 5: torn append ----------------------------------------------------


def test_vector_05_torn_append() -> None:
    w = Writer()
    w.add_terms(
        [Term(TermKind.IRI, CAT), Term(TermKind.IRI, LABEL), Term(TermKind.IRI, CAT)]
    )
    w.add_quads([(0, 1, 2, None)])
    data = w.to_bytes() + b"\xa3"  # announces a 3-entry map, no contents
    g = read(data)
    assert "TornAppendError" in _diag_codes(g)
    assert g.quads == [(0, 1, 2, None)]  # survivors intact


# -- Vector 6: header self-hash ----------------------------------------------


def test_vector_06_header_hash_ok() -> None:
    w = Writer()
    g = read(w.to_bytes())
    assert "DamagedFrame" not in _diag_codes(g)


def test_vector_06_header_hash_tampered() -> None:
    header: dict[str, object] = {
        "gts": "GTS1",
        "v": 1,
        "prof": "generic",
        "cat": {0: {"name": "identity", "cls": "encode"}},
    }
    header["id"] = header_id(header)
    header["prof"] = "tampered"  # change content after fixing the id
    data = canonical(cbor2.CBORTag(55799, header))
    g = read(data)
    assert "DamagedFrame" in _diag_codes(g)


# -- Vector 9: suppression ----------------------------------------------------


def test_vector_09_suppression() -> None:
    w = Writer()
    w.add_terms([Term(TermKind.IRI, CAT)])
    w.add_suppress([{"kind": "term", "id": 0}], reason="retracted")
    g = read(w.to_bytes())
    assert _diag_codes(g) == []
    assert g.suppressions and g.suppressions[0].targets[0]["kind"] == "term"


# -- Vector 11: literal datatype defaulting ----------------------------------


def test_vector_11_datatype_defaulting() -> None:
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.LITERAL, "hi", lang="en"),  # 0 -> langString
            Term(TermKind.LITERAL, "plain"),  # 1 -> xsd:string
            Term(TermKind.IRI, "http://www.w3.org/2001/XMLSchema#integer"),  # 2
            Term(TermKind.LITERAL, "42", datatype=2),  # 3 -> explicit
        ]
    )
    g = read(w.to_bytes())
    assert g.datatype_iri(g.term(0)) == RDF_LANG_STRING
    assert g.datatype_iri(g.term(1)) == XSD_STRING
    assert g.datatype_iri(g.term(3)) == "http://www.w3.org/2001/XMLSchema#integer"


# -- Vector 12: conflicting reifier ------------------------------------------


def test_vector_12_conflicting_reifier() -> None:
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.BNODE, "r"),  # 0 reifier
            Term(TermKind.IRI, "https://example.org/s"),  # 1
            Term(TermKind.IRI, "https://example.org/p"),  # 2
            Term(TermKind.IRI, "https://example.org/o"),  # 3
            Term(TermKind.IRI, "https://example.org/o2"),  # 4
        ]
    )
    w.add_reifies({0: (1, 2, 3)})
    w.add_reifies({0: (1, 2, 4)})  # conflict
    g = read(w.to_bytes())
    assert "ConflictingReifier" in _diag_codes(g)
    assert g.reifiers[0] == (1, 2, 3)  # first binding kept


# -- Vector 13: position-constraint violation --------------------------------


def test_vector_13_position_constraint() -> None:
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.IRI, "https://example.org/s"),  # 0
            Term(TermKind.LITERAL, "not-a-predicate"),  # 1 (literal as predicate)
            Term(TermKind.IRI, "https://example.org/o"),  # 2
        ]
    )
    w.add_quads([(0, 1, 2, None)])
    g = read(w.to_bytes())
    assert "PositionConstraint" in _diag_codes(g)
    assert g.quads == []  # offending quad rejected


# -- Vector 14: blank-node label (locality) ----------------------------------


def test_vector_14_bnode_label() -> None:
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.BNODE, "x"),  # 0
            Term(TermKind.IRI, "https://example.org/p"),  # 1
            Term(TermKind.BNODE, "x"),  # 2 (same label, distinct id — file-local)
        ]
    )
    w.add_quads([(0, 1, 2, None)])
    g = read(w.to_bytes())
    assert _diag_codes(g) == []
    assert to_nquads(g) == "_:x <https://example.org/p> _:x .\n"


# -- inline blob + content addressing ----------------------------------------


def test_inline_blob_digest() -> None:
    w = Writer(profile="image")
    payload = b"\x89PNG\r\n\x1a\n fake image bytes"
    w.add_blob(payload, mt="image/png")
    g = read(w.to_bytes())
    assert _diag_codes(g) == []
    from gmeow_tools.gts.wire import digest_str

    assert g.blobs[digest_str(payload)] == payload


# -- snapshot fold ------------------------------------------------------------


def test_snapshot_fold() -> None:
    w = Writer(profile="dist")
    snap = {
        "terms": [
            {"k": 0, "v": CAT},
            {"k": 0, "v": LABEL},
            {"k": 1, "v": "Cat", "l": "en"},
        ],
        "quads": [[0, 1, 2]],
    }
    w.add_frame("snapshot", payload=snap)
    g = read(w.to_bytes())
    assert _diag_codes(g) == []
    assert g.quads == [(0, 1, 2, None)]
    assert to_nquads(g) == f'<{CAT}> <{LABEL}> "Cat"@en .\n'

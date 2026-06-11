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


# -- robustness: the reader is TOTAL (never raises on adversarial bytes) ------


def test_robust_corrupt_compressed_payload() -> None:
    """A frame claiming zstd but carrying garbage folds to a damaged opaque node."""
    w = Writer()
    frame = {"t": "quads", "x": [2], "d": b"not zstd at all", "prev": w.head}
    frame["id"] = content_id(frame)
    g = read(w.to_bytes() + canonical(frame))
    assert "DamagedFrame" in _diag_codes(g)
    assert g.opaque and g.opaque[0].reason == "damaged"


def test_robust_out_of_bounds_quad() -> None:
    """A quad referencing a non-existent term id is rejected, and to_nquads is safe."""
    w = Writer()
    w.add_terms([Term(TermKind.IRI, "https://example.org/s")])  # only id 0 exists
    w.add_quads([(0, 5, 9, None)])  # 5 and 9 out of bounds
    g = read(w.to_bytes())
    assert "PositionConstraint" in _diag_codes(g)
    assert g.quads == []
    assert to_nquads(g) == ""  # no IndexError


def test_robust_non_integer_ids() -> None:
    """Non-integer term ids in a quad row are diagnosed, not crashed."""
    w = Writer()
    w.add_terms([Term(TermKind.IRI, "https://example.org/s")])
    w.add_frame("quads", payload=[["a", "b", "c"]])
    g = read(w.to_bytes())
    assert "DamagedFrame" in _diag_codes(g)
    assert g.quads == []


def test_robust_forward_datatype_ref() -> None:
    """A literal whose datatype ref is a forward/out-of-range id is dropped safely."""
    w = Writer()
    w.add_terms([Term(TermKind.LITERAL, "42", datatype=99)])  # 99 does not exist
    g = read(w.to_bytes())
    assert "ForwardReference" in _diag_codes(g)
    # ref dropped -> defaults to xsd:string, and rendering never IndexErrors
    assert g.datatype_iri(g.term(0)) == XSD_STRING


def test_robust_unknown_term_kind() -> None:
    """An out-of-range term-kind int defaults to IRI rather than raising."""
    w = Writer()
    w.add_frame("terms", payload=[{"k": 99, "v": "https://example.org/x"}])
    g = read(w.to_bytes())
    assert g.term(0).kind is TermKind.IRI


def test_robust_invalid_header() -> None:
    """A non-map header yields a diagnostic and an (empty) graph, never a crash."""
    g = read(canonical([1, 2, 3]))  # first item is an array, not a header map
    assert "DamagedFrame" in _diag_codes(g)
    assert g.quads == []


def test_robust_out_of_bounds_snapshot() -> None:
    """A snapshot quad with an out-of-range id is diagnosed, not crashed."""
    w = Writer(profile="dist")
    w.add_frame(
        "snapshot",
        payload={"terms": [{"k": 0, "v": "https://ex/a"}], "quads": [[0, 7, 0]]},
    )
    g = read(w.to_bytes())
    assert "PositionConstraint" in _diag_codes(g)
    assert g.quads == []  # rejected, not crashed


def test_writer_rejects_ambiguous_payload() -> None:
    """add_frame rejects both-sources and transform-without-source."""
    import pytest

    w = Writer()
    with pytest.raises(ValueError, match="mutually exclusive"):
        w.add_frame("meta", payload={"a": 1}, raw=b"x")
    with pytest.raises(ValueError, match="requires a payload"):
        w.add_frame("meta", transform=["zstd"])


def test_nquads_escapes_control_chars() -> None:
    """A literal containing control bytes serialises to escaped N-Quads."""
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.IRI, "https://ex/s"),
            Term(TermKind.IRI, "https://ex/p"),
            Term(TermKind.LITERAL, "a\x00b\x07c"),  # NUL + BEL
        ]
    )
    w.add_quads([(0, 1, 2, None)])
    g = read(w.to_bytes())
    assert '"a\\u0000b\\u0007c"' in to_nquads(g)


def test_corrupt_trailing_item_is_torn() -> None:
    """A malformed (not merely truncated) trailing CBOR item is treated as torn."""
    w = Writer()
    w.add_terms([Term(TermKind.IRI, "https://ex/s")])
    data = w.to_bytes() + b"\x1c"  # reserved additional-info -> ill-formed CBOR
    g = read(data)
    assert "TornAppendError" in _diag_codes(g)
    assert len(g.terms) == 1  # survivors intact


# -- Vectors 15-19: multi-segment composition (§3.1, GTS-SPEC v0.3) ----------

DOG = "https://example.org/Dog"


def _segment_one() -> bytes:
    w = Writer(profile="dist")
    w.add_terms(
        [
            Term(TermKind.IRI, CAT),  # 0
            Term(TermKind.IRI, LABEL),  # 1
            Term(TermKind.LITERAL, "Cat", lang="en"),  # 2
            Term(TermKind.BNODE, "b0"),  # 3
        ]
    )
    w.add_quads([(0, 1, 2, None), (3, 1, 2, None)])
    return bytes(w.to_bytes())


def _segment_two() -> bytes:
    # DELIBERATELY reuses the same numeric ids for different values, shares
    # the LABEL IRI by value, and reuses the bnode label "b0".
    w = Writer(profile="music")
    w.add_terms(
        [
            Term(TermKind.IRI, DOG),  # 0 (was CAT in segment one)
            Term(TermKind.IRI, LABEL),  # 1 (same IRI -> must unify)
            Term(TermKind.LITERAL, "Dog", lang="en"),  # 2
            Term(TermKind.BNODE, "b0"),  # 3 (same label -> must NOT unify)
        ]
    )
    w.add_quads([(0, 1, 2, None), (3, 1, 2, None)])
    return bytes(w.to_bytes())


def test_vector_15_two_segment_union() -> None:
    g = read(_segment_one() + _segment_two())
    assert _diag_codes(g) == []
    assert len(g.segment_heads) == 2
    assert g.segment_profiles == ["dist", "music"]
    values = {
        g.term(s).value for s, _, _, _ in g.quads if g.term(s).kind is TermKind.IRI
    }
    assert values == {CAT, DOG}  # ids resolved per segment, never globally
    # LABEL unified by value: exactly one IRI term carries it.
    label_ids = [i for i, t in enumerate(g.terms) if t.value == LABEL]
    assert len(label_ids) == 1
    # Blank labels stay segment-local: two distinct bnode terms named "b0".
    bnodes = [t for t in g.terms if t.kind is TermKind.BNODE]
    assert len(bnodes) == 2
    assert len(g.quads) == 4


def test_vector_16_composed_round_trip() -> None:
    g = read(_segment_one() + _segment_two())
    nq = to_nquads(g)
    assert f'<{CAT}> <{LABEL}> "Cat"@en .' in nq
    assert f'<{DOG}> <{LABEL}> "Dog"@en .' in nq


def test_vector_17_pre_segment_reader_hard_fails() -> None:
    g = read(_segment_one() + _segment_two(), allow_segments=False)
    assert "SegmentBoundary" in _diag_codes(g)
    # Nothing past the boundary folded — the forbidden outcome is misfolding.
    values = {
        g.term(s).value for s, _, _, _ in g.quads if g.term(s).kind is TermKind.IRI
    }
    assert DOG not in values
    assert CAT in values


def test_vector_18_cross_segment_suppression() -> None:
    seg1 = _segment_one()
    # Segment two suppresses segment one's Cat-label quad BY VALUE: it mints
    # its OWN ids for the same terms and issues a quad-kind suppress target.
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.IRI, CAT),  # 0 (local id; same VALUE as seg1's 0)
            Term(TermKind.IRI, LABEL),  # 1
            Term(TermKind.LITERAL, "Cat", lang="en"),  # 2
        ]
    )
    w.add_suppress([{"kind": "quad", "q": [0, 1, 2]}], reason="superseded")
    g = read(seg1 + bytes(w.to_bytes()))
    assert _diag_codes(g) == []
    assert len(g.suppressions) == 1
    (target,) = g.suppressions[0].targets
    sq = target["q"]
    assert isinstance(sq, list)
    s_id, p_id, o_id = sq[0], sq[1], sq[2]
    assert isinstance(s_id, int)
    # The remapped target must name the UNION ids of segment one's quad —
    # value-interning makes value-wise application id-exact.
    assert (s_id, p_id, o_id, None) in g.quads
    assert g.term(s_id).value == CAT


def test_vector_19_profile_union_graceful_opacity() -> None:
    seg1 = _segment_one()
    w = Writer(catalog={0: Codec("identity", "encode"), 9: Codec("djvu", "compress")})
    w.add_terms([Term(TermKind.IRI, DOG)])
    frame = {"t": "quads", "x": [9], "d": b"\x00", "prev": w.head}
    frame["id"] = content_id(frame)
    g = read(seg1 + bytes(w.to_bytes()) + canonical(frame))
    # Segment one folds fully; segment two's transformed frame is opaque.
    values = {
        g.term(s).value for s, _, _, _ in g.quads if g.term(s).kind is TermKind.IRI
    }
    assert CAT in values
    assert any(o.reason == "unknown-codec" for o in g.opaque)
    assert len(g.segment_heads) == 2

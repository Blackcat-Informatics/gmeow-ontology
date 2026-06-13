# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Conformance vectors for the GTS reference implementation (§18 of GTS-SPEC.md).

Each test builds a GTS file with the :class:`Writer` (or hand-crafts edge-case
frames) and asserts the folded :class:`Graph`, its diagnostics, and the
``gts → nquads`` output.
"""

from __future__ import annotations

import cbor2
import pytest

from gts import Term, TermKind, Writer, read, to_nquads
from gts.codec import Codec
from gts.model import RDF_LANG_STRING, XSD_STRING
from gts.wire import canonical, content_id, header_id

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
    from gts.wire import digest_str

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


def test_vector_20_language_tag_discipline() -> None:
    # Canonical payloads keep internal private-use tags (§13.1).
    w = Writer(profile="dist")
    w.add_terms([Term(TermKind.LITERAL, "Cat", lang="x-gmeow-english")])
    g = read(w.to_bytes())
    assert _diag_codes(g) == []
    assert g.term(0).lang == "x-gmeow-english"

    # Projection/docs sections MUST use public BCP-47 tags only.
    w2 = Writer(profile="dist")
    with pytest.raises(ValueError, match="private-use language tag"):
        w2.add_terms(
            [Term(TermKind.LITERAL, "Read me", lang="x-gmeow-english")],
            section="projection",
        )


def test_vector_21_degenerate_composition_is_structurally_valid() -> None:
    # Raw byte concatenation is always valid GTS; the refusal is tooling-only.
    from gts.vectors import corpus

    case = next(c for c in corpus() if c.name == "21-degenerate-composition")
    g = read(case.data)
    assert len(g.segment_heads) == 2
    assert len(g.suppressions) == 1


def test_rfc8949_deterministic_key_order() -> None:
    """§4: map keys sort BYTEWISE on their encoded form (RFC 8949 §4.2).

    For short text keys the CBOR initial byte embeds the length, so RFC 8949
    bytewise ordering coincides with RFC 7049 length-first ordering — every
    GTS wire map (frames, headers, codec catalogs) therefore hashes the same
    under both, and no compatibility break occurred when the encoder moved to
    true 8949. The orderings DIVERGE on mixed-type keys; pin the divergent
    case so the Rust implementation (#277) matches the right one.
    """
    # {"z": 1, 1000: 2}: "z" encodes 61 7a (2 bytes); 1000 encodes 19 03 e8
    # (3 bytes). RFC 7049 length-first puts "z" first; RFC 8949 bytewise puts
    # 1000 first (0x19 < 0x61). The spec mandates 8949.
    assert canonical({"z": 1, 1000: 2}).hex() == "a21903e802617a01"
    # And the coincident text-key case stays stable (hash compatibility):
    assert canonical({"x": 1, "id": 2}).hex() == "a261780162696402"


def test_corpus_matches_committed_expectations() -> None:
    """The frozen corpus (generated/gts-vectors/) is the cross-implementation
    truth: the oracle must reproduce every committed .expected.json exactly.
    The Rust core (#277) runs the same gate from the same files."""
    import json

    from gmeow_tools.config import GENERATED_DIR
    from gts.vectors import corpus, expected_for

    vdir = GENERATED_DIR / "gts-vectors"
    cases = corpus()
    assert cases, "corpus must not be empty"
    for case in cases:
        committed_bytes = (vdir / f"{case.name}.gts").read_bytes()
        assert committed_bytes == case.data, f"{case.name}: input bytes drifted"
        committed = json.loads(
            (vdir / f"{case.name}.expected.json").read_text(encoding="utf-8")
        )
        assert committed == expected_for(case), f"{case.name}: expectations drifted"


def test_prefix_fold_streaming_property() -> None:
    """§3.2/§18.23: every item-boundary prefix folds, and folds monotonically.

    A live stream in flight is indistinguishable from a torn append, so every
    intermediate fold must be a valid graph state the next frames only extend:
    terms/quads are list-prefixes while the segment count is unchanged, and
    ground (bnode-free) N-Quads lines survive the single→multi-segment
    representation switch (the §3.1 value-union relabels blank nodes).
    """
    from gts.model import Graph
    from gts.vectors import corpus
    from gts.wire import iter_items

    def ground(g: Graph) -> set[str]:
        return {ln for ln in to_nquads(g).splitlines() if "_:" not in ln}

    for case in corpus():
        items, torn = iter_items(case.data)
        # the last TRUE item boundary of a torn file is the torn offset, not EOF
        end_of_items = torn if torn is not None else len(case.data)
        boundaries = [off for off, _ in items[1:]] + [end_of_items]
        prev: Graph | None = None
        for end in boundaries:
            g = read(case.data[:end])  # MUST be total: never raises
            if prev is not None:
                if len(prev.segment_heads) == len(g.segment_heads):
                    assert g.terms[: len(prev.terms)] == prev.terms, case.name
                    assert g.quads[: len(prev.quads)] == prev.quads, case.name
                else:
                    assert ground(prev) <= ground(g), case.name
            prev = g
        if torn is not None and prev is not None:
            # §3.2: a stream cut mid-item folds exactly like the torn file
            full = read(case.data)
            assert full.terms == prev.terms, case.name
            assert full.quads == prev.quads, case.name


# --------------------------------------------------------------------------- #
# Streamable compaction (§3.3, §10.1, §13.3 - spec vectors 24-26)
# --------------------------------------------------------------------------- #


def test_vector_25b_compact_is_byte_deterministic() -> None:
    """The frozen 25b bytes are the cross-engine determinism oracle (§14.1):
    compacting the frozen 25 bytes with the frozen timestamp must reproduce
    them exactly."""
    from gmeow_tools.config import GENERATED_DIR
    from gts.compact import compact_streamable

    vdir = GENERATED_DIR / "gts-vectors"
    source = (vdir / "25-streamable-source.gts").read_bytes()
    frozen = (vdir / "25b-streamable-compacted.gts").read_bytes()
    assert compact_streamable(source, timestamp="2026-01-01T00:00:00Z") == frozen


def test_compact_preserves_the_content_graph() -> None:
    """§10.1: re-authoring of the ordering, and only the ordering — every
    source quad and blob survives; only stream# provenance is added."""
    from gts.compact import compact_streamable
    from gts.stream import STREAM_NS
    from gts.vectors import _streamable_source

    src = read(_streamable_source())
    out = read(
        compact_streamable(_streamable_source(), timestamp="2026-01-01T00:00:00Z")
    )
    assert not out.diagnostics
    src_lines = set(to_nquads(src).splitlines())
    out_lines = set(to_nquads(out).splitlines())
    assert src_lines <= out_lines
    assert all(STREAM_NS in ln for ln in out_lines - src_lines)
    assert out.blobs == src.blobs
    assert out.segment_streamable[0].claimed
    assert out.segment_streamable[0].tail == 0


def test_compact_detached_signatures_verify_against_source_frames() -> None:
    """§10.1: a detached frame signature stays a checkable claim about the
    original log — the carried COSE bytes verify against stream:sourceFrame."""
    import base64

    from gts.crypto import InMemoryKeys, verify_sig
    from gts.vectors import _streamable_compacted, _streamable_signer

    keys = InMemoryKeys()
    keys.trust(_streamable_signer())  # type: ignore[arg-type]
    g = read(_streamable_compacted())

    def value_of(tid: int) -> str:
        return g.terms[tid].value or ""

    by_subject: dict[int, dict[str, str]] = {}
    for s, p, o, _g in g.quads:
        pred = value_of(p)
        if pred.startswith("https://w3id.org/gts/stream#"):
            by_subject.setdefault(s, {})[
                pred.removeprefix("https://w3id.org/gts/stream#")
            ] = value_of(o)
    detached = [
        fields
        for fields in by_subject.values()
        if "sourceFrame" in fields and "cose" in fields
    ]
    assert detached, "compacted vector must carry detached signatures"
    for fields in detached:
        frame_id = bytes.fromhex(fields["sourceFrame"].removeprefix("blake3:"))
        pad = "=" * (-len(fields["cose"]) % 4)
        cose = base64.urlsafe_b64decode(fields["cose"] + pad)
        status, kid = verify_sig(cose, frame_id, keys)
        assert status == "valid", fields["sourceFrame"]
        assert kid == "vector-key"


def test_vector_26_streamable_lie_is_diagnosed() -> None:
    """§3.3 (vector 25 in the spec list): a covered blob delivered before its
    stream:digest description MUST surface StreamableLayoutError."""
    from gts.vectors import _streamable_lie

    g = read(_streamable_lie())
    assert "StreamableLayoutError" in [d.code for d in g.diagnostics]


def test_claimed_segment_without_index_footer_is_diagnosed() -> None:
    w = Writer(layout="streamable")
    w.add_terms([Term(TermKind.IRI, "https://example.org/Cat")])
    g = read(w.to_bytes())
    assert [d.code for d in g.diagnostics] == ["StreamableLayoutError"]
    info = g.segment_streamable[0]
    assert info.claimed and info.covered == 0


def test_index_head_contradiction_is_diagnosed() -> None:
    """§3.3 check (b): an index whose head does not name frame `count`."""
    from gts.vectors import _streamable_compacted
    from gts.wire import iter_items

    data = _streamable_compacted()
    items, _ = iter_items(data)
    # Rewrite the trailing index frame with a wrong head, re-chaining it.
    *_, (last_off, last_item) = items
    assert isinstance(last_item, dict) and last_item["t"] == "index"
    payload = dict(last_item["d"])
    payload["head"] = b"\x00" * 32
    frame: dict[str, object] = {
        "t": "index",
        "d": payload,
        "prev": last_item["prev"],
    }
    frame["id"] = content_id(frame)
    g = read(data[:last_off] + canonical(frame))
    assert "StreamableLayoutError" in [d.code for d in g.diagnostics]


def test_vector_27_appended_tail_is_legal_and_reported() -> None:
    """§3.3 (vector 26 in the spec list): the unpresaged tail after the index
    footer folds cleanly and the boundary is reported."""
    from gts.vectors import _streamable_compacted, _streamable_tail

    g = read(_streamable_tail())
    assert not g.diagnostics
    info = g.segment_streamable[0]
    base = read(_streamable_compacted()).segment_streamable[0]
    assert info.claimed
    assert info.covered == base.covered
    assert info.tail == 2
    # The appended content folded (it is unpresaged, not ignored).
    assert any(g.terms[s].value == "https://example.org/Dog" for s, *_ in g.quads)


def test_recompacting_a_tailed_file_absorbs_the_tail() -> None:
    from gts.compact import compact_streamable
    from gts.vectors import _streamable_tail

    g = read(compact_streamable(_streamable_tail(), timestamp="2026-02-02T00:00:00Z"))
    assert not g.diagnostics
    assert g.segment_streamable[0].tail == 0
    assert any(t.value == "https://example.org/Dog" for t in g.terms)


def test_compact_refusals() -> None:
    """§10.1/§14.1: dirty input, evidence-without-seal, frame suppressions,
    and mixed profiles are refused."""
    import pytest as _pytest

    from gts.compact import CompactRefusedError, compact_streamable
    from gts.vectors import _segment_one, _segment_two, _streamable_lie

    ts = "2026-01-01T00:00:00Z"
    with _pytest.raises(CompactRefusedError, match="does not verify cleanly"):
        compact_streamable(_streamable_lie(), timestamp=ts)

    ev = Writer(profile="evidence")
    ev.add_terms(
        [
            Term(TermKind.IRI, "https://example.org/Cat"),
            Term(TermKind.IRI, "http://www.w3.org/2000/01/rdf-schema#label"),
            Term(TermKind.LITERAL, "Cat", lang="en"),
        ]
    )
    ev.add_quads([(0, 1, 2, None)])
    with _pytest.raises(CompactRefusedError, match="seal-original"):
        compact_streamable(ev.to_bytes(), timestamp=ts)
    assert compact_streamable(ev.to_bytes(), timestamp=ts, seal_original=True)

    fs = Writer()
    fs.add_terms([Term(TermKind.IRI, "https://example.org/Cat")])
    fs.add_suppress([{"kind": "frame", "id": b"\x00" * 32}])
    with _pytest.raises(CompactRefusedError, match="frame-addressed"):
        compact_streamable(fs.to_bytes(), timestamp=ts)

    with _pytest.raises(CompactRefusedError, match="mixed segment profiles"):
        compact_streamable(_segment_one() + _segment_two(), timestamp=ts)


def test_compact_seal_original_round_trips_verbatim() -> None:
    """§10.1: --seal-original carries the source bytes intact as a nested GTS
    blob (§12.1) whose inner fold equals the original's."""
    from gts.compact import compact_streamable
    from gts.vectors import _streamable_source
    from gts.wire import digest_str

    src = _streamable_source()
    out = read(
        compact_streamable(src, timestamp="2026-01-01T00:00:00Z", seal_original=True)
    )
    sealed = out.blobs[digest_str(src)]
    assert sealed == src
    inner = read(sealed)
    assert not inner.diagnostics
    assert to_nquads(inner) == to_nquads(read(src))
    assert out.blob_meta[digest_str(src)]["mt"] == "application/gts"


def test_streamable_lie_detection_is_prefix_stable() -> None:
    """§3.3: the catalog-before-payload violation observed in any prefix is a
    violation of the whole file; the clean 25b vector must never show it on
    any prefix (only the missing-footer report may appear mid-flight)."""
    from gts.vectors import _streamable_compacted
    from gts.wire import iter_items

    data = _streamable_compacted()
    items, _ = iter_items(data)
    boundaries = [off for off, _ in items[1:]] + [len(data)]
    for end in boundaries:
        g = read(data[:end])
        for d in g.diagnostics:
            assert d.code == "StreamableLayoutError"
            assert "index footer" in d.detail  # never the order violation


def test_compact_carries_suppressions_with_metadata() -> None:
    """§10.1: each input suppression keeps its own frame, its reason, and its
    (shifted) targets — re-authoring of the ordering only, provenance intact."""
    from gts.vectors import _streamable_compacted

    g = read(_streamable_compacted())
    assert len(g.suppressions) == 1
    sup = g.suppressions[0]
    assert sup.reason == "superseded"
    [target] = sup.targets
    assert target["kind"] == "term"
    tid = target["id"]
    assert isinstance(tid, int)
    suppressed = g.terms[tid]
    assert suppressed.value == "Chat"
    assert suppressed.lang == "fr"

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0

"""The language-neutral GTS conformance corpus (GTS-SPEC §18).

Each :class:`VectorCase` is the *input bytes* of one conformance vector. The
``vectors`` generator (registered, drift-gated) writes every case to
``generated/gts-vectors/`` as a ``.gts`` file beside an ``.expected.json``
computed by running the Python reference oracle — committing both freezes the
corpus, and every implementation (the oracle itself, the Rust core #277) is
then gated against the same frozen truth. The richer Python-level assertions
stay in ``tests/test_gts.py``; this module owns only byte construction.
"""

from __future__ import annotations

from dataclasses import dataclass

from gmeow_tools.gts.codec import Codec
from gmeow_tools.gts.model import Term, TermKind
from gmeow_tools.gts.wire import canonical, content_id
from gmeow_tools.gts.writer import Writer

CAT = "https://example.org/Cat"
DOG = "https://example.org/Dog"
LABEL = "http://www.w3.org/2000/01/rdf-schema#label"
XSD_INT = "http://www.w3.org/2001/XMLSchema#integer"


@dataclass(frozen=True)
class VectorCase:
    """One conformance vector: a name, the GTS bytes, and a read mode."""

    name: str
    data: bytes
    #: "default" — plain read; "pre-segment" — read with allow_segments=False
    #: (the §16 hard-fail emulation, vector 17).
    mode: str = "default"


def _minimal() -> bytes:
    w = Writer(profile="dist")
    w.add_terms(
        [
            Term(TermKind.IRI, CAT),
            Term(TermKind.IRI, LABEL),
            Term(TermKind.LITERAL, "Cat", lang="en"),
        ]
    )
    w.add_quads([(0, 1, 2, None)])
    return bytes(w.to_bytes())


def _zstd_frame() -> bytes:
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.IRI, CAT),
            Term(TermKind.IRI, LABEL),
            Term(TermKind.LITERAL, "Cat", lang="en"),
        ]
    )
    w.add_quads([(0, 1, 2, None)], transform=["zstd"])
    return bytes(w.to_bytes())


def _unknown_codec() -> bytes:
    w = Writer(catalog={0: Codec("identity", "encode"), 9: Codec("brotli", "compress")})
    frame: dict[str, object] = {
        "t": "quads",
        "x": [9],
        "d": b"\x00\x01\x02",
        "prev": w.head,
    }
    frame["id"] = content_id(frame)
    return bytes(w.to_bytes()) + canonical(frame)


def _damaged_frame() -> bytes:
    w = Writer()
    w.add_terms([Term(TermKind.IRI, CAT)])
    data = bytearray(w.to_bytes())
    data[-1] ^= 0xFF  # corrupt the last byte of the last frame
    return bytes(data)


def _torn_append() -> bytes:
    w = Writer()
    w.add_terms([Term(TermKind.IRI, CAT)])
    whole = bytes(w.to_bytes())
    w2 = Writer()
    w2.add_terms([Term(TermKind.IRI, DOG)])
    extra = bytes(w2.to_bytes())[len(bytes(Writer().to_bytes())) :]
    return whole + extra[: max(1, len(extra) // 2)]  # half a trailing frame


def _header_tampered() -> bytes:
    w = Writer()
    w.add_terms([Term(TermKind.IRI, CAT)])
    data = bytearray(w.to_bytes())
    # Flip a byte inside the header region (after the 3-byte self-describe tag).
    data[10] ^= 0x01
    return bytes(data)


def _suppression() -> bytes:
    w = Writer()
    w.add_terms([Term(TermKind.IRI, CAT)])
    w.add_suppress([{"kind": "term", "id": 0}], reason="retracted")
    return bytes(w.to_bytes())


def _datatype_defaulting() -> bytes:
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.LITERAL, "hi", lang="en"),
            Term(TermKind.LITERAL, "plain"),
            Term(TermKind.IRI, XSD_INT),
            Term(TermKind.LITERAL, "42", datatype=2),
            Term(TermKind.IRI, CAT),
            Term(TermKind.IRI, LABEL),
        ]
    )
    w.add_quads([(4, 5, 0, None), (4, 5, 1, None), (4, 5, 3, None)])
    return bytes(w.to_bytes())


def _conflicting_reifier() -> bytes:
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.IRI, CAT),
            Term(TermKind.IRI, LABEL),
            Term(TermKind.LITERAL, "Cat", lang="en"),
            Term(TermKind.IRI, "https://example.org/r1"),
            Term(TermKind.LITERAL, "Chat", lang="fr"),
        ]
    )
    w.add_reifies({3: (0, 1, 2)})
    w.add_reifies({3: (0, 1, 4)})  # conflicting rebind — first binding kept
    return bytes(w.to_bytes())


def _position_constraint() -> bytes:
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.IRI, CAT),
            Term(TermKind.LITERAL, "not-a-predicate"),
            Term(TermKind.LITERAL, "x"),
        ]
    )
    w.add_quads([(0, 1, 2, None)])  # literal in predicate position
    return bytes(w.to_bytes())


def _bnode_label() -> bytes:
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.BNODE, "b0"),
            Term(TermKind.IRI, LABEL),
            Term(TermKind.LITERAL, "anonymous"),
        ]
    )
    w.add_quads([(0, 1, 2, None)])
    return bytes(w.to_bytes())


def _segment_one() -> bytes:
    w = Writer(profile="dist")
    w.add_terms(
        [
            Term(TermKind.IRI, CAT),
            Term(TermKind.IRI, LABEL),
            Term(TermKind.LITERAL, "Cat", lang="en"),
            Term(TermKind.BNODE, "b0"),
        ]
    )
    w.add_quads([(0, 1, 2, None), (3, 1, 2, None)])
    return bytes(w.to_bytes())


def _segment_two() -> bytes:
    w = Writer(profile="music")
    w.add_terms(
        [
            Term(TermKind.IRI, DOG),
            Term(TermKind.IRI, LABEL),
            Term(TermKind.LITERAL, "Dog", lang="en"),
            Term(TermKind.BNODE, "b0"),
        ]
    )
    w.add_quads([(0, 1, 2, None), (3, 1, 2, None)])
    return bytes(w.to_bytes())


def _two_segment_union() -> bytes:
    return _segment_one() + _segment_two()


def _cross_segment_suppression() -> bytes:
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.IRI, CAT),
            Term(TermKind.IRI, LABEL),
            Term(TermKind.LITERAL, "Cat", lang="en"),
        ]
    )
    w.add_suppress([{"kind": "quad", "q": [0, 1, 2]}], reason="superseded")
    return _segment_one() + bytes(w.to_bytes())


def _profile_union_opacity() -> bytes:
    w = Writer(catalog={0: Codec("identity", "encode"), 9: Codec("djvu", "compress")})
    w.add_terms([Term(TermKind.IRI, DOG)])
    frame: dict[str, object] = {"t": "quads", "x": [9], "d": b"\x00", "prev": w.head}
    frame["id"] = content_id(frame)
    return _segment_one() + bytes(w.to_bytes()) + canonical(frame)


def corpus() -> list[VectorCase]:
    """The full exportable corpus, in spec §18 order."""
    return [
        VectorCase("01-minimal", _minimal()),
        VectorCase("02-zstd-frame", _zstd_frame()),
        VectorCase("03-unknown-codec", _unknown_codec()),
        VectorCase("04-damaged-frame", _damaged_frame()),
        VectorCase("05-torn-append", _torn_append()),
        VectorCase("06-header-tampered", _header_tampered()),
        VectorCase("09-suppression", _suppression()),
        VectorCase("11-datatype-defaulting", _datatype_defaulting()),
        VectorCase("12-conflicting-reifier", _conflicting_reifier()),
        VectorCase("13-position-constraint", _position_constraint()),
        VectorCase("14-bnode-label", _bnode_label()),
        VectorCase("15-two-segment-union", _two_segment_union()),
        VectorCase("16-composed-round-trip", _two_segment_union()),
        VectorCase(
            "17-pre-segment-hard-fail", _two_segment_union(), mode="pre-segment"
        ),
        VectorCase("18-cross-segment-suppression", _cross_segment_suppression()),
        VectorCase("19-profile-union-opacity", _profile_union_opacity()),
    ]


# --------------------------------------------------------------------------- #
# The registered corpus generator: writes each case's bytes beside an
# .expected.json computed by the Python reference oracle. Committing both
# freezes the corpus; the drift gate keeps it honest; every implementation
# (this oracle, the Rust core #277) tests against the same frozen truth.
# --------------------------------------------------------------------------- #


def expected_for(case: VectorCase) -> dict[str, object]:
    """Run the reference oracle over a case and summarize the outcome."""
    from gmeow_tools.gts.nquads import to_nquads
    from gmeow_tools.gts.reader import read

    g = read(case.data, allow_segments=(case.mode != "pre-segment"))
    return {
        "mode": case.mode,
        "diagnostics": [d.code for d in g.diagnostics],
        "terms": len(g.terms),
        "quads": len(g.quads),
        "segments": len(g.segment_heads),
        "segment_heads": [h.hex() for h in g.segment_heads],
        "profiles": list(g.segment_profiles),
        "opaque_reasons": sorted(o.reason for o in g.opaque),
        "suppressions": len(g.suppressions),
        # Sorted N-Quads lines; cross-implementation comparison is modulo
        # blank-node labelling (compare bnode-free lines exactly, bnode lines
        # by isomorphism or count).
        "nquads": sorted(to_nquads(g).splitlines()),
    }

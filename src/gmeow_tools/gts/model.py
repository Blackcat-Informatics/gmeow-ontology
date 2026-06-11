# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""In-memory data model for the GTS reference implementation.

A :class:`Term` is a single RDF term carried by integer id (§7.1 of the spec). The
folded :class:`Graph` is the deterministic replay of the append-only frame log
(§7.5): four id-keyed tables, content-addressed blobs, plus any opaque/damaged
nodes and reader diagnostics.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass, field
from enum import IntEnum

# Well-known datatype IRIs used by the literal-defaulting rule (§7.1).
XSD_STRING = "http://www.w3.org/2001/XMLSchema#string"
RDF_LANG_STRING = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"


class TermKind(IntEnum):
    """The kind of an RDF term, matching the wire ``"k"`` field (§7.1)."""

    IRI = 0
    LITERAL = 1
    BNODE = 2
    TRIPLE = 3


@dataclass(frozen=True)
class Term:
    """An RDF term identified by append-order id.

    Attributes:
        kind: The term kind.
        value: IRI string, literal lexical form, or blank-node label (file-local).
        datatype: Term-id of the literal's datatype IRI, when explicit.
        lang: Literal language tag (BCP 47).
        reifier: Term-id of the reifier of a quoted triple (``kind == TRIPLE``).
    """

    kind: TermKind
    value: str | None = None
    datatype: int | None = None
    lang: str | None = None
    reifier: int | None = None


# A quad is a 4-tuple of term-ids; the graph slot is ``None`` for the default graph.
Quad = tuple[int, int, int, int | None]
Triple = tuple[int, int, int]


@dataclass
class OpaqueNode:
    """A frame the reader could not decode, surfaced rather than dropped (§7.6)."""

    id: bytes
    frame_type: str
    reason: str  # "unknown-codec" | "missing-key" | "damaged"
    sigstat: str = "none"  # "none" | "valid" | "invalid" | "unverified"
    pub: object | None = None
    recipients: list[Mapping[str, object]] | None = None


@dataclass
class Suppression:
    """A recorded ``suppress`` directive (§11) — a display/precedence overlay."""

    targets: list[Mapping[str, object]]
    reason: str | None = None
    by: int | None = None


@dataclass
class Diagnostic:
    """A machine-observable reader diagnostic (§2.3)."""

    code: str
    detail: str
    frame_index: int | None = None


@dataclass
class Signature:
    """The verification outcome for a signed frame (§9.2)."""

    frame_id: bytes
    kid: str | None
    status: str  # "valid" | "invalid" | "unverified"


@dataclass
class Graph:
    """The folded result of a GTS log.

    Quads, reifier bindings and annotations are stored by term-id; resolve them with
    :meth:`term`. ``blobs`` maps a ``blake3:<hex>`` digest to inline bytes.
    """

    terms: list[Term] = field(default_factory=list)
    quads: list[Quad] = field(default_factory=list)
    reifiers: dict[int, Triple] = field(default_factory=dict)
    annotations: list[Triple] = field(default_factory=list)
    blobs: dict[str, bytes] = field(default_factory=dict)
    meta: dict[str, object] = field(default_factory=dict)
    suppressions: list[Suppression] = field(default_factory=list)
    opaque: list[OpaqueNode] = field(default_factory=list)
    signatures: list[Signature] = field(default_factory=list)
    diagnostics: list[Diagnostic] = field(default_factory=list)
    #: Ordered per-segment head ids (§3.1) — the file's composite identity.
    #: A single-segment file has exactly one entry.
    segment_heads: list[bytes] = field(default_factory=list)
    #: Per-segment header profiles, in file order; the file's effective
    #: requirement set is the union (§3.1, §13).
    segment_profiles: list[str] = field(default_factory=list)
    #: Per-segment folded meta, in file order (§7.5) — preserved alongside the
    #: file-level shallow merge in ``meta`` so a later segment's keys win in
    #: ``meta`` but no segment's metadata is silently absorbed.
    segment_meta: list[dict[str, object]] = field(default_factory=list)

    def term(self, term_id: int) -> Term:
        """Resolve a term-id to its :class:`Term`."""
        return self.terms[term_id]

    def datatype_iri(self, t: Term) -> str:
        """Return the effective datatype IRI of a literal, applying §7.1 defaulting."""
        if t.kind is not TermKind.LITERAL:
            msg = "datatype_iri is only defined for literals"
            raise ValueError(msg)
        if t.datatype is not None:
            dt = self.terms[t.datatype]
            return dt.value or XSD_STRING
        return RDF_LANG_STRING if t.lang is not None else XSD_STRING

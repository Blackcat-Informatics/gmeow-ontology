# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Grounded agent memory over a GTS ai-package (#296, Principle 13).

A :class:`Memory` is an append-only store of CLAIMS. Under the hood every
claim is a genuinely reified RDF 1.2 statement — a ``(subject rdf:value
"text")`` triple bound by a reifier that carries confidence, standpoint
(``accordingTo``), source, and timestamp annotations from the GMEOW
vocabulary. Persistence is the GTS format's own composition rule (§3.1):
every ``store``/``revise`` appends one small self-contained SEGMENT to the
file by plain byte-append — crash-safe (a torn append is detected and
ignored, never corrupting prior knowledge), and the file is a valid,
``gts verify``-able package at every moment of its life.

Revision is supersession, never deletion (Principle 10): ``revise`` appends
a suppression of the assertion plus an audit-trail derivation link; the
original bytes remain present, hash-linked, and recoverable.
"""

from __future__ import annotations

import datetime as _dt
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from gts import Term, TermKind, Writer, read
from gts.wire import blake3_256

if TYPE_CHECKING:
    from rdflib import Dataset

    from gts.model import Graph

_RDF_VALUE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#value"
_GMEOW = "https://blackcatinformatics.ca/gmeow/"
_CONFIDENCE = _GMEOW + "confidence"
_ACCORDING_TO = _GMEOW + "accordingTo"
_SOURCE_LOCATION = _GMEOW + "sourceLocation"
_WAS_DERIVED_FROM = _GMEOW + "wasDerivedFrom"
_DCT_CREATED = "http://purl.org/dc/terms/created"

_XSD_DECIMAL = "http://www.w3.org/2001/XMLSchema#decimal"
_XSD_DATETIME = "http://www.w3.org/2001/XMLSchema#dateTime"

_PROFILE = "ai-package"


@dataclass(frozen=True)
class Claim:
    """One recalled claim — the user-facing view of a reified statement."""

    #: The assertion IRI — stable handle for :meth:`Memory.revise`.
    id: str
    text: str
    confidence: float | None = None
    according_to: str | None = None
    source: str | None = None
    created: str | None = None
    #: True when a later revision suppressed this assertion (P10: it remains
    #: in the package and is recoverable with ``include_suppressed=True``).
    suppressed: bool = False


class Memory:
    """A grounded memory persisted as a GTS ai-package on disk.

    >>> mem = Memory("assistant.gts")
    >>> claim = mem.store("Patrick prefers explicit error handling",
    ...                   source="conversation 2026-06-10", confidence=0.8,
    ...                   according_to="claude-fable-5")
    >>> mem.recall("error handling")[0].text
    'Patrick prefers explicit error handling'
    """

    def __init__(self, path: str | Path) -> None:
        """Open (or create on first ``store``) the package at ``path``."""
        self._path = Path(path)

    # -- write side ---------------------------------------------------------

    def store(
        self,
        text: str,
        *,
        source: str | None = None,
        confidence: float | None = None,
        according_to: str | None = None,
    ) -> Claim:
        """Append one claim as a reified RDF 1.2 statement in a new segment."""
        if not text.strip():
            msg = "a claim needs text"
            raise ValueError(msg)
        assertion = f"urn:gmeow:assertion:{uuid.uuid4()}"
        subject = "urn:gmeow:claim:blake3:" + blake3_256(text.encode("utf-8")).hex()
        created = _dt.datetime.now(tz=_dt.UTC).isoformat(timespec="seconds")

        w = Writer(profile=_PROFILE)
        terms: list[Term] = [
            Term(TermKind.IRI, subject),  # 0
            Term(TermKind.IRI, _RDF_VALUE),  # 1
            Term(TermKind.LITERAL, text),  # 2
            Term(TermKind.IRI, assertion),  # 3
        ]
        annotations: list[tuple[int, int, int]] = []

        def annotate(predicate: str, value: Term) -> None:
            terms.append(Term(TermKind.IRI, predicate))
            terms.append(value)
            annotations.append((3, len(terms) - 2, len(terms) - 1))

        # dt indices are term-ids WITHIN this segment's append order.
        terms.append(Term(TermKind.IRI, _XSD_DATETIME))  # 4
        dt_datetime = len(terms) - 1
        annotate(_DCT_CREATED, Term(TermKind.LITERAL, created, datatype=dt_datetime))
        if confidence is not None:
            terms.append(Term(TermKind.IRI, _XSD_DECIMAL))
            dt_decimal = len(terms) - 1
            annotate(
                _CONFIDENCE,
                Term(TermKind.LITERAL, f"{confidence}", datatype=dt_decimal),
            )
        if according_to is not None:
            annotate(_ACCORDING_TO, Term(TermKind.LITERAL, according_to))
        if source is not None:
            annotate(_SOURCE_LOCATION, Term(TermKind.LITERAL, source))

        w.add_terms(terms)
        w.add_quads([(0, 1, 2, None)])
        w.add_reifies({3: (0, 1, 2)})
        w.add_annot(annotations)
        self._append(w.to_bytes())
        return Claim(
            id=assertion,
            text=text,
            confidence=confidence,
            according_to=according_to,
            source=source,
            created=created,
        )

    def revise(
        self,
        claim: Claim | str,
        *,
        reason: str | None = None,
        superseded_by: Claim | str | None = None,
    ) -> None:
        """Suppress a claim, optionally recording its successor (P10).

        Appends a segment that suppresses the assertion BY VALUE (the §3.1
        union re-interns the assertion IRI, so the suppression reaches the
        original segment without touching its bytes) and, when a successor
        is given, an audit-trail ``wasDerivedFrom`` annotation linking the
        new assertion to the suppressed one.
        """
        old_id = claim.id if isinstance(claim, Claim) else claim
        new_id = superseded_by.id if isinstance(superseded_by, Claim) else superseded_by
        w = Writer(profile=_PROFILE)
        terms: list[Term] = [Term(TermKind.IRI, old_id)]  # 0
        if new_id is not None:
            terms.append(Term(TermKind.IRI, new_id))  # 1
            terms.append(Term(TermKind.IRI, _WAS_DERIVED_FROM))  # 2
        w.add_terms(terms)
        if new_id is not None:
            w.add_annot([(1, 2, 0)])  # successor wasDerivedFrom predecessor
        w.add_suppress([{"kind": "term", "id": 0}], reason=reason)
        self._append(w.to_bytes())

    # -- read side ----------------------------------------------------------

    def recall(
        self,
        query: str = "",
        *,
        min_confidence: float | None = None,
        limit: int = 10,
        include_suppressed: bool = False,
    ) -> list[Claim]:
        """Return claims matching ``query``, best match first.

        Matching is token overlap with the claim text (v1: no embeddings —
        deterministic, dependency-free). An empty query returns the most
        recent claims. Suppressed claims are excluded unless asked for.
        """
        claims = [c for c in self.claims() if include_suppressed or not c.suppressed]
        if min_confidence is not None:
            claims = [
                c
                for c in claims
                if c.confidence is not None and c.confidence >= min_confidence
            ]
        tokens = {t for t in query.lower().split() if t}
        if tokens:
            scored = [(len(tokens & set(c.text.lower().split())), c) for c in claims]
            claims = [c for score, c in scored if score > 0]
            claims.sort(
                key=lambda c: len(tokens & set(c.text.lower().split())),
                reverse=True,
            )
        else:
            claims.reverse()  # most recent first
        return claims[:limit]

    def claims(self) -> list[Claim]:
        """Every claim in the package, in storage order."""
        if not self._path.exists():
            return []
        g = read(self._path.read_bytes())
        suppressed = self._suppressed_terms(g)
        annotations = self._annotations_by_reifier(g)
        out: list[Claim] = []
        for rid, (s, p, o) in g.reifiers.items():
            if g.terms[p].value != _RDF_VALUE:
                continue
            text = g.terms[o].value or ""
            ann = annotations.get(rid, {})
            raw_conf = ann.get(_CONFIDENCE)
            out.append(
                Claim(
                    id=g.terms[rid].value or "",
                    text=text,
                    confidence=float(raw_conf) if raw_conf is not None else None,
                    according_to=ann.get(_ACCORDING_TO),
                    source=ann.get(_SOURCE_LOCATION),
                    created=ann.get(_DCT_CREATED),
                    suppressed=rid in suppressed or s in suppressed,
                )
            )
        return out

    def verify(self) -> list[str]:
        """Transport diagnostics for the package — empty means clean."""
        if not self._path.exists():
            return []
        g = read(self._path.read_bytes())
        return [f"{d.code}: {d.detail}" for d in g.diagnostics]

    # -- interop (extras) ---------------------------------------------------

    def to_rdflib(self) -> Dataset:
        """Return the folded graph as an ``rdflib.Dataset`` (needs gmeow[rdf]).

        An explicitly LOSSY projection to RDF 1.1: rdflib does not parse
        RDF 1.2 quoted-triple terms, so the ``rdf:reifies <<( … )>>``
        binding lines are dropped. Base quads and all statement-level
        annotations (addressed by the assertion IRI) survive — recall-
        equivalent content, minus the formal binding. Full RDF 1.2 fidelity
        is the GTS file itself.
        """
        try:
            from rdflib import Dataset
        except ImportError as exc:  # pragma: no cover
            msg = "rdflib interop needs the extra: pip install 'gmeow[rdf]'"
            raise ImportError(msg) from exc
        from gts import to_nquads

        ds = Dataset()
        if self._path.exists():
            lines = to_nquads(read(self._path.read_bytes())).splitlines()
            rdf11 = "\n".join(ln for ln in lines if "<<(" not in ln)
            ds.parse(data=rdf11, format="nquads")
        return ds

    # -- internals ----------------------------------------------------------

    def _append(self, segment: bytes) -> None:
        """Append one complete segment — the whole persistence model (§3.1).

        No rewrite, no lock dance; a torn append is detected and ignored by
        every reader, so a crash mid-write never corrupts prior knowledge.
        """
        with self._path.open("ab") as fh:
            fh.write(segment)

    @staticmethod
    def _suppressed_terms(g: Graph) -> set[int]:
        out: set[int] = set()
        for sup in g.suppressions:
            for target in sup.targets:
                tid = target.get("id")
                if target.get("kind") == "term" and isinstance(tid, int):
                    out.add(tid)
        return out

    @staticmethod
    def _annotations_by_reifier(g: Graph) -> dict[int, dict[str, str]]:
        out: dict[int, dict[str, str]] = {}
        for rid, p, v in g.annotations:
            pred = g.terms[p].value
            value = g.terms[v].value
            if pred is not None and value is not None:
                out.setdefault(rid, {})[pred] = value
        return out

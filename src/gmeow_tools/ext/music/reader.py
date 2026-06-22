# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Read a GTS ``music-package`` into the Python :py:mod:`model`."""

from __future__ import annotations

from itertools import chain
from pathlib import Path
from typing import TYPE_CHECKING

import gts
from gmeow_rdf.compat.rdflib import BNode, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import RDFS
from gmeow_rdf.compat.rdflib.term import Node
from gts.model import Graph as GTSGraph
from gts.model import TermKind

from gmeow_tools.ext.music.model import (
    Piece,
    PitchValue,
    TimeFrame,
    ToneEvent,
    TuningSystem,
    Voice,
)

if TYPE_CHECKING:
    pass

GM = Namespace("https://blackcatinformatics.ca/gmeow/")
RDF = Namespace("http://www.w3.org/1999/02/22-rdf-syntax-ns#")
XSD = Namespace("http://www.w3.org/2001/XMLSchema#")


def _gts_term_to_rdflib(g: GTSGraph, tid: int) -> Node:
    """Convert a GTS term-id into an rdflib node."""
    term = g.terms[tid]
    if term.kind is TermKind.IRI:
        return URIRef(term.value or "")
    if term.kind is TermKind.BNODE:
        return BNode(term.value or f"b{tid}")
    if term.kind is TermKind.LITERAL:
        if term.lang is not None:
            return Literal(term.value or "", lang=term.lang)
        if term.datatype is not None:
            dt_iri = g.terms[term.datatype].value or ""
            return Literal(term.value or "", datatype=URIRef(dt_iri))
        return Literal(term.value or "")
    raise ValueError(f"unknown term kind: {term.kind}")


def load_graph_from_gts(path: Path) -> Graph:
    """Load the base RDF graph from a GTS file.

    Statement-layer reifiers and annotations are ignored: a music-package
    projection works over the asserted base triples.
    """
    raw = path.read_bytes()
    gts_graph = gts.read(raw)
    graph = Graph()
    for s, p, o, _gname in gts_graph.quads:
        subj = _gts_term_to_rdflib(gts_graph, s)
        pred = _gts_term_to_rdflib(gts_graph, p)
        obj = _gts_term_to_rdflib(gts_graph, o)
        graph.add((subj, pred, obj))
    return graph


def _integer(graph: Graph, subject: Node, predicate: Node) -> int | None:
    value = graph.value(subject, predicate)
    if value is None:
        return None
    try:
        return int(str(value))
    except ValueError:
        return None


def _string(graph: Graph, subject: Node, predicate: Node) -> str | None:
    value = graph.value(subject, predicate)
    if value is None:
        return None
    return str(value)


def _bool_literal(graph: Graph, subject: Node, predicate: Node) -> bool:
    """Parse an xsd:boolean or boolean-lexical literal; default to False."""
    value = graph.value(subject, predicate)
    if not isinstance(value, Literal):
        return False
    python_value = value.toPython()
    if isinstance(python_value, bool):
        return python_value
    if isinstance(python_value, str):
        return python_value.lower() in {"true", "1"}
    return False


def _load_tuning(graph: Graph, tuning_iri: Node) -> TuningSystem:
    """Build a :py:class:`TuningSystem` from its IRI."""
    return TuningSystem(
        iri=str(tuning_iri),
        label=_string(graph, tuning_iri, RDFS.label) or str(tuning_iri),
        division_count=_integer(graph, tuning_iri, GM.divisionCount),
    )


def _load_time_frame(graph: Graph, frame_iri: Node) -> TimeFrame:
    """Build a :py:class:`TimeFrame` from its IRI."""
    return TimeFrame(
        iri=str(frame_iri),
        label=_string(graph, frame_iri, RDFS.label) or str(frame_iri),
        beats_per_measure=_integer(graph, frame_iri, GM.beatsPerMeasure),
        beat_unit=_integer(graph, frame_iri, GM.beatUnit),
    )


def _load_pitch_value(graph: Graph, pitch_iri: Node) -> PitchValue | None:
    """Build a :py:class:`PitchValue` from a PitchValue individual."""
    num = _integer(graph, pitch_iri, GM.ratioNumerator)
    den = _integer(graph, pitch_iri, GM.ratioDenominator)
    if num is not None and den is not None:
        try:
            return PitchValue.from_ratio(num, den)
        except ValueError:
            return None
    cents_literal = graph.value(pitch_iri, GM.centsFromOrigin)
    if cents_literal is not None:
        try:
            return PitchValue(cents=float(str(cents_literal)))
        except ValueError:
            return None
    return None


def _load_time_span(graph: Graph, span_iri: Node) -> tuple[int, int, int, int] | None:
    """Return (start_num, start_den, dur_num, dur_den) for a MusicalTimeSpan."""
    start_num = _integer(graph, span_iri, GM.timeStartNumerator)
    start_den = _integer(graph, span_iri, GM.timeStartDenominator)
    dur_num = _integer(graph, span_iri, GM.timeDurationNumerator)
    dur_den = _integer(graph, span_iri, GM.timeDurationDenominator)
    if start_num is None or start_den is None or dur_num is None or dur_den is None:
        return None
    if start_den <= 0 or dur_den <= 0:
        return None
    return start_num, start_den, dur_num, dur_den


def piece_from_graph(graph: Graph, piece_iri: str | None = None) -> Piece:
    """Build a :py:class:`Piece` from an rdflib graph.

    If ``piece_iri`` is not given, the first ``gmeow:MusicalExpression`` or
    ``gmeow:MusicalWork`` found is used.
    """
    from fractions import Fraction

    piece_node: Node
    if piece_iri is not None:
        piece_node = URIRef(piece_iri)
    else:
        candidate = next(
            chain(
                graph.subjects(RDF.type, GM.MusicalExpression),
                graph.subjects(RDF.type, GM.MusicalWork),
            ),
            None,
        )
        if candidate is None:
            raise ValueError("no MusicalExpression or MusicalWork found in graph")
        piece_node = candidate

    piece = Piece(
        iri=str(piece_node),
        title=_string(graph, piece_node, RDFS.label),
        composer=_string(graph, piece_node, GM.composer),
    )

    for voice_iri in graph.objects(piece_node, GM.hasVoice):
        voice = Voice(
            iri=str(voice_iri),
            label=_string(graph, voice_iri, RDFS.label),
            tuning=_load_tuning(graph, tuning_iri)
            if (tuning_iri := graph.value(voice_iri, GM.voiceTuningFrame))
            else None,
            time_frame=_load_time_frame(graph, frame_iri)
            if (frame_iri := graph.value(voice_iri, GM.voiceTimeFrame))
            else None,
        )
        for event_iri in graph.subjects(GM.segmentOf, voice_iri):
            if GM.ToneEvent not in graph.objects(event_iri, RDF.type):
                # Only ToneEvents are rendered at this level.
                continue
            span_iri = graph.value(event_iri, GM.segmentSpan)
            if span_iri is None:
                continue
            span = _load_time_span(graph, span_iri)
            if span is None:
                continue
            start_num, start_den, dur_num, dur_den = span
            pitch = None
            pitch_iri = graph.value(event_iri, GM.toneEventPitchValue)
            if pitch_iri is not None:
                pitch = _load_pitch_value(graph, pitch_iri)
            event = ToneEvent(
                onset=Fraction(start_num, start_den),
                duration=Fraction(dur_num, dur_den),
                pitch=pitch,
                is_unpitched=_bool_literal(graph, event_iri, GM.toneEventIsUnpitched),
                dynamics=_string(graph, event_iri, GM.toneEventDynamics),
                articulation=_string(graph, event_iri, GM.toneEventArticulation),
                timbre=_string(graph, event_iri, GM.toneEventTimbre),
            )
            voice.events.append(event)
        # Sort events by onset for stable output.
        voice.events.sort(key=lambda e: e.onset)
        piece.voices.append(voice)

    return piece


def piece_from_gts(path: Path, piece_iri: str | None = None) -> Piece:
    """Load a ``music-package`` GTS file and build a :py:class:`Piece`."""
    graph = load_graph_from_gts(path)
    return piece_from_graph(graph, piece_iri)

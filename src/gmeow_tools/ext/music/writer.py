# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Write a GMEOW :py:class:`Piece` to an RDF graph ready for GTS encoding."""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import RDFS

from gmeow_tools.ext.music.model import Piece, PitchValue

GM = Namespace("https://blackcatinformatics.ca/gmeow/")
RDF = Namespace("http://www.w3.org/1999/02/22-rdf-syntax-ns#")
XSD = Namespace("http://www.w3.org/2001/XMLSchema#")


def _pitch_node(graph: Graph, pitch: PitchValue) -> URIRef:
    """Add a PitchValue individual to the graph."""
    cents_str = str(pitch.cents)
    node = URIRef(f"urn:gmeow:pitch:{cents_str}")
    graph.add((node, RDF.type, GM.PitchValue))
    graph.add((node, GM.centsFromOrigin, Literal(cents_str, datatype=XSD.decimal)))
    if pitch.spelled_name:
        graph.add((node, RDFS.label, Literal(pitch.spelled_name)))
    return node


def piece_to_graph(piece: Piece) -> Graph:
    """Convert a :py:class:`Piece` into an rdflib graph."""
    graph = Graph()
    piece_iri = URIRef(piece.iri or "urn:gmeow:piece:1")
    graph.add((piece_iri, RDF.type, GM.MusicalExpression))
    if piece.title:
        graph.add((piece_iri, RDFS.label, Literal(piece.title)))
    if piece.composer:
        graph.add((piece_iri, GM.composer, Literal(piece.composer)))

    for vidx, voice in enumerate(piece.voices):
        voice_iri = URIRef(voice.iri or f"urn:gmeow:voice:{vidx + 1}")
        graph.add((voice_iri, RDF.type, GM.Voice))
        if voice.label:
            graph.add((voice_iri, RDFS.label, Literal(voice.label)))
        graph.add((piece_iri, GM.hasVoice, voice_iri))

        if voice.tuning is not None:
            tuning_iri = URIRef(voice.tuning.iri)
            graph.add((tuning_iri, RDF.type, GM.TuningSystem))
            if voice.tuning.label:
                graph.add((tuning_iri, RDFS.label, Literal(voice.tuning.label)))
            if voice.tuning.division_count is not None:
                graph.add(
                    (tuning_iri, GM.divisionCount, Literal(voice.tuning.division_count))
                )
            graph.add((voice_iri, GM.voiceTuningFrame, tuning_iri))

        if voice.time_frame is not None:
            frame_iri = URIRef(voice.time_frame.iri)
            graph.add((frame_iri, RDF.type, GM.MusicalTimeFrame))
            if voice.time_frame.label:
                graph.add((frame_iri, RDFS.label, Literal(voice.time_frame.label)))
            if voice.time_frame.beats_per_measure is not None:
                graph.add(
                    (
                        frame_iri,
                        GM.beatsPerMeasure,
                        Literal(voice.time_frame.beats_per_measure),
                    )
                )
            if voice.time_frame.beat_unit is not None:
                graph.add((frame_iri, GM.beatUnit, Literal(voice.time_frame.beat_unit)))
            graph.add((voice_iri, GM.voiceTimeFrame, frame_iri))

        for eidx, event in enumerate(voice.events):
            event_iri = URIRef(f"urn:gmeow:event:{vidx + 1}:{eidx + 1}")
            graph.add((event_iri, RDF.type, GM.ToneEvent))
            graph.add((event_iri, GM.segmentOf, voice_iri))

            span_iri = URIRef(f"urn:gmeow:span:{vidx + 1}:{eidx + 1}")
            graph.add((span_iri, RDF.type, GM.MusicalTimeSpan))
            graph.add((span_iri, GM.timeStartNumerator, Literal(event.onset.numerator)))
            graph.add(
                (span_iri, GM.timeStartDenominator, Literal(event.onset.denominator))
            )
            graph.add(
                (span_iri, GM.timeDurationNumerator, Literal(event.duration.numerator))
            )
            graph.add(
                (
                    span_iri,
                    GM.timeDurationDenominator,
                    Literal(event.duration.denominator),
                )
            )
            if voice.time_frame is not None:
                graph.add(
                    (span_iri, GM.hasMusicalTimeFrame, URIRef(voice.time_frame.iri))
                )
            graph.add((event_iri, GM.segmentSpan, span_iri))

            if event.is_unpitched:
                graph.add(
                    (
                        event_iri,
                        GM.toneEventIsUnpitched,
                        Literal("true", datatype=XSD.boolean),
                    )
                )
            elif event.pitch is not None:
                pitch_node = _pitch_node(graph, event.pitch)
                graph.add((event_iri, GM.toneEventPitchValue, pitch_node))
                if voice.tuning is not None:
                    graph.add((pitch_node, GM.hasTuningFrame, URIRef(voice.tuning.iri)))

            if event.dynamics:
                graph.add((event_iri, GM.toneEventDynamics, URIRef(event.dynamics)))
            if event.articulation:
                graph.add(
                    (event_iri, GM.toneEventArticulation, URIRef(event.articulation))
                )
            if event.timbre:
                graph.add((event_iri, GM.toneEventTimbre, URIRef(event.timbre)))

    return graph

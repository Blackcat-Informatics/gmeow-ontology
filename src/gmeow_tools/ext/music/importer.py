# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Lossy inward projection from MusicXML to a GMEOW :py:class:`Piece`."""

from __future__ import annotations

from fractions import Fraction
from pathlib import Path
from typing import TYPE_CHECKING

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


def _quarter_length_to_fraction(ql: float | Fraction) -> Fraction:
    """Convert a music21 quarter-length to a Fraction."""
    return Fraction(float(ql)).limit_denominator(64)


_SUPPORTED_SUFFIXES = {".xml", ".musicxml", ".mxl"}


def piece_from_musicxml(path: Path) -> Piece:
    """Parse a MusicXML file and project it into the canonical model.

    The import is intentionally lossy: tuning frame defaults to 12-EDO,
    metric structure defaults to 4/4, and notation semantics are discarded.
    """
    if path.suffix.lower() not in _SUPPORTED_SUFFIXES:
        raise ValueError(
            f"MusicXML import only supports {sorted(_SUPPORTED_SUFFIXES)} files; "
            f"got {path.suffix!r}"
        )

    try:
        import music21
    except ImportError as exc:
        raise RuntimeError(
            "MusicXML import requires music21; install gmeow with the [music] extra."
        ) from exc

    score = music21.converter.parse(str(path))
    voice = Voice(
        iri="urn:gmeow:voice:1",
        label="imported voice",
        tuning=TuningSystem(
            iri="https://blackcatinformatics.ca/gmeow/tuningSystem12EDO",
            label="12-EDO",
            division_count=12,
        ),
        time_frame=TimeFrame(
            iri="urn:gmeow:timeframe:1",
            label="4/4",
            beats_per_measure=4,
            beat_unit=4,
        ),
    )

    for element in score.flatten().notesAndRests:
        onset = _quarter_length_to_fraction(element.offset)
        duration = _quarter_length_to_fraction(element.quarterLength)
        if duration <= 0:
            continue
        if isinstance(element, music21.note.Rest):
            event = ToneEvent(onset=onset, duration=duration, is_unpitched=True)
        elif isinstance(element, music21.note.Note):
            midi = element.pitch.ps
            event = ToneEvent(
                onset=onset,
                duration=duration,
                pitch=PitchValue.from_midi_number(midi),
            )
        elif isinstance(element, music21.chord.Chord):
            # For import, take the highest pitch as a simplification.
            if not element.pitches:
                event = ToneEvent(onset=onset, duration=duration, is_unpitched=True)
            else:
                midi = max(p.ps for p in element.pitches)
                event = ToneEvent(
                    onset=onset,
                    duration=duration,
                    pitch=PitchValue.from_midi_number(midi),
                )
        else:
            continue
        voice.events.append(event)

    return Piece(
        iri="urn:gmeow:piece:imported",
        title=(
            score.metadata.title
            if score.metadata and score.metadata.title
            else "Imported piece"
        ),
        voices=[voice],
    )

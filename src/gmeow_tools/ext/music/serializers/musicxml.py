# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""MusicXML 4.0 projection for a GMEOW :py:class:`Piece`."""

from __future__ import annotations

import xml.etree.ElementTree as ET
from fractions import Fraction
from typing import TYPE_CHECKING

from gmeow_tools.ext.music.model import Piece, PitchValue, ToneEvent, Voice
from gmeow_tools.ext.music.solver import (
    duration_to_dots,
    duration_to_note_type,
)

if TYPE_CHECKING:
    from gmeow_tools.ext.music.loss_manifest import NotationProfile

_DEFAULT_DIVISIONS = 48


def _pitch_elements(pitch: PitchValue) -> tuple[str, float, int]:
    """Map a frame-relative pitch to MusicXML step/alter/octave.

    Microtonal deviations are expressed as decimal ``alter`` values.
    """
    midi = pitch.to_midi_number()
    rounded = round(midi)
    chroma = rounded % 12
    octave = (rounded // 12) - 1
    step = ["C", "C", "D", "D", "E", "F", "F", "G", "G", "A", "A", "B"][chroma]
    alter = round(midi - rounded, 2)
    return step, alter, octave


def _add_pitch(parent: ET.Element, pitch: PitchValue) -> None:
    pitch_el = ET.SubElement(parent, "pitch")
    step, alter, octave = _pitch_elements(pitch)
    ET.SubElement(pitch_el, "step").text = step
    if abs(alter) > 0.001:
        ET.SubElement(pitch_el, "alter").text = f"{alter:.2f}"
    ET.SubElement(pitch_el, "octave").text = str(octave)


def _add_note(
    parent: ET.Element, event: ToneEvent, divisions: int, beat_unit: Fraction
) -> None:
    note = ET.SubElement(parent, "note")
    if event.is_unpitched or event.pitch is None:
        ET.SubElement(note, "rest")
    else:
        _add_pitch(note, event.pitch)

    quarters = float(event.duration / beat_unit)
    dur = round(quarters * divisions)
    ET.SubElement(note, "duration").text = str(dur)

    note_type = duration_to_note_type(event.duration, beat_unit)
    ET.SubElement(note, "type").text = note_type
    dots = duration_to_dots(event.duration, beat_unit)
    for _ in range(dots):
        ET.SubElement(note, "dot")

    if event.dynamics:
        dyn_el = ET.SubElement(note, "dynamics")
        ET.SubElement(dyn_el, event.dynamics.lower().replace(" ", "-"))
    if event.articulation:
        notations = ET.SubElement(note, "notations")
        articulations = ET.SubElement(notations, "articulations")
        ET.SubElement(articulations, event.articulation.lower().replace(" ", "-"))


def _measure_number(index: int) -> str:
    return str(index + 1)


def _voice_to_part(
    voice: Voice,
    part_id: str,
    profile: NotationProfile,
) -> ET.Element:
    """Render one Voice as a MusicXML ``<part>``."""
    part = ET.Element("part", {"id": part_id})

    beat_unit = Fraction(1, 4)
    beats_per_measure = 4
    if voice.time_frame is not None:
        if voice.time_frame.beat_unit is not None:
            beat_unit = Fraction(1, voice.time_frame.beat_unit)
        if voice.time_frame.beats_per_measure is not None:
            beats_per_measure = voice.time_frame.beats_per_measure

    divisions = _DEFAULT_DIVISIONS
    # One beat in quarter-note equivalents = 4 / beat-unit-denominator.
    measure_length_quarters = beats_per_measure * 4 / beat_unit.denominator
    measure_length_duration = round(measure_length_quarters * divisions)

    events = sorted(voice.events, key=lambda e: e.onset)
    measure_index = 0
    current_measure = ET.SubElement(
        part, "measure", {"number": _measure_number(measure_index)}
    )
    # First measure attributes.
    attributes = ET.SubElement(current_measure, "attributes")
    ET.SubElement(attributes, "divisions").text = str(divisions)
    time_el = ET.SubElement(attributes, "time")
    ET.SubElement(time_el, "beats").text = str(beats_per_measure)
    ET.SubElement(time_el, "beat-type").text = str(beat_unit.denominator)
    clef = ET.SubElement(attributes, "clef")
    ET.SubElement(clef, "sign").text = "G"
    ET.SubElement(clef, "line").text = "2"

    accumulated = 0
    for event in events:
        quarters = float(event.duration / beat_unit)
        dur = round(quarters * divisions)
        if (
            accumulated + dur > measure_length_duration
            and dur <= measure_length_duration
        ):
            measure_index += 1
            current_measure = ET.SubElement(
                part, "measure", {"number": _measure_number(measure_index)}
            )
            accumulated = 0
        _add_note(current_measure, event, divisions, beat_unit)
        accumulated += dur

    return part


def render(piece: Piece, profile: NotationProfile) -> str:
    """Render ``piece`` to a MusicXML 4.0 string."""
    root = ET.Element("score-partwise", {"version": "4.0"})
    if piece.title:
        work = ET.SubElement(root, "work")
        ET.SubElement(work, "work-title").text = piece.title

    part_list = ET.SubElement(root, "part-list")
    for idx, voice in enumerate(piece.voices):
        part_id = f"P{idx + 1}"
        score_part = ET.SubElement(part_list, "score-part", {"id": part_id})
        ET.SubElement(score_part, "part-name").text = voice.label or part_id
        root.append(_voice_to_part(voice, part_id, profile))

    ET.indent(root, space="  ")
    header = '<?xml version="1.0" encoding="UTF-8"?>\n'
    return header + ET.tostring(root, encoding="unicode")

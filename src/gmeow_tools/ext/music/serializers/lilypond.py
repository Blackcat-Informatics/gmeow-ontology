# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""LilyPond source projection for a GMEOW :py:class:`Piece`."""

from __future__ import annotations

from fractions import Fraction
from typing import TYPE_CHECKING

from gmeow_tools.ext.music.model import Piece, PitchValue
from gmeow_tools.ext.music.solver import duration_to_note_type, midi_to_spelled_name

if TYPE_CHECKING:
    from gmeow_tools.ext.music.loss_manifest import NotationProfile


def _pitch_to_ly(pitch: PitchValue) -> str:
    """Convert a frame-relative pitch to a LilyPond note name."""
    midi = pitch.to_midi_number()
    name = midi_to_spelled_name(midi)
    # Strip microtonal deviation annotation if present.
    if "(" in name:
        name = name.split("(")[0]
    step = name[0].lower()
    accidental = ""
    if "#" in name:
        accidental = "is"
    elif "b" in name:
        accidental = "es"
    # Extract octave number directly from MIDI to handle negative octaves correctly.
    octave = (round(midi) // 12) - 1
    # Middle C (C4) is represented as c' in LilyPond.
    if octave < 4:
        suffix = "," * (3 - octave)
    elif octave > 4:
        suffix = "'" * (octave - 3)
    else:
        suffix = "'"
    return step + accidental + suffix


def _duration_to_ly(duration: Fraction, beat_unit: Fraction = Fraction(1, 4)) -> str:
    """Map a duration to a LilyPond duration token."""
    type_name = duration_to_note_type(duration, beat_unit)
    mapping = {
        "breve": "\\breve",
        "whole": "1",
        "half": "2",
        "quarter": "4",
        "eighth": "8",
        "16th": "16",
        "32nd": "32",
        "64th": "64",
        "128th": "128",
    }
    return mapping.get(type_name, "4")


def render(piece: Piece, profile: NotationProfile) -> str:
    """Render ``piece`` to a LilyPond source string."""
    voice = piece.voices[0] if piece.voices else None
    events = sorted(voice.events, key=lambda e: e.onset) if voice else []
    title = piece.title or "Untitled"
    lines = [
        '\\version "2.24.0"',
        f'\\header {{ title = "{title}" }}',
        "{",
        "  \\clef treble",
    ]
    if events:
        beat_unit = voice.beat_unit if voice and voice.beat_unit else Fraction(1, 4)
        tokens: list[str] = []
        for event in events:
            if event.is_unpitched or event.pitch is None:
                token = "r"
            else:
                token = _pitch_to_ly(event.pitch)
            token += _duration_to_ly(event.duration, beat_unit)
            tokens.append(token)
        lines.append("  " + " ".join(tokens))
    else:
        lines.append("  r1")
    lines.append("}")
    lines.append(f"% GMEOW projection profile: {profile.projection_function}")
    return "\n".join(lines) + "\n"

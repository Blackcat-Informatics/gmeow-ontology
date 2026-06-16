# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Humdrum **kern projection for a GMEOW :py:class:`Piece`."""

from __future__ import annotations

from fractions import Fraction
from typing import TYPE_CHECKING

from gmeow_tools.ext.music.model import Piece, PitchValue
from gmeow_tools.ext.music.solver import midi_to_spelled_name

if TYPE_CHECKING:
    from gmeow_tools.ext.music.loss_manifest import NotationProfile


def _pitch_to_kern(pitch: PitchValue) -> str:
    """Convert a frame-relative pitch to a **kern pitch token."""
    midi = max(0, min(127, round(pitch.to_midi_number())))
    name = midi_to_spelled_name(midi)
    if "(" in name:
        name = name.split("(")[0]
    step = name[0].lower()
    acc = ""
    if "#" in name:
        acc = "#"
    elif "b" in name:
        acc = "-"
    octave = (midi // 12) - 1
    # **kern uses letter case/repetition for octave.
    token = step.upper() * (4 - octave) if octave < 4 else step.lower() * (octave - 3)
    return token + acc


def _duration_to_kern(duration: Fraction, beat_unit: Fraction = Fraction(1, 4)) -> str:
    """Map a duration to a **kern rhythmic value prefix."""
    units = duration / beat_unit
    if units >= 8:
        return "0"  # breve
    if units >= 4:
        return "1"  # whole
    if units >= 2:
        return "2"  # half
    if units >= 1:
        return "4"
    if units >= Fraction(1, 2):
        return "8"
    if units >= Fraction(1, 4):
        return "16"
    return "32"


def render(piece: Piece, profile: NotationProfile) -> str:
    """Render ``piece`` to a single **kern spine."""
    voice = piece.voices[0] if piece.voices else None
    events = sorted(voice.events, key=lambda e: e.onset) if voice else []
    lines = [
        f"!! {profile.projection_function}",
        "**kern",
    ]
    beat_unit = voice.beat_unit if voice and voice.beat_unit else Fraction(1, 4)
    for event in events:
        if event.is_unpitched or event.pitch is None:
            token = _duration_to_kern(event.duration, beat_unit) + "r"
        else:
            token = _duration_to_kern(event.duration, beat_unit) + _pitch_to_kern(
                event.pitch
            )
        lines.append(token)
    lines.append("*-")
    return "\n".join(lines) + "\n"

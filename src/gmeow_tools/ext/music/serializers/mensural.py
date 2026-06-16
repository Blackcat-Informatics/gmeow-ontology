# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Mensural notation projection for a GMEOW :py:class:`Piece`.

This renderer emits a human-readable mensural transcription rather than a
fully-engraved score.  Mensural proportions and coloration are preserved as
text annotations; the visual form is a lossy standpointed projection.
"""

from __future__ import annotations

from fractions import Fraction
from typing import TYPE_CHECKING

from gmeow_tools.ext.music.model import Piece, PitchValue
from gmeow_tools.ext.music.solver import duration_to_note_type, midi_to_spelled_name

if TYPE_CHECKING:
    from gmeow_tools.ext.music.loss_manifest import NotationProfile


def _pitch_token(pitch: PitchValue) -> str:
    name = midi_to_spelled_name(round(pitch.to_midi_number()))
    if "(" in name:
        name = name.split("(")[0]
    return name


def render(piece: Piece, profile: NotationProfile) -> str:
    """Render ``piece`` to a mensural transcription sketch."""
    voice = piece.voices[0] if piece.voices else None
    events = sorted(voice.events, key=lambda e: e.onset) if voice else []
    lines = [
        f"% Mensural notation transcription ({profile.projection_function})",
        f"% {piece.title or 'Untitled'}",
    ]
    beat_unit = voice.beat_unit if voice and voice.beat_unit else Fraction(1, 4)
    for event in events:
        if event.is_unpitched or event.pitch is None:
            token = "rest"
        else:
            token = _pitch_token(event.pitch)
        dur = duration_to_note_type(event.duration, beat_unit)
        lines.append(f"{token} ({dur})")
    return "\n".join(lines) + "\n"

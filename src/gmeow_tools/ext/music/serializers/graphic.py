# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Graphic notation projection for a GMEOW :py:class:`Piece`.

The visual artifact is canonical in the image direction; this text renderer
emits a structured description and instruction set that a future drawing
backend could interpret.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from gmeow_tools.ext.music.model import Piece

if TYPE_CHECKING:
    from gmeow_tools.ext.music.loss_manifest import NotationProfile


def render(piece: Piece, profile: NotationProfile) -> str:
    """Render ``piece`` to a graphic-score instruction sketch."""
    voice = piece.voices[0] if piece.voices else None
    events = sorted(voice.events, key=lambda e: e.onset) if voice else []
    lines = [
        f"% Graphic score ({profile.projection_function})",
        f"% Title: {piece.title or 'Untitled'}",
        "% Symbolic transcription is a standpointed interpretation.",
    ]
    for idx, event in enumerate(events, start=1):
        if event.is_unpitched or event.pitch is None:
            desc = "unpitched sound event"
        else:
            desc = f"pitch {event.pitch.to_midi_number():.2f} MIDI"
        lines.append(f"Event {idx}: {desc} at {event.onset} for {event.duration}")
    return "\n".join(lines) + "\n"

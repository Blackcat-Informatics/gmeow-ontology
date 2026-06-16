# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""ASCII tablature projection for a GMEOW :py:class:`Piece`.

This is a deliberately minimal, guitar-oriented tab renderer.  Real tablature
is instrument-specific and would require a string/fret model; this version
honestly declares that abstraction as a projection loss.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from gmeow_tools.ext.music.model import Piece, PitchValue

if TYPE_CHECKING:
    from gmeow_tools.ext.music.loss_manifest import NotationProfile

_STRING_NAMES = ["e", "B", "G", "D", "A", "E"]


def _pitch_to_fret(pitch: PitchValue) -> tuple[int, str]:
    """Return a string index and fret number for a 12-EDO guitar mapping."""
    midi = round(pitch.to_midi_number())
    # Standard tuning: E2=40, A2=45, D3=50, G3=55, B3=59, E4=64
    open_strings = [64, 59, 55, 50, 45, 40]
    best_string = min(
        range(len(open_strings)), key=lambda i: abs(midi - open_strings[i])
    )
    fret = midi - open_strings[best_string]
    return best_string, str(max(0, fret))


def render(piece: Piece, profile: NotationProfile) -> str:
    """Render ``piece`` as a 6-line ASCII tablature snippet."""
    voice = piece.voices[0] if piece.voices else None
    events = sorted(voice.events, key=lambda e: e.onset) if voice else []
    lines = [f"% ASCII tab projection ({profile.projection_function})"]
    lines.append(f"% Title: {piece.title or 'Untitled'}")
    if not events:
        for name in _STRING_NAMES:
            lines.append(f"{name}|-|")
        return "\n".join(lines) + "\n"

    # Simple fixed-width rendering: each event occupies 3 columns.
    strings: list[list[str]] = [[name, "|"] for name in _STRING_NAMES]
    for event in events:
        if event.is_unpitched or event.pitch is None:
            for s in strings:
                s.append("- -")
        else:
            sidx, fret = _pitch_to_fret(event.pitch)
            for i, s in enumerate(strings):
                s.append(f"-{fret}-" if i == sidx else "- -")
    for s in strings:
        s.append("|")
    lines.extend("".join(cols) for cols in strings)
    return "\n".join(lines) + "\n"

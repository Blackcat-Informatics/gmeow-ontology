# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""ABC notation projection for a monophonic GMEOW :py:class:`Piece`."""

from __future__ import annotations

from fractions import Fraction
from typing import TYPE_CHECKING

from gmeow_tools.ext.music.model import Piece, PitchValue, ToneEvent

if TYPE_CHECKING:
    from gmeow_tools.ext.music.loss_manifest import NotationProfile


# ABC note names: C, D, E, F, G, A, B, c, d, ...
_OCTAVE_MIDIS = [
    ("C,,,", 0),
    ("C,,", 12),
    ("C,", 24),
    ("C", 36),
    ("c", 48),
    ("c'", 60),
    ("c''", 72),
    ("c'''", 84),
    ("c''''", 96),
    ("c'''''", 108),
    ("c''''''", 120),
]


def _pitch_to_abc(pitch: PitchValue) -> str:
    """Convert a frame-relative pitch to an ABC note token.

    Microtonal deviations are rounded to the nearest 12-EDO semitone.
    """
    midi = max(0, min(127, round(pitch.to_midi_number())))
    # Pick octave token.
    token = "c'"
    for t, base in reversed(_OCTAVE_MIDIS):
        if midi >= base:
            token = t
            break
    base_midi = next(base for t, base in _OCTAVE_MIDIS if t == token)
    chroma = (midi - base_midi) % 12
    letters = ["C", "^C", "D", "^D", "E", "F", "^F", "G", "^G", "A", "^A", "B"]
    letter = letters[chroma]
    # Merge octave decorations with letter case.
    if "'" in token or token == "c":
        # lower-case letters for octaves >= middle C
        lower = letter[0].lower()
        accident = letter[1:] if len(letter) > 1 else ""
        octave_suffix = token[1:] if token.startswith("c") else token
        return accident + lower + octave_suffix
    return letter + token[1:]


def _duration_to_abc(
    duration: Fraction,
    beat_unit: Fraction = Fraction(1, 4),
    default: Fraction = Fraction(1, 8),
) -> str:
    """Map a duration to an ABC length multiplier.

    ``duration`` is expressed in ``beat_unit`` beats.  The default ABC unit is
    an eighth note, so a quarter note (duration 1, beat_unit 1/4) becomes
    ``2``; a sixteenth note becomes ``/2``.
    """
    real_duration = duration * beat_unit
    ratio = real_duration / default
    num = ratio.numerator
    den = ratio.denominator
    if den == 1:
        return "" if num == 1 else str(num)
    if num == 1:
        return f"/{den}"
    return f"{num}/{den}"


def _event_to_abc(
    event: ToneEvent,
    beat_unit: Fraction = Fraction(1, 4),
    default: Fraction = Fraction(1, 8),
) -> str:
    if event.is_unpitched or event.pitch is None:
        token = "z"
    else:
        token = _pitch_to_abc(event.pitch)
    token += _duration_to_abc(event.duration, beat_unit, default)
    return token


def render(piece: Piece, profile: NotationProfile) -> str:
    """Render ``piece`` to ABC notation (first voice only)."""
    voice = piece.voices[0] if piece.voices else None
    events = sorted(voice.events, key=lambda e: e.onset) if voice else []

    title = piece.title or "Untitled"
    lines = [
        "X:1",
        f"T:{title}",
        "M:4/4",
        "L:1/8",
        "K:C",
        "% GMEOW music-package -> ABC projection",
        f"% profile: {profile.projection_function}",
    ]
    if events:
        beat_unit = voice.beat_unit if voice and voice.beat_unit else Fraction(1, 4)
        default = Fraction(1, 8)
        # One 4/4 measure = 4 quarter beats.
        bar_length = Fraction(4)
        tokens: list[str] = []
        accumulated = Fraction(0)
        for event in events:
            tokens.append(_event_to_abc(event, beat_unit, default))
            accumulated += event.duration
            if accumulated >= bar_length:
                tokens.append("|")
                accumulated = Fraction(0)
        if tokens and tokens[-1] != "|":
            tokens.append("|")
        lines.append(" ".join(tokens))
    else:
        lines.append("z |")
    return "\n".join(lines) + "\n"

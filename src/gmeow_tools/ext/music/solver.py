# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Solver arithmetic for frame-relative music.

All numeric interpretation lives here, never in OWL axioms (Principle 12).
"""

from __future__ import annotations

import math
from fractions import Fraction

from gmeow_tools.ext.music.model import PitchValue

_CENTS_PER_OCTAVE = 1200.0

# Letter-name MIDI offsets for C4=60 in 12-EDO spelling.
_LETTER_OFFSETS: dict[str, int] = {
    "C": 0,
    "D": 2,
    "E": 4,
    "F": 5,
    "G": 7,
    "A": 9,
    "B": 11,
}
_ACCIDENTALS = {
    "bb": -2,
    "b": -1,
    "": 0,
    "#": 1,
    "##": 2,
}


def ratio_to_cents(numerator: int, denominator: int) -> float:
    """Convert a frequency ratio to cents."""
    if numerator <= 0:
        raise ValueError("numerator must be positive")
    if denominator <= 0:
        raise ValueError("denominator must be positive")
    return _CENTS_PER_OCTAVE * math.log2(numerator / denominator)


def cents_to_ratio(cents: float) -> float:
    """Convert cents to a frequency ratio."""
    return 2 ** (cents / _CENTS_PER_OCTAVE)


def midi_to_spelled_name(midi: float, prefer_sharp: bool = True) -> str:
    """Spell a fractional MIDI note number as a note name in 12-EDO.

    Args:
        midi: MIDI note number, 60 = C4.
        prefer_sharp: prefer sharps over flats for out-of-key pitches.

    Returns:
        A note name such as ``C#4`` or ``Ab3``.
    """
    rounded = round(midi)
    chroma = rounded % 12
    octave = (rounded // 12) - 1
    cents_dev = (midi - rounded) * 100.0
    names_sharp = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
    names_flat = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"]
    names = names_sharp if prefer_sharp else names_flat
    name = names[chroma]
    if abs(cents_dev) < 0.5:
        return f"{name}{octave}"
    sign = "+" if cents_dev >= 0 else ""
    return f"{name}{octave}({sign}{cents_dev:.1f}c)"


def spelled_name_to_midi(name: str) -> float:
    """Parse a 12-EDO note name to a fractional MIDI note number.

    Supports names like ``C4``, ``F#5``, ``Bb3``.  Microtonal deviations in
    parentheses are ignored by this simple parser.
    """
    import re

    m = re.match(r"^([A-G])(bb|b|#|##)?(-?\d+)", name.strip())
    if not m:
        raise ValueError(f"unrecognised pitch name: {name!r}")
    letter, acc, octave_str = m.groups()
    acc = acc or ""
    octave = int(octave_str)
    chroma = _LETTER_OFFSETS[letter] + _ACCIDENTALS[acc]
    return (octave + 1) * 12 + chroma


def pitch_to_cents_12edo(pitch: PitchValue) -> float:
    """Return the cents deviation from the nearest 12-EDO semitone.

    Useful when a renderer must quantize a frame-relative pitch to a
    nearest 12-EDO spelling.
    """
    midi = pitch.to_midi_number()
    nearest = round(midi)
    return (midi - nearest) * 100.0


def nearest_12edo_midi(pitch: PitchValue) -> int:
    """Return the nearest 12-EDO MIDI note number for a frame-relative pitch."""
    return round(pitch.to_midi_number())


def quantize_to_12edo(pitch: PitchValue) -> PitchValue:
    """Return a pitch quantized to the nearest 12-EDO semitone."""
    midi = round(pitch.to_midi_number())
    return PitchValue.from_midi_number(midi, spelled_name=midi_to_spelled_name(midi))


def duration_to_note_type(
    duration: Fraction, beat_unit: Fraction = Fraction(1, 4)
) -> str:
    """Map a duration in beat-units to a LilyPond/MusicXML note type name.

    The mapping is approximate: tuplets and dotted values are reduced to the
    nearest standard type.
    """
    if duration <= 0:
        raise ValueError("duration must be positive")
    ratio = float(duration / beat_unit)
    # Standard type values (quarter = 1.0): whole=4, half=2, quarter=1, eighth=0.5, ...
    candidates = [
        ("breve", 8.0),
        ("whole", 4.0),
        ("half", 2.0),
        ("quarter", 1.0),
        ("eighth", 0.5),
        ("16th", 0.25),
        ("32nd", 0.125),
        ("64th", 0.0625),
        ("128th", 0.03125),
    ]
    best = min(candidates, key=lambda c: abs(math.log2(c[1] / ratio)))
    return best[0]


def duration_to_dots(duration: Fraction, beat_unit: Fraction = Fraction(1, 4)) -> int:
    """Return the number of dots that best approximates ``duration``.

    Returns 0, 1, or 2.  The exact duration is still emitted by the caller as
    the authoritative value where the format permits.
    """
    if duration <= 0:
        raise ValueError("duration must be positive")
    ratio = duration / beat_unit
    # Standard note values relative to a quarter note (beat_unit = 1/4).
    bases = [
        Fraction(8, 1),  # breve
        Fraction(4, 1),  # whole
        Fraction(2, 1),  # half
        Fraction(1, 1),  # quarter
        Fraction(1, 2),  # eighth
        Fraction(1, 4),  # 16th
        Fraction(1, 8),  # 32nd
        Fraction(1, 16),  # 64th
        Fraction(1, 32),  # 128th
    ]
    for base in bases:
        for dots in range(3):
            # dotted value = base * (2 - 1/2^dots)
            dotted = base * Fraction(2 ** (dots + 1) - 1, 2**dots)
            if abs(ratio - dotted) <= Fraction(1, 64):
                return dots
    return 0

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Python model for frame-relative musical content.

The model is a deliberately narrow waist: it captures the musical structures a
``music-package`` GTS graph can express, no more.  Every serializer consumes a
:pyclass:`Piece`; the RDF-to-model reader is lossy and opinionated, matching the
lossy nature of notation projections.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from fractions import Fraction
from typing import Self

_MIDI_C4 = 60.0
_CENTS_PER_OCTAVE = 1200.0


@dataclass(frozen=True)
class PitchValue:
    """A frame-relative pitch value.

    The canonical internal form is a cents offset from C4 in the containing
    tuning frame.  Ratios are converted to cents on ingestion; spelled names are
    projections, not canonical values.
    """

    cents: float
    spelled_name: str | None = None

    @classmethod
    def from_midi_number(cls, midi: float, *, spelled_name: str | None = None) -> Self:
        """Create a pitch from a MIDI note number (60 = C4)."""
        return cls(cents=(midi - _MIDI_C4) * 100.0, spelled_name=spelled_name)

    @classmethod
    def from_ratio(
        cls, numerator: int, denominator: int, *, spelled_name: str | None = None
    ) -> Self:
        """Create a pitch from a frequency ratio relative to the origin pitch."""
        if denominator <= 0:
            raise ValueError("denominator must be positive")
        return cls(
            cents=_CENTS_PER_OCTAVE * math.log2(numerator / denominator),
            spelled_name=spelled_name,
        )

    @classmethod
    def from_frequency(
        cls,
        frequency: float,
        reference_hz: float,
        reference_cents: float = 0.0,
        *,
        spelled_name: str | None = None,
    ) -> Self:
        """Create a pitch from an absolute frequency and a reference anchor."""
        if frequency <= 0 or reference_hz <= 0:
            raise ValueError("frequencies must be positive")
        return cls(
            cents=reference_cents
            + _CENTS_PER_OCTAVE * math.log2(frequency / reference_hz),
            spelled_name=spelled_name,
        )

    def to_midi_number(self) -> float:
        """Return the MIDI note number (fractional for microtones)."""
        return _MIDI_C4 + self.cents / 100.0

    def to_frequency(
        self, reference_hz: float = 440.0, reference_cents: float = 0.0
    ) -> float:
        """Convert to an absolute frequency using the supplied reference."""
        return reference_hz * 2 ** ((self.cents - reference_cents) / _CENTS_PER_OCTAVE)


@dataclass(frozen=True)
class TuningSystem:
    """A frame in which pitch values are interpreted."""

    iri: str
    label: str
    reference_hz: float | None = None
    # For equal-division tunings (12-EDO, 19-EDO, ...).
    division_count: int | None = None
    # Optional Scala .scl degree list in cents, origin first, period last.
    degrees_cents: tuple[float, ...] | None = None


@dataclass(frozen=True)
class TimeFrame:
    """A reference frame for musical time."""

    iri: str
    label: str
    beats_per_measure: int | None = None
    beat_unit: int | None = None  # denominator (4 = quarter, 8 = eighth)


@dataclass(frozen=True)
class TempoMark:
    """A tempo indication: beat-unit duration and beats per minute."""

    beat_unit: Fraction
    bpm: float


@dataclass
class TimeMapping:
    """Piecewise mapping from musical time to clock time."""

    tempo_marks: list[tuple[Fraction, TempoMark]] = field(default_factory=list)

    def seconds_per_beat(self, offset: Fraction) -> float:
        """Return seconds per quarter-note-equivalent at the given offset."""
        active: TempoMark | None = None
        for start, mark in sorted(self.tempo_marks, key=lambda t: t[0]):
            if start > offset:
                break
            active = mark
        if active is None:
            return 60.0 / 120.0  # default 120 bpm quarter
        # Convert beat-unit to quarter-note-equivalent duration.
        quarter_factor = Fraction(4) / active.beat_unit
        return 60.0 / active.bpm * float(quarter_factor)

    def evaluate_seconds(self, offset: Fraction) -> float:
        """Return elapsed wall-clock seconds up to ``offset``."""
        seconds = 0.0
        last = Fraction(0)
        for start, mark in sorted(self.tempo_marks, key=lambda t: t[0]):
            if start >= offset:
                break
            quarter_factor = Fraction(4) / mark.beat_unit
            seconds += float(start - last) * (60.0 / mark.bpm * float(quarter_factor))
            last = start
        spb = self.seconds_per_beat(offset)
        seconds += float(offset - last) * spb
        return seconds


@dataclass
class ToneEvent:
    """One atomic sounding unit within a voice."""

    onset: Fraction
    duration: Fraction
    pitch: PitchValue | None = None
    is_unpitched: bool = False
    dynamics: str | None = None
    articulation: str | None = None
    timbre: str | None = None

    def __post_init__(self) -> None:
        """Validate invariants after construction."""
        if self.duration <= 0:
            raise ValueError("tone event duration must be positive")


@dataclass
class Voice:
    """A continuity strand that hosts its own tuning/time frames and events."""

    iri: str | None = None
    label: str | None = None
    tuning: TuningSystem | None = None
    time_frame: TimeFrame | None = None
    time_mapping: TimeMapping | None = None
    events: list[ToneEvent] = field(default_factory=list)


@dataclass
class Piece:
    """A musical work or expression ready for projection."""

    iri: str | None = None
    title: str | None = None
    composer: str | None = None
    voices: list[Voice] = field(default_factory=list)

    def all_events(self) -> list[tuple[Voice, ToneEvent]]:
        """Return every event paired with its voice."""
        return [(voice, event) for voice in self.voices for event in voice.events]

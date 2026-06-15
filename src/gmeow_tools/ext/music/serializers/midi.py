# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""MIDI SMF projection for a GMEOW :py:class:`Piece`.

Implemented without external dependencies so the public CLI can emit MIDI even
when the optional ``music21``/``mido`` packages are not installed.
"""

from __future__ import annotations

import struct
from fractions import Fraction
from typing import TYPE_CHECKING

from gmeow_tools.ext.music.model import Piece

if TYPE_CHECKING:
    from gmeow_tools.ext.music.loss_manifest import NotationProfile

_PPQN = 480  # pulses per quarter note


def _varlen(value: int) -> bytes:
    """Encode a non-negative integer as a MIDI variable-length quantity."""
    if value < 0:
        raise ValueError("varlen value must be non-negative")
    if value == 0:
        return b"\x00"
    chunks: list[int] = []
    while value > 0:
        chunks.append(value & 0x7F)
        value >>= 7
    chunks.reverse()
    for i in range(len(chunks) - 1):
        chunks[i] |= 0x80
    return bytes(chunks)


def _track_bytes(events: list[tuple[int, bytes]]) -> bytes:
    """Build a MIDI track chunk from (delta_time, message_bytes) pairs."""
    data = b"".join(_varlen(dt) + msg for dt, msg in events)
    return b"MTrk" + struct.pack(">I", len(data)) + data


def _header_bytes(format_type: int, num_tracks: int, division: int) -> bytes:
    return (
        b"MThd"
        + struct.pack(">I", 6)
        + struct.pack(">H", format_type)
        + struct.pack(">H", num_tracks)
        + struct.pack(">H", division)
    )


def _set_tempo(bpm: float) -> bytes:
    """Return a meta set-tempo message (microseconds per quarter)."""
    us = round(60_000_000 / bpm)
    return b"\xff\x51\x03" + struct.pack(">I", us)[1:]


def _pitch_bend(cents_deviation: float) -> bytes:
    """Return a pitch-bend change message for a cents deviation from 12-EDO."""
    # MIDI pitch bend range defaults to +/- 2 semitones = 200 cents.
    bend = round((cents_deviation / 200.0) * 8192) + 8192
    bend = max(0, min(16383, bend))
    lsb = bend & 0x7F
    msb = (bend >> 7) & 0x7F
    return bytes([0xE0, lsb, msb])


def render(piece: Piece, profile: NotationProfile) -> bytes:
    """Render ``piece`` to a MIDI Type-0 SMF byte string."""
    tempo = 120.0
    beat_unit = Fraction(1, 4)
    voice = piece.voices[0] if piece.voices else None
    events = sorted(voice.events, key=lambda e: e.onset) if voice else []

    track_events: list[tuple[int, bytes]] = []
    track_events.append((0, _set_tempo(tempo)))

    current_tick = 0
    for event in events:
        onset_tick = round(float(event.onset / beat_unit) * _PPQN)
        duration_tick = round(float(event.duration / beat_unit) * _PPQN)
        delta = max(0, onset_tick - current_tick)

        if event.is_unpitched or event.pitch is None:
            # Render unpitched as a rest: just advance time.
            current_tick = onset_tick + duration_tick
            continue

        midi = event.pitch.to_midi_number()
        rounded = round(midi)
        cents_dev = (midi - rounded) * 100.0
        pitch = max(0, min(127, int(rounded)))

        if abs(cents_dev) > 0.5:
            track_events.append((delta, _pitch_bend(cents_dev)))
            delta = 0
        track_events.append((delta, bytes([0x90, pitch, 96])))
        track_events.append((duration_tick, bytes([0x80, pitch, 0])))
        if abs(cents_dev) > 0.5:
            track_events.append((0, _pitch_bend(0.0)))
        current_tick = onset_tick + duration_tick

    track = _track_bytes(track_events)
    return _header_bytes(0, 1, _PPQN) + track

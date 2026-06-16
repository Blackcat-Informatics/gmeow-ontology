# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""MEI 5.1 projection for a GMEOW :py:class:`Piece`."""

from __future__ import annotations

import xml.etree.ElementTree as ET
from fractions import Fraction
from typing import TYPE_CHECKING

from gmeow_tools.ext.music.model import Piece, PitchValue
from gmeow_tools.ext.music.solver import duration_to_note_type

if TYPE_CHECKING:
    from gmeow_tools.ext.music.loss_manifest import NotationProfile


def _add_note(
    parent: ET.Element, pitch: PitchValue, duration: Fraction, beat_unit: Fraction
) -> None:
    ns = "http://www.music-encoding.org/ns/mei"
    midi = round(pitch.to_midi_number())
    chroma = midi % 12
    octave = (midi // 12) - 1
    step = ["c", "c", "d", "d", "e", "f", "f", "g", "g", "a", "a", "b"][chroma]
    # 12-EDO chroma spelling: C/C#, D/D#, E, F/F#, G/G#, A/A#, B.
    acc = ""
    if chroma in {1, 3, 6, 8, 10}:
        acc = "s"
    note = ET.SubElement(
        parent,
        f"{{{ns}}}note",
        {
            "pname": step,
            "oct": str(octave),
            "dur": duration_to_note_type(duration, beat_unit),
        },
    )
    if acc:
        ET.SubElement(note, f"{{{ns}}}accid", {"accid": acc})


def render(piece: Piece, profile: NotationProfile) -> str:
    """Render ``piece`` to a minimal MEI 5.1 string."""
    ns = "http://www.music-encoding.org/ns/mei"

    def tag(local: str) -> str:
        return f"{{{ns}}}{local}"

    ET.register_namespace("", ns)
    root = ET.Element(tag("mei"), {"meiversion": "5.1"})
    music = ET.SubElement(root, tag("music"))
    body = ET.SubElement(music, tag("body"))
    mdiv = ET.SubElement(body, tag("mdiv"))
    score = ET.SubElement(mdiv, tag("score"))
    section = ET.SubElement(score, tag("section"))

    voice = piece.voices[0] if piece.voices else None
    beat_unit = voice.beat_unit if voice and voice.beat_unit else Fraction(1, 4)
    events = sorted(voice.events, key=lambda e: e.onset) if voice else []
    if events:
        measure = ET.SubElement(section, tag("measure"), {"n": "1"})
        staff = ET.SubElement(measure, tag("staff"), {"n": "1"})
        layer = ET.SubElement(staff, tag("layer"), {"n": "1"})
        for event in events:
            if event.is_unpitched or event.pitch is None:
                ET.SubElement(
                    layer,
                    tag("rest"),
                    {"dur": duration_to_note_type(event.duration, beat_unit)},
                )
            else:
                _add_note(layer, event.pitch, event.duration, beat_unit)

    ET.indent(root, space="  ")
    header = '<?xml version="1.0" encoding="UTF-8"?>\n'
    return header + ET.tostring(root, encoding="unicode")

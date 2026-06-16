#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Five-minute demo of the GTS music-package projection toolchain.

Run from the repository root with the [music] extra installed:

    uv run --extra music python scripts/music_five_minute_demo.py

Outputs are written to ``dist/music-demo/``.
"""

from __future__ import annotations

import shutil
from collections.abc import Callable
from fractions import Fraction
from pathlib import Path

from gmeow_tools.ext.music import importer, writer  # type: ignore[import-untyped]
from gmeow_tools.ext.music.loss_manifest import (  # type: ignore[import-untyped]
    NotationProfile,
    get_profile,
    import_manifest_turtle,
    manifest_turtle,
)
from gmeow_tools.ext.music.model import (  # type: ignore[import-untyped]
    Piece,
    PitchValue,
    TimeFrame,
    ToneEvent,
    TuningSystem,
    Voice,
)
from gmeow_tools.ext.music.serializers import (  # type: ignore[import-untyped]
    abc,
    midi,
    musicxml,
    scl,
)
from gmeow_tools.gts_producer import gts_from_graph  # type: ignore[import-untyped]

_Renderer = Callable[[Piece, NotationProfile], str | bytes]


def _demo_piece() -> Piece:
    """A short C-major scale fragment with an explicit 12-EDO tuning frame."""
    tuning = TuningSystem(
        iri="https://blackcatinformatics.ca/gmeow/tuningSystem12EDO",
        label="12-EDO",
        division_count=12,
    )
    frame = TimeFrame(
        iri="urn:gmeow:demo:timeframe:1",
        label="4/4",
        beats_per_measure=4,
        beat_unit=4,
    )
    midi_numbers = [60, 62, 64, 65, 67, 69, 71, 72]
    durations = [Fraction(1, 4)] * 7 + [Fraction(1, 2)]
    events = [
        ToneEvent(
            onset=sum(durations[:i], Fraction(0)),
            duration=durations[i],
            pitch=PitchValue.from_midi_number(midi_numbers[i]),
        )
        for i in range(len(midi_numbers))
    ]
    return Piece(
        iri="urn:gmeow:demo:piece:1",
        title="GMEOW Five-Minute Demo",
        voices=[
            Voice(
                iri="urn:gmeow:demo:voice:1",
                label="Melody",
                tuning=tuning,
                time_frame=frame,
                events=events,
            )
        ],
    )


def main() -> int:
    """Render a demo piece to several notations and round-trip through MusicXML."""
    out = Path("dist/music-demo")
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)

    piece = _demo_piece()

    # -- Outward projections ---------------------------------------------------
    renderers: dict[str, _Renderer] = {
        "musicxml": musicxml.render,
        "abc": abc.render,
        "scl": scl.render,
        "midi": midi.render,
    }
    for fmt, renderer in renderers.items():
        profile = get_profile(fmt)
        data = renderer(piece, profile)
        path = out / f"demo.{fmt}"
        if isinstance(data, bytes):
            path.write_bytes(data)
        else:
            path.write_text(data, encoding="utf-8")
        (out / f"demo.{fmt}.manifest.ttl").write_text(
            manifest_turtle(
                fmt,
                provenance=f"gmeow music render demo.gts --to {fmt} -o {path.name}",
            ),
            encoding="utf-8",
        )
        print(f"wrote {path}")

    # -- Canonical GTS package -------------------------------------------------
    gts_path = out / "demo.gts"
    gts_path.write_bytes(gts_from_graph(writer.piece_to_graph(piece)))
    print(f"wrote {gts_path}")

    # -- Inward projection from MusicXML ---------------------------------------
    source = out / "demo.musicxml"
    imported = importer.piece_from_musicxml(source)
    imported_gts = out / "demo-imported.gts"
    imported_gts.write_bytes(gts_from_graph(writer.piece_to_graph(imported)))
    (out / "demo-imported.gts.manifest.ttl").write_text(
        import_manifest_turtle(
            source,
            imported.iri or "urn:gmeow:piece:imported",
            provenance="gmeow music import demo.musicxml -o demo-imported.gts",
        ),
        encoding="utf-8",
    )
    print(f"wrote {imported_gts}")
    print(
        f"events: original={len(piece.voices[0].events)} "
        f"imported={len(imported.voices[0].events)}"
    )

    # -- Optional music21 sanity check -----------------------------------------
    try:
        import music21
    except ImportError:
        print("music21 not installed; skipping playback sanity check")
        return 0

    score = music21.converter.parse(str(source))
    notes = [
        n for n in score.flatten().notesAndRests if isinstance(n, music21.note.Note)
    ]
    print(f"music21 parsed {len(notes)} notes from demo.musicxml")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

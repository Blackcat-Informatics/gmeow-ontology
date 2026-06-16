# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Flagship GTS music-package bundle tests (issue #320).

Three representative pieces stress the music-package projection toolchain:
complex nested-tuplets (Ferneyhough-like), oral-tradition microtonal inflections
(raga Yaman), and polymeter/math-rock.  Each piece is encoded as a GTS file,
rendered to four notation formats via the ``gmeow-music`` CLI, and checked for
a well-formed loss-manifest sidecar.
"""

from __future__ import annotations

import xml.etree.ElementTree as ET
from collections.abc import Callable
from fractions import Fraction
from pathlib import Path

import pytest
from rdflib import Graph, Namespace, URIRef
from typer.testing import CliRunner

from gmeow_tools.ext.music import writer
from gmeow_tools.ext.music.cli import app as music_app
from gmeow_tools.ext.music.loss_manifest import get_profile
from gmeow_tools.ext.music.model import (
    Piece,
    PitchValue,
    TimeFrame,
    ToneEvent,
    TuningSystem,
    Voice,
)
from gmeow_tools.gts_producer import gts_from_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")

runner = CliRunner()

FORMATS = ["musicxml", "lilypond", "abc", "midi"]


def _tuning_12edo() -> TuningSystem:
    return TuningSystem(
        iri="https://blackcatinformatics.ca/gmeow/tuningSystem12EDO",
        label="12-EDO",
        division_count=12,
    )


def piece_ferneyhough() -> Piece:
    """Complex rhythm fixture: two voices, nested tuplets, 2+2+3 grouping."""
    time_frame = TimeFrame(
        iri="urn:gmeow:test:ferneyhough:timeframe",
        label="7/8 (2+2+3)",
        beats_per_measure=7,
        beat_unit=8,
    )
    tuning = _tuning_12edo()
    return Piece(
        iri="urn:gmeow:test:piece:ferneyhough",
        title="Ferneyhough-ish Study",
        composer="Test Bot",
        voices=[
            Voice(
                iri="urn:gmeow:test:ferneyhough:flute",
                label="Flute",
                tuning=tuning,
                time_frame=time_frame,
                events=[
                    ToneEvent(
                        onset=Fraction(0, 1),
                        duration=Fraction(1, 12),
                        pitch=PitchValue.from_midi_number(72, spelled_name="C5"),
                    ),
                    ToneEvent(
                        onset=Fraction(1, 12),
                        duration=Fraction(1, 12),
                        pitch=PitchValue.from_midi_number(74, spelled_name="D5"),
                    ),
                    ToneEvent(
                        onset=Fraction(2, 12),
                        duration=Fraction(1, 12),
                        pitch=PitchValue.from_midi_number(76, spelled_name="E5"),
                    ),
                    ToneEvent(
                        onset=Fraction(3, 12),
                        duration=Fraction(1, 12),
                        pitch=PitchValue.from_midi_number(77, spelled_name="F5"),
                    ),
                    ToneEvent(
                        onset=Fraction(4, 12),
                        duration=Fraction(1, 12),
                        pitch=PitchValue.from_midi_number(79, spelled_name="G5"),
                    ),
                    ToneEvent(
                        onset=Fraction(5, 12),
                        duration=Fraction(1, 8),
                        pitch=PitchValue.from_midi_number(81, spelled_name="A5"),
                    ),
                ],
            ),
            Voice(
                iri="urn:gmeow:test:ferneyhough:clarinet",
                label="Clarinet",
                tuning=tuning,
                time_frame=time_frame,
                events=[
                    ToneEvent(
                        onset=Fraction(0, 1),
                        duration=Fraction(1, 20),
                        pitch=PitchValue.from_midi_number(55, spelled_name="G3"),
                    ),
                    ToneEvent(
                        onset=Fraction(1, 20),
                        duration=Fraction(1, 20),
                        pitch=PitchValue.from_midi_number(56, spelled_name="G#3"),
                    ),
                    ToneEvent(
                        onset=Fraction(2, 20),
                        duration=Fraction(1, 20),
                        pitch=PitchValue.from_midi_number(58, spelled_name="A#3"),
                    ),
                    ToneEvent(
                        onset=Fraction(3, 20),
                        duration=Fraction(1, 20),
                        pitch=PitchValue.from_midi_number(60, spelled_name="C4"),
                    ),
                    ToneEvent(
                        onset=Fraction(4, 20),
                        duration=Fraction(1, 20),
                        pitch=PitchValue.from_midi_number(62, spelled_name="D4"),
                    ),
                ],
            ),
        ],
    )


def piece_raga_yaman() -> Piece:
    """Oral-tradition / microtonal fixture with pitch inflections and drone."""
    time_frame = TimeFrame(
        iri="urn:gmeow:test:yaman:timeframe",
        label="Teentaal 16 beats",
        beats_per_measure=16,
        beat_unit=4,
    )
    tuning = _tuning_12edo()
    return Piece(
        iri="urn:gmeow:test:piece:yaman",
        title="Raga Yaman (sketch)",
        voices=[
            Voice(
                iri="urn:gmeow:test:yaman:vocal",
                label="Vocal",
                tuning=tuning,
                time_frame=time_frame,
                events=[
                    ToneEvent(
                        onset=Fraction(0, 1),
                        duration=Fraction(1, 4),
                        pitch=PitchValue.from_midi_number(60, spelled_name="Sa"),
                    ),
                    ToneEvent(
                        onset=Fraction(1, 4),
                        duration=Fraction(1, 4),
                        pitch=PitchValue.from_midi_number(64.1, spelled_name="Ga+10c"),
                    ),
                    ToneEvent(
                        onset=Fraction(2, 4),
                        duration=Fraction(1, 8),
                        pitch=PitchValue.from_midi_number(67, spelled_name="Pa"),
                    ),
                    ToneEvent(
                        onset=Fraction(5, 8),
                        duration=Fraction(1, 8),
                        pitch=PitchValue.from_midi_number(71, spelled_name="Ni"),
                    ),
                    ToneEvent(
                        onset=Fraction(6, 8),
                        duration=Fraction(1, 4),
                        pitch=PitchValue.from_midi_number(69, spelled_name="Dha"),
                    ),
                ],
            ),
            Voice(
                iri="urn:gmeow:test:yaman:drone",
                label="Drone",
                tuning=tuning,
                time_frame=time_frame,
                events=[
                    ToneEvent(
                        onset=Fraction(0, 1),
                        duration=Fraction(4, 1),
                        pitch=PitchValue.from_midi_number(60, spelled_name="Sa"),
                    ),
                ],
            ),
        ],
    )


def piece_math_rock() -> Piece:
    """Polymeter fixture: guitar in 4/4 with drop-D tuning, drums in 7/8."""
    guitar_frame = TimeFrame(
        iri="urn:gmeow:test:mathrock:guitar:timeframe",
        label="4/4",
        beats_per_measure=4,
        beat_unit=4,
    )
    drum_frame = TimeFrame(
        iri="urn:gmeow:test:mathrock:drums:timeframe",
        label="7/8",
        beats_per_measure=7,
        beat_unit=8,
    )
    drop_d_tuning = TuningSystem(
        iri="urn:gmeow:test:mathrock:dropd",
        label="Drop-D",
        reference_hz=440.0,
    )
    return Piece(
        iri="urn:gmeow:test:piece:mathrock",
        title="Math-Rock Polymeter",
        voices=[
            Voice(
                iri="urn:gmeow:test:mathrock:guitar",
                label="Guitar (Drop-D)",
                tuning=drop_d_tuning,
                time_frame=guitar_frame,
                events=[
                    ToneEvent(
                        onset=Fraction(0, 1),
                        duration=Fraction(1, 8),
                        pitch=PitchValue.from_midi_number(50, spelled_name="D3"),
                    ),
                    ToneEvent(
                        onset=Fraction(1, 8),
                        duration=Fraction(1, 8),
                        pitch=PitchValue.from_midi_number(52, spelled_name="E3"),
                    ),
                    ToneEvent(
                        onset=Fraction(2, 8),
                        duration=Fraction(1, 4),
                        pitch=PitchValue.from_midi_number(54, spelled_name="F#3"),
                    ),
                    ToneEvent(
                        onset=Fraction(4, 8),
                        duration=Fraction(1, 8),
                        pitch=PitchValue.from_midi_number(55, spelled_name="G3"),
                    ),
                    ToneEvent(
                        onset=Fraction(5, 8),
                        duration=Fraction(3, 8),
                        pitch=PitchValue.from_midi_number(50, spelled_name="D3"),
                    ),
                ],
            ),
            Voice(
                iri="urn:gmeow:test:mathrock:drums",
                label="Drums",
                time_frame=drum_frame,
                events=[
                    ToneEvent(
                        onset=Fraction(0, 8), duration=Fraction(1, 8), is_unpitched=True
                    ),
                    ToneEvent(
                        onset=Fraction(1, 8), duration=Fraction(1, 8), is_unpitched=True
                    ),
                    ToneEvent(
                        onset=Fraction(2, 8), duration=Fraction(1, 8), is_unpitched=True
                    ),
                    ToneEvent(
                        onset=Fraction(3, 8), duration=Fraction(1, 8), is_unpitched=True
                    ),
                    ToneEvent(
                        onset=Fraction(4, 8), duration=Fraction(1, 8), is_unpitched=True
                    ),
                    ToneEvent(
                        onset=Fraction(5, 8), duration=Fraction(1, 8), is_unpitched=True
                    ),
                    ToneEvent(
                        onset=Fraction(6, 8), duration=Fraction(1, 8), is_unpitched=True
                    ),
                ],
            ),
        ],
    )


def _write_gts(piece: Piece, tmp_path: Path) -> Path:
    graph = writer.piece_to_graph(piece)
    data = gts_from_graph(graph)
    slug = "".join(c if c.isalnum() else "_" for c in (piece.title or "piece")).lower()
    gts_path = tmp_path / f"{slug}.gts"
    gts_path.write_bytes(data)
    return gts_path


def _format_ext(format_name: str) -> str:
    return {"musicxml": "musicxml", "lilypond": "ly", "abc": "abc", "midi": "mid"}[
        format_name
    ]


def _validate_rendered_format(path: Path, format_name: str) -> None:
    """Smoke-validate that a rendered file is well-formed for its format."""
    data = path.read_bytes()
    if format_name == "musicxml":
        ET.parse(path)  # raises if not well-formed XML
    elif format_name == "midi":
        assert data[:4] == b"MThd", f"{path} is not a valid MIDI file"
    elif format_name == "lilypond":
        text = data.decode("utf-8", errors="replace")
        assert "\\version" in text, f"{path} lacks a LilyPond version statement"
    elif format_name == "abc":
        text = data.decode("utf-8", errors="replace")
        assert "X:" in text, f"{path} lacks an ABC reference number"


@pytest.mark.parametrize(
    "piece_factory",
    [piece_ferneyhough, piece_raga_yaman, piece_math_rock],
    ids=["ferneyhough", "raga_yaman", "math_rock"],
)
def test_flagship_bundle_renders_with_manifests(
    tmp_path: Path, piece_factory: Callable[[], Piece]
) -> None:
    piece = piece_factory()
    gts_path = _write_gts(piece, tmp_path)

    piece_slug = piece.iri.split(":")[-1] if piece.iri is not None else "piece"
    for fmt in FORMATS:
        ext = _format_ext(fmt)
        out_path = tmp_path / f"{piece_slug}.{ext}"
        result = runner.invoke(
            music_app,
            ["render", str(gts_path), "--to", fmt, "-o", str(out_path)],
        )
        assert result.exit_code == 0, (
            f"render failed for {piece.title} -> {fmt}: {result.output}"
        )
        assert out_path.exists(), f"missing output file: {out_path}"
        assert out_path.stat().st_size > 0, f"empty output file: {out_path}"
        _validate_rendered_format(out_path, fmt)

        manifest_path = out_path.with_suffix(out_path.suffix + ".manifest.ttl")
        assert manifest_path.exists(), f"missing manifest: {manifest_path}"

        manifest_graph = Graph()
        manifest_graph.parse(manifest_path, format="turtle")

        profile = get_profile(fmt)
        notation_node = URIRef(profile.notation_system)
        function_node = URIRef(profile.projection_function)

        subjects = set(
            manifest_graph.subjects(GMEOW.targetNotationSystem, notation_node)
        )
        assert subjects, (
            f"manifest for {fmt} does not reference {profile.notation_system}"
        )

        for subject in subjects:
            assert (
                subject,
                GMEOW.projectionFunction,
                function_node,
            ) in manifest_graph, (
                f"manifest for {fmt} does not reference projection function "
                f"{profile.projection_function}"
            )
            for loss in profile.declared_losses:
                loss_node = URIRef(loss)
                assert (subject, GMEOW.declaredLoss, loss_node) in manifest_graph, (
                    f"manifest for {fmt} omits declared loss {loss}"
                )

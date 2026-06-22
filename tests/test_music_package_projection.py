# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""GTS music-package projection toolchain tests (issue #319).

Covers the narrow-waist model, GTS round-trip, every notation renderer,
MusicXML inward projection, declared-loss manifest completeness, and
import-direction lint.
"""

from __future__ import annotations

from fractions import Fraction
from pathlib import Path

import music21
import pytest
from gmeow_rdf.compat.rdflib import RDF, Namespace, URIRef
from typer.testing import CliRunner

from gmeow_tools.ext.music import importer, reader, writer
from gmeow_tools.ext.music.cli import app as music_app
from gmeow_tools.ext.music.loss_manifest import get_profile, list_formats
from gmeow_tools.ext.music.model import (
    Piece,
    PitchValue,
    TimeFrame,
    ToneEvent,
    TuningSystem,
    Voice,
)
from gmeow_tools.ext.music.serializers import abc, musicxml, scl
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.gts_producer import gts_from_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")

runner = CliRunner()


def _make_piece() -> Piece:
    """Return a simple, deterministic test piece."""
    return Piece(
        iri="urn:gmeow:test:piece:1",
        title="Test Piece",
        voices=[
            Voice(
                iri="urn:gmeow:test:voice:1",
                label="Melody",
                tuning=TuningSystem(
                    iri="https://blackcatinformatics.ca/gmeow/tuningSystem12EDO",
                    label="12-EDO",
                    division_count=12,
                ),
                time_frame=TimeFrame(
                    iri="urn:gmeow:test:timeframe:1",
                    label="4/4",
                    beats_per_measure=4,
                    beat_unit=4,
                ),
                events=[
                    ToneEvent(
                        onset=Fraction(0, 1),
                        duration=Fraction(1, 4),
                        pitch=PitchValue.from_midi_number(60, spelled_name="C4"),
                    ),
                    ToneEvent(
                        onset=Fraction(1, 4),
                        duration=Fraction(1, 4),
                        pitch=PitchValue.from_midi_number(62, spelled_name="D4"),
                    ),
                    ToneEvent(
                        onset=Fraction(1, 2),
                        duration=Fraction(1, 2),
                        pitch=PitchValue.from_midi_number(64, spelled_name="E4"),
                    ),
                ],
            )
        ],
    )


def test_gts_round_trip(tmp_path: Path) -> None:
    """A piece -> graph -> GTS -> file -> piece round-trips structurally."""
    piece = _make_piece()
    graph = writer.piece_to_graph(piece)
    data = gts_from_graph(graph)
    gts_path = tmp_path / "test.gts"
    gts_path.write_bytes(data)

    loaded = reader.piece_from_gts(gts_path)
    assert loaded.title == piece.title
    assert len(loaded.voices) == len(piece.voices)

    original_events = piece.all_events()
    loaded_events = loaded.all_events()
    assert len(loaded_events) == len(original_events)
    for (_ov, orig), (_lv, load) in zip(original_events, loaded_events, strict=True):
        assert orig.onset == load.onset
        assert orig.duration == load.duration
        assert orig.pitch is not None and load.pitch is not None
        assert round(orig.pitch.cents, 4) == round(load.pitch.cents, 4)


def test_musicxml_renders_and_music21_parses(tmp_path: Path) -> None:
    """The MusicXML renderer emits a file music21 can ingest."""
    piece = _make_piece()
    profile = get_profile("musicxml")
    xml = musicxml.render(piece, profile)

    assert "<?xml" in xml
    assert "<score-partwise" in xml

    source = tmp_path / "test.musicxml"
    source.write_text(xml, encoding="utf-8")
    score = music21.converter.parse(str(source))
    notes = [
        n for n in score.flatten().notesAndRests if isinstance(n, music21.note.Note)
    ]
    assert len(notes) == 3
    assert notes[0].pitch.ps == 60.0
    assert notes[1].pitch.ps == 62.0
    assert notes[2].pitch.ps == 64.0


def test_scl_format() -> None:
    """Scala .scl output follows the documented format."""
    piece = _make_piece()
    profile = get_profile("scl")
    output = scl.render(piece, profile)
    lines = output.strip().splitlines()

    assert lines[0].startswith("!")
    non_comment = [line for line in lines if not line.startswith("!")]
    assert non_comment[0] == "12"
    assert non_comment[1] == piece.title
    assert non_comment[2] == "0."
    assert non_comment[-1] == "1200.000000"
    assert len(non_comment) == 15  # count + title + 13 degrees (origin + octave)


def test_abc_round_trip_via_music21(tmp_path: Path) -> None:
    """ABC notation renders and music21 parses it back to three notes."""
    piece = _make_piece()
    profile = get_profile("abc")
    abc_str = abc.render(piece, profile)

    assert "X:1" in abc_str
    assert "T:Test Piece" in abc_str

    score = music21.converter.parse(abc_str, format="abc")
    notes = [
        n for n in score.flatten().notesAndRests if isinstance(n, music21.note.Note)
    ]
    assert len(notes) == 3


def test_loss_manifest_completeness() -> None:
    """Static loss profiles mirror the music-slice ontology profiles."""
    graph = load_merged_graph(include_imports=False)

    format_to_profile = {
        "musicxml": GMEOW.profileMusicXML,
        "mei": GMEOW.profileMEI,
        "tab": GMEOW.profileTablature,
        "lilypond": GMEOW.profileLilyPond,
        "abc": GMEOW.profileABC,
        "scl": GMEOW.profileSCL,
        "midi": GMEOW.profileMIDI,
        "kern": GMEOW.profileKern,
        "mensural": GMEOW.profileMensural,
        "graphic": GMEOW.profileGraphic,
    }

    for fmt, profile_iri in format_to_profile.items():
        static = get_profile(fmt)
        ontology_params = {
            str(o) for o in graph.objects(profile_iri, GMEOW.representableParameter)
        }
        ontology_losses = {
            str(o) for o in graph.objects(profile_iri, GMEOW.declaredLoss)
        }
        assert ontology_params == set(static.representable_parameters), fmt
        assert ontology_losses == set(static.declared_losses), fmt

        proj = graph.value(profile_iri, GMEOW.projectionFunction)
        notation = graph.value(profile_iri, GMEOW.notationSystemOf)
        assert str(proj) == static.projection_function, fmt
        assert str(notation) == static.notation_system, fmt

    for fmt in list_formats():
        static = get_profile(fmt)
        for loss in static.declared_losses:
            assert (URIRef(loss), RDF.type, GMEOW.ProjectionLoss) in graph, loss
        for param in static.representable_parameters:
            assert (URIRef(param), RDF.type, GMEOW.MusicalParameter) in graph, param


def test_import_provenance_and_lint(tmp_path: Path) -> None:
    """MusicXML import produces a GTS + provenance manifest and rejects non-XML."""
    piece = _make_piece()
    profile = get_profile("musicxml")
    xml = musicxml.render(piece, profile)

    source = tmp_path / "source.musicxml"
    source.write_text(xml, encoding="utf-8")

    # Import-direction lint: only MusicXML extensions are accepted.
    bad = tmp_path / "source.txt"
    bad.write_text(xml, encoding="utf-8")
    with pytest.raises(ValueError, match="MusicXML import only supports"):
        importer.piece_from_musicxml(bad)

    # CLI import writes both the GTS and a provenance sidecar.
    out = tmp_path / "out.gts"
    result = runner.invoke(music_app, ["import", str(source), "-o", str(out)])
    assert result.exit_code == 0, result.output
    assert out.exists()

    manifest = out.with_suffix(out.suffix + ".manifest.ttl")
    assert manifest.exists()
    manifest_text = manifest.read_text(encoding="utf-8")
    assert "prov:wasDerivedFrom" in manifest_text
    assert str(source.absolute().as_uri()) in manifest_text

    imported = reader.piece_from_gts(out)
    assert len(imported.voices[0].events) == len(piece.voices[0].events)

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""``gmeow-music`` CLI entry point."""

from __future__ import annotations

from pathlib import Path
from typing import Annotated, Protocol

import typer

from gmeow_tools.ext.music import importer, reader, writer
from gmeow_tools.ext.music.loss_manifest import (
    NotationProfile,
    get_profile,
    import_manifest_turtle,
    manifest_turtle,
)
from gmeow_tools.ext.music.model import Piece
from gmeow_tools.ext.music.serializers import (
    abc,
    graphic,
    kern,
    lilypond,
    mei,
    mensural,
    midi,
    musicxml,
    scl,
    tab,
)
from gmeow_tools.gts_producer import gts_from_graph

app = typer.Typer(
    name="gmeow-music",
    help="GMEOW music-package projection tools.",
    no_args_is_help=True,
)


class _SerializerModule(Protocol):
    """Protocol for renderer modules."""

    def render(self, piece: Piece, profile: NotationProfile) -> str | bytes: ...


_TEXT_SERIALIZERS: dict[str, _SerializerModule] = {
    "musicxml": musicxml,
    "mei": mei,
    "tab": tab,
    "lilypond": lilypond,
    "abc": abc,
    "scl": scl,
    "kern": kern,
    "mensural": mensural,
    "graphic": graphic,
}

_BINARY_SERIALIZERS: dict[str, _SerializerModule] = {
    "midi": midi,
}

_ALL_FORMATS = sorted(_TEXT_SERIALIZERS.keys() | _BINARY_SERIALIZERS.keys())


def _render_piece(piece: Piece, format_name: str) -> str | bytes:
    profile = get_profile(format_name)
    if format_name in _TEXT_SERIALIZERS:
        return _TEXT_SERIALIZERS[format_name].render(piece, profile)
    if format_name in _BINARY_SERIALIZERS:
        return _BINARY_SERIALIZERS[format_name].render(piece, profile)
    raise typer.BadParameter(f"unsupported format: {format_name}")


@app.command()
def render(
    source: Annotated[Path, typer.Argument(help="Source .gts music-package file.")],
    to: Annotated[str, typer.Option(help=f"Output format: {', '.join(_ALL_FORMATS)}.")],
    out: Annotated[Path, typer.Option("--out", "-o", help="Output file.")],
) -> None:
    """Project a GTS music-package to a notation format."""
    piece = reader.piece_from_gts(source)
    result = _render_piece(piece, to.lower())
    out.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(result, bytes):
        out.write_bytes(result)
    else:
        out.write_text(result, encoding="utf-8")
    manifest_path = out.with_suffix(out.suffix + ".manifest.ttl")
    manifest_path.write_text(
        manifest_turtle(
            to.lower(),
            provenance=f"gmeow music render {source.name} --to {to} -o {out.name}",
        ),
        encoding="utf-8",
    )
    typer.echo(f"wrote {out}")
    typer.echo(f"wrote {manifest_path}")


@app.command(name="import")
def import_(
    source: Annotated[Path, typer.Argument(help="Source MusicXML file.")],
    out: Annotated[Path, typer.Option("--out", "-o", help="Output .gts file.")],
) -> None:
    """Project a MusicXML file into a GTS music-package."""
    piece = importer.piece_from_musicxml(source)
    graph = writer.piece_to_graph(piece)
    data = gts_from_graph(graph)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(data)
    manifest_path = out.with_suffix(out.suffix + ".manifest.ttl")
    manifest_path.write_text(
        import_manifest_turtle(
            source,
            piece.iri or "urn:gmeow:piece:imported",
            provenance=f"gmeow music import {source.name} -o {out.name}",
        ),
        encoding="utf-8",
    )
    typer.echo(f"wrote {out}")
    typer.echo(f"wrote {manifest_path}")

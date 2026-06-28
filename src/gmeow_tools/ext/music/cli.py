# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""``gmeow-music`` CLI entry point."""

from __future__ import annotations

from pathlib import Path
from typing import Annotated, Any, NoReturn, cast

import typer
from gmeow_native import music as _native_music

app = typer.Typer(
    name="gmeow-music",
    help="GMEOW music-package projection tools.",
    no_args_is_help=True,
)

_music = cast(Any, _native_music)
_ALL_FORMATS: list[str] = sorted(cast(list[str], _music.list_formats()))


def _raise_music_runtime_error(message: str) -> NoReturn:
    typer.echo(f"Error: {message}", err=True)
    raise typer.Exit(1)


def _raise_music_value_error(exc: ValueError) -> NoReturn:
    message = str(exc)
    if message.startswith(
        (
            "unsupported format:",
            "MusicXML import only supports",
        )
    ):
        raise typer.BadParameter(message) from exc
    _raise_music_runtime_error(message)


@app.command()
def render(
    source: Annotated[Path, typer.Argument(help="Source .gts music-package file.")],
    to: Annotated[str, typer.Option(help=f"Output format: {', '.join(_ALL_FORMATS)}.")],
    out: Annotated[Path, typer.Option("--out", "-o", help="Output file.")],
) -> None:
    """Project a GTS music-package to a notation format."""
    try:
        written = _music.render_file(str(source), to.lower(), str(out))
    except ValueError as exc:
        _raise_music_value_error(exc)
    except OSError as exc:
        _raise_music_runtime_error(str(exc))
    for path in written:
        typer.echo(f"wrote {path}")


@app.command(name="import")
def import_(
    source: Annotated[Path, typer.Argument(help="Source MusicXML file.")],
    out: Annotated[Path, typer.Option("--out", "-o", help="Output .gts file.")],
) -> None:
    """Project a MusicXML file into a GTS music-package."""
    try:
        written = _music.import_file(str(source), str(out))
    except ValueError as exc:
        _raise_music_value_error(exc)
    except OSError as exc:
        _raise_music_runtime_error(str(exc))
    for path in written:
        typer.echo(f"wrote {path}")

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Notation projection serializers.

Each module exposes a ``render(piece, profile) -> str | bytes`` function.  The
CLI dispatches on the target format name; every renderer also emits a
Turtle loss-manifest sidecar via :py:mod:`gmeow_tools.ext.music.loss_manifest`.
"""

from __future__ import annotations

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

__all__ = [
    "abc",
    "graphic",
    "kern",
    "lilypond",
    "mei",
    "mensural",
    "midi",
    "musicxml",
    "scl",
    "tab",
]

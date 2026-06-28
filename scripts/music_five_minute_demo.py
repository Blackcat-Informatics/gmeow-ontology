#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Five-minute demo of the Rust music-package projection toolchain.

Run from the repository root:

    make native-py
    uv run python scripts/music_five_minute_demo.py

Outputs are written to ``dist/music-demo/``.
"""

from __future__ import annotations

import shutil
from pathlib import Path
from typing import Any, cast

from gmeow_native import music as _native_music

music = cast(Any, _native_music)

_SEED_MUSICXML = """<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <work><work-title>GMEOW Five-Minute Demo</work-title></work>
  <part-list><score-part id="P1"><part-name>Melody</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>48</divisions>
        <time><beats>4</beats><beat-type>4</beat-type></time>
        <clef><sign>G</sign><line>2</line></clef>
      </attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>12</duration></note>
      <note><pitch><step>D</step><octave>4</octave></pitch><duration>12</duration></note>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>12</duration></note>
      <note><pitch><step>F</step><octave>4</octave></pitch><duration>12</duration></note>
      <note><pitch><step>G</step><octave>4</octave></pitch><duration>12</duration></note>
      <note><pitch><step>A</step><octave>4</octave></pitch><duration>12</duration></note>
      <note><pitch><step>B</step><octave>4</octave></pitch><duration>12</duration></note>
      <note><pitch><step>C</step><octave>5</octave></pitch><duration>24</duration></note>
    </measure>
  </part>
</score-partwise>
"""


def _print_written(paths: list[str]) -> None:
    for path in paths:
        print(f"wrote {path}")


def main() -> int:
    """Import a seed MusicXML fragment, then render the GTS package outward."""
    out = Path("dist/music-demo")
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)

    seed = out / "seed.musicxml"
    seed.write_text(_SEED_MUSICXML, encoding="utf-8")
    print(f"wrote {seed}")

    gts_path = out / "demo.gts"
    _print_written(cast(list[str], music.import_file(str(seed), str(gts_path))))

    for fmt in ("musicxml", "abc", "scl", "midi"):
        suffix = "midi" if fmt == "midi" else fmt
        _print_written(
            cast(
                list[str],
                music.render_file(str(gts_path), fmt, str(out / f"demo.{suffix}")),
            )
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

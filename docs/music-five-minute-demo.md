<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Five-minute demo: GTS music-package projections

This walkthrough shows the `music-package` GTS profile in action.  In a few
commands you can project frame-relative canonical music data out to common
notation formats and pull a MusicXML file back into the same canonical model.

## What you need

- A GMEOW checkout with the `[music]` optional dependency installed:

  ```bash
  uv sync --extra music
  ```

- The `gmeow-music` CLI entry point (installed automatically by `uv sync`).

## Run the scripted demo

```bash
uv run --extra music python scripts/music_five_minute_demo.py
```

The script creates a short C-major scale fragment in the canonical model,
renders it to MusicXML, ABC, Scala `.scl`, and MIDI, emits a canonical
`demo.gts` package, then re-imports the MusicXML file into a second GTS file.
All outputs land in `dist/music-demo/`.

Expected output:

```text
wrote dist/music-demo/demo.musicxml
wrote dist/music-demo/demo.abc
wrote dist/music-demo/demo.scl
wrote dist/music-demo/demo.midi
wrote dist/music-demo/demo.gts
events: original=8 imported=8
music21 parsed 8 notes from demo.musicxml
```

## Use the CLI directly

### Render a GTS music-package to a notation file

```bash
# From the demo script output
gmeow music render dist/music-demo/demo.gts --to musicxml -o out.musicxml
```

Each renderer writes a sidecar manifest describing what it can represent and
what it knowingly loses:

```bash
cat out.musicxml.manifest.ttl
```

### Import a MusicXML file into a GTS music-package

```bash
gmeow music import out.musicxml -o reimported.gts
cat reimported.gts.manifest.ttl
```

Import is intentionally lossy: tuning frame defaults to 12-EDO, metric
structure defaults to 4/4, and notation-specific semantics are discarded.  The
manifest records the provenance of that inward projection.

## What the files mean

| File | Role |
|------|------|
| `demo.gts` | Canonical GTS package carrying the frame-relative piece. |
| `demo.musicxml` | Outward projection to MusicXML 4.0. |
| `demo.abc` | Outward projection to ABC notation. |
| `demo.scl` | Outward projection of the tuning frame to Scala `.scl`. |
| `demo.midi` | Outward projection to Type-0 MIDI bytes. |
| `*.manifest.ttl` | Declared-loss / provenance sidecar for each projection. |
| `demo-imported.gts` | Inward MusicXML projection back to the canonical model. |

## Next steps

- Read GTS-SPEC.md §13 for the `music-package` profile wire format.
- See `src/gmeow_tools/ext/music/` for the solver, renderers, and importer.
- Add new notation targets by implementing a renderer module and registering a
  `NotationProjectionProfile` in `slices/extensions/music/module.ttl`.

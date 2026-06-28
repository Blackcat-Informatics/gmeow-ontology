# Retention: `tests/test_music*.py` (package toolchain)

Covers `test_music.py`, `test_music_flagship_bundles.py`,
`test_music_package_projection.py`.

**Category:** Python tool algorithm

## What it tests

The music GTS package toolchain (`gmeow_tools.ext.music` + the `gmeow-music` CLI):
end-to-end piece→graph→GTS→notation rendering (MusicXML/LilyPond/ABC/MIDI/Scala),
the importer, GTS round-trips, and the loss-manifest completeness check (static
Python loss profiles match the ontology profile definitions). `test_music.py` also
scans the committed SHACL shapes for the oral-tradition guarantee.

## Why it cannot move to Rust today

The writer / reader / serializers / importer / loss-manifest are live **Python**,
and the rendering pipeline is driven through the Python `gmeow-music` CLI. The
tests assert end-to-end rendered output and round-trips that only a Rust port can
subsume. (The music *ontology* TBox/conformance tests were already deleted —
covered by `slices/extensions/music/tests/structural.ttl` + `conformance_music_*.rs`.)

## What is needed to move it to Rust

Port the piece→graph→GTS→notation pipeline, the importer, and the loss-manifest
consistency check to Rust with crate tests + goldens; render via a Rust music CLI.
Then delete these files and this dossier.

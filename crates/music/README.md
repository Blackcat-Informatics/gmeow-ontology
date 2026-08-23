<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-music

`gmeow-music` is the Rust-owned music-package toolchain. It handles GTS package
I/O, MusicXML import, notation renderers, and loss manifests for music-oriented
GMEOW data.

## Source Map

The crate is currently concentrated in `src/lib.rs`, with the optional Python
surface in `src/py.rs` behind the `python` feature. Core responsibilities are:

- rational musical time and notation primitives;
- MusicXML import into GMEOW RDF structures;
- GTS package read/write and manifest handling;
- rendered notation and loss-report outputs.

## Checks

```bash
make native-py
make rust-docs
```

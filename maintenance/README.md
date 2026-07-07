<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# `maintenance/` — unattached, unmaintained off-gate artifacts

This directory holds one-shot maintainer tools that are **wired into nothing** — no
Makefile target, no `make check` / CI gate, no pytest collection (it is outside
`testpaths`), no import from any crate or module, and no lint (it is in the ruff
`extend-exclude`). They are **not part of the ontology** and carry no ongoing
maintenance obligation; each is kept only because we might need to run it again.

Anything here is expected to be run **off-gate** by a maintainer (the repository
itself runs no on-gate inference or network). Treat these as reference artifacts,
not as supported entry points.

## Contents

- [`affect-classifier-capture/capture_fixtures.py`](affect-classifier-capture/capture_fixtures.py)
  — regenerates the affect classifier capture fixtures
  (`crates/affect-ingest/fixtures/*-sample.json`) byte-for-byte by running each
  pinned Hugging Face model over the three fixed target texts. See that crate's
  `fixtures/README.md` for the full reproduction procedure.

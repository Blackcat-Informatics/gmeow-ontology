# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The bundle is self-sufficient: transpile runs from the wheel, no repo (#bundle).

The CLI razor is ``gmeow`` does not need a repo. These tests PROVE it for the
transpile path by running it in a child process with **every repo source path
pointed at a nonexistent directory** — so the up-projection lift map, the
projection queries, the equivalence/projection cells, the merged ontology graph,
and the saturation refusal set must ALL come from ``generated/dist/gmeow-full.gts``.

A fresh child process per run is the honest test: an in-process patch could be
fooled by a loader cache populated from the repo on an earlier call.
"""

from __future__ import annotations

import json
import subprocess
import sys

# Child program: optionally blind every repo path, then transpile a fixed source
# and print JSON metrics. ``wheel`` argv blinds the paths; ``repo`` does not.
_CHILD = r"""
import sys, json, tempfile
from pathlib import Path

if sys.argv[1] == "wheel":
    # Patch the config constants at the SOURCE before importing any consumer
    # module. Python caches modules, so every later `from gmeow_tools.config
    # import <PATH>` binds the blinded value — no need to chase each module's copy.
    import gmeow_tools.config as cfg

    GONE = Path("/nonexistent/gmeow-wheel-sim")
    cfg.ONTOLOGY_FILE = GONE / "ontology/gmeow.ttl"
    cfg.MAPPINGS_DIR = GONE / "generated/mappings"
    cfg.MAPPING_DSL_DIR = GONE / "dsl/mappings"
    cfg.SLICES_DIR = GONE / "slices"
    cfg.PROJECTION_QUERY_DIR = GONE / "generated/queries"

from rdflib import Graph, Literal, URIRef
from gmeow_tools.transpile import transpile_graph

src = Graph()
a = URIRef("https://ex.org/ada")
src.add((a, URIRef("https://schema.org/name"), Literal("Ada Lovelace", lang="en")))
src.add((a, URIRef("https://schema.org/knows"), URIRef("https://ex.org/bob")))
src.add((a, URIRef("https://schema.org/birthDate"), Literal("1815-12-10")))
with tempfile.TemporaryDirectory() as d:
    rep = transpile_graph(src, "probe", out_dir=Path(d))
    out = Graph()
    out.parse(Path(d) / "index.ttl", format="turtle")
    leak = sum(
        1
        for _s, _p, o in out
        if isinstance(o, Literal) and (o.language or "").startswith("x-gmeow")
    )
    print(json.dumps({
        "lifted": rep.lifted,
        "asserted": rep.transform.asserted,
        "projected": rep.transform.projected,
        "output": len(out),
        "xgmeow_leak": leak,
    }))
"""


def _run(mode: str) -> dict[str, int]:
    proc = subprocess.run(
        [sys.executable, "-c", _CHILD, mode],
        capture_output=True,
        text=True,
        timeout=300,
        check=True,
    )
    result: dict[str, int] = json.loads(proc.stdout.strip().splitlines()[-1])
    return result


def test_transpile_runs_purely_from_the_bundle() -> None:
    """Wheel mode (every repo path blinded) transpiles non-trivially from the bundle."""
    wheel = _run("wheel")
    assert wheel["lifted"] > 0, "nothing lifted — the bundled lift map was not read"
    assert wheel["projected"] > 0, "no projections — the bundled queries were not read"
    assert wheel["output"] > wheel["asserted"], "no fan-out — maximal did not fire"
    assert wheel["xgmeow_leak"] == 0, "internal tag leaked into the consumer output"


def test_wheel_mode_matches_repo_mode_exactly() -> None:
    """The bundle is a faithful stand-in: blinded run == repo run, metric for metric."""
    assert _run("wheel") == _run("repo")

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Acceptance tests for the OKF (Open Knowledge Format) import lane.

Marked ``maintainer`` and additionally gated on a built ``gts`` binary with OKF
support (an acceptance lane — set ``GMEOW_GTS_BIN``), mirroring the network/HermiT
lanes. The OKF *export* side is folded by the native ``stage-export-okf`` and
covered by the Rust ``stages::okf`` tests; this file exercises the lift back into
GMEOW: feeding the emitted bundle through ``gts from-okf`` and mapping the
recognized ``okf:`` subset.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

import pytest
from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, SKOS, URIRef

from gmeow_tools.bundle import bundled_okf

pytestmark = pytest.mark.maintainer

_OKF_NS = "https://blackcatinformatics.ca/projects/gts/okf#"
_OKF_DIR_NAME = "gmeow-okf"


def _gts_available() -> bool:
    """Whether an *OKF-capable* gts binary is locatable for the acceptance lane.

    Locating any ``gts`` is not enough: the PyPI ``gts`` wheel is built without
    ``--features okf`` and has no ``from-okf`` subcommand, so it would make the
    acceptance tests *fail* rather than skip. Probe the actual capability so the
    lane skips cleanly on an okf-less binary and activates the moment an
    okf-capable ``gts`` (Rust, built ``--features okf``) is present.
    """
    from gmeow_tools.okf_import import OkfBinaryNotFoundError, find_gts_binary

    try:
        binary = find_gts_binary()
    except OkfBinaryNotFoundError:
        return False
    probe = subprocess.run(
        [str(binary), "from-okf", "--help"],
        capture_output=True,
        text=True,
        check=False,
    )
    return probe.returncode == 0


_requires_gts = pytest.mark.skipif(
    not _gts_available(),
    reason="gts binary with OKF support not found (build it, set GMEOW_GTS_BIN)",
)


@pytest.fixture(scope="module")
def bundle(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """Materialize the Rust-folded OKF bundle to disk once for the module.

    The bundle is produced by the native ``stage-export-okf`` and folded into
    ``gmeow.gts``; ``bundled_okf()`` returns it as ``{bundle-relative-path: bytes}``,
    which we write out so ``gts from-okf`` can fold the real emitted surface.
    """
    root = tmp_path_factory.mktemp("okf")
    docs = bundled_okf()
    assert docs, "no OKF bundle folded into gmeow.gts"
    for relpath, data in docs.items():
        target = root / relpath
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    return root / _OKF_DIR_NAME


@_requires_gts
def test_gts_from_okf_folds_our_bundle(bundle: Path, tmp_path: Path) -> None:
    """The bundle we emit is conformant: ``gts from-okf`` folds it without error."""
    from gmeow_tools.okf_import import find_gts_binary

    out = tmp_path / "folded.gts"
    proc = subprocess.run(
        [str(find_gts_binary()), "from-okf", str(bundle), "-o", str(out)],
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.returncode == 0, proc.stderr
    assert out.stat().st_size > 0


@_requires_gts
def test_lift_roundtrips_recognized_subset_and_retains_unknown(tmp_path: Path) -> None:
    """Lift maps the recognized okf: subset and retains unknown keys verbatim."""
    from gmeow_tools.okf_import import lift_okf_graph, okf_dir_to_graph

    okf = tmp_path / "hand"
    (okf / "concepts").mkdir(parents=True)
    (okf / "concepts" / "widget.md").write_text(
        "---\n"
        "type: Class\n"
        "title: Widget\n"
        "description: A small UI component.\n"
        "resource: https://example.org/onto/Widget\n"
        "scope_notes:\n  - Use for interactive controls.\n"
        "custom_field: keep-me\n"
        "---\nA small UI component.\n",
        encoding="utf-8",
    )
    graph = okf_dir_to_graph(okf)
    lifted, report = lift_okf_graph(graph)
    widget = URIRef("https://example.org/onto/Widget")
    assert (widget, RDF.type, OWL.Class) in lifted
    assert next(lifted.objects(widget, RDFS.label), None) is not None
    assert next(lifted.objects(widget, SKOS.definition), None) is not None
    assert next(lifted.objects(widget, SKOS.scopeNote), None) is not None
    # the unknown frontmatter key survives as a provenance-bearing okf: annotation
    custom = URIRef(_OKF_NS + "custom_field")
    assert next(lifted.objects(widget, custom), None) is not None
    assert report.lifted >= 4
    assert report.retained >= 1

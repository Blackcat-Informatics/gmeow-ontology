# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Bundle blob integrity — the coverage that was MISSING when the #861 pipeline
cutover silently dropped the gmeow.gts blob writer.

The new pipeline (#861/#863) regenerated ``generated/dist/gmeow.gts`` from the
named RDF graphs alone and folded ZERO content-addressed blobs: every wheel-mode
consumer archive (``mappings``/``cells``/``queries``/``tests``) vanished, and all
75 per-slice ``gmeow:guideBlob`` reference triples dangled with no backing blob.
Nothing caught it: ``docs_model_golden`` snapshots only a term count, and the one
test that exercised real bundle output
(``test_extract_docs_unpacks_site_from_bundled_snapshot``) was red and being bypassed.

These two assertions ARE that missing coverage — they read the committed bundle
through the same loaders the wheel-mode tools use and fail loudly if the blob
writer ever regresses again:

* the four consumer archives are present and non-empty;
* every ``gmeow:guideBlob`` digest reference is backed by an actual blob (no
  dangling references — the docs content is really embedded, not just pointed at).
"""

from __future__ import annotations

from gmeow_tools.bundle import (
    _GUIDE_BLOB,
    _bundle_graph,
    bundled_axioms,
    bundled_cells,
    bundled_ontology_docs,
    bundled_queries,
    bundled_reasoning,
    bundled_shapes,
    bundled_sssom,
    bundled_tests,
)


def test_bundle_carries_the_consumer_archives() -> None:
    """The wheel-mode consumer archives are folded into gmeow.gts as blobs.

    Covers: lift maps / cells / queries / test specs (the #861-dropped writer,
    restored) plus the shapes surface, compiled logic/DL axioms, and reasoning
    reports added in #746.

    Each assert pins Rust↔Python rep-string agreement against the committed
    bundle: if the Rust producer's rep-string const and the Python ``REP_X``
    const ever drift, ``_archive()`` silently returns ``{}`` and the bundle
    ships without that surface — this is the guard that catches it.
    """
    assert bundled_sssom(), "mappings-archive blob missing from gmeow.gts"
    assert bundled_cells(), "cells-archive blob missing from gmeow.gts"
    assert bundled_queries(), "queries-archive blob missing from gmeow.gts"
    assert bundled_tests(), "tests-archive blob missing from gmeow.gts"
    assert bundled_shapes(), "shapes-archive blob missing from gmeow.gts"
    assert bundled_axioms(), "axioms-archive blob missing from gmeow.gts"
    assert bundled_reasoning(), "reasoning-archive blob missing from gmeow.gts"


def test_bundle_carries_the_ontology_docs_site() -> None:
    """The full ontology-docs site is folded into gmeow.gts as the
    ``ontology-docs`` blob (#897) — the producer half of repo-free
    ``gmeow extract-docs``, which was designed but never wired until now."""
    site = bundled_ontology_docs()
    assert site, "ontology-docs blob missing from gmeow.gts"
    # Members carry the internal language-tag prefix the consumer filters on, and
    # the English landing page + structural assets are present.
    assert all(name.startswith("x-gmeow-") for name in site), (
        f"every member must carry an internal-tag prefix, got e.g. {list(site)[:3]}"
    )
    assert "x-gmeow-english/index.html" in site, "English landing page missing"
    assert "x-gmeow-english/assets/gmeow.css" in site, "site CSS asset missing"


def test_no_dangling_guide_blob_references() -> None:
    """Every gmeow:guideBlob digest reference is backed by a blob actually present
    in the bundle — the docs guide content is embedded, not a dangling pointer."""
    graph = _bundle_graph()
    refs: set[str] = {
        value
        for _s, p, o, _gid in graph.quads
        if graph.terms[p].value == _GUIDE_BLOB
        and (value := graph.terms[o].value) is not None
    }
    present = set(graph.blob_meta.keys())
    dangling = sorted(d for d in refs if d not in present)
    assert refs, (
        "no gmeow:guideBlob references found — the documentation graph regressed"
    )
    assert not dangling, (
        f"{len(dangling)} dangling gmeow:guideBlob reference(s) with no backing blob "
        f"(the docs content is not embedded): {dangling[:3]}"
    )

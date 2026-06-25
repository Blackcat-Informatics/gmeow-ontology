# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Rust-backed reverse minting layer for up-projection (#942)."""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools.up_projection import _graph_from_native_nt


def apply_reverse(source: Graph) -> Graph:
    """Run only the native reverse-projection minting layer over ``source``."""
    from gmeow_native import pipeline

    source_nt = source.serialize(format="nt", encoding="utf-8").decode("utf-8")
    graph_nt = pipeline.up_projection_reverse_nt(source_nt)
    return _graph_from_native_nt(graph_nt)

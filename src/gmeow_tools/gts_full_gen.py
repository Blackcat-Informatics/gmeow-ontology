# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0

"""The ``gts-full`` generator: the offline-ready GMEOW bundle.

``generated/dist/gmeow-full.gts`` is the complete GMEOW ontology (core +
extensions) together with the vendored import closure, documentation blobs,
SSSOM alignment axioms, and the RDF 1.2 statement-metadata layer. It is the
artifact shipped inside the ``gmeow`` PyPI package so the CLI works without a
checkout.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from blake3 import blake3
from rdflib import Graph, Literal, URIRef

from gmeow_tools.config import (
    GTS_FULL_SNAPSHOT_FILE,
    MAPPINGS_DIR,
    NAMESPACE,
    PROJECT_ROOT,
    STATEMENT_RDF12_FILE,
)
from gmeow_tools.generator import Generator, register
from gmeow_tools.graph import iter_import_files, iter_module_files
from gmeow_tools.gts_producer import compile_gts
from gmeow_tools.mappings import build_alignment_graph, load_mappings
from gmeow_tools.slices import discover_slices

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path


def _doc_blobs(graph: Graph) -> list[tuple[bytes, str, str]]:
    """Content-addressed slice guides, linked via ``gmeow:guideBlob``."""
    guide_blob = URIRef(NAMESPACE + "guideBlob")
    blobs: list[tuple[bytes, str, str]] = []
    for slice_iri, entry in sorted(discover_slices().items()):
        guide = entry.path / "docs.md"
        if not guide.exists():
            continue
        payload = guide.read_bytes()
        digest = "blake3:" + blake3(payload).hexdigest()
        graph.add((URIRef(slice_iri), guide_blob, Literal(digest)))
        blobs.append((payload, "text/markdown", f"docs:{entry.name}"))
    return blobs


@register
class GtsFullSnapshotGenerator(Generator):
    """Emit the offline-ready GTS snapshot of GMEOW plus its import closure."""

    name: str = "gts-full"

    @property
    def inputs(self) -> Sequence[Path]:
        """Everything the snapshot folds.

        Ontology, imports, statements, alignments, and guides.
        """
        from gmeow_tools.config import ONTOLOGY_FILE, SLICES_DIR

        return [
            ONTOLOGY_FILE,
            *iter_module_files(),
            *iter_import_files(),
            STATEMENT_RDF12_FILE,
            *sorted(MAPPINGS_DIR.glob("*.sssom.tsv")),
            *sorted(SLICES_DIR.glob("*/*/docs.md")),
        ]

    @property
    def outputs(self) -> Sequence[Path]:
        """The offline bundle shipped with the ``gmeow`` package."""
        return [GTS_FULL_SNAPSHOT_FILE]

    def render(self, staging: Path) -> None:
        """Compile the full snapshot into the staging tree."""
        from gmeow_tools.graph import load_merged_graph

        graph = load_merged_graph(include_imports=True)
        doc_blobs = _doc_blobs(graph)
        alignments = build_alignment_graph(load_mappings())
        data = compile_gts(
            graph,
            STATEMENT_RDF12_FILE,
            alignment_graph=alignments,
            doc_blobs=doc_blobs,
        )
        target = staging / GTS_FULL_SNAPSHOT_FILE.relative_to(PROJECT_ROOT)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)

    def compare(self, fresh: Path, committed: Path) -> list[str]:
        """Byte comparison, with semantic-vs-encoding drift diagnosis."""
        try:
            rel = str(committed.relative_to(PROJECT_ROOT))
        except ValueError:
            rel = committed.name
        if not committed.exists():
            return [f"{rel} (missing committed file)"]
        if not fresh.exists():
            return [f"{rel} (not produced in staging)"]
        fresh_bytes, committed_bytes = fresh.read_bytes(), committed.read_bytes()
        if fresh_bytes == committed_bytes:
            return []
        from gts import read, to_nquads

        a, b = read(fresh_bytes), read(committed_bytes)
        semantic = sorted(to_nquads(a).splitlines()) != sorted(
            to_nquads(b).splitlines()
        )
        kind = (
            "semantic drift — sources changed"
            if semantic
            else "encoding-only drift (identical fold; codec/library skew)"
        )
        return [f"{rel} ({kind})"]

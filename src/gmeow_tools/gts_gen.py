# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0

"""The ``gts`` generator: the committed dist snapshot — the narrow waist (#267, #12).

``generated/gmeow.gts`` is the statement-complete fold of the canonical
sources (base graph + RDF 1.2 statement layer + SSSOM alignments), emitted
byte-deterministically and drift-gated like every other artifact. Every
data-graph exporter consumes THIS file instead of re-reading rdflib/pyoxigraph
sources — one producer, many shims, zero drift between projections.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from gmeow_tools.config import (
    GTS_SNAPSHOT_FILE,
    MAPPINGS_DIR,
    PROJECT_ROOT,
    STATEMENT_RDF12_FILE,
)
from gmeow_tools.generator import Generator, register
from gmeow_tools.graph import iter_module_files
from gmeow_tools.gts_producer import compile_gts

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path


def build_snapshot_bytes() -> bytes:
    """Build the dist snapshot exactly as the generator commits it.

    Docs ride the package (#325): every slice guide embeds as a
    content-addressed markdown blob, linked from the graph via
    ``gmeow:guideBlob``, and the build FAILS if any guide anchors a missing
    term — docs-in-sync is a build invariant (Principle 7). Shared by the
    generator's render and the reproducibility tests, so there is exactly
    one definition of "the snapshot" (Principle 4).
    """
    from blake3 import blake3
    from rdflib import Literal, URIRef

    from gmeow_tools.config import NAMESPACE
    from gmeow_tools.graph import load_merged_graph
    from gmeow_tools.mappings import build_alignment_graph, load_mappings
    from gmeow_tools.slices import discover_slices
    from gmeow_tools.validate import guide_anchor_lint

    graph = load_merged_graph(include_imports=False)
    lint = guide_anchor_lint(graph)
    if lint.errors:
        details = "; ".join(lint.errors[:5])
        msg = (
            f"docs-in-sync invariant violated (#325): {len(lint.errors)} "
            f"guide anchor error(s) — {details}"
        )
        raise ValueError(msg)
    doc_blobs: list[tuple[bytes, str, str]] = []
    guide_blob = URIRef(NAMESPACE + "guideBlob")
    # The manifest-declared IRI is the slice's identity (slices.py contract);
    # directory names are checkout organization only.
    for slice_iri, entry in sorted(discover_slices().items()):
        guide = entry.path / "docs.md"
        if not guide.exists():
            continue
        payload = guide.read_bytes()
        digest = "blake3:" + blake3(payload).hexdigest()
        graph.add((URIRef(slice_iri), guide_blob, Literal(digest)))
        doc_blobs.append((payload, "text/markdown", f"docs:{entry.name}"))

    return compile_gts(
        graph,
        STATEMENT_RDF12_FILE,
        alignment_graph=build_alignment_graph(load_mappings()),
        doc_blobs=doc_blobs,
    )


@register
class GtsSnapshotGenerator(Generator):
    """Emit the byte-deterministic GTS dist snapshot of the canonical sources."""

    name: str = "gts"

    @property
    def inputs(self) -> Sequence[Path]:
        """Everything the snapshot folds: ontology, statements, alignments, guides."""
        from gmeow_tools.config import SLICES_DIR

        return [
            PROJECT_ROOT / "ontology" / "gmeow.ttl",
            *iter_module_files(),
            STATEMENT_RDF12_FILE,
            *sorted(MAPPINGS_DIR.glob("*.sssom.tsv")),
            *sorted(SLICES_DIR.glob("*/*/docs.md")),
        ]

    @property
    def outputs(self) -> Sequence[Path]:
        """One committed artifact: the snapshot itself."""
        return [GTS_SNAPSHOT_FILE]

    def render(self, staging: Path) -> None:
        """Compile the snapshot into the staging tree."""
        data = build_snapshot_bytes()
        target = staging / GTS_SNAPSHOT_FILE.relative_to(PROJECT_ROOT)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)

    def compare(self, fresh: Path, committed: Path) -> list[str]:
        """Byte comparison, with semantic-vs-encoding drift diagnosis.

        Identical bytes pass. On mismatch, fold both and say whether the
        difference is SEMANTIC (different terms/quads/reifiers/annotations —
        the sources changed) or ENCODING-ONLY (identical fold, different
        bytes — typically a compression/library version bump). Both count
        as drift (Principle 7: the committed artifact is the contract).
        """
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

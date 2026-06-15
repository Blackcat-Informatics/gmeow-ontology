# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0

"""The ``gts`` generator: the one committed offline GMEOW bundle (#267, #530).

``generated/dist/gmeow.gts`` is the statement-complete, repo-free bundle of the
authored GMEOW ontology, its gUFO import closure, statement layer, alignments,
documentation, and transform surface. The default graph remains authored,
import-free GMEOW; the import closure rides as ``gmeow:graph/imports``.
"""

from __future__ import annotations

import io
import tarfile
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING

from blake3 import blake3
from rdflib import Graph, Literal, URIRef

from gmeow_tools.config import (
    GTS_GRAPH_IMPORTS,
    GTS_GRAPH_METADATA,
    GTS_SNAPSHOT_FILE,
    MAPPINGS_DIR,
    NAMESPACE,
    PROJECT_ROOT,
    STATEMENT_RDF12_FILE,
)
from gmeow_tools.generator import Generator, register
from gmeow_tools.graph import iter_import_files, iter_module_files
from gmeow_tools.gts_producer import compile_gts
from gmeow_tools.mappings import build_alignment_graph, load_mappings
from gmeow_tools.self_desc import SELF_DESC_FILE
from gmeow_tools.slices import discover_slices
from gts import Signer

if TYPE_CHECKING:
    from collections.abc import Sequence

#: Default documentation archive language until a configurable resolver lands.
_DEFAULT_DOC_LANG = "x-gmeow-english"

#: Paths/files to skip when bundling project docs.
_PROJECT_DOC_SKIP = frozenset({"_generated", ".gitignore", ".DS_Store"})


def _is_project_doc_path(path: Path) -> bool:
    """True when *path* should be bundled into the project-docs archive."""
    if path.name.startswith("."):
        return False
    return all(part not in _PROJECT_DOC_SKIP for part in path.parts)


def _tar_directory(root: Path, lang: str = _DEFAULT_DOC_LANG) -> bytes:
    """Return a deterministic, uncompressed tar of *root* under *lang/*."""
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as tar:
        files = sorted(
            p for p in root.rglob("*") if p.is_file() and _is_project_doc_path(p)
        )
        for source in files:
            arcname = f"{lang}/{source.relative_to(root).as_posix()}"
            info = tarfile.TarInfo(name=arcname)
            data = source.read_bytes()
            info.size = len(data)
            info.mtime = 0
            info.uid = 0
            info.gid = 0
            info.uname = ""
            info.gname = ""
            info.mode = 0o644
            tar.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def _tar_members(members: list[tuple[str, bytes]]) -> bytes:
    """Return a deterministic, uncompressed tar of explicit members."""
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as tar:
        for arcname, data in sorted(members):
            info = tarfile.TarInfo(name=arcname)
            info.size = len(data)
            info.mtime = 0
            info.uid = 0
            info.gid = 0
            info.uname = ""
            info.gname = ""
            info.mode = 0o644
            tar.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def _transform_blobs() -> list[tuple[bytes, str, str]]:
    """Fold transform inputs into the bundle for repo-free consumer commands."""
    import json

    from gmeow_tools.bundle import (
        REP_CELLS,
        REP_DENIED,
        REP_MAPPINGS,
        REP_QUERIES,
    )
    from gmeow_tools.config import (
        MAPPING_DSL_DIR,
        PROJECTION_QUERY_DIR,
        SLICES_DIR,
    )
    from gmeow_tools.transform import _denied_cells

    sssom = [(p.name, p.read_bytes()) for p in sorted(MAPPINGS_DIR.glob("*.sssom.tsv"))]
    queries = [
        (p.name, p.read_bytes()) for p in sorted(PROJECTION_QUERY_DIR.glob("*.rq"))
    ]
    cell_paths = (
        sorted((MAPPING_DSL_DIR / "equivalences").glob("*.ttl"))
        + sorted((MAPPING_DSL_DIR / "projections").glob("*.ttl"))
        + sorted(SLICES_DIR.glob("*/*/mappings/*.ttl"))
    )
    cells = [
        (p.relative_to(PROJECT_ROOT).as_posix(), p.read_bytes()) for p in cell_paths
    ]
    denied = sorted(_denied_cells())
    return [
        (_tar_members(sssom), "application/x-tar", REP_MAPPINGS),
        (_tar_members(queries), "application/x-tar", REP_QUERIES),
        (_tar_members(cells), "application/x-tar", REP_CELLS),
        (json.dumps(denied).encode("utf-8"), "application/json", REP_DENIED),
    ]


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
        blobs.append((payload, "text/markdown", f"docs:{entry.group}/{entry.name}"))
    return blobs


def _project_doc_blobs() -> list[tuple[bytes, str, str]]:
    """A single deterministic tar archive of the project docs tree."""
    docs_dir = PROJECT_ROOT / "docs"
    if not docs_dir.is_dir():
        return []
    return [(_tar_directory(docs_dir), "application/x-tar", "project-docs")]


def _ontology_doc_blobs() -> list[tuple[bytes, str, str]]:
    """A deterministic tar archive of the ontology docs tree (#440)."""
    from gmeow_tools.ontology_docs import build_ontology_docs

    with tempfile.TemporaryDirectory(dir=PROJECT_ROOT, prefix=".gmeow-tmp-") as tmp:
        docs_dir = Path(tmp) / "ontology-docs"
        build_ontology_docs(docs_dir)
        return [(_tar_directory(docs_dir), "application/x-tar", "ontology-docs")]


def _imports_graph() -> Graph:
    """Return the vendored gUFO/import closure only, for ``gmeow:graph/imports``."""
    graph = Graph()
    for source in iter_import_files():
        graph.parse(source, format="turtle")
    return graph


def _metadata_graph() -> Graph:
    """Return self-description metadata, partitioned out of the default graph."""
    graph = Graph()
    graph.parse(SELF_DESC_FILE, format="turtle")
    return graph


def build_snapshot_bytes(
    *,
    signer: Signer | None = None,
    public_key_armor: str | None = None,
) -> bytes:
    """Build the unified offline snapshot exactly as the generator commits it.

    Docs ride the package (#325): every slice guide embeds as a
    content-addressed markdown blob, linked from the graph via
    ``gmeow:guideBlob``, and the build FAILS if any guide anchors a missing
    term — docs-in-sync is a build invariant (Principle 7). Shared by the
    generator's render and the reproducibility tests, so there is exactly
    one definition of "the snapshot" (Principle 4).
    """
    from gmeow_tools.graph import load_merged_graph
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
    blobs = (
        _doc_blobs(graph)
        + _project_doc_blobs()
        + _ontology_doc_blobs()
        + _transform_blobs()
    )

    return compile_gts(
        graph,
        STATEMENT_RDF12_FILE,
        alignment_graph=build_alignment_graph(load_mappings()),
        extra_named_graphs=[
            (_imports_graph(), GTS_GRAPH_IMPORTS, "imports"),
            (_metadata_graph(), GTS_GRAPH_METADATA, "metadata"),
        ],
        doc_blobs=blobs,
        signer=signer,
        public_key_armor=public_key_armor,
    )


def compile_full_snapshot(
    *,
    signer: Signer | None = None,
    public_key_armor: str | None = None,
) -> bytes:
    """Compatibility release entry point for the unified ``gmeow.gts`` bundle."""
    return build_snapshot_bytes(signer=signer, public_key_armor=public_key_armor)


@register
class GtsSnapshotGenerator(Generator):
    """Emit the byte-deterministic unified GTS bundle."""

    name: str = "gts"

    @property
    def inputs(self) -> Sequence[Path]:
        """Everything the snapshot folds."""
        from gmeow_tools.config import (
            MAPPING_DSL_DIR,
            ONTOLOGY_FILE,
            PROJECTION_QUERY_DIR,
            SLICES_DIR,
        )
        from gmeow_tools.ontology_docs import ontology_docs_inputs

        doc_files = [
            p
            for p in (PROJECT_ROOT / "docs").rglob("*")
            if p.is_file() and _is_project_doc_path(p)
        ]

        return [
            ONTOLOGY_FILE,
            *iter_module_files(),
            *iter_import_files(),
            STATEMENT_RDF12_FILE,
            SELF_DESC_FILE,
            # transform surface folded for the repo-free consumer CLI (#bundle)
            *sorted(MAPPINGS_DIR.glob("*.sssom.tsv")),
            *sorted(PROJECTION_QUERY_DIR.glob("*.rq")),
            *sorted((MAPPING_DSL_DIR / "equivalences").glob("*.ttl")),
            *sorted((MAPPING_DSL_DIR / "projections").glob("*.ttl")),
            *sorted(SLICES_DIR.glob("*/*/mappings/*.ttl")),
            *sorted(SLICES_DIR.glob("*/*/docs.md")),
            *ontology_docs_inputs(),
            *sorted(doc_files),
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

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

import io
import tarfile
import tempfile
from pathlib import Path
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
    """Return a deterministic, uncompressed tar of *root* under *lang/*.

    All regular files are placed at ``<lang>/<relative-path>``. Metadata is
    normalized so the bytes are a pure function of the file contents and names.
    """
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
    """A single deterministic tar archive of the ontology docs tree (#440).

    The tar is built independently from canonical sources so that
    ``gmeow-full.gts`` does not depend on the committed ``ontology-docs/``
    directory.
    """
    from gmeow_tools.ontology_docs import build_ontology_docs

    with tempfile.TemporaryDirectory(dir=PROJECT_ROOT, prefix=".gmeow-tmp-") as tmp:
        docs_dir = Path(tmp) / "ontology-docs"
        build_ontology_docs(docs_dir)
        return [(_tar_directory(docs_dir), "application/x-tar", "ontology-docs")]


def compile_full_snapshot(
    *,
    signer: Signer | None = None,
    public_key_armor: str | None = None,
) -> bytes:
    """Compile the full offline GMEOW snapshot, optionally signed.

    This is the shared body used by the registered ``gts-full`` generator and
    by the ``gmeow gts compile-full`` release command. The committed artifact
    produced by the generator is always unsigned; the release command supplies
    a signer and the armored transport public key.
    """
    from gmeow_tools.graph import load_merged_graph

    graph = load_merged_graph(include_imports=True)
    doc_blobs = _doc_blobs(graph) + _project_doc_blobs() + _ontology_doc_blobs()
    alignments = build_alignment_graph(load_mappings())
    return compile_gts(
        graph,
        STATEMENT_RDF12_FILE,
        alignment_graph=alignments,
        doc_blobs=doc_blobs,
        signer=signer,
        public_key_armor=public_key_armor,
    )


@register
class GtsFullSnapshotGenerator(Generator):
    """Emit the offline-ready GTS snapshot of GMEOW plus its import closure."""

    name: str = "gts-full"

    @property
    def inputs(self) -> Sequence[Path]:
        """Everything the snapshot folds.

        Ontology, imports, statements, alignments, guides, and project docs.
        Ontology docs are rebuilt independently from these canonical sources
        and embedded, so the committed ``ontology-docs/`` tree is not an input.
        """
        from gmeow_tools.config import ONTOLOGY_FILE, SLICES_DIR

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
            *sorted(MAPPINGS_DIR.glob("*.sssom.tsv")),
            *sorted(SLICES_DIR.glob("*/*/docs.md")),
            *sorted(doc_files),
        ]

    @property
    def outputs(self) -> Sequence[Path]:
        """The offline bundle shipped with the ``gmeow`` package."""
        return [GTS_FULL_SNAPSHOT_FILE]

    def render(self, staging: Path) -> None:
        """Compile the full snapshot into the staging tree."""
        data = compile_full_snapshot()
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

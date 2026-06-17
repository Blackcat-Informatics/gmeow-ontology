# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

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
from gts import Signer
from rdflib import Graph, Literal, URIRef

from gmeow_tools.config import (
    GTS_GRAPH_IMPORTS,
    GTS_GRAPH_METADATA,
    GTS_SNAPSHOT_FILE,
    MAPPINGS_DIR,
    NAMESPACE,
    PROJECT_ROOT,
    SLICES_DIR,
    STATEMENT_RDF12_FILE,
)
from gmeow_tools.generator import Generator, register
from gmeow_tools.graph import iter_import_files, iter_module_files
from gmeow_tools.gts_producer import compile_gts
from gmeow_tools.i18n_catalog import (
    discover_doc_languages,
    load_ontology_docs_template_catalog,
    merge_all_markdown,
    merge_terms,
    translated_ontology_docs_templates,
)
from gmeow_tools.mappings import build_alignment_graph, load_mappings
from gmeow_tools.self_desc import SELF_DESC_FILE
from gmeow_tools.slices import discover_slices

if TYPE_CHECKING:
    from collections.abc import Sequence

#: Intentional canonical-carrier documentation archive language for bundled docs.
#: ``x-gmeow-english`` is the one canonical source for docs embedded in the
#: offline bundle (Principle 4). No configurable resolver is currently required.
DEFAULT_DOC_ARCHIVE_LANG = "x-gmeow-english"

#: Paths/files to skip when bundling project docs.
_PROJECT_DOC_SKIP = frozenset({"_generated", ".gitignore", ".DS_Store"})

#: Deterministic committed snapshot encoding. ``zstd`` output can vary across
#: codec/library builds even when the decoded fold is identical; gzip with
#: ``mtime=0`` stays compact without depending on zstandard frame bytes.
_SNAPSHOT_TRANSFORM = ["gzip"]


def _is_project_doc_path(path: Path) -> bool:
    """True when *path* should be bundled into the project-docs archive."""
    if path.name.startswith("."):
        return False
    return all(part not in _PROJECT_DOC_SKIP for part in path.parts)


def _tar_directory(root: Path, lang: str = DEFAULT_DOC_ARCHIVE_LANG) -> bytes:
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


def _transform_blobs(_graph: Graph) -> list[tuple[bytes, str, str]]:
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
    """A deterministic tar archive of the project docs tree.

    Includes the English tree at ``x-gmeow-english/`` plus, for every
    discovered language with slice PO translations, a translated tree at
    ``x-gmeow-<lang>/``.  Missing Markdown translations produce the original
    English content.
    """
    docs_dir = PROJECT_ROOT / "docs"
    if not docs_dir.is_dir():
        return []

    members: list[tuple[str, bytes]] = []

    def add_tree(tree_root: Path, lang: str) -> None:
        for source in sorted(
            p for p in tree_root.rglob("*") if p.is_file() and _is_project_doc_path(p)
        ):
            arcname = f"x-gmeow-{lang}/{source.relative_to(tree_root).as_posix()}"
            members.append((arcname, source.read_bytes()))

    def _has_md_translations(lang: str) -> bool:
        return bool(
            list((PROJECT_ROOT / "dist" / "i18n" / "docs").rglob(f"*.{lang}.po"))
        )

    # English carrier stays at the historical prefix for backward compatibility.
    add_tree(docs_dir, "english")

    for lang in discover_doc_languages(PROJECT_ROOT):
        if not _has_md_translations(lang):
            continue
        with tempfile.TemporaryDirectory(
            dir=PROJECT_ROOT, prefix=".gmeow-tmp-pdoc-"
        ) as tmp:
            tmp_path = Path(tmp)
            merge_all_markdown(PROJECT_ROOT, lang, tmp_path, include_readme=True)
            # Project docs live under docs/ in the archive; README.md at root.
            docs_tmp = tmp_path / "docs"
            if docs_tmp.is_dir():
                add_tree(docs_tmp, lang)
            readme_tmp = tmp_path / "README.md"
            if readme_tmp.is_file():
                members.append((f"x-gmeow-{lang}/README.md", readme_tmp.read_bytes()))

    return [(_tar_members(members), "application/x-tar", "project-docs")]


def _ontology_doc_blobs() -> list[tuple[bytes, str, str]]:
    """A deterministic tar archive of the ontology docs tree (#440).

    English content remains at ``x-gmeow-english/``.  For every other
    discovered language a separate tree is rendered with translated ontology-docs
    template strings.
    """
    from gmeow_tools.ontology_docs import build_ontology_docs, cached_ontology_docs_tree

    members: list[tuple[str, bytes]] = []

    def add_tree(tree_root: Path, lang: str) -> None:
        for source in sorted(
            p for p in tree_root.rglob("*") if p.is_file() and _is_project_doc_path(p)
        ):
            arcname = f"x-gmeow-{lang}/{source.relative_to(tree_root).as_posix()}"
            members.append((arcname, source.read_bytes()))

    en_tree = cached_ontology_docs_tree()
    add_tree(en_tree, "english")

    for lang in discover_doc_languages(PROJECT_ROOT):
        catalog = load_ontology_docs_template_catalog(lang, root=PROJECT_ROOT)
        if not catalog:
            continue
        with tempfile.TemporaryDirectory(
            dir=PROJECT_ROOT, prefix=".gmeow-tmp-odoc-"
        ) as tmp:
            tmp_path = Path(tmp)
            with translated_ontology_docs_templates(catalog):
                build_ontology_docs(tmp_path)
            add_tree(tmp_path, lang)

    return [(_tar_members(members), "application/x-tar", "ontology-docs")]


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
    from gmeow_tools.graph import iter_source_files, load_merged_graph
    from gmeow_tools.validate import guide_anchor_lint

    graph = load_merged_graph(include_imports=False)
    # guide_anchor_lint is graph-free now (#579): it takes the source PATHS and
    # builds its own oxigraph store. The graph above is still used by the doc-blob
    # builders below.
    lint = guide_anchor_lint([str(p) for p in iter_source_files(include_imports=False)])
    if lint.errors:
        details = "; ".join(lint.errors[:5])
        msg = (
            f"docs-in-sync invariant violated (#325): {len(lint.errors)} "
            f"guide anchor error(s) — {details}"
        )
        raise ValueError(msg)

    po_paths = sorted(p for p in SLICES_DIR.glob("*/*/i18n/*.po") if p.stem != "en")
    multilingual_graph = merge_terms(graph, po_paths)

    blobs = (
        _doc_blobs(multilingual_graph)
        + _project_doc_blobs()
        + _ontology_doc_blobs()
        + _transform_blobs(multilingual_graph)
    )

    return compile_gts(
        multilingual_graph,
        STATEMENT_RDF12_FILE,
        alignment_graph=build_alignment_graph(load_mappings()),
        extra_named_graphs=[
            (_imports_graph(), GTS_GRAPH_IMPORTS, "imports"),
            (_metadata_graph(), GTS_GRAPH_METADATA, "metadata"),
        ],
        doc_blobs=blobs,
        transform=_SNAPSHOT_TRANSFORM,
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
    def implementation_paths(self) -> Sequence[Path]:
        """Implementation and dependency-lock files that affect snapshot bytes.

        The GTS format engine now ships as the external ``gmeow-gts`` package; its
        exact pinned version is captured by ``uv.lock``, so a gmeow-gts upgrade
        (codec/writer/wire changes) invalidates the snapshot cache through the
        lockfile rather than by hashing files inside site-packages.
        """
        return [
            PROJECT_ROOT / "pyproject.toml",
            PROJECT_ROOT / "uv.lock",
            PROJECT_ROOT / "src" / "gmeow_tools" / "bundle.py",
            PROJECT_ROOT / "src" / "gmeow_tools" / "config.py",
            PROJECT_ROOT / "src" / "gmeow_tools" / "graph.py",
            PROJECT_ROOT / "src" / "gmeow_tools" / "gts_producer.py",
            PROJECT_ROOT / "src" / "gmeow_tools" / "i18n_catalog.py",
            PROJECT_ROOT / "src" / "gmeow_tools" / "mappings.py",
            PROJECT_ROOT / "src" / "gmeow_tools" / "ontology_docs.py",
            PROJECT_ROOT / "src" / "gmeow_tools" / "self_desc.py",
            PROJECT_ROOT / "src" / "gmeow_tools" / "slices.py",
            PROJECT_ROOT / "src" / "gmeow_tools" / "transform.py",
            PROJECT_ROOT / "src" / "gmeow_tools" / "validate.py",
        ]

    @property
    def inputs(self) -> Sequence[Path]:
        """Everything the snapshot folds."""
        from gmeow_tools.config import (
            MAPPING_DSL_DIR,
            ONTOLOGY_FILE,
            PROJECTION_QUERY_DIR,
            SLICES_DIR,
        )
        from gmeow_tools.ontology_docs import ontology_docs_cache_inputs

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
            *sorted(p for p in SLICES_DIR.glob("*/*/i18n/*.po") if p.stem != "en"),
            *ontology_docs_cache_inputs(),
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
        """Semantic-fold comparison — the snapshot is a SEMANTIC contract.

        Identical bytes pass. On a byte mismatch, fold both snapshots and
        compare their canonical N-Quads. Only a SEMANTIC difference (different
        terms/quads/reifiers/annotations — the sources changed) counts as
        drift. ENCODING-ONLY differences (identical fold, different bytes —
        a compression/library version skew between CI and local codecs) are
        NOT drift: the gts contract is the graph it folds to, not the exact
        compressed bytes, which are not reproducible across zstd/libzstd builds.
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
        if sorted(to_nquads(a).splitlines()) == sorted(to_nquads(b).splitlines()):
            # Identical fold, different bytes: codec/library skew, not drift.
            return []
        return [f"{rel} (semantic drift — sources changed)"]

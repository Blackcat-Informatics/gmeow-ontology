# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Bundle-backed access to the folded ontology surface and its transforms.

The `gmeow` wheel ships ONE artifact — `generated/dist/gmeow-full.gts` — that folds
the **complete useful ontology surface AND its transforms**: the SSSOM lift maps,
the compiled projection queries, the equivalence/projection cells, and the merged
ontology graph (with and without imports). This module reads them back so the
consumer loaders (`build_lift_map`, `project_graph`, `load_cells`,
`_build_merged_graph`) can run **from the wheel alone, with no repo checkout** —
the CLI razor: `gmeow` does not need a repo, `gmeow-dev` does.

Each input group rides as a deterministic tar blob keyed by a representation label
(the `docs:`-archive precedent), except the two merged graphs, which ride as single
canonical N-Triples blobs. The repo path stays the dev fast-path; the bundle is the
shipped path. The loaders try the repo first, falling back here when no source tree.

Read side only — the generator (`gts_full_gen`) builds the blobs.
"""

from __future__ import annotations

import io
import tarfile
from functools import lru_cache

from gmeow_tools.config import GTS_FULL_SNAPSHOT_FILE, ONTOLOGY_FILE

#: Representation labels for the folded transform blobs (the `rep` field in
#: ``graph.blob_meta``). Kept in lock-step with the generator that writes them.
REP_MAPPINGS = "mappings-archive"  # tar of generated/mappings/*.sssom.tsv
REP_QUERIES = "queries-archive"  # tar of generated/queries/*.rq
REP_CELLS = "cells-archive"  # tar of the cell/projection TTL sources (repo-rel paths)
REP_MERGED_IMPORTS = "merged:imports"  # N-Triples of load_merged_graph(imports=True)
REP_MERGED_NOIMPORTS = "merged:noimports"  # N-Triples of include_imports=False
REP_DENIED = "transform:denied"  # JSON of the saturation refusal set (alignment lint)


def repo_sources_present() -> bool:
    """True when the canonical ontology source tree is on disk (a dev checkout).

    The loaders use this to decide whether to read the repo (fast dev path) or
    fall back to the bundle (a wheel-only install).
    """
    return ONTOLOGY_FILE.exists()


@lru_cache(maxsize=1)
def _bundle_graph() -> object:
    """The parsed default bundle (`gmeow-full.gts`), cached for the process."""
    import gts

    return gts.read(GTS_FULL_SNAPSHOT_FILE.read_bytes())


def _blob_by_rep(rep: str) -> bytes | None:
    """The decoded payload of the single blob carrying *rep*, or ``None``."""
    graph = _bundle_graph()
    for digest, meta in graph.blob_meta.items():  # type: ignore[attr-defined]
        if meta.get("rep") == rep:
            payload: bytes | None = graph.blobs.get(digest)  # type: ignore[attr-defined]
            return payload
    return None


@lru_cache(maxsize=8)
def _archive(rep: str) -> dict[str, bytes]:
    """Untar the blob carrying *rep* into ``{member-name: bytes}`` (cached)."""
    raw = _blob_by_rep(rep)
    if raw is None:
        return {}
    out: dict[str, bytes] = {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r") as tar:
        for member in tar.getmembers():
            if not member.isfile():
                continue
            handle = tar.extractfile(member)
            if handle is not None:
                out[member.name] = handle.read()
    return out


def bundled_sssom() -> dict[str, bytes]:
    """Every folded SSSOM file as ``{filename: tsv-bytes}`` (empty if unbundled)."""
    return _archive(REP_MAPPINGS)


def bundled_queries() -> dict[str, bytes]:
    """Every folded projection query as ``{"<profile>.rq": query-bytes}``."""
    return _archive(REP_QUERIES)


def bundled_cells() -> dict[str, bytes]:
    """Every folded cell/projection TTL as ``{repo-relative-path: ttl-bytes}``.

    Keys preserve the repo-relative path (``dsl/mappings/equivalences/x.ttl``,
    ``dsl/mappings/projections/y.ttl``, ``slices/<g>/<n>/mappings/z.ttl``) so a
    loader can route to exactly the directories it reads in repo mode.
    """
    return _archive(REP_CELLS)


def _is_slice_mapping(relpath: str) -> bool:
    """True for a ``slices/<group>/<name>/mappings/<file>.ttl`` repo-relative path."""
    parts = relpath.split("/")
    return (
        len(parts) == 5
        and parts[0] == "slices"
        and parts[3] == "mappings"
        and relpath.endswith(".ttl")
    )


def bundled_cells_under(prefix: str) -> dict[str, bytes]:
    """Folded cell TTLs under repo-relative *prefix* PLUS every slice mappings file.

    Mirrors the two repo loaders exactly: ``load_cells`` reads
    ``dsl/mappings/equivalences/`` + slice mappings; ``_projection_files`` reads
    ``dsl/mappings/projections/`` + slice mappings. Pass the directory prefix; the
    slice mappings are always included (both loaders read them).
    """
    return {
        rel: data
        for rel, data in bundled_cells().items()
        if rel.startswith(prefix) or _is_slice_mapping(rel)
    }


def bundled_merged_ttl(*, include_imports: bool) -> bytes | None:
    """The folded merged-ontology N-Triples (with or without imports), or ``None``."""
    rep = REP_MERGED_IMPORTS if include_imports else REP_MERGED_NOIMPORTS
    return _blob_by_rep(rep)


def bundled_denied_cells() -> list[tuple[str, str, str]] | None:
    """The precomputed saturation refusal set (the alignment-lint ERROR rows).

    Computed at bundle-build time (when the source tree is present) and folded in,
    so the consumer transform need not re-run the alignment lint — which reads the
    SSSOM tables and the vendored target axioms. ``None`` when unbundled.
    """
    import json

    raw = _blob_by_rep(REP_DENIED)
    if raw is None:
        return None
    return [(row[0], row[1], row[2]) for row in json.loads(raw.decode("utf-8"))]

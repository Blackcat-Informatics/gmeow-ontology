# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Bundle-backed access to the folded ontology surface and its transforms.

The `gmeow` wheel ships ONE artifact — `generated/dist/gmeow.gts` — that folds
the **complete useful ontology surface AND its transforms**: the SSSOM lift maps,
the compiled projection queries, the equivalence/projection cells, and the
ontology/import named graphs. This module reads them back so the
consumer loaders (`build_lift_map`, `project_graph`, `load_cells`,
`_build_merged_graph`) can run **from the wheel alone, with no repo checkout** —
the CLI razor: `gmeow` does not need a repo, `gmeow-dev` does.

Each input group rides as a deterministic tar blob keyed by a representation label
(the `docs:`-archive precedent). The merged graph fallback is reconstructed from
the GTS default graph plus ``gmeow:graph/imports``. The repo path stays the dev
fast-path; the bundle is the shipped path. The loaders try the repo first,
falling back here when no source tree.

Read side only — the ``gts`` generator builds the blobs.
"""

from __future__ import annotations

import io
import tarfile
from functools import lru_cache

from gts.model import Graph as GtsGraph

from gmeow_tools.config import (
    GTS_GRAPH_IMPORTS,
    GTS_SNAPSHOT_FILE,
    NAMESPACE,
    ONTOLOGY_FILE,
)

#: Representation labels for the folded transform blobs (the `rep` field in
#: ``graph.blob_meta``). Kept in lock-step with the generator that writes them.
REP_MAPPINGS = "mappings-archive"  # tar of generated/mappings/*.sssom.tsv
REP_QUERIES = "queries-archive"  # tar of generated/queries/*.rq
REP_CELLS = "cells-archive"  # tar of the cell/projection TTL sources (repo-rel paths)
REP_TESTS = "tests-archive"  # tar of the slice test-DSL specs (repo-rel paths, #783)
REP_REASONING = "reasoning-archive"  # tar of canonical reasoning products (#667)
REP_DENIED = "transform:denied"  # JSON of the saturation refusal set (alignment lint)
_GUIDE_BLOB = NAMESPACE + "guideBlob"


def repo_sources_present() -> bool:
    """True when the canonical ontology source tree is on disk (a dev checkout).

    The loaders use this to decide whether to read the repo (fast dev path) or
    fall back to the bundle (a wheel-only install).
    """
    return ONTOLOGY_FILE.exists()


@lru_cache(maxsize=1)
def _bundle_graph() -> GtsGraph:
    """The parsed default bundle (`gmeow.gts`), cached for the process."""
    import gts

    return gts.read(GTS_SNAPSHOT_FILE.read_bytes())


def _blob_by_rep(rep: str) -> bytes | None:
    """The decoded payload of the single blob carrying *rep*, or ``None``."""
    graph = _bundle_graph()
    for digest, meta in graph.blob_meta.items():
        if meta.get("rep") == rep:
            payload: bytes | None = graph.blobs.get(digest)
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


def bundled_tests() -> dict[str, bytes]:
    """Every folded slice test-DSL spec as ``{repo-relative-path: ttl-bytes}`` (#783).

    Keys preserve the repo-relative path (``slices/<g>/<n>/tests/<file>.ttl``) so a
    loader can route to exactly the slice ``tests/`` directory it reads in repo mode.
    The archive holds the non-recursive ``tests/*.ttl`` fixtures only (no
    ``tests/counter-examples/`` data, no ``tests/*.py`` harness code), matching
    :func:`gmeow_tools.slices.iter_slice_test_files`.
    """
    return _archive(REP_TESTS)


def bundled_reasoning() -> dict[str, bytes]:
    """Every folded native-reasoning product as ``{repo-path: nq-bytes}`` (#667).

    The closure / explanations / divergence-ledger the native EL/DL engine derived,
    embedded RDFC-1.0 canonical so a repo-free ``gmeow.gts`` consumer can read the
    reasoning results (maximal information flow, north-star (d)) WITHOUT re-running
    the engine. Keys preserve the repo-relative path
    (``generated/logic/<file>.ttl``); the bytes are canonical N-Quads (stable
    regardless of the reasoner's emission order), not the human-readable Turtle.
    """
    return _archive(REP_REASONING)


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
    """Reconstruct merged-ontology N-Triples from the bundled named graphs."""
    from gts.nquads import term_token

    graph = _bundle_graph()
    scopes: set[str | None] = {None}
    if include_imports:
        scopes.add(GTS_GRAPH_IMPORTS)
    lines: list[str] = []
    for s, p, o, graph_id in graph.quads:
        scope = graph.terms[graph_id].value if graph_id is not None else None
        if scope not in scopes:
            continue
        predicate = graph.terms[p].value
        if predicate == _GUIDE_BLOB:
            continue
        lines.append(
            f"{term_token(graph, s)} {term_token(graph, p)} {term_token(graph, o)} ."
        )
    return ("\n".join(sorted(lines)) + "\n").encode("utf-8")


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

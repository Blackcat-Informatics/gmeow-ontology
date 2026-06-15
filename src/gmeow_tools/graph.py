"""Load and merge the modular GMEOW sources into a single rdflib graph.

The pure-Python merged graph is used for the steps that do not need a reasoner
(syntax validation, SHACL, structural lint, JSON-LD context, serialization).
Reasoning and the canonical release product are produced by ROBOT in
``reason.py`` (which collapses the OWL import closure correctly).
"""

from __future__ import annotations

from functools import lru_cache
from pathlib import Path

from rdflib import Graph

from gmeow_tools.config import (
    IMPORTS_DIR,
    ONTOLOGY_FILE,
    PREFIXES,
)
from gmeow_tools.slices import iter_slice_module_files


def iter_module_files() -> list[Path]:
    """Return every slice's module file (``slices/*/*/module.ttl``), sorted.

    The single module enumerator (#287): every canonical terms file lives in
    a slice; there is no other location.
    """
    return iter_slice_module_files()


def iter_import_files() -> list[Path]:
    """Return vendored import Turtle files (gUFO + extracted subsets)."""
    return sorted(IMPORTS_DIR.glob("*.ttl"))


def iter_source_files(*, include_imports: bool = True) -> list[Path]:
    """Return every Turtle source that makes up the ontology.

    Args:
        include_imports: Whether to include the vendored import files
            (``imports/*.ttl``) in addition to the root ontology and modules.

    Returns:
        Ordered list of existing Turtle source paths.
    """
    files = [ONTOLOGY_FILE, *iter_module_files()]
    if include_imports:
        files += iter_import_files()
    return [f for f in files if f.exists()]


def bind_prefixes(graph: Graph) -> None:
    """Bind the canonical GMEOW prefix registry onto a graph."""
    for prefix, namespace in PREFIXES.items():
        graph.bind(prefix, namespace, replace=True)


@lru_cache(maxsize=2)
def _build_merged_graph(include_imports: bool) -> Graph:
    """Parse and merge all ontology sources into one rdflib graph (cached).

    The expensive disk-parsing work is cached by ``include_imports`` flag.
    Callers receive a *copy* so mutations do not corrupt the cache.
    """
    if not ONTOLOGY_FILE.exists():
        # Wheel-only install (no source tree): reconstruct from the merged graph
        # folded into the bundle (#bundle — the CLI razor: gmeow needs no repo).
        from gmeow_tools.bundle import bundled_merged_ttl

        nt = bundled_merged_ttl(include_imports=include_imports)
        if nt is None:
            raise FileNotFoundError(f"root ontology not found: {ONTOLOGY_FILE}")
        merged = Graph()
        merged.parse(data=nt, format="nt")  # blob is canonical N-Triples
        bind_prefixes(merged)
        return merged
    merged = Graph()
    for source in iter_source_files(include_imports=include_imports):
        merged.parse(source, format="turtle")
    bind_prefixes(merged)
    return merged


def load_merged_graph(*, include_imports: bool = True) -> Graph:
    """Parse and merge all ontology sources into one rdflib graph.

    Args:
        include_imports: Whether to merge the vendored import files too.

    Returns:
        A single graph containing the union of all parsed source triples, with
        the canonical prefixes bound. The returned graph is a shallow copy so
        callers may mutate it safely.

    Raises:
        FileNotFoundError: If the root ontology file is missing.
    """
    cached = _build_merged_graph(include_imports)
    g = Graph()
    for triple in cached:
        g.add(triple)
    bind_prefixes(g)
    return g


def shared_merged_graph(*, include_imports: bool = False) -> Graph:
    """Return the cached merged graph directly, without copying.

    This is the **read-only** fast path: callers that only query the merged
    ontology avoid the per-call triple-by-triple copy that
    :func:`load_merged_graph` pays. The returned graph is the shared cache —
    callers MUST NOT mutate it. Use :func:`load_merged_graph` when you need a
    graph you can add to, or :func:`gmeow_tools.sparql.store_with` for a fast,
    isolated store seeded with extra instance data.
    """
    return _build_merged_graph(include_imports)

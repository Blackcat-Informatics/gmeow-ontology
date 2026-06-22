"""Load and merge the modular GMEOW sources into a single rdflib graph.

The pure-Python merged graph is used for the steps that do not need a reasoner
(syntax validation, SHACL, structural lint, JSON-LD context, serialization).
Reasoning and the canonical release product are produced by ROBOT in
``reason.py`` (which collapses the OWL import closure correctly).
"""

from __future__ import annotations

from functools import lru_cache
from pathlib import Path
from typing import Protocol

from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools.config import (
    IMPORTS_DIR,
    ONTOLOGY_FILE,
    PREFIXES,
)
from gmeow_tools.slices import iter_slice_module_files


class _Bindable(Protocol):
    """A graph that can bind a prefix — the compat ``Graph`` or upstream rdflib's."""

    def bind(self, prefix: str | None, namespace: object, *, replace: bool) -> None:
        """Bind ``prefix`` to ``namespace``."""
        ...


def iter_module_files(root: Path | None = None) -> list[Path]:
    """Return every slice's module file (``slices/*/*/module.ttl``), sorted.

    The single module enumerator (#287): every canonical terms file lives in
    a slice; there is no other location.

    Args:
        root: Repository root to search from.  Defaults to the configured
            project slices directory.
    """
    if root is None:
        return iter_slice_module_files()
    return iter_slice_module_files(root / "slices")


def iter_import_files(root: Path | None = None) -> list[Path]:
    """Return vendored import Turtle files (gUFO + extracted subsets).

    Args:
        root: Repository root to search from.  Defaults to the configured
            project imports directory.
    """
    imports_dir = IMPORTS_DIR if root is None else root / "imports"
    return sorted(imports_dir.glob("*.ttl"))


def iter_source_files(
    *,
    root: Path | None = None,
    include_imports: bool = True,
) -> list[Path]:
    """Return every Turtle source that makes up the ontology.

    Args:
        root: Repository root to resolve source paths against.  Defaults to the
            configured project root.
        include_imports: Whether to include the vendored import files
            (``imports/*.ttl``) in addition to the root ontology and modules.

    Returns:
        Ordered list of existing Turtle source paths.
    """
    ontology_file = ONTOLOGY_FILE if root is None else root / "ontology" / "gmeow.ttl"
    files = [ontology_file, *iter_module_files(root)]
    if include_imports:
        files += iter_import_files(root)
    return [f for f in files if f.exists()]


def bind_prefixes(graph: _Bindable) -> None:
    """Bind the canonical GMEOW prefix registry onto a graph (either engine)."""
    for prefix, namespace in PREFIXES.items():
        graph.bind(prefix, namespace, replace=True)


@lru_cache(maxsize=2)
def _build_merged_graph(
    include_imports: bool,
    root: Path | None = None,
) -> Graph:
    """Parse and merge all ontology sources into one rdflib graph (cached).

    The expensive disk-parsing work is cached by ``include_imports`` flag and
    optional ``root``. Callers receive a *copy* so mutations do not corrupt the
    cache.
    """
    ontology_file = ONTOLOGY_FILE if root is None else root / "ontology" / "gmeow.ttl"
    if root is None and not ontology_file.exists():
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
    for source in iter_source_files(
        root=root,
        include_imports=include_imports,
    ):
        merged.parse(source, format="turtle")
    bind_prefixes(merged)
    return merged


def load_merged_graph(
    *,
    root: Path | None = None,
    include_imports: bool = True,
) -> Graph:
    """Parse and merge all ontology sources into one rdflib graph.

    Args:
        root: Repository root to resolve source paths against.  Defaults to the
            configured project root.
        include_imports: Whether to merge the vendored import files too.

    Returns:
        A single graph containing the union of all parsed source triples, with
        the canonical prefixes bound. The returned graph is a shallow copy so
        callers may mutate it safely.

    Raises:
        FileNotFoundError: If the root ontology file is missing in a wheel-only
            install (when *root* is omitted).
    """
    cached = _build_merged_graph(include_imports, root)
    g = Graph()
    for triple in cached:
        g.add(triple)
    bind_prefixes(g)
    return g


def shared_merged_graph(
    *,
    root: Path | None = None,
    include_imports: bool = False,
) -> Graph:
    """Return the cached merged graph directly, without copying.

    This is the **read-only** fast path: callers that only query the merged
    ontology avoid the per-call triple-by-triple copy that
    :func:`load_merged_graph` pays. The returned graph is the shared cache —
    callers MUST NOT mutate it. Use :func:`load_merged_graph` when you need a
    graph you can add to, or :func:`gmeow_tools.sparql.store_with` for a fast,
    isolated store seeded with extra instance data.
    """
    return _build_merged_graph(include_imports, root)

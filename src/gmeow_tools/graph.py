"""Load and merge the modular GMEOW sources into a single rdflib graph.

The pure-Python merged graph is used for the steps that do not need a reasoner
(syntax validation, SHACL, structural lint, JSON-LD context, serialization).
Reasoning and the canonical release product are produced by ROBOT in
``reason.py`` (which collapses the OWL import closure correctly).
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph

from gmeow_tools.config import (
    IMPORTS_DIR,
    MODULES_DIR,
    ONTOLOGY_FILE,
    PREFIXES,
)


def iter_module_files() -> list[Path]:
    """Return the GMEOW module Turtle files in sorted order."""
    return sorted(MODULES_DIR.glob("*.ttl"))


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


def load_merged_graph(*, include_imports: bool = True) -> Graph:
    """Parse and merge all ontology sources into one rdflib graph.

    Args:
        include_imports: Whether to merge the vendored import files too.

    Returns:
        A single graph containing the union of all parsed source triples, with
        the canonical prefixes bound.

    Raises:
        FileNotFoundError: If the root ontology file is missing.
    """
    if not ONTOLOGY_FILE.exists():
        raise FileNotFoundError(f"root ontology not found: {ONTOLOGY_FILE}")
    merged = Graph()
    for source in iter_source_files(include_imports=include_imports):
        merged.parse(source, format="turtle")
    bind_prefixes(merged)
    return merged

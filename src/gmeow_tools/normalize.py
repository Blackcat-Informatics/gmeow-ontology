"""Canonical Turtle serialization for stable, review-friendly diffs.

OWL-heavy ontologies edited in Protégé or by hand produce noisy diffs (reordered
triples, churned blank-node ids). Re-serializing each source through rdflib's
``longturtle`` canonical form makes diffs reflect real semantic changes only.

This is an explicit, opt-in step (``gmeow normalize``); it is not part of the
``check`` gate, since it rewrites the authored files.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph

from gmeow_tools.graph import bind_prefixes, iter_source_files


def canonicalize(path: Path) -> bool:
    """Rewrite one Turtle file in canonical ``longturtle`` form.

    Args:
        path: The Turtle file to normalize in place.

    Returns:
        ``True`` if the file content changed, ``False`` otherwise.
    """
    graph = Graph().parse(path, format="turtle")
    bind_prefixes(graph)
    canonical = graph.serialize(format="longturtle")
    before = path.read_text(encoding="utf-8")
    if before == canonical:
        return False
    path.write_text(canonical, encoding="utf-8")
    return True


def normalize_modules() -> list[Path]:
    """Canonicalize the authored ontology sources (root + modules).

    Returns:
        The list of files whose content changed.
    """
    changed: list[Path] = []
    for source in iter_source_files(include_imports=False):
        if canonicalize(source):
            changed.append(source)
    return changed

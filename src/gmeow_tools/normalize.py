"""Canonical Turtle serialization for stable, review-friendly diffs.

OWL-heavy ontologies edited in Protégé or by hand produce noisy diffs (reordered
triples, churned blank-node ids). Re-serializing each source through the native
``gmeow_rdf`` canonical Turtle form (#819 Task 9) makes diffs reflect real
semantic changes only — it is the rdflib-free replacement for ``longturtle``,
serialized over the gmeow-rdf IR (oxigraph is only the ingest-edge parser).

This is an explicit, opt-in step (``gmeow normalize``); it is not part of the
``check`` gate, since it rewrites the authored files.
"""

from __future__ import annotations

from pathlib import Path

import gmeow_rdf

from gmeow_tools.config import PREFIXES
from gmeow_tools.graph import iter_source_files

# The canonical GMEOW prefix registry, as the (prefix, namespace) pairs the
# native serializer abbreviates with (only the ones a file uses are emitted).
_EXTRA_PREFIXES: list[tuple[str, str]] = sorted(PREFIXES.items())


def canonicalize(path: Path) -> bool:
    """Rewrite one Turtle file in native canonical form.

    Args:
        path: The Turtle file to normalize in place.

    Returns:
        ``True`` if the file content changed, ``False`` otherwise.
    """
    before = path.read_bytes()
    canonical = gmeow_rdf.canonicalize_turtle(before, _EXTRA_PREFIXES)
    if before == canonical:
        return False
    path.write_bytes(canonical)
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

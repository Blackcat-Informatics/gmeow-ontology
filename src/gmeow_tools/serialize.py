"""Emit the ontology in every published RDF 1.1 serialization.

Given a graph (typically the reasoned release product), write Turtle, RDF/XML,
N-Triples and JSON-LD into ``dist/``. Each output is staged in a temporary
directory, re-parsed, and verified by **graph isomorphism** against the original
before being moved into ``dist/``. A lossy serialization can never reach disk.
"""

from __future__ import annotations

import shutil
import tempfile
from pathlib import Path

from rdflib import Graph
from rdflib.compare import isomorphic

from gmeow_tools.config import DIST_DIR
from gmeow_tools.graph import bind_prefixes

#: (serializer, parser) format names keyed by output file extension. The two
#: differ for RDF/XML: ``pretty-xml`` is serialize-only; parsing uses ``xml``.
_FORMATS: dict[str, tuple[str, str]] = {
    "ttl": ("turtle", "turtle"),
    "rdf": ("pretty-xml", "xml"),
    "nt": ("nt", "nt"),
    "jsonld": ("json-ld", "json-ld"),
}


def serialize_graph(
    graph: Graph,
    *,
    stem: str = "gmeow",
    dist_dir: Path = DIST_DIR,
) -> dict[str, Path]:
    """Serialize a graph to all published formats and verify round-trips.

    Args:
        graph: The graph to serialize.
        stem: Output filename stem (e.g. ``"gmeow"`` → ``gmeow.ttl``).
        dist_dir: Target directory (created if absent).

    Returns:
        Mapping of file extension to the written path.

    Raises:
        ValueError: If a serialized file fails isomorphism on re-parse.
    """
    dist_dir.mkdir(parents=True, exist_ok=True)
    bind_prefixes(graph)
    staged: dict[str, Path] = {}
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        for ext, (writer, reader) in _FORMATS.items():
            out = tmp_path / f"{stem}.{ext}"
            graph.serialize(destination=out, format=writer)
            # Round-trip: a published artifact that is not isomorphic to the
            # original after re-parsing is a defect.
            check = Graph()
            check.parse(out, format=reader)
            if not isomorphic(graph, check):
                raise ValueError(f"round-trip failed isomorphism for {ext}: {out}")
            staged[ext] = out

        # All formats passed isomorphism — atomically publish.
        written: dict[str, Path] = {}
        for ext, src in staged.items():
            dest = dist_dir / f"{stem}.{ext}"
            shutil.copy2(src, dest)
            written[ext] = dest
    return written

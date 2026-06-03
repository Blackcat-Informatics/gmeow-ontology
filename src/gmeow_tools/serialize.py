"""Emit the ontology in every published RDF 1.1 serialization.

Given a graph (typically the reasoned release product), write Turtle, RDF/XML,
N-Triples and JSON-LD into ``dist/``. Each output is re-parsed to guarantee
round-trip integrity before the function returns.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph

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
        ValueError: If a serialized file fails to re-parse (round-trip check).
    """
    dist_dir.mkdir(parents=True, exist_ok=True)
    bind_prefixes(graph)
    written: dict[str, Path] = {}
    for ext, (writer, reader) in _FORMATS.items():
        out = dist_dir / f"{stem}.{ext}"
        graph.serialize(destination=out, format=writer)
        # Round-trip: a published artifact that cannot be re-parsed is a defect.
        check = Graph()
        check.parse(out, format=reader)
        if len(check) == 0 and len(graph) > 0:
            raise ValueError(f"round-trip produced an empty graph: {out}")
        written[ext] = out
    return written

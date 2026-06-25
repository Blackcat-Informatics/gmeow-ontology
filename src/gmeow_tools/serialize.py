"""Emit the ontology in every published RDF 1.1 and RDF 1.2-star serialization.

Given a graph (typically the reasoned release product), write Turtle, RDF/XML,
N-Triples, RDF-1.2-star JSON-LD, and YAML-LD-star into ``dist/``. Each output is
staged in a temporary directory, re-parsed, and verified by **graph isomorphism**
against the original before being moved into ``dist/``. A lossy serialization can
never reach disk.
"""

from __future__ import annotations

import shutil
import tempfile
from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools.config import DIST_DIR
from gmeow_tools.graph import bind_prefixes
from gmeow_tools.rdf_canonical import graphs_isomorphic

#: Native RDF-1.2-star serializations handled by the Rust ``gmeow-pipeline`` leaf
#: (#699). The value is the format string passed to ``serialize_yaml_ld``.
_STAR_FORMATS: dict[str, str] = {
    "jsonld": "jsonld",
    "yamlld": "yamlld",
}

#: (serializer, parser) format names keyed by output file extension. The two
#: differ for RDF/XML: ``pretty-xml`` is serialize-only; parsing uses ``xml``.
_FORMATS: dict[str, tuple[str, str] | None] = {
    "ttl": ("turtle", "turtle"),
    "rdf": ("pretty-xml", "xml"),
    "nt": ("nt", "nt"),
}


def _native_jsonld_star(nquads_bytes: bytes, fmt: str) -> bytes:
    """Call the Rust JSON-LD-star / YAML-LD-star serializer via ``gmeow_native``."""
    import gmeow_native.pipeline as _pipeline

    result: bytes = _pipeline.serialize_yaml_ld(nquads_bytes, fmt)
    return result


def _round_trip_star(nquads_bytes: bytes, star_bytes: bytes, fmt: str) -> bool:
    """Verify that RDF-1.2-star bytes re-parse isomorphic to the source N-Quads.

    The check is delegated entirely to the Rust codec
    (``gmeow_native.pipeline.roundtrip_isomorphic``), the single authority for the
    JSON-LD-star / YAML-LD-star parse-and-canonicalize path (#699).
    """
    import gmeow_native.pipeline as _pipeline

    result: bool = _pipeline.roundtrip_isomorphic(nquads_bytes, star_bytes, fmt)
    return result


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

    # N-Quads form of the input, used by the native RDF-1.2-star serializers.
    nquads_bytes = graph.serialize(format="nquads").encode("utf-8")

    staged: dict[str, Path] = {}
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)

        # RDF 1.1 serializations.
        for ext, writer_reader in _FORMATS.items():
            assert writer_reader is not None
            writer, reader = writer_reader
            out = tmp_path / f"{stem}.{ext}"
            graph.serialize(destination=out, format=writer)
            check = Graph()
            check.parse(out, format=reader)
            if not graphs_isomorphic(graph, check):
                raise ValueError(f"round-trip failed isomorphism for {ext}: {out}")
            staged[ext] = out

        # RDF 1.2-star serializations (#699).
        for ext, fmt in _STAR_FORMATS.items():
            out = tmp_path / f"{stem}.{ext}"
            bytes_ = _native_jsonld_star(nquads_bytes, fmt)
            out.write_bytes(bytes_)
            if not _round_trip_star(nquads_bytes, bytes_, fmt):
                raise ValueError(f"round-trip failed isomorphism for {ext}: {out}")
            staged[ext] = out

        # All formats passed isomorphism — atomically publish.
        written: dict[str, Path] = {}
        for ext, src in staged.items():
            dest = dist_dir / f"{stem}.{ext}"
            shutil.copy2(src, dest)
            written[ext] = dest
    return written

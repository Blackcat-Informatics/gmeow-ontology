"""Project GMEOW data to pure target-vocabulary profiles via SPARQL CONSTRUCT.

The complex (non-1:1) correspondences are specified declaratively as FnO function
descriptions + EDOAL complex alignments under ``projections/``; this module is
their *executor* — it runs the per-profile CONSTRUCT queries
(``queries/projections/*.rq``) in-process via rdflib over the asserted ontology +
input data, emitting a pure-profile graph. The projections are deliberately
**lossy and directional** (a consumable view, never the canonical model), which is
why they live outside the reasoned core.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from rdflib import Graph

from gmeow_tools.config import DIST_DIR, FIXTURES_DIR, PREFIXES, PROJECTION_QUERY_DIR
from gmeow_tools.graph import bind_prefixes, load_merged_graph


@dataclass(frozen=True, slots=True)
class Profile:
    """A target projection profile (its CONSTRUCT query + output prefixes)."""

    name: str
    prefixes: tuple[str, ...] = field(default_factory=tuple)


#: Registry of target profiles. Each maps to queries/projections/<name>.rq.
PROFILES: dict[str, Profile] = {
    "schema-org": Profile("schema-org", ("schema", "rdfs")),
    "geosparql": Profile("geosparql", ("geo",)),
    "vcard": Profile("vcard", ("vcard",)),
    "foaf": Profile("foaf", ("foaf", "wgs84")),
}

#: Worked-example inputs (locations + naming + languages + identity fixtures).
_EXAMPLE_FIXTURES = ("places.ttl", "names.ttl", "languages.ttl", "identity.ttl")


def project_graph(profile: str, source: Graph) -> Graph:
    """Run a profile's CONSTRUCT over a source graph, returning the projection.

    Args:
        profile: A key of :data:`PROFILES`.
        source: The graph to project (ontology + instance data).

    Returns:
        A fresh graph of pure target-vocabulary triples, prefixes bound.
    """
    prof = PROFILES[profile]
    query = (PROJECTION_QUERY_DIR / f"{profile}.rq").read_text(encoding="utf-8")
    constructed = source.query(query).graph
    out = Graph()
    if constructed is not None:
        out += constructed
    bind_prefixes(out)
    for prefix in prof.prefixes:
        out.bind(prefix, PREFIXES[prefix])
    return out


def _serialize(graph: Graph, path: Path) -> Path:
    """Write a projection to Turtle and verify it re-parses (round-trip)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    graph.serialize(destination=path, format="turtle")
    check = Graph()
    check.parse(path, format="turtle")
    if len(check) != len(graph):
        raise ValueError(f"projection round-trip changed triple count: {path}")
    return path


def _example_source() -> Graph:
    """The asserted ontology plus the locations + naming worked-example fixtures."""
    graph = load_merged_graph(include_imports=False)
    for fixture in _EXAMPLE_FIXTURES:
        graph.parse(FIXTURES_DIR / fixture, format="turtle")
    return graph


def project_examples(dist_dir: Path = DIST_DIR) -> list[Path]:
    """Project the worked-example fixtures to every profile into ``dist_dir``."""
    source = _example_source()
    return [
        _serialize(project_graph(name, source), dist_dir / f"gmeow-example-{name}.ttl")
        for name in PROFILES
    ]


def project_file(input_path: Path, profile: str, *, dist_dir: Path = DIST_DIR) -> Path:
    """Project an input data file to one profile (ontology merged in for context)."""
    from rdflib.util import guess_format

    source = load_merged_graph(include_imports=False)
    source.parse(input_path, format=guess_format(str(input_path)) or "turtle")
    out = project_graph(profile, source)
    return _serialize(out, dist_dir / f"gmeow-{input_path.stem}-{profile}.ttl")

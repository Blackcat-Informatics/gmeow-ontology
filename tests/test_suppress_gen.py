"""Tests for SUPPRESS-GEN (#72) — disclosure control by generalization (P10).

The unified mechanism: at projection time, withhold OR coarsen a value under a
trigger, never delete. The withhold half (gmeow:displayable) is covered by the
projection-suppression tests; here we exercise the coarsen half — a place that
carries gmeow:coarsenTo emits the enclosing region's coordinates instead of its
precise point, while a place without the marker keeps its exact point.
"""

from __future__ import annotations

from rdflib import Graph, Literal, URIRef

from gmeow_tools.config import FIXTURES_DIR
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.projections import project_graph

GEO = "http://www.opengis.net/ont/geosparql#"
SCHEMA = "https://schema.org/"
LOC = "https://example.org/loc/"


def _source() -> Graph:
    graph = load_merged_graph(include_imports=False)
    graph.parse(FIXTURES_DIR / "suppress-gen.ttl", format="turtle")
    return graph


def _wkt_points(graph: Graph) -> list[str]:
    return [
        str(o)
        for o in graph.objects(None, URIRef(GEO + "asWKT"))
        if isinstance(o, Literal)
    ]


def test_coarsen_emits_enclosing_region_not_precise_point() -> None:
    """A coarsenTo-marked place projects the city's coordinates, not its own."""
    g = project_graph("geosparql", _source())
    points = _wkt_points(g)
    # The enclosing city's coordinates ARE emitted (the coarsened value).
    assert any("51.5072" in p and "-0.1276" in p for p in points), points
    # The precise coordinates of the marked place are NEVER emitted.
    assert not any("51.500001" in p for p in points), points


def test_uncoarsened_place_keeps_its_precise_point() -> None:
    """A place WITHOUT the marker still projects its exact coordinates (control)."""
    g = project_graph("geosparql", _source())
    points = _wkt_points(g)
    assert any("51.5099" in p for p in points), points


def test_coarsened_geometry_attaches_to_the_marked_place() -> None:
    """The coarsened geometry is the marked place's geo:hasGeometry, not the city's."""
    g = project_graph("geosparql", _source())
    geoms = set(g.objects(URIRef(LOC + "secretLab"), URIRef(GEO + "hasGeometry")))
    assert geoms, "secretLab should still carry a (coarsened) geometry"
    wkts = {str(o) for geom in geoms for o in g.objects(geom, URIRef(GEO + "asWKT"))}
    assert any("51.5072" in w for w in wkts), wkts
    assert not any("51.500001" in w for w in wkts), wkts


def _schema_lats(graph: Graph, place: str) -> set[str]:
    """All schema:latitude literals reachable from a place via schema:geo."""
    out: set[str] = set()
    for geo in graph.objects(URIRef(LOC + place), URIRef(SCHEMA + "geo")):
        out.update(str(o) for o in graph.objects(geo, URIRef(SCHEMA + "latitude")))
    return out


def test_schema_org_coarsens_place_coordinates() -> None:
    """schema.org: a coarsenTo place emits the city's lat, not its precise lat."""
    g = project_graph("schema-org", _source())
    marked = _schema_lats(g, "secretLab")
    assert any("51.5072" in v for v in marked), marked
    assert not any("51.500001" in v for v in marked), marked


def test_schema_org_uncoarsened_place_keeps_precise_coordinates() -> None:
    """schema.org control: an unmarked place keeps its exact lat."""
    g = project_graph("schema-org", _source())
    assert any("51.5099" in v for v in _schema_lats(g, "openCafe")), "control leaked"

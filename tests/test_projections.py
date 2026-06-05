"""Tests for the FnO/EDOAL projection layer (GMEOW → pure target profiles)."""

from __future__ import annotations

from pathlib import Path

from rdflib import RDF, Graph, Literal, URIRef

from gmeow_tools.config import FIXTURES_DIR, PROJECTIONS_DIR
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.projections import PROFILES, project_examples, project_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
SCHEMA = "https://schema.org/"
GEO = "http://www.opengis.net/ont/geosparql#"
VCARD = "http://www.w3.org/2006/vcard/ns#"
FOAF = "http://xmlns.com/foaf/0.1/"
WGS84 = "http://www.w3.org/2003/01/geo/wgs84_pos#"
LOC = "https://example.org/loc/"
NAMES = "https://example.org/names/"


LANG = "https://example.org/lang/"


def _source() -> Graph:
    graph = load_merged_graph(include_imports=False)
    graph.parse(FIXTURES_DIR / "places.ttl", format="turtle")
    graph.parse(FIXTURES_DIR / "names.ttl", format="turtle")
    graph.parse(FIXTURES_DIR / "languages.ttl", format="turtle")
    return graph


def _assert_no_gmeow_leakage(graph: Graph) -> None:
    """A pure-profile projection must contain no GMEOW predicates or type objects."""
    for _, p, _ in graph:
        assert not str(p).startswith(GMEOW), f"GMEOW predicate leaked: {p}"
    for _, _, o in graph.triples((None, RDF.type, None)):
        assert not str(o).startswith(GMEOW), f"GMEOW class leaked: {o}"


def test_fno_edoal_specs_parse() -> None:
    required = (
        "functions.fno.ttl",
        "transforms.fno.ttl",
        "schema-org.edoal.ttl",
        "geosparql.edoal.ttl",
        "vcard.edoal.ttl",
        "foaf.edoal.ttl",
    )
    for name in required:
        spec = PROJECTIONS_DIR / name
        assert spec.exists(), f"missing projection spec: {name}"
        assert len(Graph().parse(spec, format="turtle")) > 0


def test_schema_org_projection() -> None:
    g = project_graph("schema-org", _source())
    # value→class place typing.
    assert (URIRef(LOC + "canada"), RDF.type, URIRef(SCHEMA + "Country")) in g
    assert (URIRef(LOC + "spruceGrove"), RDF.type, URIRef(SCHEMA + "City")) in g
    assert (URIRef(LOC + "canada"), RDF.type, URIRef(SCHEMA + "Place")) in g
    # co-equal multilingual names BOTH emitted; deadname suppressed.
    names = set(g.objects(URIRef(NAMES + "patrick"), URIRef(SCHEMA + "name")))
    assert Literal("Patrick Colm Audley", lang="en") in names
    assert Literal("欧德理", lang="zh-Hans") in names
    assert not any("suppressed" in str(o) for o in g.objects())
    # name parts + honorific.
    assert (
        URIRef(NAMES + "patrick"),
        URIRef(SCHEMA + "givenName"),
        Literal("Patrick"),
    ) in g
    assert (
        URIRef(NAMES + "patrick"),
        URIRef(SCHEMA + "honorificPrefix"),
        Literal("Mx", lang="en"),
    ) in g
    _assert_no_gmeow_leakage(g)


def test_languages_projection() -> None:
    g = project_graph("schema-org", _source())
    # Language → schema:Language; ProgrammingLanguage → schema:ComputerLanguage.
    assert (URIRef(LANG + "german"), RDF.type, URIRef(SCHEMA + "Language")) in g
    assert (
        URIRef(LANG + "python"),
        RDF.type,
        URIRef(SCHEMA + "ComputerLanguage"),
    ) in g
    # The BCP-47 tag GMEOW refuses as identity, reconstructed on demand (de + Latn).
    assert Literal("de-Latn") in set(
        g.objects(URIRef(LANG + "german"), URIRef(SCHEMA + "alternateName"))
    )
    # The reified LanguageProficiency relator flattened to schema:knowsLanguage.
    known = set(g.objects(URIRef(LANG + "learner"), URIRef(SCHEMA + "knowsLanguage")))
    assert URIRef(LANG + "german") in known
    assert URIRef(LANG + "english") in known
    # Co-equal endonym/exonym language names BOTH emitted.
    names = set(g.objects(URIRef(LANG + "german"), URIRef(SCHEMA + "name")))
    assert Literal("Deutsch", lang="de") in names
    assert Literal("German", lang="en") in names
    _assert_no_gmeow_leakage(g)


def test_geosparql_projection() -> None:
    g = project_graph("geosparql", _source())
    wkt = URIRef(GEO + "wktLiteral")
    points = [
        o for o in g.objects(None, URIRef(GEO + "asWKT")) if isinstance(o, Literal)
    ]
    assert any(p.datatype == wkt and "POINT" in str(p) for p in points)
    assert (URIRef(LOC + "canada"), RDF.type, URIRef(GEO + "Feature")) in g
    _assert_no_gmeow_leakage(g)


def test_vcard_projection() -> None:
    g = project_graph("vcard", _source())
    assert (None, RDF.type, URIRef(VCARD + "Address")) in g
    # given/family hang off a vcard:Name node (their correct vCard-RDF domain).
    name_node = URIRef(NAMES + "patrick-vcardname")
    assert (URIRef(NAMES + "patrick"), URIRef(VCARD + "hasName"), name_node) in g
    assert (name_node, RDF.type, URIRef(VCARD + "Name")) in g
    assert (name_node, URIRef(VCARD + "given-name"), Literal("Patrick")) in g
    assert Literal("Patrick Colm Audley", lang="en") in set(
        g.objects(URIRef(NAMES + "patrick"), URIRef(VCARD + "fn"))
    )
    assert not any("suppressed" in str(o) for o in g.objects())
    _assert_no_gmeow_leakage(g)


def test_foaf_projection() -> None:
    g = project_graph("foaf", _source())
    assert (
        URIRef(LOC + "westbourne112"),
        RDF.type,
        URIRef(WGS84 + "SpatialThing"),
    ) in g
    assert list(g.objects(URIRef(LOC + "westbourne112"), URIRef(WGS84 + "lat")))
    assert Literal("欧德理", lang="zh-Hans") in set(
        g.objects(URIRef(NAMES + "patrick"), URIRef(FOAF + "name"))
    )
    _assert_no_gmeow_leakage(g)


def test_project_examples_round_trip(tmp_path: Path) -> None:
    written = project_examples(dist_dir=tmp_path)
    assert {p.name for p in written} == {
        f"gmeow-example-{name}.ttl" for name in PROFILES
    }
    for path in written:
        assert path.exists()
        assert len(Graph().parse(path, format="turtle")) > 0

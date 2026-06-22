"""Tests for the FnO/EDOAL projection layer (GMEOW → pure target profiles)."""

from __future__ import annotations

from pathlib import Path

import gmeow_rdf
from gmeow_rdf.compat.rdflib import RDF, XSD, Graph, Literal, URIRef
from gmeow_rdf.compat.rdflib.term import Node

from gmeow_tools import sparql
from gmeow_tools.config import (
    FIXTURES_DIR,
    MAPPING_DSL_DIR,
    PROJECTION_QUERY_DIR,
    PROJECTIONS_DIR,
)
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
IDENT = "https://example.org/identity/"
BOT = "https://w3id.org/bot#"
EX_PLACES = "https://blackcatinformatics.ca/gmeow/examples/places/"


_SOURCE_FIXTURES = (
    "places.ttl",
    "names.ttl",
    "languages.ttl",
    "identity.ttl",
    "contact-fields.ttl",
    "coreference.ttl",
)


def _source() -> gmeow_rdf.Store:
    """The merged ontology + worked-example fixtures as a fast gmeow_rdf store."""
    paths = [FIXTURES_DIR / name for name in _SOURCE_FIXTURES]
    return sparql.store_with(*paths, include_imports=False)


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
        spec = (
            MAPPING_DSL_DIR / name
            if name == "transforms.fno.ttl"
            else PROJECTIONS_DIR / name
        )
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
    # place name without purpose fallback projection (issue #109 nit #1)
    assert (
        URIRef(LOC + "westbourne112"),
        URIRef(SCHEMA + "name"),
        Literal("112 Westbourne Road", lang="en"),
    ) in g
    assert (
        URIRef("https://example.org/coref/recordedPerson"),
        URIRef(SCHEMA + "sameAs"),
        URIRef("https://example.org/coref/authority/person-123"),
    ) in g
    assert (
        URIRef("https://example.org/coref/recordedPerson"),
        URIRef(SCHEMA + "sameAs"),
        URIRef("https://example.org/coref/authority/person-near"),
    ) not in g
    _assert_no_gmeow_leakage(g)


def test_schema_org_projects_explicit_generic_mereology() -> None:
    src = load_merged_graph(include_imports=False)
    part = URIRef("https://example.org/part")
    whole = URIRef("https://example.org/whole")
    component = URIRef("https://example.org/component")
    composite = URIRef("https://example.org/composite")
    room = URIRef("https://example.org/room")
    building = URIRef("https://example.org/building")
    message = URIRef("https://example.org/message")
    body = URIRef("https://example.org/body")
    src.add((part, URIRef(GMEOW + "partOf"), whole))
    src.add((composite, URIRef(GMEOW + "hasPart"), component))
    src.add((room, URIRef(GMEOW + "containedInPlace"), building))
    src.add((message, URIRef(GMEOW + "hasBodyPart"), body))

    g = project_graph("schema-org", src)
    assert (part, URIRef(SCHEMA + "isPartOf"), whole) in g
    assert (composite, URIRef(SCHEMA + "hasPart"), component) in g
    assert (room, URIRef(SCHEMA + "isPartOf"), building) in g
    assert (message, URIRef(SCHEMA + "hasPart"), body) in g
    _assert_no_gmeow_leakage(g)


def test_geo_coordinates_cast_to_decimal() -> None:
    """Geographic coordinates project as canonical xsd:decimal, regardless of how
    the source typed them — a scientific-notation xsd:double (5.355e+01) and a bare
    xsd:integer (47) both come out clean decimal (53.55 / 47), not the ugly,
    inconsistent source datatypes. The cast lives in the down-projection."""
    from gmeow_rdf.compat.rdflib import XSD, BNode, Literal

    wgs84_lat = URIRef("http://www.w3.org/2003/01/geo/wgs84_pos#lat")
    src = load_merged_graph(include_imports=False)
    place, coords = URIRef("https://example.org/place"), BNode()
    src.add((place, RDF.type, URIRef(GMEOW + "Place")))
    src.add((place, URIRef(GMEOW + "hasCoordinates"), coords))
    src.add(
        (coords, URIRef(GMEOW + "latitude"), Literal("5.355e+01", datatype=XSD.double))
    )
    src.add((coords, URIRef(GMEOW + "longitude"), Literal(47, datatype=XSD.integer)))

    g = project_graph("foaf", src)
    lat = next(o for _s, p, o in g if p == wgs84_lat)
    lon = next(o for _s, p, o in g if str(p).endswith("#long"))
    assert isinstance(lat, Literal) and isinstance(lon, Literal)
    assert lat.datatype == XSD.decimal, f"lat should be xsd:decimal, got {lat.datatype}"
    assert "e" not in str(lat).lower(), f"no scientific notation: {lat}"
    assert float(lat) == 53.55
    assert lon.datatype == XSD.decimal and float(lon) == 47.0


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


def test_contact_fields_projection() -> None:
    g = project_graph("schema-org", _source())
    cf = "https://example.org/cf/"
    sam = URIRef(cf + "sam")
    # description (1:1), url (from hasWebPage), nickname (from a nickname PersonName).
    assert (
        sam,
        URIRef(SCHEMA + "description"),
        Literal("Backend engineer; enjoys ontologies and strong tea.", lang="en"),
    ) in g
    assert (sam, URIRef(SCHEMA + "url"), URIRef("https://sam.example.com/")) in g
    assert Literal("Sammy", lang="en") in set(
        g.objects(sam, URIRef(SCHEMA + "alternateName"))
    )
    # birthDate from the birth event, reached by traversing the principal
    # Participation (#41); the event's instant is a DL-clean xsd:dateTime.
    _birth = Literal("1990-04-12T00:00:00Z", datatype=XSD.dateTime)
    assert (sam, URIRef(SCHEMA + "birthDate"), _birth) in g
    title = Literal("Principal Engineer", lang="en")
    assert (sam, URIRef(SCHEMA + "jobTitle"), title) in g
    # department from subOrganizationOf (inverse, parent→child).
    assert (
        URIRef(cf + "acme"),
        URIRef(SCHEMA + "department"),
        URIRef(cf + "engDept"),
    ) in g
    _assert_no_gmeow_leakage(g)


def test_contact_fields_vcard_and_foaf() -> None:
    cf = "https://example.org/cf/"
    sam = URIRef(cf + "sam")
    gv = project_graph("vcard", _source())
    vnicks = set(gv.objects(sam, URIRef(VCARD + "nickname")))
    assert Literal("Sammy", lang="en") in vnicks
    assert (
        sam,
        URIRef(VCARD + "bday"),
        Literal("1990-04-12T00:00:00Z", datatype=XSD.dateTime),
    ) in gv
    assert Literal("Principal Engineer", lang="en") in set(
        gv.objects(sam, URIRef(VCARD + "title"))
    )
    assert (sam, URIRef(VCARD + "hasURL"), URIRef("https://sam.example.com/")) in gv
    _assert_no_gmeow_leakage(gv)
    gf = project_graph("foaf", _source())
    assert Literal("Sammy", lang="en") in set(gf.objects(sam, URIRef(FOAF + "nick")))
    assert (sam, URIRef(FOAF + "homepage"), URIRef("https://sam.example.com/")) in gf
    _assert_no_gmeow_leakage(gf)


def test_geosparql_projection() -> None:
    g = project_graph("geosparql", _source())
    wkt = URIRef(GEO + "wktLiteral")
    points = [
        o for o in g.objects(None, URIRef(GEO + "asWKT")) if isinstance(o, Literal)
    ]
    assert any(p.datatype == wkt and "POINT" in str(p) for p in points)
    assert (URIRef(LOC + "canada"), RDF.type, URIRef(GEO + "Feature")) in g
    _assert_no_gmeow_leakage(g)


def test_geosparql_pose_projection() -> None:
    """Pose positions project to WKT POINT; orientation is intentionally dropped."""
    g = project_graph("geosparql", _source())
    wkt = URIRef(GEO + "wktLiteral")
    points = {
        str(o)
        for o in g.objects(None, URIRef(GEO + "asWKT"))
        if isinstance(o, Literal) and o.datatype == wkt and "POINT" in str(o)
    }
    # Drone pose position (53.5449, -113.9244) projects to
    # POINT(-113.9244 53.5449).
    assert any("-113.9244" in p and "53.5449" in p for p in points)
    # Telescope pose position (53.54495, -113.92435) projects to
    # POINT(-113.92435 53.54495).
    assert any("-113.92435" in p and "53.54495" in p for p in points)
    # Orientation literals must NOT leak into the GeoSPARQL profile.
    for orient_val in ("0.70710678", "45.0", "10.0", "ZYX"):
        lit = Literal(orient_val)
        assert lit not in set(g.objects()), (
            f"Orientation literal {orient_val!r} leaked into GeoSPARQL projection"
        )
        assert lit not in set(g.subjects()), (
            f"Orientation literal {orient_val!r} leaked as"
            f" subject into GeoSPARQL projection"
        )
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


def test_gender_projection_displayable_and_suppressed() -> None:
    """Displayable gender identity → schema:gender/foaf:gender; suppressed never leaks.

    Orientation, gender expression and sexAssignedAtBirth have no schema/FOAF target
    and are dropped — the documented lossy drop.
    """
    for profile, ns in (("schema-org", SCHEMA), ("foaf", FOAF)):
        g = project_graph(profile, _source())
        gender = URIRef(ns + "gender")
        # Co-equal bigender: BOTH of alex's displayable identities are emitted.
        alex = set(g.objects(URIRef(IDENT + "alex"), gender))
        assert Literal("woman", lang="en") in alex
        assert Literal("non-binary", lang="en") in alex
        # The fresh self-described value (escape hatch) projects via its rdfs:label.
        sam = set(g.objects(URIRef(IDENT + "sam"), gender))
        assert Literal("demifluid (self-described)", lang="en") in sam
        # Transition: robin's CURRENT gender is emitted; the SUPPRESSED one is NOT.
        robin = set(g.objects(URIRef(IDENT + "robin"), gender))
        assert Literal("non-binary", lang="en") in robin
        assert Literal("man", lang="en") not in robin
        # Lossy drops: no orientation / sexAssignedAtBirth / expression leaks anywhere.
        blob = g.serialize(format="turtle")
        for dropped in ("asexual", "biromantic", "intersex", "androgynous", "saab"):
            assert dropped not in blob, f"{dropped} must not project to {profile}"
        _assert_no_gmeow_leakage(g)


def test_ontolex_projection() -> None:
    g = project_graph("ontolex", _source())
    ontolex_entry = URIRef("http://www.w3.org/ns/lemon/ontolex#LexicalEntry")
    ontolex_form = URIRef("http://www.w3.org/ns/lemon/ontolex#Form")
    ontolex_lexical_form = URIRef("http://www.w3.org/ns/lemon/ontolex#lexicalForm")
    ontolex_written_rep = URIRef("http://www.w3.org/ns/lemon/ontolex#writtenRep")
    lime_language = URIRef("http://www.w3.org/ns/lemon/lime#language")

    # 1. ex:nameLatin should be an ontolex:LexicalEntry.
    name_latin = URIRef(NAMES + "nameLatin")
    assert (name_latin, RDF.type, ontolex_entry) in g

    # 2. ex:nameLatin should have an ontolex:lexicalForm ontolex:Form.
    forms = list(g.objects(name_latin, ontolex_lexical_form))
    assert len(forms) == 1
    form = forms[0]
    assert form == URIRef(str(name_latin) + "-form")
    assert (form, RDF.type, ontolex_form) in g

    # 3. Form should have ontolex:writtenRep with retagged language tag ("en").
    assert (form, ontolex_written_rep, Literal("Patrick Colm Audley", lang="en")) in g

    # 4. ex:nameLatin should have lime:language as the external BCP-47 tag literal.
    assert (name_latin, lime_language, Literal("en", datatype=XSD.language)) in g

    # 5. ex:nameHan should project with Hans script code (e.g. "zh-Hans").
    name_han = URIRef(NAMES + "nameHan")
    assert (name_han, RDF.type, ontolex_entry) in g
    han_forms = list(g.objects(name_han, ontolex_lexical_form))
    assert len(han_forms) == 1
    han_form = han_forms[0]
    assert (han_form, ontolex_written_rep, Literal("欧德理", lang="zh-Hans")) in g

    _assert_no_gmeow_leakage(g)


def test_project_examples_round_trip(tmp_path: Path) -> None:
    written = project_examples(dist_dir=tmp_path)
    names = {p.name for p in written}
    # One Turtle per profile.
    for profile in PROFILES:
        assert f"gmeow-example-{profile}.ttl" in names
    # OAI-DC emits one XML per subject; verify at least one exists.
    xml_names = {
        n for n in names if n.startswith("gmeow-example-oai_dc-") and n.endswith(".xml")
    }
    assert len(xml_names) >= 1
    for path in written:
        assert path.exists()
        if path.suffix == ".ttl":
            assert len(Graph().parse(path, format="turtle")) > 0
        elif path.suffix == ".xml":
            from xml.etree import ElementTree as ET

            ET.parse(path)  # raises if malformed


# --------------------------------------------------------------------------- #
# Issue #98 — vCard geo, BOT, and "emit all, privilege none" tests
# --------------------------------------------------------------------------- #


def test_vcard_geo_projection() -> None:
    """Place coordinates → vcard:hasGeo with vcard:Geo node (RFC 5870 geo: URI)."""
    g = project_graph("vcard", _source())
    wb = URIRef(LOC + "westbourne112")
    # vcard:hasGeo points to a typed vcard:Geo node.
    geo_nodes = list(g.objects(wb, URIRef(VCARD + "hasGeo")))
    assert len(geo_nodes) == 1, f"expected one vcard:hasGeo target, got {geo_nodes}"
    geo_node = geo_nodes[0]
    assert (geo_node, RDF.type, URIRef(VCARD + "Geo")) in g
    # Latitude and longitude are emitted as datatype properties on the Geo node.
    assert (
        geo_node,
        URIRef(VCARD + "latitude"),
        Literal("53.544972", datatype=XSD.decimal),
    ) in g or (
        geo_node,
        URIRef(VCARD + "latitude"),
        Literal(53.544972),
    ) in g
    assert (
        geo_node,
        URIRef(VCARD + "longitude"),
        Literal("-113.924398", datatype=XSD.decimal),
    ) in g or (
        geo_node,
        URIRef(VCARD + "longitude"),
        Literal(-113.924398),
    ) in g
    # The node IRI is a geo: URI (RFC 5870).
    assert str(geo_node).startswith("geo:"), f"expected geo: URI, got {geo_node}"
    _assert_no_gmeow_leakage(g)


def test_bot_projection() -> None:
    """Building topology: placeType → BOT class; containsPlace →
    bot:hasStorey/hasSpace/bot:containsZone; adjacentTo → bot:adjacentZone."""
    site = URIRef(LOC + "acmeSite")
    lobby = URIRef(LOC + "lobby")
    extra = Graph()
    extra.add((site, RDF.type, URIRef(GMEOW + "Place")))
    extra.add((site, URIRef(GMEOW + "placeType"), URIRef(GMEOW + "placeTypeSite")))
    extra.add((site, URIRef(GMEOW + "containsPlace"), URIRef(LOC + "officeBuilding")))
    extra.add((lobby, RDF.type, URIRef(GMEOW + "Place")))
    extra.add((lobby, URIRef(GMEOW + "placeType"), URIRef(GMEOW + "placeTypeRoom")))
    extra.add((URIRef(LOC + "cornerOffice"), URIRef(GMEOW + "adjacentTo"), lobby))
    src = sparql.store_with(
        *[FIXTURES_DIR / name for name in _SOURCE_FIXTURES], extra_triples=extra
    )

    g = project_graph("bot", src)
    building = URIRef(LOC + "officeBuilding")
    floor = URIRef(LOC + "secondFloor")
    room = URIRef(LOC + "cornerOffice")
    # Class typing.
    assert (site, RDF.type, URIRef(BOT + "Site")) in g
    assert (building, RDF.type, URIRef(BOT + "Building")) in g
    assert (floor, RDF.type, URIRef(BOT + "Storey")) in g
    assert (room, RDF.type, URIRef(BOT + "Space")) in g
    assert (lobby, RDF.type, URIRef(BOT + "Space")) in g
    # Specific containment edges (existing hasStorey / hasSpace branches).
    assert (building, URIRef(BOT + "hasStorey"), floor) in g
    assert (floor, URIRef(BOT + "hasSpace"), room) in g
    # Generic containment edge (issue #83: containsPlace → bot:containsZone).
    assert (site, URIRef(BOT + "containsZone"), building) in g
    # Adjacency edge (issue #83: adjacentTo → bot:adjacentZone).
    assert (room, URIRef(BOT + "adjacentZone"), lobby) in g
    _assert_no_gmeow_leakage(g)


def test_schema_org_contested_place_names_emit_all() -> None:
    """Principle 9 — 'emit all, privilege none': every displayable place name projects
    as schema:name; the endonym/exonym distinction is a frame choice, not a ground-truth
    privileging. Superseded names remain suppressed (Principle 10)."""
    src = load_merged_graph(include_imports=False)
    src.parse(FIXTURES_DIR / "places-contested.ttl", format="turtle")
    g = project_graph("schema-org", src)
    disputed = URIRef(EX_PLACES + "disputedPlace")
    names = set(g.objects(disputed, URIRef(SCHEMA + "name")))
    # Both co-equal toponyms are emitted as schema:name via mapSchemaPlaceAllNames.
    assert Literal("Konstantinoupoli", lang="x-gmeow-el") in names
    assert Literal("Constantinople", lang="x-gmeow-en") in names
    # The superseded historical name is suppressed — never leaks.
    assert Literal("Byzantium", lang="x-gmeow-la") not in names
    _assert_no_gmeow_leakage(g)


# --------------------------------------------------------------------------- #
# Issue #205 — qb projection DataSet + DSD well-formedness
# --------------------------------------------------------------------------- #

QB = "http://purl.org/linked-data/cube#"
AGG = "https://example.org/agg/"


def test_qb_projection_well_formed() -> None:
    """QB projection emits a complete DataSet + DSD per Observation (IC-1, IC-2)."""
    src = load_merged_graph(include_imports=False)
    src.parse(FIXTURES_DIR / "aggregation.ttl", format="turtle")

    query = (PROJECTION_QUERY_DIR / "qb.rq").read_text(encoding="utf-8")
    result = src.query(query)
    assert result.graph is not None
    g = result.graph

    assert len(g) > 0, "qb projection produced no triples"

    obs = set(g.subjects(RDF.type, URIRef(QB + "Observation")))
    assert len(obs) == 2, f"expected 2 Observations, got {len(obs)}"

    # IC-1: every Observation has exactly one qb:dataSet.
    for o in obs:
        datasets = list(g.objects(o, URIRef(QB + "dataSet")))
        assert len(datasets) == 1, f"Observation {o} must have exactly one dataSet"

    # IC-2: every DataSet has exactly one qb:structure.
    all_datasets = set(g.subjects(RDF.type, URIRef(QB + "DataSet")))
    assert len(all_datasets) == 2, f"expected 2 DataSets, got {len(all_datasets)}"
    for d in all_datasets:
        dsd_list = list(g.objects(d, URIRef(QB + "structure")))
        assert len(dsd_list) == 1, f"DataSet {d} must have exactly one structure"

    # Reachability: Observation → DataSet → DSD, and DSD declares expected components.
    for o in obs:
        dataset = next(g.objects(o, URIRef(QB + "dataSet")))
        dsd = next(g.objects(dataset, URIRef(QB + "structure")))
        assert (dsd, RDF.type, URIRef(QB + "DataStructureDefinition")) in g

        comps = list(g.objects(dsd, URIRef(QB + "component")))
        assert len(comps) == 3, f"DSD {dsd} must declare 3 components, got {len(comps)}"

        # Measure-type dimension.
        mt_comps = [
            c
            for c in comps
            if (c, URIRef(QB + "dimension"), URIRef(QB + "measureType")) in g
        ]
        assert len(mt_comps) == 1, "expected one measureType dimension component"

        # observedFeature dimension.
        of_comps = [
            c
            for c in comps
            if (c, URIRef(QB + "dimension"), URIRef(GMEOW + "observedFeature")) in g
        ]
        assert len(of_comps) == 1, "expected one observedFeature dimension component"

        # obsValue measure.
        ov_comps = [
            c
            for c in comps
            if (c, URIRef(QB + "measure"), URIRef(QB + "obsValue")) in g
        ]
        assert len(ov_comps) == 1, "expected one obsValue measure component"

    # IC-3 / IC-4: component properties are explicitly typed.
    assert (
        URIRef(GMEOW + "observedFeature"),
        RDF.type,
        URIRef(QB + "DimensionProperty"),
    ) in g
    assert (
        URIRef(QB + "obsValue"),
        RDF.type,
        URIRef(QB + "MeasureProperty"),
    ) in g

    # Verify the observation scalar values are present.
    pop_count = URIRef(AGG + "popCount")
    avg_income = URIRef(AGG + "avgIncome")
    assert (
        pop_count,
        URIRef(QB + "obsValue"),
        Literal("1000000", datatype=XSD.integer),
    ) in g
    assert (
        avg_income,
        URIRef(QB + "obsValue"),
        Literal("45000.50", datatype=XSD.decimal),
    ) in g


# --------------------------------------------------------------------------- #
# #34 phases 2-3 coverage profiles
# --------------------------------------------------------------------------- #


def _fixture_store(name: str) -> gmeow_rdf.Store:
    data = Graph().parse(FIXTURES_DIR / name, format="turtle")
    return sparql.store_with(include_imports=False, extra_triples=data)


def test_logo_projection_schema_and_foaf() -> None:
    """#410 follow-through: gmeow:hasLogo projects to BOTH schema:logo and
    foaf:logo — the last residual schema term that closed the #34 parity gap.
    hasLogo is distinct from depicts, so the logo image is NOT also emitted as
    schema:about/schema:image of its bearer."""
    ex = "https://example.org/logo/"
    ent = URIRef(ex + "gmeow")
    img = URIRef(ex + "logoImg")
    old = URIRef(ex + "oldLogo")  # displayable false → must be suppressed (P10)
    gs = project_graph("schema-org", _fixture_store("logo.ttl"))
    assert (ent, URIRef(SCHEMA + "logo"), img) in gs
    assert (ent, URIRef(SCHEMA + "logo"), old) not in gs
    # a logo represents, it does not depict: no spurious aboutness edge.
    assert (ent, URIRef(SCHEMA + "about"), img) not in gs
    assert (img, URIRef(SCHEMA + "about"), ent) not in gs
    _assert_no_gmeow_leakage(gs)
    gf = project_graph("foaf", _fixture_store("logo.ttl"))
    assert (ent, URIRef(FOAF + "logo"), img) in gf
    assert (ent, URIRef(FOAF + "logo"), old) not in gf
    _assert_no_gmeow_leakage(gf)


def test_skos_projection_tag_concept_hierarchy() -> None:
    """A gmeow:Tag projects to a skos:Concept (prefLabel, broader, inScheme) and a
    gmeow:TagScheme to a skos:ConceptScheme; a displayable-false tag is suppressed."""
    skos = "http://www.w3.org/2004/02/skos/core#"
    g = Graph()
    g.parse(
        data="""
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        gmeow:tagLamp a gmeow:Tag ; rdfs:label "LAMP" ;
            gmeow:broaderTag gmeow:tagStack ; gmeow:tagInScheme gmeow:tech .
        gmeow:tagStack a gmeow:Tag ; rdfs:label "Stack" .
        gmeow:tech a gmeow:TagScheme .
        gmeow:hidden a gmeow:Tag ; rdfs:label "secret" ; gmeow:displayable false .
        """,
        format="turtle",
    )
    out = project_graph("skos", g)
    lamp = URIRef(GMEOW + "tagLamp")
    assert (lamp, RDF.type, URIRef(skos + "Concept")) in out
    assert (lamp, URIRef(skos + "prefLabel"), Literal("LAMP")) in out
    assert (lamp, URIRef(skos + "broader"), URIRef(GMEOW + "tagStack")) in out
    assert (lamp, URIRef(skos + "inScheme"), URIRef(GMEOW + "tech")) in out
    assert (URIRef(GMEOW + "tech"), RDF.type, URIRef(skos + "ConceptScheme")) in out
    # displayable false → never projected (P10)
    assert (URIRef(GMEOW + "hidden"), RDF.type, URIRef(skos + "Concept")) not in out
    _assert_no_gmeow_leakage(out)


def test_gedcom_projection() -> None:
    """Minted Marriage + symmetric spouseIn; sex ONLY from the saab datum."""
    g = project_graph("gedcom", _fixture_store("genealogy.ttl"))
    ged = "http://www.w3.org/2000/10/swap/pim/gedcom#"
    ex = "https://example.org/genealogy/"
    marriage = URIRef(ex + "couple-mt-marriage")
    assert (marriage, RDF.type, URIRef(ged + "Marriage")) in g
    spouses = set(g.subjects(URIRef(ged + "spouseIn"), marriage))
    assert spouses == {URIRef(ex + "mira"), URIRef(ex + "tomas")}
    # husband/wife emit ONLY in the clean documentary configuration (the
    # 1910 couple below); the modern couple's marriage carries neither.
    # THE CLEAN-OR-UNKNOWN RULE: an unambiguous recorded datum maps cleanly;
    # the partner WITHOUT one gets no row (never inferred from anything else);
    # EVERY ambiguous configuration degrades to "U" — never a forced M/F.
    sex = URIRef(ged + "sex")
    assert (URIRef(ex + "mira"), sex, Literal("F")) in g
    assert not list(g.objects(URIRef(ex + "tomas"), sex))
    assert set(g.objects(URIRef(ex + "record-intersex"), sex)) == {Literal("U")}
    assert set(g.objects(URIRef(ex + "record-contested"), sex)) == {Literal("U")}
    assert set(g.objects(URIRef(ex + "record-variation"), sex)) == {Literal("U")}
    # contested-free flat kinship flattens; the suppressed relative is absent.
    assert (URIRef(ex + "mira"), URIRef(ged + "child"), URIRef(ex + "lena")) in g
    assert URIRef(ex + "suppressed-relative") not in set(g.subjects()) | set(
        g.objects()
    )
    # THE CLEAN-CONFIGURATION RULE for the traditional terms: the 1910 couple
    # (both partners with clean documentary saab, one M one F) emits
    # husband/wife on its marriage; the modern couple (one partner without a
    # recorded datum) stays symmetric spouseIn-only.
    m1910 = URIRef(ex + "couple-1910-marriage")
    assert (m1910, URIRef(ged + "husband"), URIRef(ex + "ancestor-stefan")) in g
    assert (m1910, URIRef(ged + "wife"), URIRef(ex + "ancestor-helena")) in g
    assert not list(g.objects(marriage, URIRef(ged + "husband")))
    assert not list(g.objects(marriage, URIRef(ged + "wife")))
    # the withinFamily anchor yields childIn + the family's marriage edge
    fam = URIRef(ex + "family-1910")
    assert (URIRef(ex + "ancestor-jozef"), URIRef(ged + "childIn"), fam) in g
    assert (fam, URIRef(ged + "marriage"), m1910) in g
    _assert_no_gmeow_leakage(g)


def test_bibo_projection_doi_guard() -> None:
    """The two-cell DOI guard: 10.-prefixed → bibo:doi, else bibo:identifier."""
    g = project_graph("bibo", _fixture_store("publications.ttl"))
    bibo = "http://purl.org/ontology/bibo/"
    ex = "https://example.org/publications/"
    assert (
        URIRef(ex + "graphrag-paper"),
        URIRef(bibo + "doi"),
        Literal("10.5072/zenodo.999901"),
    ) in g
    assert not list(
        g.objects(URIRef(ex + "graphrag-paper"), URIRef(bibo + "identifier"))
    )
    assert (
        URIRef(ex + "retrieval-patent"),
        URIRef(bibo + "identifier"),
        Literal("US-2026-0123456-A1"),
    ) in g
    assert not list(g.objects(URIRef(ex + "retrieval-patent"), URIRef(bibo + "doi")))
    _assert_no_gmeow_leakage(g)


def test_bibframe_projection_minted_identifiers() -> None:
    """identifiedBy rides minted nodes: bf:Doi under the guard, bf:Identifier else."""
    g = project_graph("bibframe", _fixture_store("publications.ttl"))
    bf = "http://id.loc.gov/ontologies/bibframe/"
    ex = "https://example.org/publications/"
    # the minted node IRI carries the ENCODE_FOR_URI'd identifier value so
    # multiple identifiers on one work mint distinct nodes
    doi_node = URIRef(ex + "graphrag-paper-doi-10.5072%2Fzenodo.999901")
    assert (URIRef(ex + "graphrag-paper"), URIRef(bf + "identifiedBy"), doi_node) in g
    assert (doi_node, RDF.type, URIRef(bf + "Doi")) in g
    assert (doi_node, RDF.value, Literal("10.5072/zenodo.999901")) in g
    id_node = URIRef(ex + "retrieval-patent-identifier-US-2026-0123456-A1")
    assert (id_node, RDF.type, URIRef(bf + "Identifier")) in g
    # the reified title relator survives as a bf:Title node (reused IRI)
    assert (URIRef(ex + "paper-title"), RDF.type, URIRef(bf + "Title")) in g
    _assert_no_gmeow_leakage(g)


def test_org_projection_membership_relator() -> None:
    """The Membership relator survives reified with member/organization/role."""
    g = project_graph("org", _fixture_store("employment-contested.ttl"))
    org = "http://www.w3.org/ns/org#"
    memberships = list(g.subjects(RDF.type, URIRef(org + "Membership")))
    assert memberships, "no org:Membership rows"
    m = memberships[0]
    assert next(g.objects(m, URIRef(org + "member")), None) is not None
    assert next(g.objects(m, URIRef(org + "organization")), None) is not None
    assert next(g.objects(m, URIRef(org + "role")), None) is not None
    _assert_no_gmeow_leakage(g)


def test_sioc_projection_email_partial() -> None:
    """EmailMessage→Post, threading + authorship flatten to SIOC edges."""
    g = project_graph("sioc", _fixture_store("contact-fields.ttl"))
    sioc = "http://rdfs.org/sioc/ns#"
    # the contacts fixture may carry no messages (account triples alone don't
    # exercise threading); build a minimal in-memory one when ANY of the
    # asserted post/thread edges is missing — not merely when g is empty
    if not (
        any(g.subjects(RDF.type, URIRef(sioc + "Post")))
        and any(g.subject_objects(URIRef(sioc + "reply_of")))
        and any(g.subject_objects(URIRef(sioc + "has_container")))
    ):
        gm = GMEOW
        data = Graph()
        ex = "https://example.org/sioc/"
        for turtle in [
            f"<{ex}m1> a <{gm}EmailMessage> ; <{gm}partOfThread> <{ex}t1> .",
            f"<{ex}m2> a <{gm}EmailMessage> ; <{gm}inReplyTo> <{ex}m1> .",
            f"<{ex}t1> a <{gm}Thread> .",
        ]:
            data.parse(data=turtle, format="turtle")
        g = project_graph(
            "sioc", sparql.store_with(include_imports=False, extra_triples=data)
        )
    assert list(g.subjects(RDF.type, URIRef(sioc + "Post")))
    assert list(g.subject_objects(URIRef(sioc + "reply_of")))
    assert list(g.subject_objects(URIRef(sioc + "has_container")))
    _assert_no_gmeow_leakage(g)


def test_owl_time_boundary_instants() -> None:
    """startedAtTime/endedAtTime emit minted time:Instant boundary nodes."""
    g = project_graph("owl-time", _fixture_store("events.ttl"))
    time_ns = "http://www.w3.org/2006/time#"
    beginnings = list(g.subject_objects(URIRef(time_ns + "hasBeginning")))
    assert beginnings, "no boundary instants emitted"
    _interval, instant = beginnings[0]
    assert (instant, RDF.type, URIRef(time_ns + "Instant")) in g
    assert next(g.objects(instant, URIRef(time_ns + "inXSDDateTime")), None) is not None
    _assert_no_gmeow_leakage(g)


def test_claim_review_per_review_translation() -> None:
    """N competing Assessments -> N coexisting ClaimReviews (#34 correction).

    Each review is authored by its vantage and carries its OWN Rating; the
    contested claim loses nothing and no winner is picked — what stays
    refused is only an aggregated rating on the claim itself (P9).
    """
    g = project_graph("schema-org", _fixture_store("claim-reviews.ttl"))
    schema = "https://schema.org/"
    ex = "https://example.org/reviews/"
    reviews_of = {
        str(r): str(next(g.objects(r, URIRef(schema + "author"))))
        for r in g.subjects(RDF.type, URIRef(schema + "ClaimReview"))
        if (r, URIRef(schema + "itemReviewed"), URIRef(ex + "claim-disputed")) in g
    }
    assert len(reviews_of) == 2, "both competing verdicts must survive"
    assert set(reviews_of.values()) == {ex + "auditor-a", ex + "auditor-b"}
    # each review carries its own rating value — 0.9 and 0.2 both present
    values = set()
    for r in g.subjects(RDF.type, URIRef(schema + "ClaimReview")):
        if (r, URIRef(schema + "itemReviewed"), URIRef(ex + "claim-disputed")) in g:
            rating = next(g.objects(r, URIRef(schema + "reviewRating")))
            values.add(str(next(g.objects(rating, URIRef(schema + "ratingValue")))))
    assert values == {"0.9", "0.2"}
    # the claim ITSELF carries no rating (the P9 line that remains)
    assert not list(
        g.objects(URIRef(ex + "claim-disputed"), URIRef(schema + "reviewRating"))
    )
    # the singular case emits exactly one review
    singles = [
        r
        for r in g.subjects(RDF.type, URIRef(schema + "ClaimReview"))
        if (r, URIRef(schema + "itemReviewed"), URIRef(ex + "claim-single")) in g
    ]
    assert len(singles) == 1
    # the ANONYMOUS assessment passes through author-less: the review and its
    # rating emit, schema:author does not — anonymous stays anonymous, and
    # attribution is never an admission gate (the vantage atom is optional)
    anons = [
        r
        for r in g.subjects(RDF.type, URIRef(schema + "ClaimReview"))
        if (r, URIRef(schema + "itemReviewed"), URIRef(ex + "claim-anon")) in g
    ]
    assert len(anons) == 1
    assert not list(g.objects(anons[0], URIRef(schema + "author")))
    anon_rating = next(g.objects(anons[0], URIRef(schema + "reviewRating")))
    assert str(next(g.objects(anon_rating, URIRef(schema + "ratingValue")))) == "0.49"
    _assert_no_gmeow_leakage(g)


def test_prov_projection_qualification_pattern() -> None:
    """#415 (#34 verdict 8): the PROV qualification pattern over BuildActivity.

    The minted prov:Association binds the Builder participant to the Plan, and
    the Plan node's IRI IS the declared buildConfigUri — the URI the build
    system itself names, never a synthetic id. The output's generatedAtTime is
    the build's end instant.
    """
    g = project_graph("prov", _fixture_store("builds.ttl"))
    prov = "http://www.w3.org/ns/prov#"
    ex = "https://example.org/builds/"
    build = URIRef(ex + "buildV1")
    assoc = URIRef(ex + "buildV1-association")
    plan = URIRef(
        "https://github.com/example/meowgraph/blob/v1.0.0/.github/workflows/release.yml"
    )
    assert (build, RDF.type, URIRef(prov + "Activity")) in g
    assert (build, URIRef(prov + "used"), URIRef(ex + "releaseCommit")) in g
    assert (build, URIRef(prov + "generated"), URIRef(ex + "distTarball")) in g
    assert (build, URIRef(prov + "qualifiedAssociation"), assoc) in g
    assert (assoc, RDF.type, URIRef(prov + "Association")) in g
    assert (assoc, URIRef(prov + "agent"), URIRef(ex + "ciRunner")) in g
    assert (assoc, URIRef(prov + "hadPlan"), plan) in g
    assert (plan, RDF.type, URIRef(prov + "Plan")) in g
    ended = list(g.objects(build, URIRef(prov + "endedAtTime")))
    generated_at = list(
        g.objects(URIRef(ex + "distTarball"), URIRef(prov + "generatedAtTime"))
    )
    assert len(ended) == 1 and ended == generated_at
    # A build WITHOUT a buildConfigUri still binds its agent in the qualified
    # Association, but mints NO Plan: the optional-Plan branch (#415).
    build2 = URIRef(ex + "buildV2")
    assoc2 = URIRef(ex + "buildV2-association")
    assert (build2, URIRef(prov + "qualifiedAssociation"), assoc2) in g
    assert (assoc2, URIRef(prov + "agent"), URIRef(ex + "ciRunner")) in g
    assert not list(g.objects(assoc2, URIRef(prov + "hadPlan")))
    _assert_no_gmeow_leakage(g)


def test_schema_org_move_action_endpoints() -> None:
    """#412: endpoint-bearing events derive schema:MoveAction structurally.

    The typing is structural (any event with a from/to endpoint), so a
    single-endpoint emigration still derives the action shape — and the open
    eventType value never becomes a class (P9).
    """
    g = project_graph("schema-org", _fixture_store("migrations.ttl"))
    ex = "https://example.org/migrations/"
    move = URIRef(SCHEMA + "MoveAction")
    galician = URIRef(ex + "flow-galician")
    lanark = URIRef(ex + "flow-lanarkshire")
    assert (galician, RDF.type, move) in g
    assert (galician, URIRef(SCHEMA + "fromLocation"), URIRef(ex + "sniatyn")) in g
    assert (galician, URIRef(SCHEMA + "toLocation"), URIRef(ex + "mundare")) in g
    # one known endpoint still derives the action shape — both single-endpoint
    # branches: to-only (lanark) and from-only (departure).
    assert (lanark, RDF.type, move) in g
    assert (lanark, URIRef(SCHEMA + "toLocation"), URIRef(ex + "wallsend")) in g
    assert not list(g.objects(lanark, URIRef(SCHEMA + "fromLocation")))
    departure = URIRef(ex + "flow-galician-departure")
    assert (departure, RDF.type, move) in g
    assert (departure, URIRef(SCHEMA + "fromLocation"), URIRef(ex + "sniatyn")) in g
    assert not list(g.objects(departure, URIRef(SCHEMA + "toLocation")))
    _assert_no_gmeow_leakage(g)


def test_odrl_permission_scoped_duty() -> None:
    """#414 (#34 verdict 7): a permission's own duty rides odrl:duty on the
    permission node, while the statement-scoped duty stays odrl:obligation —
    the two scopes coexist (the CC-attribution pattern)."""
    g = project_graph("odrl", _fixture_store("rights.ttl"))
    odrl = "http://www.w3.org/ns/odrl/2/"
    ex = "https://example.org/rights/"
    perm = URIRef(ex + "perm-reproduce")
    # The permission's own duty rides odrl:duty on the permission node; it is a
    # DISTINCT instance from the statement-scoped duty (a node cannot be both an
    # unconditional obligation and a permission-conditional duty).
    perm_duty = URIRef(ex + "duty-attribute-perm")
    stmt_duty = URIRef(ex + "duty-attribute")
    assert (perm, URIRef(odrl + "duty"), perm_duty) in g
    assert (perm_duty, RDF.type, URIRef(odrl + "Duty")) in g
    assert (perm_duty, URIRef(odrl + "action"), URIRef(GMEOW + "actionAttribute")) in g
    # ODRL 2.2 does not inherit the permission's target onto its duty — the cell
    # states it explicitly from the statement's asset.
    assert (perm_duty, URIRef(odrl + "target"), URIRef(ex + "photo")) in g
    # the statement-scoped duty stays an unconditional obligation, NOT a duty.
    assert (URIRef(ex + "photo-rights"), URIRef(odrl + "obligation"), stmt_duty) in g
    assert not list(g.subjects(URIRef(odrl + "duty"), stmt_duty))
    # gmeow:actionAttribute rides as an odrl:action OBJECT value by design (the
    # consumer maps it); the guard still pins no predicate/type leakage.
    _assert_no_gmeow_leakage(g)


def test_cc_requires_attribution_from_permission_duty() -> None:
    """#414 follow-through: the permission-scoped attribution duty coarsens to
    CC REL's class-level requirement on the governed work."""
    g = project_graph("cc", _fixture_store("rights.ttl"))
    cc = "http://creativecommons.org/ns#"
    ex = "https://example.org/rights/"
    assert (
        URIRef(ex + "photo"),
        URIRef(cc + "requires"),
        URIRef(cc + "Attribution"),
    ) in g
    _assert_no_gmeow_leakage(g)


def test_schema_org_identifier_property_value() -> None:
    """#409: external-identifier records project to schema:PropertyValue.

    The reified gmeow:Identifier node IS reused as the PropertyValue bundle
    (propertyID ← scheme, value, url, name); a bare scheme+value record still
    projects its propertyID+value; persons and organizations share the one
    universal record class.
    """
    g = project_graph("schema-org", _fixture_store("identifier-records.ttl"))
    ex = "https://example.org/ids/"
    pv = URIRef(SCHEMA + "PropertyValue")
    geni = URIRef(ex + "id-geni")
    assert (URIRef(ex + "patrick"), URIRef(SCHEMA + "identifier"), geni) in g
    assert (geni, RDF.type, pv) in g
    assert (geni, URIRef(SCHEMA + "propertyID"), Literal("geni")) in g
    assert (geni, URIRef(SCHEMA + "value"), Literal("6000000001001221435")) in g
    assert (
        geni,
        URIRef(SCHEMA + "url"),
        URIRef("https://www.geni.com/profile/index/6000000001001221435"),
    ) in g
    # the record's display caption is its rdfs:label (NOT a bespoke *Name —
    # a Name is a reified Appellation), projected to schema:name (#409)
    assert (geni, URIRef(SCHEMA + "name"), Literal("Geni profile", lang="en")) in g
    # the bare scheme+value record still projects propertyID + value, no url/name
    nip = URIRef(ex + "id-nip05")
    assert (nip, RDF.type, pv) in g
    assert (nip, URIRef(SCHEMA + "propertyID"), Literal("nip05")) in g
    assert (
        nip,
        URIRef(SCHEMA + "value"),
        Literal("patrick@blackcatinformatics.ca"),
    ) in g
    assert not list(g.objects(nip, URIRef(SCHEMA + "url")))
    assert not list(g.objects(nip, URIRef(SCHEMA + "name")))
    # the org case reuses the SAME class
    ror = URIRef(ex + "id-ror")
    assert (URIRef(ex + "blackcat"), URIRef(SCHEMA + "identifier"), ror) in g
    assert (ror, RDF.type, pv) in g
    assert (ror, URIRef(SCHEMA + "propertyID"), Literal("ror")) in g
    _assert_no_gmeow_leakage(g)


def test_schema_org_web_presence() -> None:
    """#410: the web-presence + media surface.

    WebSite typing; the page→site hierarchy as schema:isPartOf; a page's
    principal subject as schema:mainEntity (+ inverse mainEntityOfPage);
    ProfilePage derived structurally for pages about an Agent (but NOT for a
    page about a plain Document); inLanguage from the content language; image
    from a depiction; DataDownload distributions with contentUrl + format.
    """
    g = project_graph("schema-org", _fixture_store("web-presence.ttl"))
    ex = "https://example.org/web/"
    assert (URIRef(ex + "site"), RDF.type, URIRef(SCHEMA + "WebSite")) in g
    # ProfilePage IS derived for pages about an Agent (person, org)...
    assert (URIRef(ex + "profile-page"), RDF.type, URIRef(SCHEMA + "ProfilePage")) in g
    assert (URIRef(ex + "about-page"), RDF.type, URIRef(SCHEMA + "ProfilePage")) in g
    # ...but NOT for a page about a plain Document.
    assert (URIRef(ex + "doc-page"), RDF.type, URIRef(SCHEMA + "ProfilePage")) not in g
    # the page→site hierarchy (the breadcrumb trail)
    assert (
        URIRef(ex + "profile-page"),
        URIRef(SCHEMA + "isPartOf"),
        URIRef(ex + "site"),
    ) in g
    # principal subject ↔ mainEntity / mainEntityOfPage
    assert (
        URIRef(ex + "profile-page"),
        URIRef(SCHEMA + "mainEntity"),
        URIRef(ex + "patrick"),
    ) in g
    assert (
        URIRef(ex + "patrick"),
        URIRef(SCHEMA + "mainEntityOfPage"),
        URIRef(ex + "profile-page"),
    ) in g
    # image from depiction; encodingFormat from media type
    assert (
        URIRef(ex + "patrick"),
        URIRef(SCHEMA + "image"),
        URIRef(ex + "headshot"),
    ) in g
    assert (
        URIRef(ex + "headshot"),
        URIRef(SCHEMA + "encodingFormat"),
        Literal("image/avif"),
    ) in g
    # both content languages
    langs = set(g.objects(URIRef(ex + "article"), URIRef(SCHEMA + "inLanguage")))
    assert Literal("en") in langs and Literal("fr") in langs
    # dataset distributions: DataDownload + contentUrl + encodingFormat
    assert (
        URIRef(ex + "dataset"),
        URIRef(SCHEMA + "distribution"),
        URIRef(ex + "dump-ttl"),
    ) in g
    assert (URIRef(ex + "dump-ttl"), RDF.type, URIRef(SCHEMA + "DataDownload")) in g
    assert (
        URIRef(ex + "dump-ttl"),
        URIRef(SCHEMA + "encodingFormat"),
        Literal("text/turtle"),
    ) in g
    cu = list(g.objects(URIRef(ex + "dump-ttl"), URIRef(SCHEMA + "contentUrl")))
    assert any(str(o) == "https://blackcatinformatics.ca/gmeow-full.ttl" for o in cu)
    _assert_no_gmeow_leakage(g)


def test_schema_org_syndication() -> None:
    """#412: postings, quotations, shares, and feeds.

    A social/microblog posting → schema:SocialMediaPosting, a blog posting →
    schema:BlogPosting (by the open kind value); quoted content → Quotation +
    citation; shared content → sharedContent; a DataFeed + members →
    DataFeed/dataFeedElement/DataFeedItem.
    """
    g = project_graph("schema-org", _fixture_store("syndication.ttl"))
    ex = "https://example.org/synd/"
    toot, blog, note = (URIRef(ex + n) for n in ("tootThreatModel", "blogPost", "note"))
    assert (toot, RDF.type, URIRef(SCHEMA + "SocialMediaPosting")) in g
    assert (note, RDF.type, URIRef(SCHEMA + "SocialMediaPosting")) in g
    assert (blog, RDF.type, URIRef(SCHEMA + "BlogPosting")) in g
    # quotation: the quoted article is typed Quotation and linked by citation
    assert (toot, URIRef(SCHEMA + "citation"), URIRef(ex + "article")) in g
    assert (URIRef(ex + "article"), RDF.type, URIRef(SCHEMA + "Quotation")) in g
    # the inline quoted text rides as schema:text on the Quotation (no drop)
    assert (
        URIRef(ex + "article"),
        URIRef(SCHEMA + "text"),
        Literal("the agent's actions are auditable provenance", lang="en"),
    ) in g
    # share/repost
    assert (toot, URIRef(SCHEMA + "sharedContent"), URIRef(ex + "diagram")) in g
    # feed + items
    assert (URIRef(ex + "feed"), RDF.type, URIRef(SCHEMA + "DataFeed")) in g
    assert (URIRef(ex + "feed"), URIRef(SCHEMA + "dataFeedElement"), blog) in g
    assert (blog, RDF.type, URIRef(SCHEMA + "DataFeedItem")) in g
    _assert_no_gmeow_leakage(g)


def test_sioc_feed_posting_resolves_partial() -> None:
    """#412: FeedPostings resolve the email slice's declared SIOC partiality —
    a posting IS a sioc:Post, a share is sioc:links_to."""
    g = project_graph("sioc", _fixture_store("syndication.ttl"))
    sioc = "http://rdfs.org/sioc/ns#"
    ex = "https://example.org/synd/"
    assert (URIRef(ex + "tootThreatModel"), RDF.type, URIRef(sioc + "Post")) in g
    assert (URIRef(ex + "blogPost"), RDF.type, URIRef(sioc + "Post")) in g
    assert (
        URIRef(ex + "tootThreatModel"),
        URIRef(sioc + "links_to"),
        URIRef(ex + "diagram"),
    ) in g
    _assert_no_gmeow_leakage(g)


def test_schema_org_people_org_presence() -> None:
    """#411/#417/#413: contact types, founder/owns, service dissolution,
    publisher/inventor contributions, and the offerings surface.

    Notably the language and day-of-week projections point at the first-class
    targets (schema:ComputerLanguage entity, schema:Monday enumeration member),
    NOT flattened label strings.
    """
    g = project_graph("schema-org", _fixture_store("people-org-presence.ttl"))
    ex = "https://example.org/po/"
    # contact type, founder, owns, service dissolution
    assert (
        URIRef(ex + "workEmail"),
        URIRef(SCHEMA + "contactType"),
        Literal("work", lang="en"),
    ) in g
    assert (URIRef(ex + "bii"), URIRef(SCHEMA + "founder"), URIRef(ex + "patrick")) in g
    assert (URIRef(ex + "patrick"), URIRef(SCHEMA + "owns"), URIRef(ex + "bii")) in g
    assert list(g.objects(URIRef(ex + "yahoo360"), URIRef(SCHEMA + "dissolutionDate")))
    # publisher + inventor via the Contribution pattern
    assert (
        URIRef(ex + "book"),
        URIRef(SCHEMA + "publisher"),
        URIRef(ex + "press"),
    ) in g
    assert (
        URIRef(ex + "patent"),
        URIRef(SCHEMA + "inventor"),
        URIRef(ex + "patrick"),
    ) in g
    # offerings
    assert (
        URIRef(ex + "bii"),
        URIRef(SCHEMA + "makesOffer"),
        URIRef(ex + "consultOffer"),
    ) in g
    assert (URIRef(ex + "consultOffer"), RDF.type, URIRef(SCHEMA + "Offer")) in g
    assert (URIRef(ex + "consulting"), RDF.type, URIRef(SCHEMA + "Service")) in g
    assert (
        URIRef(ex + "consulting"),
        URIRef(SCHEMA + "areaServed"),
        URIRef(ex + "canada"),
    ) in g
    # programmingLanguage points at the ENTITY, which is a ComputerLanguage —
    # NOT a flattened label string
    assert (
        URIRef(ex + "gmeowProj"),
        URIRef(SCHEMA + "programmingLanguage"),
        URIRef(ex + "python"),
    ) in g
    assert (URIRef(ex + "python"), RDF.type, URIRef(SCHEMA + "ComputerLanguage")) in g
    # dayOfWeek is the schema enumeration MEMBER (only the actual day), not a string
    days = set(g.objects(URIRef(ex + "hours"), URIRef(SCHEMA + "dayOfWeek")))
    assert days == {URIRef(SCHEMA + "Monday")}
    assert (
        URIRef(ex + "hours"),
        URIRef(SCHEMA + "opens"),
        Literal("09:00:00", datatype=XSD.time),
    ) in g
    _assert_no_gmeow_leakage(g)


def test_foaf_account_and_gedcom_marriage() -> None:
    """#411/#417 FOAF account idiom + #413 gedcom marriage date/place."""
    gf = project_graph("foaf", _fixture_store("people-org-presence.ttl"))
    ex = "https://example.org/po/"
    assert (URIRef(ex + "patrick"), URIRef(FOAF + "account"), URIRef(ex + "gh")) in gf
    assert (
        URIRef(ex + "gh"),
        URIRef(FOAF + "accountServiceHomepage"),
        URIRef("https://github.com"),
    ) in gf
    assert (URIRef(ex + "gh"), URIRef(FOAF + "accountName"), Literal("paudley")) in gf
    _assert_no_gmeow_leakage(gf)
    gg = project_graph("gedcom", _fixture_store("people-org-presence.ttl"))
    ged = "http://www.w3.org/2000/10/swap/pim/gedcom#"
    marriage = URIRef(ex + "couple-marriage")
    assert (
        marriage,
        URIRef(ged + "date"),
        Literal("1952-06-14T00:00:00Z", datatype=XSD.dateTime),
    ) in gg
    assert (
        marriage,
        URIRef(ged + "place"),
        Literal("Mundare, Alberta", lang="en"),
    ) in gg


def test_schema_org_affordances_and_exif() -> None:
    """#412 contact affordances + #413 EXIF tags.

    A contact point yields a structural schema:CommunicateAction with a
    mailto:/tel: target (the editorial action name is site copy, NOT
    fabricated). EXIF tags project to schema:exifData PropertyValues.
    """
    g = project_graph("schema-org", _fixture_store("people-org-presence.ttl"))
    ex = "https://example.org/po/"
    # communicate affordances (structure only; no fabricated names). Two emails
    # + one phone → THREE distinct action nodes (value-unique minting), each
    # with an IRI target, never one merged node carrying every target.
    comm = set(g.objects(URIRef(ex + "bii"), URIRef(SCHEMA + "potentialAction")))
    assert len(comm) == 3, "each email/phone must mint a DISTINCT CommunicateAction"
    target_objs = [o for a in comm for o in g.objects(a, URIRef(SCHEMA + "target"))]
    assert all(isinstance(o, URIRef) for o in target_objs), "target must be an IRI"
    targets = {str(o) for o in target_objs}
    assert targets == {
        "mailto:work.with.us@blackcatinformatics.ca",
        "mailto:hello@blackcatinformatics.ca",
        "tel:+16042452120",
    }
    for a in comm:
        assert (a, RDF.type, URIRef(SCHEMA + "CommunicateAction")) in g
        # exactly one target per action — no collapse
        assert len(list(g.objects(a, URIRef(SCHEMA + "target")))) == 1
        # the editorial name is NOT fabricated
        assert not list(g.objects(a, URIRef(SCHEMA + "name")))
    # EXIF tags → schema:exifData PropertyValues
    tags = set(g.objects(URIRef(ex + "photo"), URIRef(SCHEMA + "exifData")))
    assert URIRef(ex + "exAperture") in tags
    ap = URIRef(ex + "exAperture")
    assert (ap, RDF.type, URIRef(SCHEMA + "PropertyValue")) in g
    assert (ap, URIRef(SCHEMA + "propertyID"), Literal("FNumber")) in g
    assert (ap, URIRef(SCHEMA + "value"), Literal("f/2.0")) in g
    assert (ap, URIRef(SCHEMA + "name"), Literal("Aperture", lang="en")) in g
    _assert_no_gmeow_leakage(g)


def test_exif_round_trip_is_lossless() -> None:
    """#413: the real site index.ttl's schema:exifData round-trips through
    GMEOW's gmeow:ExifTag model with ZERO loss — every propertyID/value/name
    (including language tags) is reproduced. The condition for modelling EXIF
    as reified key-value records rather than dropping the raw tags.

    The source is the committed fixture exif-round-trip-source.ttl: the 12 real
    PropertyValues from the paudley site index.ttl, captured verbatim so this
    guard runs in every environment. Counter (multiset) semantics ensure a
    repeated tag cannot be silently collapsed and slip through unnoticed."""
    from collections import Counter

    src = Graph()
    src.parse(FIXTURES_DIR / "exif-round-trip-source.ttl", format="turtle")
    exif = URIRef(SCHEMA + "exifData")
    orig: Counter[tuple[Node | None, Node | None, Node | None]] = Counter()
    for _img, _p, pv in src.triples((None, exif, None)):
        pid = next(src.objects(pv, URIRef(SCHEMA + "propertyID")), None)
        val = next(src.objects(pv, URIRef(SCHEMA + "value")), None)
        label_o = next(src.objects(pv, URIRef(SCHEMA + "name")), None)
        orig[(pid, val, label_o)] += 1
    assert orig, "expected exifData in the parity fixture"
    # express each occurrence (multiset) in GMEOW and project back
    gm = Graph()
    rdfs_label = URIRef("http://www.w3.org/2000/01/rdf-schema#label")
    i = 0
    for (pid, val, label_o), count in orig.items():
        for _ in range(count):
            img = URIRef(f"https://example.org/rt/img{i}")
            tag = URIRef(f"https://example.org/rt/tag{i}")
            i += 1
            gm.add((img, RDF.type, URIRef(GMEOW + "MediaObject")))
            gm.add((img, URIRef(GMEOW + "hasExifTag"), tag))
            gm.add((tag, RDF.type, URIRef(GMEOW + "ExifTag")))
            if pid is not None:
                gm.add((tag, URIRef(GMEOW + "exifTagId"), pid))
            if val is not None:
                gm.add((tag, URIRef(GMEOW + "exifTagValue"), val))
            if label_o is not None:
                gm.add((tag, rdfs_label, label_o))
    store = sparql.store_with(include_imports=False, extra_triples=gm)
    proj = project_graph("schema-org", store)
    back: Counter[tuple[Node | None, Node | None, Node | None]] = Counter()
    for _s, _p, pv in proj.triples((None, exif, None)):
        pid = next(proj.objects(pv, URIRef(SCHEMA + "propertyID")), None)
        val = next(proj.objects(pv, URIRef(SCHEMA + "value")), None)
        label_o = next(proj.objects(pv, URIRef(SCHEMA + "name")), None)
        back[(pid, val, label_o)] += 1
    assert orig == back, f"EXIF round-trip lost data: {orig - back}"


def test_schema_org_business_facet() -> None:
    """#449 commercial facet: slogan, priceRange, accepted currencies/payment, taxID."""
    g = project_graph("schema-org", _fixture_store("org-business.ttl"))
    ex = "https://example.org/orgbiz/"
    acme = URIRef(ex + "acme")
    assert (
        acme,
        URIRef(SCHEMA + "slogan"),
        Literal("We make everything", lang="en"),
    ) in g
    assert (acme, URIRef(SCHEMA + "priceRange"), Literal("$$$")) in g
    currencies = {
        str(o) for o in g.objects(acme, URIRef(SCHEMA + "currenciesAccepted"))
    }
    assert currencies == {"USD", "CAD"}
    # the accepted PaymentMethod collapses to its label text
    assert (acme, URIRef(SCHEMA + "paymentAccepted"), Literal("cash", lang="en")) in g
    # taxID rides the Identifier facet (scheme "tax"), not a bespoke term
    assert (acme, URIRef(SCHEMA + "taxID"), Literal("12-3456789")) in g
    _assert_no_gmeow_leakage(g)


def test_schema_org_clean_wins() -> None:
    """#449 clean-1:1 wins: identical-term schema/foaf projections that were missing."""
    g = project_graph("schema-org", _fixture_store("clean-wins.ttl"))
    ex = "https://example.org/clean/"
    doc, org, sub = URIRef(ex + "doc"), URIRef(ex + "org"), URIRef(ex + "subOrg")
    assert (
        doc,
        URIRef(SCHEMA + "conformsTo"),
        URIRef("https://example.org/spec/v1"),
    ) in g
    assert (
        doc,
        URIRef(SCHEMA + "validFrom"),
        Literal("2026-01-01T00:00:00Z", datatype=XSD.dateTime),
    ) in g
    assert (
        doc,
        URIRef(SCHEMA + "validThrough"),
        Literal("2027-01-01T00:00:00Z", datatype=XSD.dateTime),
    ) in g
    assert (doc, URIRef(SCHEMA + "editor"), URIRef(ex + "editor")) in g
    assert (URIRef(ex + "data"), URIRef(SCHEMA + "owner"), org) in g
    assert (URIRef(ex + "data"), RDF.type, URIRef(SCHEMA + "Dataset")) in g
    # inverse: the PARENT gains schema:subOrganization pointing at the child
    assert (org, URIRef(SCHEMA + "subOrganization"), sub) in g
    _assert_no_gmeow_leakage(g)


def test_foaf_phone_clean_win() -> None:
    """#449 clean-1:1 win: telephone → foaf:phone."""
    g = project_graph("foaf", _fixture_store("clean-wins.ttl"))
    ex = "https://example.org/clean/"
    assert (URIRef(ex + "org"), URIRef(FOAF + "phone"), URIRef("tel:+16045551234")) in g
    _assert_no_gmeow_leakage(g)


def test_projection_bcp47_retag_no_internal_tags() -> None:
    """Projected consumer literals carry public BCP-47 tags, never x-gmeow-* —
    the projection-boundary retag catches names with no gmeow:nameLanguage link."""
    for fixture in ("names.ttl", "organizations.ttl"):
        for profile in ("schema-org", "foaf", "vcard"):
            g = project_graph(profile, _fixture_store(fixture))
            leaked = [
                o
                for _s, _p, o in g
                if isinstance(o, Literal)
                and o.language
                and o.language.startswith("x-gmeow")
            ]
            assert not leaked, f"{profile}/{fixture} leaked internal tags: {leaked[:3]}"


def test_schema_org_gap_clusters() -> None:
    """#449 genuine-gap clusters: person + media facets project to schema.org."""
    g = project_graph("schema-org", _fixture_store("gap-clusters.ttl"))
    ex = "https://example.org/gap/"
    ada, cam, clip = URIRef(ex + "ada"), URIRef(ex + "cambridge"), URIRef(ex + "clip")
    assert (ada, URIRef(SCHEMA + "nationality"), URIRef(ex + "uk")) in g
    assert (ada, URIRef(SCHEMA + "alumniOf"), cam) in g
    # inverse: the org gains schema:alumni pointing at the person
    assert (cam, URIRef(SCHEMA + "alumni"), ada) in g
    assert (
        clip,
        URIRef(SCHEMA + "transcript"),
        Literal("Hello, world.", lang="en"),
    ) in g
    assert (
        clip,
        URIRef(SCHEMA + "caption"),
        Literal("An introductory clip", lang="en"),
    ) in g
    _assert_no_gmeow_leakage(g)


def test_doap_gap_clusters() -> None:
    """#449 genuine-gap cluster: software-project web-presence facets → DOAP."""
    g = project_graph("doap", _fixture_store("gap-clusters.ttl"))
    ex = "https://example.org/gap/"
    doap = "http://usefulinc.com/ns/doap#"
    proj = URIRef(ex + "proj")
    assert (
        proj,
        URIRef(doap + "bug-database"),
        URIRef("https://github.com/example/gmeow/issues"),
    ) in g
    assert (
        proj,
        URIRef(doap + "download-page"),
        URIRef("https://example.org/download"),
    ) in g
    _assert_no_gmeow_leakage(g)

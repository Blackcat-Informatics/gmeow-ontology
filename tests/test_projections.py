"""Tests for the FnO/EDOAL projection layer (GMEOW → pure target profiles)."""

from __future__ import annotations

from pathlib import Path

import pyoxigraph
from rdflib import RDF, XSD, Graph, Literal, URIRef

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


def _source() -> pyoxigraph.Store:
    """The merged ontology + worked-example fixtures as a fast pyoxigraph store."""
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


def _fixture_store(name: str) -> pyoxigraph.Store:
    data = Graph().parse(FIXTURES_DIR / name, format="turtle")
    return sparql.store_with(include_imports=False, extra_triples=data)


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
    doi_node = URIRef(ex + "graphrag-paper-doi")
    assert (URIRef(ex + "graphrag-paper"), URIRef(bf + "identifiedBy"), doi_node) in g
    assert (doi_node, RDF.type, URIRef(bf + "Doi")) in g
    assert (doi_node, RDF.value, Literal("10.5072/zenodo.999901")) in g
    id_node = URIRef(ex + "retrieval-patent-identifier")
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
    _assert_no_gmeow_leakage(g)


def test_sioc_projection_email_partial() -> None:
    """EmailMessage→Post, threading + authorship flatten to SIOC edges."""
    g = project_graph("sioc", _fixture_store("contact-fields.ttl"))
    # the contacts fixture may carry no messages; build a minimal in-memory one
    if len(g) == 0:
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
    sioc = "http://rdfs.org/sioc/ns#"
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

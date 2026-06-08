"""Tests for the mapping DSL compiler (parser + four emitters)."""

from __future__ import annotations

from pathlib import Path

import pytest
from rdflib import RDF, RDFS, BNode, Graph, URIRef
from rdflib.namespace import Namespace
from rdflib.plugins.sparql import prepareQuery

from gmeow_tools.config import MAPPINGS_DIR, PREFIXES
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.mapping_compile import (
    _PROFILES,
    _validate_sssom,
    emit_edoal,
    emit_fno,
    emit_sparql,
    emit_sssom,
)
from gmeow_tools.mapping_dsl import CompileError, Expr, load_dsl, render_expr
from gmeow_tools.mappings import load_mappings
from gmeow_tools.projection_lint import fno_type_mismatches

GM = Namespace(PREFIXES["gmeow"])
FNO = Namespace(PREFIXES["fno"])
FNOM = Namespace(PREFIXES["fnom"])
ALIGN = Namespace(PREFIXES["align"])
EDOAL = Namespace(PREFIXES["edoal"])


def test_dsl_parses() -> None:
    dsl = load_dsl()
    # Every SSSOM data row became a TermEquivalence cell (incl. the 7 gUFO↔BFO
    # foundational-spine cells, issue #40, the 13 standpoint cells — PROV-O x3,
    # nanopub, CRMinf x3, Wikidata x2, schema.org, Web Annotation, DnS x2, #43 —
    # the 3 pronoun-set Wikidata cells, #46, and the maximal rights + rights-Wikidata
    # cells, #21 — ODRL action/constraint/operator/conflict vocabularies, CC REL,
    # dcterms, schema.org, SPDX, PREMIS, RightsStatements.org, ma-ont, Wikidata).
    # Issue #105 place naming: +8 names cells (PlaceName→CIDOC E48, hasPlaceName,
    # nameLanguage→dcterms/schema/P407, endonym/exonym) and -3 retired places
    # alternateName cells, net +5. OntoLex-Lemon: +3, net +8. Issue #65 locations
    # core: +3 Location alignments (CRM/BFO closeMatch). Issue #76 universal
    # mereology: +12 part/whole links (BFO, gUFO, schema.org, DCTERMS, CIDOC CRM).
    # Issue #72 SUPPRESS-GEN: +2 (coarsenTo→dpv:Generalisation,
    # coarserThan→skos:broader).
    # Issue #74 universal coreference: +5 authority/counterpart/version rows and
    # -1 retired places-local authorityLink row, net +4.
    # Issue #71 determinacy: +6 (Determinacy→gufo/BFO, 4 seeds→Wikidata, disputed).
    # Issue #73 privacy: +16 (SensitivityLevel↔dpv/gufo, hasSensitivity↔dpv, 2 seeds→
    # Wikidata, DataSubject/DataController/PrivacyNotice/hasPrivacyNotice/actionProcess
    # PersonalData↔dpv/schema.org/odrl).
    # Issue #70 frame-relativity: +9 (QUDT x3, FIBO x2, OWL-Time TRS x2, Lexvo x2).
    # Issue #67 expanded temporal: +52 net after removing 11 duplicates that
    # were split across classes.ttl/properties.ttl and consolidating them in
    # temporal.ttl per Principle 4 (one canonical source).
    # Issue #66 base observations: +16 (SOSA/SSN x6, PROV-O x4, CIDOC E13 x3,
    # determinacy x1, Wikidata x2).
    # Issue #68 standpoint enhancement: +2 (StandpointClaim→sosa:Observation,
    # Agent→sosa:Sensor).
    # Issue #95 distance/metric: +1 (proximity→schema:distance).
    # Issue #27 tagging building block: +16 (Tag↔skos:Concept/schema:DefinedTerm,
    # TagScheme↔skos:ConceptScheme/schema:DefinedTermSet, hasTag↔skos:hasTopConcept/
    # schema:keywords, isAbout↔schema:about/oa:hasTarget, Tagging↔oa:Annotation,
    # broaderTag↔skos:broader, narrowerTag↔skos:narrower, relatedTag↔skos:related,
    # tagInScheme↔skos:inScheme, 2 MOAT alignments, 2 tag-relation seeds).
    # Issue #78 pose/orientation: +16 (IEEE 1872 POS x5, Wikidata x11).
    # Issue #80 connectivity: +2 (Route→gtfs:Route, Route→schema:Trip).
    # Issue #81 lifecycle: +8 (+7 as originally authored, +1 eventTypeDissolution
    # mapping added per review feedback).
    # Issue #75 Profile meta-pattern: +1 (gmeow:Profile skos:relatedMatch prof:Profile).
    # Issue #77 universal quantity: +4 (Quantity→qudt:QuantityValue,
    # quantityValue→qudt:quantityValue, quantityUncertainty→qudt:standardUncertainty,
    # Quantity→sosa:Result).
    # Issue #69 universal claim construct: +6 (IdentityFacet→sosa:Observation,
    # NameUsage→sosa:Observation, RightsStatement→sosa:Observation,
    # KinRelationship→sosa:Observation, facetSubject→sosa:hasFeatureOfInterest,
    # facetVantage→sosa:madeBySensor).
    # Issue #101 spatial aggregation: +5 (SpatialAggregation→qb:Observation,
    # Dataset→qb:DataSet, AggregationFunction→qb:MeasureProperty,
    # containsPlace→geo:sfContains, hasCentroid→geo:hasCentroid).
    # Place is intentionally NOT aligned to qb:DimensionProperty (category
    # mismatch: object class vs. metaclass of properties).
    # Issue #82 terrestrial realm deepening: +20 (LinkedGeoData x3, CIDOC-CRM+CRMgeo x4,
    # Pleiades x3, WHG x2, sf: x3, Wikidata x5).
    # Issue #102 accessibility: +14 (hasAccessibilityFeature→schema:a11yFeature,
    # hasBarrier→schema:a11yHazard, AccessibilityAssertion→sosa:Observation,
    # 7 facet→ICF, 4 duplicate ICF facet alignments for step-free/auditory/cognitive
    # /clearance bridging to shared ICF categories).
    # Issue #99 data quality: +10 (DQV x4, GeoDCAT-AP/OA x1, PROV-O lineage x1,
    # Wikidata x3).
    # Issue #94 motion: +2 (LocationState→mf:TemporalGeometry,
    # Trajectory→mf:TemporalTrajectory).
    # Issue #100 capacity/occupancy: +3 (Brick Capacity, Brick Occupancy,
    # schema.org maximumAttendeeCapacity).
    # Issue #161 cross-cutting versions: +3 (versionLabel→schema:version,
    # versionLabel→doap:revision, VersionSet→doap:Project).
    # Issue #97 cross-cutting multilingual labels: +11
    # (hasOrganizationName→schema/vcard/foaf, hasTitle→dcterms/schema/headline,
    # CreativeWorkTitle→schema, AgreementName→schema, SoftwareName→schema,
    # hasAgreementName→schema, hasSoftwareName→schema).
    # Issue #103 regulatory overlays: +14 (RegulatoryOverlay→schema/Wikidata,
    # overlayType*→Wikidata x8, civilTimeZone→time:TimeZone/Wikidata,
    # overlayAuthority→schema:organizer, overlayRegulation→schema:legislation).
    # Issue #96 streaming: +10 observations (removed eqObs028 per review),
    # +4 places (eqPlaces104-107); closeMatch→broadMatch/relatedMatch per review.
    # Issue #106 events spacetime/trajectory: +6 (LocationState→E92/SP1,
    # Trajectory→E92, eventSpacetime/eventTrajectory→E92/SP1 x2).
    # Issue #104 sensory environment: +5 (SOSA/SSN x5:
    # SensoryObservation→sosa:Observation, SensoryEnvironment→sosa:FeatureOfInterest,
    # CoordinateMatrix→sosa:Result, hasMeasuredCondition→sosa:hasResult,
    # SensoryPerception→sosa:Observation).
    # Issue #83 indoor realm (BOT / ifcOWL): +11 (placeType* → bot/ifc x8,
    # Place→bot:Zone, containsPlace→bot:containsZone,
    # adjacentTo→bot:adjacentZone).
    # Issue #84 virtual + network address space: +11 (NetworkAddress→Wikidata,
    # networkAddressType*→Wikidata x6 incl. port,
    # virtualLocationType*→schema.org/Wikidata x3, networkAddressTypeBGP→Wikidata).
    # Issue #85 celestial realm: +19 (IVOA refframe x3, refposition x4 incl.
    # heliocentric, object-type x4, UAT x2 in places.ttl; IVOA timescale x6 in
    # temporal.ttl).
    # Issue #86 mathematical / n-D reference frames: +9 (Wikidata/ML-Schema
    # alignments in places.ttl).
    # Issue #87 psychological / cognitive realm: +4 (
    # MentalReferenceFrame→mf:mental process,
    # SensoryPerception→mf:perceptual process,
    # referenceFrameAffectiveCircumplex→mfoem:affective process,
    # referenceFrameAffectiveCircumplex→mfoem:emotion process).
    # Issue #88 robotic realm: +9 (Wikidata x2, CORA x2, KnowRob x4, SOMA x1).
    # Issue #89 narrative realm: +4 (schema:Book, schema:Episode in
    # narrative.ttl; wd:Q1774138, wd:Q15706943 in narrative.ttl).
    # Issue #124 temporal measurement under observation: +4
    # (measuredDate→time:hasTime, measuredDate→time:inXSDDateTimeStamp,
    # DatingMethod→dcterms:method, DatingMethod→crm:P33_used_specific_technique).
    # Issue #31 enhanced Wikidata ontology usage: +26 (Agent, Document, CreativeWork,
    # Article, WebPage, Dataset, MediaObject, LifeEvent, eventTypeBirth/Death/Marriage/
    # Divorce/Adoption/Graduation, TelephoneNumber, EmailAddress, Mailbox, Credential,
    # CryptographicKey, Certification, CalendarSystem, languageCode, Occupation, Skill,
    # Group, Activity).
    assert len(dsl.equivalences) == 1060
    # 27 projection transforms declared (incl. fnPronounSetToText #46,
    # fnSelectEndonym + fnSelectExonym #105, fnCoarsenToGranularity #72,
    # fnTagToKeyword + fnTaggingToAnnotation #27,
    # fnPosePositionToWktPoint #78,
    # fnRetagGeoJson + fnCoarsenToGranularityGeoJson #82).
    assert len(dsl.functions) == 28
    # One MappingSet per TSV (incl. gmeow-foundational, gmeow-standpoint,
    # gmeow-events, gmeow-rights, gmeow-coreference, gmeow-determinacy, gmeow-privacy).
    # Issue #70 adds gmeow-qudt, gmeow-fibo, gmeow-temporal.
    # Issue #67 expands gmeow-temporal (merged, not double-counted).
    # Issue #66 adds gmeow-observations.
    # Issue #27 adds gmeow-tags.
    # gmeow-colourspace is intentionally omitted (no TermEquivalence entries).
    # Issue #27 adds gmeow-tags.
    # Issue #80 adds gmeow-connectivity.
    # Issue #81 lifecycle: +1 (gmeow-lifecycle.sssom.tsv).
    # Issue #101 spatial aggregation: +1 (gmeow-aggregation.sssom.tsv).
    # Issue #102 accessibility: +1 (gmeow-accessibility.sssom.tsv).
    # Issue #99 data quality: +1 (gmeow-quality.sssom.tsv).
    # Issue #161 versions: +1 (gmeow-versions.sssom.tsv).
    # Issue #104 sensory environment: +1 (gmeow-sensory-environment.sssom.tsv).
    # Issue #89 narrative realm: +1 (gmeow-narrative.sssom.tsv).
    assert len(dsl.mapping_sets) == 32
    # Projection cells across all eight profiles (incl. ical, owl-time, odrl, cc).
    assert len(dsl.projections) > 30
    profiles = {b.profile for cell in dsl.projections for b in cell.bindings}
    assert profiles == set(_PROFILES)


def test_fno_type_derived_from_ontology_range() -> None:
    """fno:type is derived from the predicate's rdfs:range — never authored."""
    dsl = load_dsl()
    onto = load_merged_graph(include_imports=False)
    fno = emit_fno(dsl, onto)
    # The eventTime parameter must carry exactly the ontology range of eventTime.
    expected = onto.value(GM.eventTime, RDFS.range)
    assert expected is not None
    for param in fno.subjects(RDF.type, FNO.Parameter):
        if fno.value(param, FNO.predicate) == GM.eventTime:
            assert fno.value(param, FNO.type) == expected
            break
    else:  # pragma: no cover
        pytest.fail("no parameter bound to gmeow:eventTime was emitted")


def test_emitted_fno_satisfies_type_invariant(tmp_path: Path) -> None:
    """The emitted FnO catalog passes fno_type_mismatches by construction."""
    import shutil

    from gmeow_tools.config import PROJECTIONS_DIR

    dsl = load_dsl()
    onto = load_merged_graph(include_imports=False)
    proj = tmp_path / "projections"
    proj.mkdir()
    fno_out = proj / "functions.fno.ttl"
    emit_fno(dsl, onto).serialize(destination=fno_out, format="turtle")
    shutil.copy2(PROJECTIONS_DIR / "transforms.fno.ttl", proj / "transforms.fno.ttl")
    assert fno_type_mismatches(proj) == []


def test_sparql_executors_are_valid_queries() -> None:
    dsl = load_dsl()
    for profile in _PROFILES:
        query = emit_sparql(dsl, profile)
        prepareQuery(query)  # raises on a malformed query


def test_per_profile_fanout() -> None:
    """One transform (fnSelectDisplayName) fans out to schema/foaf/vcard cells."""
    dsl = load_dsl()
    seen: dict[str, object] = {}
    for profile in ("schema-org", "foaf", "vcard"):
        graph = emit_edoal(dsl, profile)
        for cell in graph.subjects(RDF.type, ALIGN.Cell):
            trans = graph.value(cell, EDOAL.transformation)
            if trans is None:
                continue
            fn = graph.value(trans, RDFS.seeAlso)
            if fn == GM.fnSelectDisplayName:
                entity2 = graph.value(cell, ALIGN.entity2)
                seen[profile] = graph.value(entity2, EDOAL.uri)
    assert seen["schema-org"] == URIRef(PREFIXES["schema"] + "name")
    assert seen["foaf"] == URIRef(PREFIXES["foaf"] + "name")
    assert seen["vcard"] == URIRef(PREFIXES["vcard"] + "fn")


def test_vcard_name_node_minting() -> None:
    """The synthetic vcard:Name node IRI is minted as {subject}-vcardname."""
    dsl = load_dsl()
    query = emit_sparql(dsl, "vcard")
    assert 'IRI(CONCAT(STR(?ent), "-vcardname"))' in query
    assert "?ent vcard:hasName ?vname ." in query


def test_value_class_table_emitted() -> None:
    dsl = load_dsl()
    query = emit_sparql(dsl, "schema-org")
    assert "VALUES ( ?pt ?ptClass )" in query
    assert "( gmeow:placeTypeCountry schema:Country )" in query


def test_schema_org_accessibility_predicate_separation() -> None:
    """The schema-org projection must not cross-emit accessibility predicates:
    hasAccessibilityFeature rows must not materialize schema:accessibilityHazard
    and hasBarrier rows must not materialize schema:accessibilityFeature."""
    dsl = load_dsl()
    query = emit_sparql(dsl, "schema-org")
    # Each branch should use a distinct variable so there is no cross-emission.
    assert "?place schema:accessibilityFeature ?featureFacet" in query
    assert "?place schema:accessibilityHazard ?hazardFacet" in query
    # Ensure the value vars are distinct (not the shared "?facet" that caused
    # the cross-emission bug).
    assert "?featureFacet" in query
    assert "?hazardFacet" in query


def test_sssom_roundtrips_to_committed() -> None:
    """The emitted SSSOM rows equal the committed rows (set-wise)."""
    generated = emit_sssom(load_dsl())
    committed = load_mappings(MAPPINGS_DIR)
    committed_by_file: dict[str, set[tuple[str, str, str]]] = {}
    for m in committed:
        committed_by_file.setdefault(m.source.name, set()).add(
            (m.subject_id, m.predicate_id, m.object_id)
        )
    for file, text in generated.items():
        rows = set()
        for line in text.splitlines():
            if line.startswith("#") or line.startswith("subject_id"):
                continue
            cols = line.split("\t")
            if len(cols) >= 3:
                rows.add((cols[0], cols[1], cols[2]))
        assert rows == committed_by_file[file], f"SSSOM drift in {file}"


def test_malformed_expression_raises() -> None:
    """An expression node with neither a variable nor an operator is rejected."""
    from rdflib import BNode

    from gmeow_tools.mapping_dsl import _expr

    graph = Graph()
    node = BNode()
    graph.add((node, GM.somethingElse, URIRef("urn:x")))
    with pytest.raises(CompileError):
        _expr(graph, node)


def test_render_expr_unknown_operator_raises() -> None:
    with pytest.raises(CompileError):
        render_expr(Expr(op=URIRef("urn:opNope"), args=()))


def test_sparql_string_escapes_control_chars() -> None:
    """String constants with newlines/tabs/quotes stay valid one-line literals."""
    from gmeow_tools.mapping_dsl import sparql_string

    out = sparql_string('a"b\nc\td\\e')
    assert "\n" not in out and "\t" not in out
    assert out == '"a\\"b\\nc\\td\\\\e"'


def test_fno_emit_rejects_input_without_range() -> None:
    """A transform input predicate with no rdfs:range cannot be compiled.

    Keeps the headline guarantee honest: fno:type is always derived, so the
    failure mode can never silently become a *missing* type.
    """
    from gmeow_tools.mapping_dsl import Dsl, ProjectionFunction

    fn = ProjectionFunction(
        iri=GM.fnNoRange,
        label="x",
        description="",
        inputs=(GM.predicateWithNoRange,),
        optional_inputs=(),
        output=GM.projectedX,
        output_type=RDFS.Literal,
    )
    dsl = Dsl(equivalences=(), projections=(), functions={GM.fnNoRange: fn})
    with pytest.raises(CompileError, match="no rdfs:range"):
        emit_fno(dsl, Graph())


def test_fno_emit_rejects_param_iri_collision() -> None:
    """Two predicates minting the same param IRI are rejected, not silently merged."""
    from gmeow_tools.mapping_dsl import Dsl, ProjectionFunction

    onto = Graph()
    onto.add((GM.placeType, RDFS.range, GM.PlaceType))
    onto.add(
        (URIRef(GM + "PlaceType"), RDFS.range, GM.PlaceType)
    )  # collides on paramPlaceType
    functions = {
        GM.fnA: ProjectionFunction(
            GM.fnA, "a", "", (GM.placeType,), (), GM.outA, RDFS.Literal
        ),
        GM.fnB: ProjectionFunction(
            GM.fnB, "b", "", (URIRef(GM + "PlaceType"),), (), GM.outB, RDFS.Literal
        ),
    }
    dsl = Dsl(equivalences=(), projections=(), functions=functions)
    with pytest.raises(CompileError, match="param IRI collision"):
        emit_fno(dsl, onto)


def test_edoal_traversal_uses_compose_inverse() -> None:
    """Multi-hop traversals derive an EDOAL relation path (compose/inverse)."""
    from rdflib.collection import Collection

    graph = emit_edoal(load_dsl(), "schema-org")
    birth = sub_org = None
    for cell in graph.subjects(RDF.type, ALIGN.Cell):
        label = str(graph.value(cell, RDFS.label) or "")
        if label.startswith("Birth"):
            birth = cell
        elif "subOrganizationOf" in label:
            sub_org = cell
    assert birth is not None and sub_org is not None

    # Birth: compose( inverse(participationParticipant), participationEvent, eventTime )
    # — the reified-Participation traversal that replaced the flat hasPrincipal (#41).
    compose = graph.value(graph.value(birth, ALIGN.entity1), EDOAL.compose)
    assert compose is not None
    steps = list(Collection(graph, compose))
    assert len(steps) == 3
    inv = graph.value(steps[0], EDOAL.inverse)
    assert graph.value(inv, EDOAL.uri) == GM.participationParticipant
    assert graph.value(steps[1], EDOAL.uri) == GM.participationEvent
    assert graph.value(steps[2], EDOAL.uri) == GM.eventTime

    # subOrganizationOf: a bare inverse (single step, no compose)
    e1 = graph.value(sub_org, ALIGN.entity1)
    assert graph.value(e1, EDOAL.compose) is None
    assert (
        graph.value(graph.value(e1, EDOAL.inverse), EDOAL.uri) == GM.subOrganizationOf
    )


def test_edoal_has_no_orphan_relation_nodes() -> None:
    """Template-only mappings must not leave unattached relation bnodes."""
    graph = emit_edoal(load_dsl(), "vcard")

    for relation in graph.subjects(RDF.type, EDOAL.Relation):
        assert list(graph.subjects(None, relation)), (
            f"orphan EDOAL relation node for {graph.value(relation, EDOAL.uri)}"
        )


def test_render_expr_extensions() -> None:
    from rdflib import Literal

    from gmeow_tools.mapping_dsl import Expr

    assert (
        render_expr(Expr(op=GM.opAdd, args=(Expr(var="a"), Expr(var="b"))))
        == "(?a + ?b)"
    )
    assert (
        render_expr(
            Expr(op=GM.opStrLang, args=(Expr(var="s"), Expr(const=Literal("fr"))))
        )
        == 'STRLANG(?s, "fr")'
    )
    assert (
        render_expr(Expr(op=GM.opIn, args=(Expr(var="x"), Expr(const=GM.a))))
        == "(?x IN (gmeow:a))"
    )
    assert render_expr(Expr(op=GM.opNot, args=(Expr(var="b"),))) == "(!?b)"


def test_retagging_uses_dedicated_bcp47_tag() -> None:
    """Retagging must not treat every registry languageCode as a BCP-47 tag."""
    ontolex = emit_sparql(load_dsl(), "ontolex")

    assert "gmeow:bcp47Tag" in ontolex
    assert "gmeow:languageCode" not in ontolex
    assert "lime:language ?langTag" in ontolex
    assert "STR(?langTag)" in ontolex


def test_render_path_extensions() -> None:
    from rdflib.collection import Collection

    from gmeow_tools.mapping_dsl import _render_path

    graph = Graph()
    inv = BNode()
    graph.add((inv, RDF.type, GM.InversePath))
    graph.add((inv, GM.pathStep, GM.foo))
    assert _render_path(graph, inv) == "^gmeow:foo"

    oom = BNode()
    graph.add((oom, RDF.type, GM.OneOrMorePath))
    graph.add((oom, GM.pathStep, GM.bar))
    assert _render_path(graph, oom) == "gmeow:bar+"

    neg = BNode()
    members = BNode()
    Collection(graph, members, [GM.a, GM.b])
    graph.add((neg, RDF.type, GM.NegatedPropertySet))
    graph.add((neg, GM.pathSet, members))
    assert _render_path(graph, neg) == "!(gmeow:a|gmeow:b)"


def test_fno_emits_fnom_implementation_mapping() -> None:
    """Each function declares its SPARQL implementation via fno:/fnom: vocabulary."""
    graph = emit_fno(load_dsl(), load_merged_graph(include_imports=False))
    assert (
        len(set(graph.subjects(RDF.type, FNO.Implementation))) == 7
    )  # one per profile WITH transforms (owl-time is pure templateAtoms — none;
    # web-annotation added #27)
    bound = False
    for mapping in graph.subjects(RDF.type, FNO.Mapping):
        if graph.value(mapping, FNO.function) != GM.fnComposeBcp47:
            continue
        for pmap in graph.objects(mapping, FNO.parameterMapping):
            if graph.value(pmap, FNOM.functionParameter) == GM.paramLanguageCode:
                assert str(graph.value(pmap, FNOM.implementationProperty)) == "code"
                bound = True
    assert bound


def test_sssom_object_label_and_provenance() -> None:
    out = emit_sssom(load_dsl())
    wikidata = out["gmeow-wikidata.sssom.tsv"]
    header = next(ln for ln in wikidata.splitlines() if ln.startswith("subject_id"))
    assert "object_label" in header.split("\t")
    assert "\thuman\t" in wikidata  # wd:Q5's label, now in object_label
    assert "# mapping_tool: gmeow compile-mappings" in wikidata
    assert "# mapping_date:" in wikidata
    assert "# curie_map:" in wikidata


def test_sssom_validates_with_sssom_toolkit() -> None:
    """Every generated SSSOM file passes sssom-py schema validation."""
    errors = _validate_sssom(emit_sssom(load_dsl()))
    assert not errors, "\n".join(errors)


def test_malformed_sssom_fails_validation() -> None:
    """An invalid SSSOM TSV (unknown prefix) is caught by the validation gate."""
    bad = (
        "# mapping_tool: gmeow compile-mappings\n"
        "# curie_map:\n"
        "#   gmeow: https://example.org/\n"
        "subject_id\tpredicate_id\tobject_id\n"
        "unknown:Foo\tskos:closeMatch\tgmeow:B\n"
    )
    problems = _validate_sssom({"mappings/bad.sssom.tsv": bad})
    assert problems
    assert any("Missing prefix" in p for p in problems)


def test_compile_all_check_stops_on_sssom_validation_failure(  # type: ignore[no-untyped-def]
    monkeypatch,
) -> None:
    """``compile_all()`` aborts when ``_validate_sssom`` reports errors."""
    from gmeow_tools import mapping_compile as mc

    def _bad_validate(_: dict[str, str]) -> list[str]:
        return ["mappings/x.sssom.tsv: synthetic validation failure"]

    monkeypatch.setattr(mc, "_validate_sssom", _bad_validate)
    with pytest.raises(CompileError, match="SSSOM validation failed"):
        mc.compile_all()


def test_drift_flags_orphaned_sssom(tmp_path: Path, monkeypatch) -> None:  # type: ignore[no-untyped-def]
    """A committed SSSOM file the DSL no longer produces is reported as drift."""
    from gmeow_tools import mapping_compile as mc

    dsl = load_dsl()
    onto = load_merged_graph(include_imports=False)
    rdf_graphs, sparql_texts, sssom_texts = mc._artifacts(dsl, onto)
    root = tmp_path / "root"
    mc._write_tree(root, rdf_graphs, sparql_texts, sssom_texts)

    # A committed mappings dir that matches the fresh output plus one orphan.
    committed = tmp_path / "mappings"
    committed.mkdir()
    for name in (root / "mappings").glob("*.sssom.tsv"):
        (committed / name.name).write_text(name.read_text(encoding="utf-8"))
    (committed / "gmeow-orphan.sssom.tsv").write_text("subject_id\n", encoding="utf-8")

    monkeypatch.setattr(mc, "MAPPINGS_DIR", committed)
    drifted = mc._drift(root)
    assert any("gmeow-orphan.sssom.tsv" in d and "orphaned" in d for d in drifted)

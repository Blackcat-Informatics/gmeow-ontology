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
    # Every SSSOM data row became a TermEquivalence cell.
    assert len(dsl.equivalences) == 353
    # 14 projection transforms declared.
    assert len(dsl.functions) == 14
    # One MappingSet per TSV.
    assert len(dsl.mapping_sets) == 12
    # Projection cells across all four profiles.
    assert len(dsl.projections) > 30
    profiles = {b.profile for cell in dsl.projections for b in cell.bindings}
    assert profiles == set(_PROFILES)


def test_fno_type_derived_from_ontology_range() -> None:
    """fno:type is derived from the predicate's rdfs:range — never authored."""
    dsl = load_dsl()
    onto = load_merged_graph(include_imports=False)
    fno = emit_fno(dsl, onto)
    # The eventDate parameter must carry exactly the ontology range of eventDate.
    expected = onto.value(GM.eventDate, RDFS.range)
    assert expected is not None
    for param in fno.subjects(RDF.type, FNO.Parameter):
        if fno.value(param, FNO.predicate) == GM.eventDate:
            assert fno.value(param, FNO.type) == expected
            break
    else:  # pragma: no cover
        pytest.fail("no parameter bound to gmeow:eventDate was emitted")


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

    # Birth: compose( inverse(hasPrincipal), eventDate )
    compose = graph.value(graph.value(birth, ALIGN.entity1), EDOAL.compose)
    assert compose is not None
    steps = list(Collection(graph, compose))
    assert len(steps) == 2
    inv = graph.value(steps[0], EDOAL.inverse)
    assert graph.value(inv, EDOAL.uri) == GM.hasPrincipal
    assert graph.value(steps[1], EDOAL.uri) == GM.eventDate

    # subOrganizationOf: a bare inverse (single step, no compose)
    e1 = graph.value(sub_org, ALIGN.entity1)
    assert graph.value(e1, EDOAL.compose) is None
    assert (
        graph.value(graph.value(e1, EDOAL.inverse), EDOAL.uri) == GM.subOrganizationOf
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
        len(set(graph.subjects(RDF.type, FNO.Implementation))) == 4
    )  # one per profile
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

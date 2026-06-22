"""Tests for the mapping DSL compiler (parser + four emitters)."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest
from gmeow_rdf.compat.rdflib import RDF, RDFS, BNode, Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import Namespace
from gmeow_rdf.compat.rdflib.plugins.sparql import prepareQuery

from gmeow_tools.config import MAPPING_DSL_DIR, MAPPINGS_DIR, PREFIXES, PROJECT_ROOT
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

_DETERMINISTIC_EDOAL_PROFILES = tuple(sorted(_PROFILES))


def test_dsl_parses() -> None:
    """Validate the mapping DSL structurally — exact counts live elsewhere.

    Exact surface counts are a GENERATED artifact
    (generated/mappings/dsl-stats.json, emitted by the compiler and enforced
    by the drift gate): a vocabulary addition changes that file via
    `gmeow regenerate`, and the delta is reviewed in the PR diff. This test
    keeps only what should never need editing — structural invariants and
    catastrophe floors that catch the compiler silently dropping content.
    """
    dsl = load_dsl()

    # Catastrophe floors: orders-of-magnitude backstops, NOT running totals.
    assert len(dsl.equivalences) >= 1000
    assert len(dsl.functions) >= 30
    assert len(dsl.mapping_sets) >= 40
    assert len(dsl.projections) > 30

    # Every equivalence cell belongs to a declared mapping set's SSSOM file.
    set_files = set(dsl.mapping_sets)  # keyed by SSSOM file name
    cell_files = {cell.sssom_file for cell in dsl.equivalences}
    assert cell_files <= set_files, cell_files - set_files

    # At most a handful of sets are intentionally cell-less (e.g.
    # gmeow-colourspace documents refusals only); a wide gap means the
    # parser dropped cells wholesale.
    assert len(set_files - cell_files) <= 3, set_files - cell_files

    # The committed stats artifact agrees with the live parse — belt to the
    # drift gate's braces, and a clear local failure message when a branch
    # forgets to regenerate.
    stats = json.loads(
        (PROJECT_ROOT / "generated" / "mappings" / "dsl-stats.json").read_text(
            encoding="utf-8"
        )
    )
    live = {
        "equivalences": len(dsl.equivalences),
        "functions": len(dsl.functions),
        "mapping_sets": len(dsl.mapping_sets),
        "projections": len(dsl.projections),
    }
    committed = {k: stats[k] for k in live}
    assert live == committed, (
        f"DSL surface changed: {live} != committed {committed} — "
        "run `gmeow regenerate mappings` and commit dsl-stats.json"
    )

    profiles = {b.profile for cell in dsl.projections for b in cell.bindings}
    assert profiles == set(_PROFILES)


@pytest.mark.parametrize("profile", _DETERMINISTIC_EDOAL_PROFILES)
def test_edoal_serialization_is_deterministic(profile: str) -> None:
    """Issue #36: two compiles must emit byte-identical EDOAL."""
    dsl = load_dsl()
    first = emit_edoal(dsl, profile).serialize(format="turtle")
    second = emit_edoal(dsl, profile).serialize(format="turtle")
    assert first == second, f"{profile} EDOAL serialization is non-deterministic"


def test_fno_serialization_is_deterministic() -> None:
    """Issue #36: the FnO transform catalog must serialize byte-identically too."""
    dsl = load_dsl()
    onto = load_merged_graph(include_imports=False)
    first = emit_fno(dsl, onto).serialize(format="turtle")
    second = emit_fno(dsl, onto).serialize(format="turtle")
    assert first == second, "FnO serialization is non-deterministic"


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

    dsl = load_dsl()
    onto = load_merged_graph(include_imports=False)
    proj = tmp_path / "projections"
    proj.mkdir()
    fno_out = proj / "functions.fno.ttl"
    emit_fno(dsl, onto).serialize(destination=fno_out, format="turtle")
    shutil.copy2(MAPPING_DSL_DIR / "transforms.fno.ttl", proj / "transforms.fno.ttl")
    assert fno_type_mismatches(proj) == []


def test_sparql_executors_are_valid_queries() -> None:
    dsl = load_dsl()
    for profile in _PROFILES:
        query = emit_sparql(dsl, profile)
        # Large projection CONSTRUCT queries (schema.org, etc.) push pyparsing
        # past the default 1000-frame limit.
        old = sys.getrecursionlimit()
        sys.setrecursionlimit(3000)
        try:
            prepareQuery(query)  # raises on a malformed query
        finally:
            sys.setrecursionlimit(old)


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


def test_project_when_renders_filter_exists() -> None:
    """The projectWhen positive guard atom renders FILTER EXISTS in SPARQL."""
    dsl = load_dsl()
    query = emit_sparql(dsl, "schema-org")
    assert (
        "FILTER EXISTS { ?ent gmeow:eligibleForConsumer gmeow:consumerPublicSite . }"
        in query
    )


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
    from gmeow_rdf.compat.rdflib import BNode

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


# The fail-closed FnO derivation guards — an input predicate with no ontology
# rdfs:range, and two predicates minting the same param IRI — moved into the native
# emitter with the rest of emit_fno (#848): the Rust `build_catalog` raises a
# SliceError on either condition (no degraded fallback). Because the native emitter
# sources every input from PROJECT_ROOT (Python passes only the repo root), the
# guards can no longer be exercised from a synthetic in-Python Dsl/Graph; they are
# now pinned by the Rust inline tests in crates/slice/src/fno_emit.rs
# (`untyped_input_predicate_is_a_hard_error`, `param_iri_collision_is_a_hard_error`).
# The committed corpus passing `gmeow-dev regenerate mappings` confirms the
# headline guarantee holds end-to-end: fno:type is always derived, never missing.


def test_edoal_traversal_uses_compose_inverse() -> None:
    """Multi-hop traversals derive an EDOAL relation path (compose/inverse)."""
    from gmeow_rdf.compat.rdflib.collection import Collection

    graph = emit_edoal(load_dsl(), "schema-org")
    birth = sub_org = None
    for cell in graph.subjects(RDF.type, ALIGN.Cell):
        label = str(graph.value(cell, RDFS.label) or "")
        if label.startswith("Birth life-event date"):
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
    from gmeow_rdf.compat.rdflib import Literal

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
    from gmeow_rdf.compat.rdflib.collection import Collection

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
        len(set(graph.subjects(RDF.type, FNO.Implementation))) == 9
    )  # one per profile WITH transforms (owl-time is pure templateAtoms — none;
    # web-annotation added #27; mailmap added #234; resume added #23)
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
    assert "# mapping_tool: gmeow regenerate (mappings)" in wikidata
    assert "# mapping_date:" in wikidata
    assert "# curie_map:" in wikidata


def test_sssom_validates_with_sssom_toolkit() -> None:
    """Every generated SSSOM file passes sssom-py schema validation."""
    errors = _validate_sssom(emit_sssom(load_dsl()))
    assert not errors, "\n".join(errors)


def test_malformed_sssom_fails_validation() -> None:
    """An invalid SSSOM TSV (unknown prefix) is caught by the validation gate."""
    bad = (
        "# mapping_tool: gmeow regenerate (mappings)\n"
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
    """Generator run aborts when ``_validate_sssom`` reports errors."""
    from gmeow_tools import mapping_compile as mc
    from gmeow_tools.generator import run

    def _bad_validate(_: dict[str, str]) -> list[str]:
        return ["mappings/x.sssom.tsv: synthetic validation failure"]

    monkeypatch.setattr(mc, "_validate_sssom", _bad_validate)
    with pytest.raises(CompileError, match="SSSOM validation failed"):
        run("mappings")


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

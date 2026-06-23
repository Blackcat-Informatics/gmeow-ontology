"""Tests for the flattened export views (tabular + LLM).

Marked ``maintainer``: these exercise the secondary external-export surface
(CSV/CSVW, Markdown, JSONL, llms.txt) rather than the core ontology, so they run
in CI and ``make test`` but are excluded from the fast ``make check`` gate.
"""

from __future__ import annotations

import csv
import json
from pathlib import Path
from unittest.mock import patch

import pytest
from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, URIRef
from gmeow_rdf.compat.rdflib.namespace import SKOS
from gts.model import TermKind

from gmeow_tools.config import NAMESPACE, ONTOLOGY_IRI
from gmeow_tools.export import (
    collect_terms,
    curie,
    write_csvs,
    write_csvw,
    write_jsonl,
    write_llms_txt,
    write_markdown,
    write_nquads,
    write_obographs,
    write_shex,
    write_skos,
    write_statements_jsonl,
    write_trig,
)
from gmeow_tools.gts_views import FoldView, load_fold

pytestmark = pytest.mark.maintainer


def test_curie_compacts_known_namespaces() -> None:
    assert curie(NAMESPACE + "Person") == "gmeow:Person"
    assert curie("https://schema.org/Person") == "schema:Person"
    # Unknown namespace is returned unchanged.
    assert curie("https://example.org/x") == "https://example.org/x"


def test_collect_terms_covers_classes_properties_individuals() -> None:
    terms = collect_terms()
    by_cat: dict[str, set[str]] = {}
    for t in terms:
        by_cat.setdefault(t.category, set()).add(t.curie)
    # Classes from several slices.
    for cls in ("gmeow:Person", "gmeow:EmailMessage", "gmeow:CryptographicKey"):
        assert cls in by_cat["class"]
    # Properties.
    for prop in ("gmeow:from", "gmeow:signingKey", "gmeow:trustor"):
        assert prop in by_cat["property"]
    # Value-vocabulary individuals (the scheme-as-value pattern).
    assert "gmeow:keySchemePGP" in by_cat["individual"]


def test_term_attributes_are_populated() -> None:
    terms = {t.curie: t for t in collect_terms()}
    person = terms["gmeow:Person"]
    assert person.label == "Person"
    assert person.definition
    assert "gmeow:boxTBox" in person.box_roles
    assert "gmeow:Agent" in person.parents
    assert "equivalentClass=foaf:Person" in person.alignments

    signing_key = terms["gmeow:signingKey"]
    assert signing_key.prop_kind == "object"
    assert signing_key.domain == "gmeow:CryptographicSignature"
    assert signing_key.range == "gmeow:CryptographicKey"
    assert signing_key.functional is True

    has_name = terms["gmeow:hasName"]
    assert "gmeow:boxRBox" in has_name.box_roles
    assert has_name.use_when
    assert has_name.how_to_use
    assert "gmeow:consumerPublicSite" in has_name.use_for_consumer


def test_collect_terms_preserves_multi_valued_advisory_texts() -> None:
    graph = Graph()
    lang = URIRef(NAMESPACE + "langEnglish")
    term = URIRef(NAMESPACE + "MultiAdviceConcept")

    graph.add((lang, RDF.type, URIRef(NAMESPACE + "Language")))
    graph.add((lang, URIRef(NAMESPACE + "languageTag"), Literal("x-gmeow-english")))
    graph.add((lang, URIRef(NAMESPACE + "bcp47Tag"), Literal("en")))

    graph.add((term, RDF.type, OWL.Class))
    graph.add(
        (term, RDFS.label, Literal("Multi Advice Concept", lang="x-gmeow-english"))
    )
    graph.add(
        (
            term,
            SKOS.definition,
            Literal(
                "A concept for testing multi-valued advisory export.",
                lang="x-gmeow-english",
            ),
        )
    )
    graph.add(
        (
            term,
            URIRef(NAMESPACE + "useWhen"),
            Literal("Use when alpha applies.", lang="x-gmeow-english"),
        )
    )
    graph.add(
        (
            term,
            URIRef(NAMESPACE + "useWhen"),
            Literal("Use when beta applies.", lang="x-gmeow-english"),
        )
    )
    graph.add(
        (
            term,
            SKOS.example,
            Literal("Example alpha.", lang="x-gmeow-english"),
        )
    )
    graph.add(
        (
            term,
            SKOS.example,
            Literal("Example beta.", lang="x-gmeow-english"),
        )
    )

    terms = collect_terms(_view_of(graph))
    concept = {t.curie: t for t in terms}["gmeow:MultiAdviceConcept"]
    assert concept.use_when == ["Use when alpha applies.", "Use when beta applies."]
    assert concept.examples == ["Example alpha.", "Example beta."]


def _write_all_exports(dist_dir: Path) -> list[Path]:
    """Write all export views into *dist_dir* (test helper)."""
    terms = collect_terms()
    written: list[Path] = []
    written.extend(write_csvs(terms, dist_dir))
    written.append(write_csvw(dist_dir))
    written.append(write_jsonl(terms, dist_dir))
    written.append(write_markdown(terms, dist_dir))
    written.append(write_llms_txt(terms, dist_dir))
    return written


def test_export_all_writes_every_view(tmp_path: Path) -> None:
    written = _write_all_exports(tmp_path)
    names = {p.name for p in written}
    assert names == {
        "gmeow-classes.csv",
        "gmeow-properties.csv",
        "gmeow-individuals.csv",
        "gmeow-terms.csvw.json",
        "gmeow-terms.jsonl",
        "gmeow-terms.md",
        "llms.txt",
    }
    for path in written:
        assert path.exists() and path.stat().st_size > 0


def test_classes_csv_is_well_formed(tmp_path: Path) -> None:
    _write_all_exports(tmp_path)
    with (tmp_path / "gmeow-classes.csv").open(encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    by_curie = {r["curie"]: r for r in rows}
    assert "gmeow:Person" in by_curie
    assert by_curie["gmeow:Person"]["iri"] == NAMESPACE + "Person"
    assert by_curie["gmeow:Person"]["subClassOf"] == "gmeow:Agent"
    assert "gmeow:boxTBox" in by_curie["gmeow:Person"]["boxRoles"]


def test_properties_csv_records_functionality(tmp_path: Path) -> None:
    _write_all_exports(tmp_path)
    with (tmp_path / "gmeow-properties.csv").open(encoding="utf-8") as handle:
        rows = {r["curie"]: r for r in csv.DictReader(handle)}
    assert rows["gmeow:signingKey"]["functional"] == "true"
    assert rows["gmeow:from"]["functional"] == "false"
    assert "gmeow:boxRBox" in rows["gmeow:from"]["boxRoles"]
    assert "co-equal person-to-PersonName" in rows["gmeow:hasName"]["useWhen"]
    assert "gmeow:consumerPublicSite" in rows["gmeow:hasName"]["useForConsumer"]


def test_jsonl_catalog_parses(tmp_path: Path) -> None:
    _write_all_exports(tmp_path)
    lines = (tmp_path / "gmeow-terms.jsonl").read_text(encoding="utf-8").splitlines()
    records = [json.loads(line) for line in lines]
    assert records, "catalog should not be empty"
    for rec in records:
        assert {"category", "curie", "iri", "label", "definition"} <= rec.keys()
    categories = {r["category"] for r in records}
    assert categories == {"class", "property", "individual"}
    has_name = next(r for r in records if r["curie"] == "gmeow:hasName")
    assert "gmeow:boxRBox" in has_name["boxRoles"]
    assert has_name["useWhen"]
    assert "gmeow:consumerPublicSite" in has_name["useForConsumer"]


def test_csvw_descriptor_is_valid(tmp_path: Path) -> None:
    _write_all_exports(tmp_path)
    descriptor = json.loads((tmp_path / "gmeow-terms.csvw.json").read_text("utf-8"))
    assert descriptor["@context"] == "http://www.w3.org/ns/csvw"
    assert {t["url"] for t in descriptor["tables"]} == {
        "gmeow-classes.csv",
        "gmeow-properties.csv",
        "gmeow-individuals.csv",
    }


def test_markdown_reference_has_all_sections(tmp_path: Path) -> None:
    _write_all_exports(tmp_path)
    md = (tmp_path / "gmeow-terms.md").read_text(encoding="utf-8")
    # The header counts individuals, so the section must actually be emitted.
    for section in ("## Classes", "## Properties", "## Individuals"):
        assert section in md
    assert "gmeow:keySchemePGP" in md
    assert "*Box roles:* `gmeow:boxTBox`" in md
    assert "*Use when:* Use for a direct, co-equal person-to-PersonName" in md


def test_llms_txt_bundle(tmp_path: Path) -> None:
    _write_all_exports(tmp_path)
    text = (tmp_path / "llms.txt").read_text(encoding="utf-8")
    assert text.startswith("# GMEOW")
    assert "## Classes" in text and "## Properties" in text
    assert "gmeow:EmailMessage" in text
    assert "[box roles: gmeow:boxTBox]" in text


def test_llms_txt_has_no_blank_nodes(tmp_path: Path) -> None:
    _write_all_exports(tmp_path)
    fresh_content = (tmp_path / "llms.txt").read_text(encoding="utf-8")
    assert "_:" not in fresh_content, (
        "Found raw blank node ID in freshly generated llms.txt"
    )


# --------------------------------------------------------------------------- #
# Language-tag retagging tests (#164)
# --------------------------------------------------------------------------- #


def _view_of(graph: Graph) -> FoldView:
    """A FoldView over a handcrafted rdflib graph, via the real producer."""
    from gts import read

    from gmeow_tools.gts_producer import gts_from_graph

    return FoldView(read(gts_from_graph(graph)))


def test_export_retags_internal_to_bcp47() -> None:
    """A canonical @x-gmeow-english literal is retagged to @en for public export."""
    graph = Graph()
    lang = URIRef(NAMESPACE + "langEnglish")
    term = URIRef(NAMESPACE + "TestConcept")

    graph.add((lang, RDF.type, URIRef(NAMESPACE + "Language")))
    graph.add((lang, URIRef(NAMESPACE + "languageTag"), Literal("x-gmeow-english")))
    graph.add((lang, URIRef(NAMESPACE + "bcp47Tag"), Literal("en")))

    graph.add((term, RDF.type, OWL.Class))
    graph.add((term, RDFS.label, Literal("Test Concept", lang="x-gmeow-english")))
    graph.add(
        (
            term,
            SKOS.definition,
            Literal("A concept for testing.", lang="x-gmeow-english"),
        )
    )

    terms = collect_terms(_view_of(graph))
    by_curie = {t.curie: t for t in terms}
    assert by_curie["gmeow:TestConcept"].label == "Test Concept"
    assert by_curie["gmeow:TestConcept"].definition == "A concept for testing."


def test_export_deterministic_when_both_tags_present() -> None:
    """When both internal and external-tagged literals exist, selection is
    deterministic."""
    graph = Graph()
    lang = URIRef(NAMESPACE + "langEnglish")
    term = URIRef(NAMESPACE + "TestConcept")

    graph.add((lang, RDF.type, URIRef(NAMESPACE + "Language")))
    graph.add((lang, URIRef(NAMESPACE + "languageTag"), Literal("x-gmeow-english")))
    graph.add((lang, URIRef(NAMESPACE + "bcp47Tag"), Literal("en")))

    graph.add((term, RDF.type, OWL.Class))
    # Canonical internal tag (should win because it has a mapping).
    graph.add((term, RDFS.label, Literal("Internal Name", lang="x-gmeow-english")))
    # External tag co-existing (should be ignored in favour of the mapped internal one).
    graph.add((term, RDFS.label, Literal("External Name", lang="en")))

    terms = collect_terms(_view_of(graph))
    by_curie = {t.curie: t for t in terms}
    # public_text prefers the internal-tagged literal when a mapping exists.
    assert by_curie["gmeow:TestConcept"].label == "Internal Name"


def test_export_does_not_invent_en_when_bcp47_missing() -> None:
    """If a language has languageTag but no bcp47Tag, the raw internal tag is kept."""
    graph = Graph()
    lang = URIRef(NAMESPACE + "langConlang")
    term = URIRef(NAMESPACE + "TestConcept")

    graph.add((lang, RDF.type, URIRef(NAMESPACE + "Language")))
    graph.add((lang, URIRef(NAMESPACE + "languageTag"), Literal("x-gmeow-conlang")))
    # deliberately NO bcp47Tag

    graph.add((term, RDF.type, OWL.Class))
    graph.add((term, RDFS.label, Literal("Conlang Name", lang="x-gmeow-conlang")))

    terms = collect_terms(_view_of(graph))
    by_curie = {t.curie: t for t in terms}
    # No BCP-47 mapping → raw internal tag returned as-is.
    assert by_curie["gmeow:TestConcept"].label == "Conlang Name"


# --------------------------------------------------------------------------- #
# Dataset / semantic-web tiers (#377, #12)
# --------------------------------------------------------------------------- #


def _oxigraph_quads(path: Path) -> set[str]:
    """Parse with gmeow_rdf (the trusted RDF 1.2 path, #177) → quad strings."""
    import gmeow_rdf

    fmt = (
        gmeow_rdf.RdfFormat.N_QUADS
        if path.suffix == ".nq"
        else gmeow_rdf.RdfFormat.TRIG
    )
    return {str(q) for q in gmeow_rdf.parse(path.read_bytes(), format=fmt)}


def test_nquads_carries_the_full_statement_layer(tmp_path: Path) -> None:
    """gmeow.nq is the LOSSLESS dataset: base quads + reifiers + annotations."""
    view = load_fold()
    path = write_nquads(view, tmp_path)
    quads = _oxigraph_quads(path)
    expected = len(view.graph.quads) + len(view.reifiers()) + len(view.annotations())
    assert len(quads) == expected
    text = path.read_text(encoding="utf-8")
    assert "rdf-syntax-ns#reifies" in text  # the RDF 1.2 statement layer
    assert "<<(" in text  # quoted triple terms survive


def test_write_trig_delegates_to_gts_trig(tmp_path: Path) -> None:
    """write_trig bottoms out in the upstream gts.trig.to_trig primitive (#702)."""
    view = load_fold()
    expected = "<<delegate-test>>"
    with patch("gmeow_tools.export.to_trig", return_value=expected) as mock_to_trig:
        path = write_trig(view, tmp_path)
    mock_to_trig.assert_called_once()
    (graph_arg,), _ = mock_to_trig.call_args
    # Internal tags are remapped before delegation.
    assert all(
        not (
            term.kind is TermKind.LITERAL
            and term.lang
            and term.lang.startswith("x-gmeow-")
        )
        for term in graph_arg.terms
    )
    assert path.read_text(encoding="utf-8") == expected


def test_trig_carries_the_full_statement_layer(tmp_path: Path) -> None:
    """gmeow.trig is lossless: base quads + reifiers + annotations."""
    view = load_fold()
    path = write_trig(view, tmp_path)
    quads = _oxigraph_quads(path)
    expected = len(view.graph.quads) + len(view.reifiers()) + len(view.annotations())
    assert len(quads) == expected
    text = path.read_text(encoding="utf-8")
    assert "rdf:reifies" in text  # the RDF 1.2 statement layer
    assert "<<(" in text  # quoted triple terms survive


def test_dataset_forms_remap_internal_language_tags(tmp_path: Path) -> None:
    """No @x-gmeow-* tag reaches a published dataset form (#287)."""
    view = load_fold()
    for path in (write_nquads(view, tmp_path), write_trig(view, tmp_path)):
        assert "@x-gmeow-" not in path.read_text(encoding="utf-8"), path.name
    # The fold itself DOES carry internal tags — the remap must not be vacuous.
    assert any(t.lang and t.lang.startswith("x-gmeow-") for t in view.graph.terms)


def test_statements_jsonl_rows_match_the_reifiers(tmp_path: Path) -> None:
    """One flat record per reified statement, annotations attached."""
    view = load_fold()
    path = write_statements_jsonl(view, tmp_path)
    rows = [json.loads(line) for line in path.read_text("utf-8").splitlines()]
    assert len(rows) == len(view.reifiers())
    for row in rows:
        assert {"id", "subject", "predicate", "object", "annotations"} <= row.keys()
    assert any(row["annotations"] for row in rows), "no annotations surfaced"
    # The JSON form must not leak internal tags through "lang" fields either
    # (the gate's @-grep cannot see them — this pin can).
    for row in rows:
        for value in (row["subject"], row["object"], *row["annotations"].values()):
            if isinstance(value, dict) and "lang" in value:
                assert not str(value["lang"]).startswith("x-gmeow-")


def test_skos_extract_is_a_concept_scheme(tmp_path: Path) -> None:
    """Classes as skos:Concept on their ORIGINAL IRIs, broader within gmeow."""
    path = write_skos(load_fold(), tmp_path)
    g = Graph().parse(path, format="turtle")
    scheme = URIRef(ONTOLOGY_IRI)
    person = URIRef(NAMESPACE + "Person")
    assert (scheme, RDF.type, SKOS.ConceptScheme) in g
    assert (person, RDF.type, SKOS.Concept) in g
    assert (person, SKOS.broader, URIRef(NAMESPACE + "Agent")) in g
    assert (person, SKOS.inScheme, scheme) in g
    assert next(g.objects(scheme, SKOS.hasTopConcept), None) is not None
    # Properties are excluded (declared loss) — and labels carry PUBLIC tags.
    assert (URIRef(NAMESPACE + "from"), RDF.type, SKOS.Concept) not in g
    label = next(g.objects(person, SKOS.prefLabel))
    assert isinstance(label, Literal) and label.language == "en"


def test_obographs_nodes_and_is_a_edges(tmp_path: Path) -> None:
    """OBO Graphs basic profile: labeled class nodes, IRI-only is_a edges."""
    path = write_obographs(load_fold(), tmp_path)
    graph = json.loads(path.read_text("utf-8"))["graphs"][0]
    nodes = {n["id"]: n for n in graph["nodes"]}
    person = NAMESPACE + "Person"
    assert nodes[person]["lbl"] == "Person"
    assert nodes[person]["meta"]["definition"]["val"]
    assert {"sub": person, "pred": "is_a", "obj": NAMESPACE + "Agent"} in graph["edges"]
    assert not any(n["id"].startswith("_:") for n in graph["nodes"])
    referenced = {e["obj"] for e in graph["edges"]} | {e["sub"] for e in graph["edges"]}
    assert referenced <= set(nodes), "dangling edge endpoint"


def test_shex_shapes_for_domained_classes(tmp_path: Path) -> None:
    """One shape per domained class; functional → '?', class range → @ref."""
    text = write_shex(load_fold(), tmp_path).read_text(encoding="utf-8")
    assert "gmeow:CryptographicSignature {" in text
    # signingKey: functional, domain CryptographicSignature, range CryptographicKey
    assert "gmeow:signingKey @gmeow:CryptographicKey ?" in text

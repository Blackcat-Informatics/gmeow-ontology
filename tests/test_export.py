"""Tests for the flattened export views (tabular + LLM).

Marked ``ci_only``: these exercise the secondary external-export surface
(CSV/CSVW, Markdown, JSONL, llms.txt) rather than the core ontology, so they run
in CI and ``make test`` but are excluded from the fast ``make check`` gate.
"""

from __future__ import annotations

import csv
import json
from pathlib import Path

import pytest
from rdflib import OWL, RDF, RDFS, Graph, Literal, URIRef
from rdflib.namespace import SKOS

from gmeow_tools.config import NAMESPACE
from gmeow_tools.export import (
    collect_terms,
    curie,
    write_csvs,
    write_csvw,
    write_jsonl,
    write_llms_txt,
    write_markdown,
)

pytestmark = pytest.mark.ci_only


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
    assert "gmeow:Agent" in person.parents
    assert "equivalentClass=foaf:Person" in person.alignments

    signing_key = terms["gmeow:signingKey"]
    assert signing_key.prop_kind == "object"
    assert signing_key.domain == "gmeow:CryptographicSignature"
    assert signing_key.range == "gmeow:CryptographicKey"
    assert signing_key.functional is True


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


def test_properties_csv_records_functionality(tmp_path: Path) -> None:
    _write_all_exports(tmp_path)
    with (tmp_path / "gmeow-properties.csv").open(encoding="utf-8") as handle:
        rows = {r["curie"]: r for r in csv.DictReader(handle)}
    assert rows["gmeow:signingKey"]["functional"] == "true"
    assert rows["gmeow:from"]["functional"] == "false"


def test_jsonl_catalog_parses(tmp_path: Path) -> None:
    _write_all_exports(tmp_path)
    lines = (tmp_path / "gmeow-terms.jsonl").read_text(encoding="utf-8").splitlines()
    records = [json.loads(line) for line in lines]
    assert records, "catalog should not be empty"
    for rec in records:
        assert {"category", "curie", "iri", "label", "definition"} <= rec.keys()
    categories = {r["category"] for r in records}
    assert categories == {"class", "property", "individual"}


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


def test_llms_txt_bundle(tmp_path: Path) -> None:
    _write_all_exports(tmp_path)
    text = (tmp_path / "llms.txt").read_text(encoding="utf-8")
    assert text.startswith("# GMEOW")
    assert "## Classes" in text and "## Properties" in text
    assert "gmeow:EmailMessage" in text


def test_llms_txt_has_no_blank_nodes(tmp_path: Path) -> None:
    _write_all_exports(tmp_path)
    fresh_content = (tmp_path / "llms.txt").read_text(encoding="utf-8")
    assert "_:" not in fresh_content, (
        "Found raw blank node ID in freshly generated llms.txt"
    )


# --------------------------------------------------------------------------- #
# Language-tag retagging tests (#164)
# --------------------------------------------------------------------------- #


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

    terms = collect_terms(graph=graph)
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

    terms = collect_terms(graph=graph)
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

    terms = collect_terms(graph=graph)
    by_curie = {t.curie: t for t in terms}
    # No BCP-47 mapping → raw internal tag returned as-is.
    assert by_curie["gmeow:TestConcept"].label == "Conlang Name"

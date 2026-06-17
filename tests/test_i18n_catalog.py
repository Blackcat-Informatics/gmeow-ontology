"""Tests for the shared i18n catalog module."""

from __future__ import annotations

from pathlib import Path

import pytest
from rdflib import Graph, Literal, Namespace
from rdflib.namespace import RDFS, SKOS

from gmeow_tools.config import NAMESPACE, SLICES_DIR
from gmeow_tools.i18n_catalog import (
    LOCALIZABLE_PREDICATES,
    TranslationKey,
    build_pot,
    extract_terms,
    load_po_catalog,
    merge_terms,
    write_po,
    write_pot,
)
from gmeow_tools.i18n_sync import PoEntry, parse_po

GMEOW = Namespace(NAMESPACE)


SAMPLE_TURTLE = f"""\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix gmeow: <{NAMESPACE}> .

<{NAMESPACE}slices/lifecycle> a owl:Ontology ;
    rdfs:label "Lifecycle Module"@x-gmeow-english ;
    skos:definition "Definitions for lifecycle."@x-gmeow-english ;
    rdfs:comment "Universal lifecycle."@x-gmeow-english .

gmeow:Entity a owl:Class ;
    rdfs:label "Entity"@x-gmeow-english ;
    skos:definition "A thing that exists."@x-gmeow-english .

gmeow:hasName a owl:DatatypeProperty ;
    rdfs:label "has name"@x-gmeow-english .
"""


def _sample_graph() -> Graph:
    graph = Graph()
    graph.parse(data=SAMPLE_TURTLE, format="turtle")
    return graph


def test_localizable_predicates_contains_expected() -> None:
    assert RDFS.label in LOCALIZABLE_PREDICATES
    assert SKOS.definition in LOCALIZABLE_PREDICATES
    assert GMEOW.name in LOCALIZABLE_PREDICATES
    assert GMEOW.title in LOCALIZABLE_PREDICATES
    assert GMEOW.description in LOCALIZABLE_PREDICATES
    assert GMEOW.fullName in LOCALIZABLE_PREDICATES


def test_extract_terms_finds_expected_keys() -> None:
    graph = _sample_graph()
    keys = list(extract_terms(graph))

    lifecycle_slice = f"{NAMESPACE}slices/lifecycle"
    gmeow_fallback = NAMESPACE

    expected = [
        # Fallback namespace sorts before ``/slices/`` because it is a prefix.
        TranslationKey(
            slice_iri=gmeow_fallback,
            term_iri=f"{NAMESPACE}Entity",
            predicate=str(RDFS.label),
            english_value="Entity",
        ),
        TranslationKey(
            slice_iri=gmeow_fallback,
            term_iri=f"{NAMESPACE}Entity",
            predicate=str(SKOS.definition),
            english_value="A thing that exists.",
        ),
        TranslationKey(
            slice_iri=gmeow_fallback,
            term_iri=f"{NAMESPACE}hasName",
            predicate=str(RDFS.label),
            english_value="has name",
        ),
        TranslationKey(
            slice_iri=lifecycle_slice,
            term_iri=f"{NAMESPACE}slices/lifecycle",
            predicate=str(RDFS.comment),
            english_value="Universal lifecycle.",
        ),
        TranslationKey(
            slice_iri=lifecycle_slice,
            term_iri=f"{NAMESPACE}slices/lifecycle",
            predicate=str(RDFS.label),
            english_value="Lifecycle Module",
        ),
        TranslationKey(
            slice_iri=lifecycle_slice,
            term_iri=f"{NAMESPACE}slices/lifecycle",
            predicate=str(SKOS.definition),
            english_value="Definitions for lifecycle.",
        ),
    ]
    assert keys == expected


def test_extract_terms_uses_untagged_literal_as_fallback() -> None:
    ttl = f"""\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix gmeow: <{NAMESPACE}> .
gmeow:Untagged a owl:Class ;
    rdfs:label "Untagged label" .
"""
    graph = Graph().parse(data=ttl, format="turtle")
    keys = list(extract_terms(graph))
    assert len(keys) == 1
    assert keys[0].english_value == "Untagged label"


def test_extract_terms_prefers_english_over_untagged() -> None:
    ttl = f"""\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix gmeow: <{NAMESPACE}> .
gmeow:Both a owl:Class ;
    rdfs:label "English"@x-gmeow-english ;
    rdfs:label "Plain" .
"""
    graph = Graph().parse(data=ttl, format="turtle")
    keys = list(extract_terms(graph))
    assert len(keys) == 1
    assert keys[0].english_value == "English"


def test_extract_terms_raises_on_multiple_distinct_english_values() -> None:
    ttl = f"""\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix gmeow: <{NAMESPACE}> .
gmeow:Conflict a owl:Class ;
    rdfs:label "One"@x-gmeow-english ;
    rdfs:label "Two"@x-gmeow-english .
"""
    graph = Graph().parse(data=ttl, format="turtle")
    with pytest.raises(ValueError, match="multiple distinct @x-gmeow-english values"):
        list(extract_terms(graph))


def test_build_pot_is_parseable() -> None:
    graph = _sample_graph()
    entries = list(extract_terms(graph))
    pot = build_pot(entries)

    assert "Project-Id-Version: gmeow" in pot
    assert "Content-Type: text/plain; charset=UTF-8" in pot
    assert "Content-Transfer-Encoding: 8bit" in pot

    parsed = parse_po(pot, require_msgctxt=False)
    # Drop the header entry (empty msgid/msgctxt).
    body = [e for e in parsed if e.msgctxt]
    assert len(body) == len(entries)
    by_ctxt = {e.msgctxt: e for e in body}
    for entry in entries:
        ctxt = f"{entry.term_iri}|{entry.predicate}"
        assert ctxt in by_ctxt
        assert by_ctxt[ctxt].msgid == entry.english_value
        assert by_ctxt[ctxt].msgstr == ""


def test_build_pot_escapes_special_characters() -> None:
    entries = [
        TranslationKey(
            slice_iri=f"{NAMESPACE}slices/test",
            term_iri=f"{NAMESPACE}Test",
            predicate=str(RDFS.label),
            english_value='Line one\nLine two "quoted" and \\backslash\\',
        )
    ]
    pot = build_pot(entries)
    assert 'Line two \\"quoted\\"' in pot
    assert "Line one\\nLine two" in pot
    assert "\\\\backslash\\\\" in pot


def test_load_po_catalog_for_lifecycle_french() -> None:
    po_path = SLICES_DIR / "core" / "lifecycle" / "i18n" / "fr.po"
    catalog = load_po_catalog(po_path)

    assert catalog
    # All values carry the internal French tag.
    assert all(tag == "x-gmeow-french" for _, _, tag in catalog)
    # Spot-check a known translation.
    assert (
        catalog[(f"{NAMESPACE}EntityExistence", str(RDFS.label), "x-gmeow-french")]
        == "Existence d'entité"
    )


def test_merge_terms_adds_french_translations() -> None:
    base = _sample_graph()
    po_path = SLICES_DIR / "core" / "lifecycle" / "i18n" / "fr.po"
    merged = merge_terms(base, [po_path])

    # Original English values are retained.
    assert Literal("Entity", lang="x-gmeow-english") in merged.objects(
        GMEOW.Entity, RDFS.label
    )

    # French translations were added for terms present in both the graph and PO.
    french_label = merged.value(GMEOW.EntityExistence, RDFS.label, any=False)
    assert isinstance(french_label, Literal)
    assert french_label.language == "x-gmeow-french"
    assert str(french_label) == "Existence d'entité"


def test_merge_terms_does_not_mutate_base_graph(tmp_path: Path) -> None:
    base = _sample_graph()
    original_triples = set(base)

    entries = [
        PoEntry(
            msgctxt=f"{NAMESPACE}Entity|{RDFS.label}",
            msgid="Entity",
            msgstr="Entité",
        )
    ]
    po_path = tmp_path / "fr.po"
    write_po(po_path, entries, lang="fr")

    merged = merge_terms(base, [po_path])
    assert set(base) == original_triples
    assert len(merged) > len(base)


def test_write_pot_round_trip(tmp_path: Path) -> None:
    entries = [
        PoEntry(
            msgctxt=f"{NAMESPACE}Entity|{RDFS.label}",
            msgid="Entity",
            msgstr="",
        ),
        PoEntry(
            msgctxt=f"{NAMESPACE}Entity|{SKOS.definition}",
            msgid="A thing.",
            msgstr="",
        ),
    ]
    pot_path = tmp_path / "messages.pot"
    write_pot(pot_path, entries)

    text = pot_path.read_text(encoding="utf-8")
    assert "Project-Id-Version: gmeow" in text
    assert "Language:" not in text
    parsed = parse_po(text, require_msgctxt=False)
    body = [e for e in parsed if e.msgctxt]
    assert {e.msgctxt for e in body} == {e.msgctxt for e in entries}


def test_write_po_round_trip(tmp_path: Path) -> None:
    entries = [
        PoEntry(
            msgctxt=f"{NAMESPACE}Entity|{RDFS.label}",
            msgid="Entity",
            msgstr="Entité",
        ),
    ]
    po_path = tmp_path / "fr.po"
    write_po(po_path, entries, lang="fr")

    text = po_path.read_text(encoding="utf-8")
    assert "Language: fr" in text
    catalog = load_po_catalog(po_path)
    assert (
        catalog[(f"{NAMESPACE}Entity", str(RDFS.label), "x-gmeow-french")] == "Entité"
    )

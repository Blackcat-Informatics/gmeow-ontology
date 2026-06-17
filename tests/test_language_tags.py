# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the language-tag selection primitives (#571)."""

from __future__ import annotations

import pytest
from rdflib import Graph, Literal, Namespace
from rdflib.namespace import RDF, RDFS, SKOS

from gmeow_tools.export import marked
from gmeow_tools.language_tags import (
    UnknownLanguageError,
    filter_graph,
    filter_literals,
    resolve_lang_input,
    select_literal,
)

NS = Namespace("https://example.org/ns#")


def _tag_map() -> dict[str, str]:
    return {
        "x-gmeow-english": "en",
        "x-gmeow-french": "fr",
        "x-gmeow-chinese": "zh",
    }


def test_resolve_lang_input_defaults_to_english() -> None:
    selector = resolve_lang_input(None, _tag_map())
    assert selector.requested == ("en",)


def test_resolve_lang_input_accepts_public_bcp47() -> None:
    selector = resolve_lang_input("fr", _tag_map())
    assert selector.requested == ("fr",)


def test_resolve_lang_input_accepts_internal_tag() -> None:
    selector = resolve_lang_input("x-gmeow-french", _tag_map())
    assert selector.requested == ("fr",)


def test_resolve_lang_input_preserves_order_and_dedupes() -> None:
    selector = resolve_lang_input("fr,en,fr,zh", _tag_map())
    assert selector.requested == ("fr", "en", "zh")


def test_resolve_lang_input_rejects_unknown_tag() -> None:
    with pytest.raises(UnknownLanguageError) as exc_info:
        resolve_lang_input("klingon", _tag_map())
    assert "klingon" in str(exc_info.value)
    assert "en" in exc_info.value.available


def test_resolve_lang_input_rejects_unknown_internal_tag() -> None:
    with pytest.raises(UnknownLanguageError):
        resolve_lang_input("x-gmeow-klingon", _tag_map())


def test_select_literal_prefers_requested_language() -> None:
    literals = [
        Literal("Hello", lang="x-gmeow-english"),
        Literal("Bonjour", lang="x-gmeow-french"),
    ]
    selector = resolve_lang_input("fr", _tag_map())
    lit, fallback = select_literal(literals, selector, tag_map=_tag_map())
    assert lit is not None
    assert str(lit) == "Bonjour"
    assert lit.language == "fr"
    assert fallback is False


def test_select_literal_falls_back_to_english() -> None:
    literals = [Literal("Hello", lang="x-gmeow-english")]
    selector = resolve_lang_input("fr", _tag_map())
    lit, fallback = select_literal(literals, selector, tag_map=_tag_map())
    assert lit is not None
    assert str(lit) == "Hello"
    assert fallback is True


def test_select_literal_prefers_internal_tag_over_external_same_language() -> None:
    """Canonical internal-tagged literals win over external-tagged co-existing ones."""
    literals = [
        Literal("External", lang="en"),
        Literal("Internal", lang="x-gmeow-english"),
    ]
    selector = resolve_lang_input("en", _tag_map())
    lit, fallback = select_literal(literals, selector, tag_map=_tag_map())
    assert lit is not None
    assert str(lit) == "Internal"
    assert fallback is False


def test_filter_literals_returns_all_requested_values() -> None:
    literals = [
        Literal("Alpha", lang="x-gmeow-english"),
        Literal("Beta", lang="x-gmeow-english"),
        Literal("Gamma", lang="x-gmeow-french"),
    ]
    selector = resolve_lang_input("en", _tag_map())
    results = filter_literals(literals, selector, tag_map=_tag_map())
    texts = [str(lit) for lit, _fallback in results]
    assert texts == ["Alpha", "Beta"]
    assert all(not fallback for _, fallback in results)


def test_filter_literals_falls_back_when_requested_missing() -> None:
    literals = [Literal("Hello", lang="x-gmeow-english")]
    selector = resolve_lang_input("zh", _tag_map())
    results = filter_literals(literals, selector, tag_map=_tag_map())
    assert len(results) == 1
    assert results[0][1] is True


def test_filter_graph_keeps_only_selected_language() -> None:
    graph = Graph()
    term = NS["Thing"]
    graph.add((term, RDF.type, NS.Class))
    graph.add((term, RDFS.label, Literal("Hello", lang="en")))
    graph.add((term, RDFS.label, Literal("Bonjour", lang="fr")))
    graph.add((term, SKOS.definition, Literal("An English definition", lang="en")))

    selector = resolve_lang_input("fr", {"en": "en", "fr": "fr"})
    filter_graph(graph, selector)

    labels = {str(lit) for lit in graph.objects(term, RDFS.label)}
    assert labels == {"Bonjour"}
    # Non-selected language triples are removed.
    assert "Hello" not in labels
    # Definition in English is removed because fr was requested and present for label,
    # but definition has no fr value -> fallback to English? Wait filter_graph removes
    # all current language objects and adds selected. For definition predicate, there is
    # no fr candidate, so it falls back to English and re-adds it.
    definitions = set(graph.objects(term, SKOS.definition))
    assert len(definitions) == 1
    assert str(definitions.pop()) == "An English definition"


def test_marked_is_public_export() -> None:
    """The fallback marker helper is exported publicly as ``marked``."""
    assert marked("hello", False) == "hello"
    assert marked("hello", True) == "hello [fallback: en]"
    assert marked("bonjour", True, fallback_lang="fr") == "bonjour [fallback: fr]"

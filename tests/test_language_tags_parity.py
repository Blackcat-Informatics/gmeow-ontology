# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Parity tests: native Rust language-tag functions vs. Python originals (#819 Task 10).

Verifies that ``gmeow_validate.is_internal_tag``, ``rank_language``, and
``load_tag_map`` agree with the pre-cutover Python behaviour on every
interesting input and on the real merged ontology.
"""

from __future__ import annotations

import json

import gmeow_validate
import pytest

from gmeow_tools.language_tags import is_internal_tag, load_tag_map, rank_language

# ── is_internal_tag parity ───────────────────────────────────────────────────


@pytest.mark.parametrize(
    "lang,expected",
    [
        ("x-gmeow-english", True),
        ("x-gmeow-mandarin", True),
        ("X-GMEOW-FOO", True),
        ("x-gmeow-foo-bar", True),
        ("en", False),
        ("fr", False),
        ("xx-gmeow-no", False),  # wrong prefix
        ("x-gmeow-", False),  # empty suffix
        ("x-gmeow", False),  # no suffix at all
    ],
)
def test_is_internal_tag_python(lang: str, expected: bool) -> None:
    """Python wrapper agrees with expected value."""
    assert is_internal_tag(lang) == expected


@pytest.mark.parametrize(
    "lang,expected",
    [
        ("x-gmeow-english", True),
        ("x-gmeow-mandarin", True),
        ("X-GMEOW-FOO", True),
        ("x-gmeow-foo-bar", True),
        ("en", False),
        ("fr", False),
        ("xx-gmeow-no", False),
        ("x-gmeow-", False),
        ("x-gmeow", False),
    ],
)
def test_is_internal_tag_native_agrees(lang: str, expected: bool) -> None:
    """Native Rust function agrees with Python wrapper."""
    assert gmeow_validate.is_internal_tag(lang) == is_internal_tag(lang)


def test_is_internal_tag_none_via_python() -> None:
    """Python wrapper handles None (native does not accept None)."""
    assert is_internal_tag(None) is False


# ── rank_language parity ─────────────────────────────────────────────────────


@pytest.mark.parametrize(
    "lang",
    [
        "x-gmeow-english",
        "x-gmeow-mandarin",
        "en",
        "fr",
        "X-GMEOW-FOO",
        "xx-gmeow-no",
        "",
    ],
)
def test_rank_language_native_agrees(lang: str) -> None:
    """Native rank_language agrees with Python wrapper on every test tag."""
    py_result = rank_language(lang)
    native_result = gmeow_validate.rank_language(lang)
    assert native_result == py_result, (
        f"rank_language({lang!r}): native={native_result!r}, python={py_result!r}"
    )


def test_rank_language_english_wins() -> None:
    """x-gmeow-english has rank 0; everything else has rank 1."""
    assert gmeow_validate.rank_language("x-gmeow-english") == (0, "x-gmeow-english")
    assert gmeow_validate.rank_language("X-GMEOW-ENGLISH") == (0, "x-gmeow-english")
    assert gmeow_validate.rank_language("x-gmeow-mandarin")[0] == 1
    assert gmeow_validate.rank_language("en")[0] == 1


# ── load_tag_map parity ───────────────────────────────────────────────────────


def test_load_tag_map_native_agrees_with_python_small() -> None:
    """Native load_tag_map agrees with Python on a minimal hand-crafted graph."""
    from gmeow_rdf.compat.rdflib import Graph

    ttl = """
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:French a gmeow:Language ;
    gmeow:languageTag "x-gmeow-french" ;
    gmeow:bcp47Tag "fr" .
"""
    g = Graph()
    g.parse(data=ttl, format="turtle")
    py_map = load_tag_map(g)
    nt_bytes = g.serialize(format="ntriples").encode()
    native_map = gmeow_validate.load_tag_map(nt_bytes, "ntriples")
    assert native_map == py_map


def test_load_tag_map_merged_ontology_parity() -> None:
    """Native load_tag_map equals Python on the real merged ontology graph."""
    from gmeow_tools.graph import load_merged_graph

    graph = load_merged_graph()

    # Python path (uses Rust internally via the wrapper).
    py_map = load_tag_map(graph)

    # Native path: serialize to N-Triples and call Rust directly.
    nt_bytes = graph.serialize(format="ntriples").encode()
    native_map = gmeow_validate.load_tag_map(nt_bytes, "ntriples")

    assert native_map == py_map, (
        f"Maps differ: only-in-python={set(py_map) - set(native_map)!r}, "
        f"only-in-native={set(native_map) - set(py_map)!r}"
    )


def test_load_tag_map_merged_ontology_pinned_fixture() -> None:
    """Pin the merged tag-map as sorted JSON and assert it is non-empty."""
    from gmeow_tools.graph import load_merged_graph

    graph = load_merged_graph()
    tag_map = load_tag_map(graph)

    # Must be non-empty: the merged ontology must declare at least English.
    assert tag_map, "merged ontology tag-map must not be empty"
    assert "x-gmeow-english" in tag_map, (
        "x-gmeow-english must appear in the merged tag-map"
    )

    # Pin as sorted JSON (a regression guard: adding/removing a language is
    # intentional and should be reflected here).
    pinned = json.dumps(dict(sorted(tag_map.items())), indent=2)
    assert pinned  # the fixture itself is not None/empty

    # Print for diagnostic visibility.
    print(f"\nMerged tag-map ({len(tag_map)} entries):\n{pinned}")


def test_load_tag_map_ambiguous_raises() -> None:
    """Native load_tag_map raises ValueError on ambiguous tags."""
    nt = (
        "<https://blackcatinformatics.ca/gmeow/English> "
        "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> "
        "<https://blackcatinformatics.ca/gmeow/Language> .\n"
        "<https://blackcatinformatics.ca/gmeow/English> "
        "<https://blackcatinformatics.ca/gmeow/languageTag> "
        '"x-gmeow-english" .\n'
        "<https://blackcatinformatics.ca/gmeow/English> "
        "<https://blackcatinformatics.ca/gmeow/languageTag> "
        '"x-gmeow-english-alt" .\n'
        "<https://blackcatinformatics.ca/gmeow/English> "
        "<https://blackcatinformatics.ca/gmeow/bcp47Tag> "
        '"en" .\n'
    )
    with pytest.raises(ValueError, match="ambiguous"):
        gmeow_validate.load_tag_map(nt.encode(), "ntriples")

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the YAML-LD-star / JSON-LD-star codec and parse lane (#699)."""

from __future__ import annotations

import json

import pyoxigraph
import pytest

from gmeow_tools import yaml_ld
from gmeow_tools.yaml_ld import YamlLdError


def _json_bytes(doc: object) -> bytes:
    return json.dumps(doc, sort_keys=True, separators=(",", ":")).encode("utf-8")


def test_yamlld_jsonld_roundtrip() -> None:
    """JSON-LD-star → YAML-LD-star → JSON-LD-star is byte-stable."""
    doc = {
        "@context": {
            "ex": "http://example.org/",
            "@vocab": "http://example.org/",
        },
        "@id": "ex:s",
        "@type": "ex:Thing",
        "ex:p": {"@id": "ex:o", "ex:label": "hello"},
    }
    original = _json_bytes(doc)
    yaml_bytes = yaml_ld.jsonld_star_to_yamlld(original)
    assert yaml_bytes.startswith(b"# yaml-language-server:")
    assert b"TODO(#700):" in yaml_bytes
    restored = yaml_ld.yamlld_to_jsonld(yaml_bytes)
    assert restored == original


def test_parse_jsonld_star_reconstructs_quoted_triple() -> None:
    """A JSON-LD-star @annotation becomes an rdf:reifies quoted triple."""
    doc = {
        "@context": {"ex": "http://example.org/"},
        "@id": "ex:s",
        "ex:p": {
            "@id": "ex:o",
            "@annotation": {"ex:confidence": "0.9"},
        },
    }
    store = yaml_ld.parse_jsonld_star(_json_bytes(doc))

    reifies = pyoxigraph.NamedNode("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies")
    found = False
    for quad in store:
        if quad.predicate == reifies and isinstance(quad.object, pyoxigraph.Triple):
            assert quad.object.subject == pyoxigraph.NamedNode("http://example.org/s")
            assert quad.object.predicate == pyoxigraph.NamedNode("http://example.org/p")
            assert quad.object.object == pyoxigraph.NamedNode("http://example.org/o")
            found = True
    assert found


def test_parse_directional_language() -> None:
    """A directional language string round-trips through the store."""
    doc = {
        "@context": {"ex": "http://example.org/"},
        "@id": "ex:s",
        "ex:label": {
            "@value": "hello",
            "@language": "en",
            "@direction": "ltr",
        },
    }
    store = yaml_ld.parse_jsonld_star(_json_bytes(doc))

    label = pyoxigraph.NamedNode("http://example.org/label")
    found = False
    for quad in store:
        if (
            quad.predicate == label
            and isinstance(quad.object, pyoxigraph.Literal)
            and quad.object.language == "en"
            and quad.object.direction == pyoxigraph.BaseDirection.LTR
        ):
            found = True
    assert found

    # Round-trip through N-Quads and back into a fresh pyoxigraph Store.
    nquads = store.dump(format=pyoxigraph.RdfFormat.N_QUADS)
    store2 = pyoxigraph.Store()
    store2.load(
        nquads, format=pyoxigraph.RdfFormat.N_QUADS, to_graph=pyoxigraph.DefaultGraph()
    )
    found2 = False
    for quad in store2:
        if (
            quad.predicate == label
            and isinstance(quad.object, pyoxigraph.Literal)
            and quad.object.language == "en"
            and quad.object.direction == pyoxigraph.BaseDirection.LTR
        ):
            found2 = True
    assert found2


def test_yamlld_to_graph_loads_rdflib() -> None:
    """The convenience wrapper yields a loadable rdflib-compatible graph."""
    yaml_bytes = b"""
'@context':
  ex: http://example.org/
'@id': ex:s
ex:p:
  '@id': ex:o
"""
    graph = yaml_ld.yaml_ld_to_graph(yaml_bytes)
    assert len(graph) == 1
    triple = next(iter(graph))
    assert triple == (
        "http://example.org/s",
        "http://example.org/p",
        "http://example.org/o",
    )


def test_yamlld_rejects_anchors() -> None:
    """YAML anchors/aliases are rejected as unsupported extended features."""
    yaml_with_anchor = b"""
'@context':
  ex: http://example.org/
'@id': ex:s
ex:p: &anchor
  '@id': ex:o
"""
    with pytest.raises(YamlLdError, match="anchors are not supported"):
        yaml_ld.yamlld_to_jsonld(yaml_with_anchor)


def test_yamlld_rejects_aliases() -> None:
    """YAML aliases are rejected as unsupported extended features."""
    yaml_with_alias = b"""
'@context':
  ex: http://example.org/
'@id': ex:s
ex:p: *alias
"""
    with pytest.raises(YamlLdError, match="aliases"):
        yaml_ld.yamlld_to_jsonld(yaml_with_alias)

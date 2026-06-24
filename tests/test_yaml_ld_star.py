# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Python-surface tests for the YAML-LD-star / JSON-LD-star lane (#699).

The comprehensive serialization, isomorphism, and determinism tests live in
Rust (`crates/pipeline/src/stages/yaml_ld.rs`). This file only exercises the
Python wrappers, PyYAML-specific rejection, and the language-server header.
"""

from __future__ import annotations

import json

import pyoxigraph
import pytest
from gmeow_rdf.compat.rdflib import RDF, Literal, URIRef

from gmeow_tools import yaml_ld
from gmeow_tools.yaml_ld import YamlLdError


def test_jsonld_star_to_yamlld_includes_language_server_header() -> None:
    """The YAML-LD-star emitter carries the yaml-language-server schema header."""
    doc = {
        "@context": {"ex": "http://example.org/"},
        "@id": "ex:s",
        "ex:p": {"@id": "ex:o"},
    }
    yaml_bytes = yaml_ld.jsonld_star_to_yamlld(
        json.dumps(doc, sort_keys=True, separators=(",", ":")).encode("utf-8")
    )
    assert yaml_bytes.startswith(b"# yaml-language-server:")
    assert b"TODO(#700):" in yaml_bytes


def test_parse_yaml_ld_loads_store() -> None:
    """The parse_yaml_ld wrapper returns a populated pyoxigraph.Store."""
    yaml_bytes = b"""
'@context':
  ex: http://example.org/
'@id': ex:s
ex:p:
  '@id': ex:o
"""
    store = yaml_ld.parse_yaml_ld(yaml_bytes)
    assert any(
        quad.subject == pyoxigraph.NamedNode("http://example.org/s")
        and quad.predicate == pyoxigraph.NamedNode("http://example.org/p")
        and quad.object == pyoxigraph.NamedNode("http://example.org/o")
        for quad in store
    )


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


def test_yamlld_annotated_to_graph_downcasts_to_statement_metadata() -> None:
    """An annotated YAML-LD-star value object downcasts to GMEOW statement
    metadata in the rdflib-compat graph, not to a quoted triple (#699)."""
    yaml_bytes = b"""
'@context':
  ex: http://example.org/
  gmeow: https://blackcatinformatics.ca/gmeow/
'@id': ex:s
ex:p:
  '@value': hello
  '@annotation':
    '@id': ex:r
    gmeow:confidence:
      '@value': '0.9'
"""
    graph = yaml_ld.yaml_ld_to_graph(yaml_bytes)

    # Base triple is preserved.
    assert (
        URIRef("http://example.org/s"),
        URIRef("http://example.org/p"),
        Literal("hello"),
    ) in graph

    # No quoted triple terms reach the rdflib graph.
    for _s, _p, o in graph:
        assert not str(o).startswith("<<"), (
            "rdflib graph must not contain quoted triples"
        )

    # Statement-metadata skeleton is present.
    r = URIRef("http://example.org/r")

    assert (
        r,
        RDF.type,
        URIRef("https://blackcatinformatics.ca/gmeow/StatementMetadata"),
    ) in graph
    assert (
        r,
        URIRef("https://blackcatinformatics.ca/gmeow/qSubject"),
        URIRef("http://example.org/s"),
    ) in graph
    assert (
        r,
        URIRef("https://blackcatinformatics.ca/gmeow/qPredicate"),
        URIRef("http://example.org/p"),
    ) in graph
    assert (
        r,
        URIRef("https://blackcatinformatics.ca/gmeow/qObjectLiteral"),
        Literal("hello"),
    ) in graph
    assert (
        r,
        URIRef("https://blackcatinformatics.ca/gmeow/confidence"),
        Literal("0.9"),
    ) in graph

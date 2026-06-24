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

from gmeow_tools import yaml_ld
from gmeow_tools.yaml_ld import YamlLdError


def _json_bytes(doc: object) -> bytes:
    return json.dumps(doc, sort_keys=True, separators=(",", ":")).encode("utf-8")


def test_jsonld_star_to_yamlld_includes_language_server_header() -> None:
    """The YAML-LD-star emitter carries the yaml-language-server schema header."""
    doc = {
        "@context": {"ex": "http://example.org/"},
        "@id": "ex:s",
        "ex:p": {"@id": "ex:o"},
    }
    yaml_bytes = yaml_ld.jsonld_star_to_yamlld(_json_bytes(doc))
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


def _has_quad(
    store: pyoxigraph.Store,
    subject: pyoxigraph.NamedNode | pyoxigraph.BlankNode | None,
    predicate: pyoxigraph.NamedNode | None,
    object: (
        pyoxigraph.NamedNode
        | pyoxigraph.BlankNode
        | pyoxigraph.Literal
        | pyoxigraph.Triple
        | None
    ),
) -> bool:
    for quad in store:
        if subject is not None and quad.subject != subject:
            continue
        if predicate is not None and quad.predicate != predicate:
            continue
        if object is not None and quad.object != object:
            continue
        return True
    return False


def test_parse_annotation_reifier_uses_explicit_id() -> None:
    """Annotation @id becomes the reifier subject, not the object node."""
    doc = {
        "@context": {"ex": "http://example.org/"},
        "@id": "ex:s",
        "ex:p": {
            "@id": "ex:o",
            "@annotation": {
                "@id": "ex:reifier",
                "ex:confidence": 0.9,
            },
        },
    }
    store = yaml_ld.parse_jsonld_star(_json_bytes(doc))

    s = pyoxigraph.NamedNode("http://example.org/s")
    p = pyoxigraph.NamedNode("http://example.org/p")
    o = pyoxigraph.NamedNode("http://example.org/o")
    reifier = pyoxigraph.NamedNode("http://example.org/reifier")
    reifies = pyoxigraph.NamedNode("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies")
    confidence = pyoxigraph.NamedNode("http://example.org/confidence")

    assert _has_quad(store, s, p, o)
    assert _has_quad(
        store,
        reifier,
        reifies,
        pyoxigraph.Triple(s, p, o),
    )
    assert _has_quad(store, reifier, confidence, pyoxigraph.Literal(0.9))


def test_parse_annotation_value_reifier_uses_explicit_id() -> None:
    """Annotation @id on a value object becomes the reifier subject."""
    doc = {
        "@context": {"ex": "http://example.org/"},
        "@id": "ex:s",
        "ex:p": {
            "@value": "hello",
            "@annotation": {
                "@id": "ex:reifier",
                "ex:confidence": 0.9,
            },
        },
    }
    store = yaml_ld.parse_jsonld_star(_json_bytes(doc))

    s = pyoxigraph.NamedNode("http://example.org/s")
    p = pyoxigraph.NamedNode("http://example.org/p")
    lit = pyoxigraph.Literal("hello")
    reifier = pyoxigraph.NamedNode("http://example.org/reifier")
    reifies = pyoxigraph.NamedNode("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies")
    confidence = pyoxigraph.NamedNode("http://example.org/confidence")

    assert _has_quad(store, s, p, lit)
    assert _has_quad(
        store,
        reifier,
        reifies,
        pyoxigraph.Triple(s, p, lit),
    )
    assert _has_quad(store, reifier, confidence, pyoxigraph.Literal(0.9))


def test_parse_annotation_without_id_uses_blank_reifier() -> None:
    """An annotation object with no @id reifies via a fresh blank node."""
    doc = {
        "@context": {"ex": "http://example.org/"},
        "@id": "ex:s",
        "ex:p": {
            "@id": "ex:o",
            "@annotation": {
                "ex:confidence": 0.9,
            },
        },
    }
    store = yaml_ld.parse_jsonld_star(_json_bytes(doc))

    s = pyoxigraph.NamedNode("http://example.org/s")
    p = pyoxigraph.NamedNode("http://example.org/p")
    o = pyoxigraph.NamedNode("http://example.org/o")
    reifies = pyoxigraph.NamedNode("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies")
    confidence = pyoxigraph.NamedNode("http://example.org/confidence")

    assert _has_quad(store, s, p, o)
    reifier_quads = [
        q
        for q in store
        if q.predicate == reifies and q.object == pyoxigraph.Triple(s, p, o)
    ]
    assert len(reifier_quads) == 1
    reifier = reifier_quads[0].subject
    assert isinstance(reifier, pyoxigraph.BlankNode)
    assert _has_quad(store, reifier, confidence, pyoxigraph.Literal(0.9))


def test_parse_annotation_does_not_emit_at_id_triple() -> None:
    """The @id key inside @annotation must not leak as a gmeow:@id property."""
    doc = {
        "@context": {"ex": "http://example.org/"},
        "@id": "ex:s",
        "ex:p": {
            "@id": "ex:o",
            "@annotation": {
                "@id": "ex:reifier",
                "ex:confidence": 0.9,
            },
        },
    }
    store = yaml_ld.parse_jsonld_star(_json_bytes(doc))

    at_id_pred = pyoxigraph.NamedNode("https://blackcatinformatics.ca/gmeow/@id")
    assert not any(q.predicate == at_id_pred for q in store)
    assert not any(
        isinstance(q.object, pyoxigraph.NamedNode)
        and q.object.value == "http://example.org/reifier"
        and q.predicate.value.startswith("https://blackcatinformatics.ca/gmeow/")
        for q in store
    )

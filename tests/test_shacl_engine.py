"""Unit tests for the pyoxigraph→gmeow_shacl validation seam (#578).

These pin the adapter contract the three production entry points depend on:
the structured report passes through faithfully, severity bucketing and the
byte-stable ``"<focus>: <message>"`` line format match the legacy pySHACL path,
and a parse error hard-fails (never a silent ``conforms``).
"""

from __future__ import annotations

import pytest
from rdflib import RDF, Graph, Literal, URIRef

from gmeow_tools import shacl_engine as se

_NS = "http://example.org/ns#"
_PERSON = URIRef(_NS + "Person")
_ALICE = URIRef(_NS + "alice")

_SHAPES = """@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <http://example.org/ns#> .
ex:PersonShape a sh:NodeShape ;
    sh:targetClass ex:Person ;
    sh:property [
        sh:path ex:name ;
        sh:minCount 1 ;
        sh:severity sh:Violation ;
        sh:message "name required" ;
    ] .
"""


def _alice_graph() -> Graph:
    g = Graph()
    g.add((_ALICE, RDF.type, _PERSON))
    return g


def test_version_is_reported() -> None:
    assert se.gmeow_shacl_version()  # non-empty version string


def test_conforming_graph_has_no_results() -> None:
    g = _alice_graph()
    g.add((_ALICE, URIRef(_NS + "name"), Literal("Alice")))
    report = se.validate_graph(g, _SHAPES)
    assert report["conforms"] is True
    assert report["results"] == []


def test_violation_partitions_to_errors_with_stable_line() -> None:
    # alice is a Person with no ex:name → one Violation.
    report = se.validate_graph(_alice_graph(), _SHAPES)
    assert report["conforms"] is False
    violations, warnings = se.partition_results(report["results"])
    assert warnings == []
    # Byte-stable line: bare IRI (no angle brackets), then ": ", then message —
    # identical to the legacy _partition_shacl_results output.
    assert violations == [f"{_NS}alice: name required"]


def test_warning_severity_buckets_to_warnings() -> None:
    shapes = _SHAPES.replace("sh:Violation", "sh:Warning")
    report = se.validate_graph(_alice_graph(), shapes)
    violations, warnings = se.partition_results(report["results"])
    assert violations == []
    assert warnings == [f"{_NS}alice: name required"]


def test_parse_error_hard_fails() -> None:
    # A malformed shapes document must raise, never silently "conform" (P11/§11).
    with pytest.raises(ValueError):
        se.validate_graph(_alice_graph(), "this is not valid turtle @@@")


def test_term_normalization() -> None:
    assert se._term_str("<http://x>") == "http://x"
    assert se._term_str("_:b0") == "b0"
    assert se._term_str('"literal"') == '"literal"'
    assert se._term_str(None) == "None"

"""The dual-run SHACL cross-check gate: pySHACL ≡ gmeow_shacl (#578).

Mirrors :mod:`tests.test_engine_crosscheck`. The fast tests pin the comparison
machinery (key normalization, the property-vs-node discriminator, divergence
diffing, ledger writing) and run a small *real* dual-run through both engines.
The full merged-ontology + all-examples cross-check is slow (it runs pySHACL ~72
times) and is opt-in via ``GMEOW_RUN_SLOW=1`` — CI runs it as the report-only
``gmeow-dev shacl-crosscheck`` gate.
"""

from __future__ import annotations

import os

import pytest
from rdflib import RDF, Graph, Literal, URIRef

from gmeow_tools import shacl_crosscheck as xc

_NS = "http://example.org/ns#"
_SH = "http://www.w3.org/ns/shacl#"

_SHAPES = """@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <http://example.org/ns#> .
ex:PersonShape a sh:NodeShape ;
    sh:targetClass ex:Person ;
    sh:property [
        sh:path ex:name ;
        sh:minCount 1 ;
        sh:message "name required" ;
    ] .
"""


def _alice() -> Graph:
    g = Graph()
    g.add((URIRef(_NS + "alice"), RDF.type, URIRef(_NS + "Person")))
    return g


def test_norm_iri_handles_both_engine_renderings() -> None:
    assert xc._norm_iri("<http://x>") == "http://x"  # gmeow_shacl
    assert xc._norm_iri("http://x") == "http://x"  # pySHACL bare
    assert xc._norm_iri("_:b0") is None  # gmeow_shacl blank
    assert xc._norm_iri("N1a2b3c4") is None  # pySHACL blank-node id
    assert xc._norm_iri(None) is None


def test_build_key_property_uses_path_not_shape() -> None:
    # A property-level result (has a path) keys on the path, never sourceShape —
    # the engines render sourceShape incompatibly.
    key = xc._build_key(
        f"<{_NS}alice>",
        f"<{_SH}Violation>",
        f"<{_SH}MinCountConstraintComponent>",
        f"<{_NS}name>",
        f"<{_NS}PersonShape>",
    )
    assert key == (
        f"{_NS}alice",
        f"{_SH}Violation",
        f"{_SH}MinCountConstraintComponent",
        "path",
        f"{_NS}name",
    )


def test_build_key_node_uses_iri_shape() -> None:
    # A node-level result (no path) keys on the sourceShape when it is an IRI.
    key = xc._build_key(
        f"<{_NS}alice>",
        f"<{_SH}Violation>",
        f"<{_SH}ClassConstraintComponent>",
        None,
        f"<{_NS}PersonShape>",
    )
    assert key[-2:] == ("shape", f"{_NS}PersonShape")


def test_small_dual_run_agrees() -> None:
    # Both engines on the same tiny input must produce the same key-set.
    data = _alice()  # a Person with no name → one minCount violation
    gmeow = xc._gmeow_keys(data, _SHAPES)
    shapes_graph = Graph().parse(data=_SHAPES, format="turtle")
    pyshacl = xc._pyshacl_keys(data, shapes_graph)
    assert gmeow == pyshacl
    assert len(gmeow) == 1  # the constraint actually fired (not vacuous agreement)


def test_diff_unit_reports_each_side() -> None:
    a: set[xc.ResultKey] = {("x", "v", "c")}
    b: set[xc.ResultKey] = {("y", "v", "c")}
    divs = xc._diff_unit("u", gmeow=a, pyshacl=b)
    sides = {d.side for d in divs}
    assert sides == {"only-pyshacl", "only-gmeow_shacl"}
    assert all(d.unit == "u" and d.reason for d in divs)  # every entry explained


def test_write_ledger_roundtrip(tmp_path) -> None:  # type: ignore[no-untyped-def]
    empty = xc.write_ledger([], path=tmp_path / "ledger.ttl")
    text = empty.read_text(encoding="utf-8")
    assert "No divergences" in text
    Graph().parse(empty, format="turtle")  # valid Turtle

    d = xc.Divergence("merged", "only-pyshacl", "k1 | k2", "a reason")
    full = xc.write_ledger([d], path=tmp_path / "ledger2.ttl")
    g = Graph().parse(full, format="turtle")  # valid Turtle
    assert (None, None, Literal("only-pyshacl")) in g


@pytest.mark.skipif(
    os.environ.get("GMEOW_RUN_SLOW") != "1",
    reason="slow full dual-run (runs pySHACL ~72 times) — opt in with "
    "GMEOW_RUN_SLOW=1; CI runs it as the `gmeow-dev shacl-crosscheck` gate",
)
def test_full_crosscheck_observed_subset_of_ledger() -> None:
    from gmeow_tools.shacl_crosscheck import LEDGER_FILE

    observed = xc.crosscheck_all()
    # The committed ledger is the set of accepted (explained) divergences. Every
    # observed divergence must already be recorded — a new, unexplained one fails.
    ledgered = set()
    if LEDGER_FILE.exists():
        g = Graph().parse(LEDGER_FILE, format="turtle")
        for s in g.subjects(RDF.type, URIRef(xc._XCK + "Divergence")):
            unit = g.value(s, URIRef(xc._XCK + "unit"))
            key = g.value(s, URIRef(xc._XCK + "key"))
            side = g.value(s, URIRef(xc._XCK + "side"))
            ledgered.add((str(unit), str(side), str(key)))
    new = [d for d in observed if (d.unit, d.side, d.key) not in ledgered]
    assert not new, "unledgered SHACL divergences:\n" + "\n".join(
        f"  [{d.side}] {d.unit}: {d.key} ({d.reason})" for d in new
    )

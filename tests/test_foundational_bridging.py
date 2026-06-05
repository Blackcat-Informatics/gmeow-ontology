"""Tests for the gUFO↔BFO foundational-spine bridge (issue #40).

The bridge aligns gUFO's *nature* categories to BFO 2020 (ISO/IEC 21838-2) by
reference — never by import. Three things are checked:

* the expected ``skos:closeMatch`` cells are present in the built alignment graph;
* **every emitted ``bfo:`` IRI is a real BFO class** — verified offline against the
  vendored ``imports/targets/bfo.ttl`` snapshot, with the cell's ``object_label``
  matching BFO's own label (the Principle-7 "verify, don't assume" gate);
* the bridge is *link-only* — no BFO class leaks into the reasoned import closure.

A ``network``-marked test additionally confirms the vendored snapshot still matches
live BFO, so the offline check cannot silently rot.
"""

from __future__ import annotations

import pytest
from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS, SKOS

from gmeow_tools.config import ALIGNMENT_TARGETS, LinkPolicy
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.mappings import (
    Mapping,
    build_alignment_graph,
    expand_curie,
    load_mappings,
)
from gmeow_tools.target_axioms import load_target_snapshot

GUFO = "http://purl.org/nemo/gufo#"
BFO = "http://purl.obolibrary.org/obo/"

#: The expected gUFO-nature → BFO closeMatch cells (subject local, bfo local, label).
EXPECTED_CELLS: tuple[tuple[str, str, str], ...] = (
    ("Endurant", "BFO_0000002", "continuant"),
    ("Object", "BFO_0000040", "material entity"),
    ("FunctionalComplex", "BFO_0000030", "object"),
    ("Collection", "BFO_0000027", "object aggregate"),
    ("Relator", "BFO_0000020", "specifically dependent continuant"),
    ("Quality", "BFO_0000019", "quality"),
    ("Event", "BFO_0000003", "occurrent"),
)


def _alignment_graph() -> Graph:
    return build_alignment_graph(load_mappings())


def _foundational_mappings() -> list[Mapping]:
    """Every gUFO→BFO row (subject in gufo:, object in bfo:)."""
    return [
        m
        for m in load_mappings()
        if m.subject_id.startswith("gufo:") and m.object_id.startswith("bfo:")
    ]


def test_expected_cells_present_in_alignment_graph() -> None:
    graph = _alignment_graph()
    for gufo_local, bfo_local, _label in EXPECTED_CELLS:
        triple = (
            URIRef(GUFO + gufo_local),
            SKOS.closeMatch,
            URIRef(BFO + bfo_local),
        )
        assert triple in graph, f"missing bridge cell {gufo_local} → {bfo_local}"


def test_bridge_uses_closematch_only() -> None:
    # UFO and BFO build their categories on different bases, so no cell may claim
    # exact equivalence — every foundational row is a fuzzy closeMatch.
    mappings = _foundational_mappings()
    for m in mappings:
        assert m.predicate_id == "skos:closeMatch", (
            f"{m.subject_id} → {m.object_id} uses {m.predicate_id}; "
            "foundational-spine cells must be skos:closeMatch"
        )
    assert len(mappings) == len(EXPECTED_CELLS)


def test_every_bfo_iri_is_a_real_class_in_the_snapshot() -> None:

    label, verified offline against the vendored snapshot."""
    snapshot = load_target_snapshot("bfo")
    assert snapshot is not None, (
        "imports/targets/bfo.ttl is missing — run "
        "`gmeow refresh-target-axioms --target bfo`"
    )
    for m in _foundational_mappings():
        iri = expand_curie(m.object_id)
        assert (iri, RDF.type, OWL.Class) in snapshot, (
            f"{m.object_id} is not a declared owl:Class in the BFO snapshot — "
            "the IRI is invented or mistyped"
        )
        snapshot_labels = {str(o) for o in snapshot.objects(iri, RDFS.label)}
        assert m.object_label in snapshot_labels, (
            f"{m.object_id} object_label {m.object_label!r} does not match BFO's "
            f"own label(s) {snapshot_labels}"
        )


def test_bridge_is_link_only_no_import() -> None:
    """No BFO class enters the reasoned import closure — the bridge is by reference
    (Principle 5). The snapshot lives in imports/targets/, a subdir not merged."""
    merged = load_merged_graph(include_imports=True)
    bfo_subjects = [
        s for s in merged.subjects(RDF.type, OWL.Class) if str(s).startswith(BFO)
    ]
    assert not bfo_subjects, (
        f"BFO classes leaked into the reasoned graph: {bfo_subjects[:3]} — "
        "the foundational bridge must stay link-only"
    )


def test_bfo_is_import_ok_upper_ontology() -> None:
    target = ALIGNMENT_TARGETS["bfo"]
    assert target.kind == "upper"
    assert target.policy is LinkPolicy.IMPORT_OK


def test_coverage_reported() -> None:
    # Coverage = mapped gUFO nature categories ÷ the categories GMEOW actually
    # grounds classes in. Stereotypes (Kind/SubKind/…) intentionally have no BFO
    # cell (BFO has no meta-level); this asserts the bridge is non-trivial.
    mapped = {m.subject_id.split(":", 1)[1] for m in _foundational_mappings()}
    assert {"Endurant", "Object", "Event", "Relator", "Quality"} <= mapped


@pytest.mark.network
def test_vendored_snapshot_matches_live_bfo() -> None:
    """The offline snapshot must not silently rot: every BFO IRI we reference still
    exists, as a class, with the same label, in the live ontology."""
    from gmeow_tools.target_axioms import fetch_target_axioms

    live = fetch_target_axioms("bfo")
    for _gufo_local, bfo_local, label in EXPECTED_CELLS:
        iri = URIRef(BFO + bfo_local)
        assert (iri, RDF.type, OWL.Class) in live, f"{bfo_local} gone from live BFO"
        assert label in {str(o) for o in live.objects(iri, RDFS.label)}, (
            f"{bfo_local} label changed in live BFO (was {label!r})"
        )

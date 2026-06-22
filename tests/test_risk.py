# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The risk slice (#354, EPIC #348).

Counterfactual causal structure without counterfactual machinery: cascades
relate event TYPES, never instances — the no-occurrence gate makes that
executable. Hazard is GMEOW's first logic:Disposition use; causal links are
standpoint-indexed claims; severity is the fourth ordered-vocabulary use;
Mitigation bridges to the deontic and procedural worlds by deliberately open
range (the tenurePosition precedent — no extension→extension dependency).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
LOGIC = Namespace("https://blackcatinformatics.ca/logic/")

FIXTURES = Path(__file__).parent / "fixtures" / "shapes"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    g = Graph()
    g.parse(FIXTURES / f"{name}.ttl", format="turtle")
    return g


# --------------------------------------------------------------------------- #
# Structural invariants
# --------------------------------------------------------------------------- #


def test_hazard_is_a_disposition() -> None:
    """First logic:Disposition use in GMEOW — a hazard that never manifests
    is fully real."""
    g = _graph()
    assert (GM.Hazard, RDF.type, LOGIC.Kind) in g
    assert (GM.Hazard, RDFS.subClassOf, LOGIC.Disposition) in g
    assert (GM.Hazard, RDFS.subClassOf, GM.RiskFactor) in g
    assert (GM.hazardBearer, RDF.type, OWL.FunctionalProperty) in g
    assert (GM.manifestedAsType, RDFS.range, GM.EventType) in g


def test_type_level_links_are_never_transitive() -> None:
    """Chain composition is solver work (P12) — no causal property may ever
    carry a transitivity axiom."""
    g = _graph()
    causal_props = (
        GM.typeCauses,
        GM.typeEnables,
        GM.typePrevents,
        GM.typeMitigates,
        GM.linkNext,
    )
    for prop in causal_props:
        assert (prop, RDF.type, OWL.ObjectProperty) in g, prop
        assert (prop, RDF.type, OWL.TransitiveProperty) not in g, prop
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g, prop
    for prop in (GM.typeCauses, GM.typeEnables, GM.typePrevents, GM.typeMitigates):
        assert (prop, RDFS.domain, GM.EventType) in g, prop
        assert (prop, RDFS.range, GM.EventType) in g, prop


def test_causal_link_constituents() -> None:
    g = _graph()
    assert (GM.CausalLink, RDFS.subClassOf, LOGIC.Relator) in g
    assert (GM.CausalLink, RDFS.subClassOf, GM.RiskFactor) in g
    functional = (GM.linkAntecedent, GM.linkConsequent, GM.causalModality)
    for prop in functional:
        assert (prop, RDF.type, OWL.FunctionalProperty) in g, prop
    # Source-variable values are deliberately NOT OWL-functional (PR #385):
    # divergent estimates/grades/statuses coexist via the statement layer;
    # single-valuedness per base graph is SHACL's job.
    multi_source = (
        GM.linkStrength,
        GM.cascadeSeverity,
        GM.hazardSeverity,
        GM.mitigationStatus,
    )
    for prop in multi_source:
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g, prop
    # Mechanism prose is localizable: NOT functional, range-open (#376 lesson).
    assert (GM.linkMechanism, RDF.type, OWL.FunctionalProperty) not in g
    assert g.value(GM.linkMechanism, RDFS.range) is None


def test_severity_is_the_fourth_ordered_vocabulary() -> None:
    g = _graph()
    assert (GM.moreSevereThan, RDF.type, OWL.TransitiveProperty) in g
    assert (GM.moreSevereThan, RDFS.domain, GM.SeverityLevel) in g
    chain = [
        (GM.severityCatastrophic, GM.severitySevere),
        (GM.severitySevere, GM.severityModerate),
        (GM.severityModerate, GM.severityMinor),
    ]
    for graver, lesser in chain:
        assert (graver, GM.moreSevereThan, lesser) in g


def test_mitigation_measure_is_range_open() -> None:
    """The tenurePosition precedent, fourth use: norms and procedures plug in
    with no extension→extension dependency (P16)."""
    g = _graph()
    assert g.value(GM.mitigationMeasure, RDFS.range) is None
    assert (GM.mitigationCounters, RDFS.range, GM.RiskFactor) in g
    assert (GM.RiskFactor, RDF.type, LOGIC.Category) in g
    assert (GM.RiskFactor, RDFS.subClassOf, GM.Entity) in g


def test_no_occurrence_gate() -> None:
    """The EPIC #358-pattern gate: loading the risk fixtures and worked
    example entails ZERO gmeow:Event instances — cascades are expressible
    without anything having happened."""
    g = Graph()
    g.parse(FIXTURES / "risk-wellformed.ttl", format="turtle")
    g.parse(
        Path(__file__).parent.parent
        / "slices"
        / "extensions"
        / "risk"
        / "examples"
        / "trust-collapse.ttl",
        format="turtle",
    )
    assert list(g.subjects(RDF.type, GM.Event)) == []
    # And the feared kinds ARE present — as types.
    assert len(list(g.subjects(RDF.type, GM.EventType))) >= 3


# --------------------------------------------------------------------------- #
# Closed-world SHACL shapes
# --------------------------------------------------------------------------- #


def test_wellformed_risk_fixture_conforms() -> None:
    result = run_shacl(_fixture("risk-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_risk_fixture_is_flagged() -> None:
    result = run_shacl(_fixture("risk-malformed"))
    assert not result.ok
    errors = "\n".join(result.errors)
    assert "exactly one gmeow:hazardBearer" in errors
    assert "at least one feared gmeow:manifestedAsType" in errors
    assert "antecedent and consequent must be distinct" in errors
    assert "exactly one gmeow:causalModality" in errors
    assert "no causal link may reach itself" in errors
    assert "an ungraded cascade is just a story" in errors
    assert "at least one gmeow:mitigationMeasure" in errors
    assert "CausalLink (barrier on the chain) or a Hazard" in errors


# --------------------------------------------------------------------------- #
# Competency
# --------------------------------------------------------------------------- #


def test_competency_severity_order_query() -> None:
    query_path = COMPETENCY_DIR / "risk-severity-order.rq"
    query = query_path.read_text(encoding="utf-8")
    tops: set[object] = set()
    for row in _graph().query(query):
        assert isinstance(row, ResultRow)
        tops.add(row[0])
    assert tops == {GM.severityCatastrophic}

"""The plan ⟂ execution slice (#226).

GMEOW models occurrences richly (Event, Participation, Activity, Allen relations)
but had no prescriptive layer. This module de-conflates plan from execution:
Procedure / ProcedureStep (InformationObject) vs Execution (Event), with reified
ControlFlow and DataFlow relators following the Participation idiom.
"""

from __future__ import annotations

from functools import lru_cache

from rdflib import OWL, RDF, RDFS, Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")


@lru_cache(maxsize=1)
def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# gUFO grounding
# --------------------------------------------------------------------------- #


def test_procedure_is_information_object() -> None:
    g = _graph()
    assert (GM.Procedure, RDF.type, OWL.Class) in g
    assert (GM.Procedure, RDFS.subClassOf, GM.InformationObject) in g


def test_procedure_step_is_information_object() -> None:
    g = _graph()
    assert (GM.ProcedureStep, RDF.type, OWL.Class) in g
    assert (GM.ProcedureStep, RDFS.subClassOf, GM.InformationObject) in g


def test_execution_is_event() -> None:
    g = _graph()
    assert (GM.Execution, RDF.type, OWL.Class) in g
    assert (GM.Execution, RDFS.subClassOf, GM.Event) in g


def test_control_flow_is_relator() -> None:
    g = _graph()
    assert (GM.ControlFlow, RDF.type, OWL.Class) in g
    assert (GM.ControlFlow, RDFS.subClassOf, GUFO.Relator) in g


def test_data_flow_is_relator() -> None:
    g = _graph()
    assert (GM.DataFlow, RDF.type, OWL.Class) in g
    assert (GM.DataFlow, RDFS.subClassOf, GUFO.Relator) in g


# --------------------------------------------------------------------------- #
# Value vocabularies are individuals, never subclasses (Principle 9)
# --------------------------------------------------------------------------- #


def test_procedure_type_values_are_individuals() -> None:
    g = _graph()
    for term in (
        "procedureTypeRecipe",
        "procedureTypeLabProtocol",
        "procedureTypeDataPipeline",
        "procedureTypeAgentFlow",
        "procedureTypeCiBuild",
        "procedureTypeBusinessProcess",
        "procedureTypeResearchPlan",
        "procedureTypeIngestion",
    ):
        uri = GM[term]
        assert (uri, RDF.type, GM.ProcedureType) in g
        assert (uri, RDF.type, OWL.Class) not in g


def test_step_type_values_are_individuals() -> None:
    g = _graph()
    for term in (
        "stepTypeAtomic",
        "stepTypeStart",
        "stepTypeEnd",
        "stepTypeBranch",
        "stepTypeSubprocess",
        "stepTypeParallel",
    ):
        uri = GM[term]
        assert (uri, RDF.type, GM.StepType) in g
        assert (uri, RDF.type, OWL.Class) not in g


def test_execution_status_values_are_individuals() -> None:
    g = _graph()
    for term in (
        "executionStatusPending",
        "executionStatusRunning",
        "executionStatusSucceeded",
        "executionStatusFailed",
        "executionStatusCancelled",
        "executionStatusSkipped",
    ):
        uri = GM[term]
        assert (uri, RDF.type, GM.ExecutionStatus) in g
        assert (uri, RDF.type, OWL.Class) not in g


def test_branch_condition_type_values_are_individuals() -> None:
    g = _graph()
    for term in (
        "branchConditionIf",
        "branchConditionSwitch",
        "branchConditionLoop",
        "branchConditionParallel",
    ):
        uri = GM[term]
        assert (uri, RDF.type, GM.BranchConditionType) in g
        assert (uri, RDF.type, OWL.Class) not in g


# --------------------------------------------------------------------------- #
# Seed profiles
# --------------------------------------------------------------------------- #


def test_ingestion_procedure_has_six_steps() -> None:
    g = _graph()
    steps = list(g.objects(GM.procedureIngestionCanonical, GM.hasProcedureStep))
    assert len(steps) == 6


def test_research_inquiry_has_priority_theme_status() -> None:
    g = _graph()
    assert (GM.inquiryPriority, RDF.type, OWL.DatatypeProperty) in g
    assert (GM.inquiryTheme, RDF.type, OWL.DatatypeProperty) in g
    assert (GM.inquiryStatus, RDF.type, OWL.ObjectProperty) in g
    assert (GM.resolvedByArtifact, RDF.type, OWL.ObjectProperty) in g


# --------------------------------------------------------------------------- #
# Composition — recursive subprocess via subProcedureOf
# --------------------------------------------------------------------------- #


def test_subprocedure_composition() -> None:
    g = _graph()
    assert (GM.subProcedureOf, RDF.type, OWL.TransitiveProperty) in g
    assert (GM.hasSubProcedure, RDF.type, OWL.TransitiveProperty) in g
    assert (GM.subProcedureOf, RDFS.subPropertyOf, GM.partOf) in g
    assert (GM.hasSubProcedure, RDFS.subPropertyOf, GM.hasPart) in g


# --------------------------------------------------------------------------- #
# Execution linkage — plan ⟂ execution
# --------------------------------------------------------------------------- #


def test_execution_links_to_procedure_and_step() -> None:
    g = _graph()
    assert (GM.executesProcedure, RDF.type, OWL.ObjectProperty) in g
    assert (GM.executesStep, RDF.type, OWL.ObjectProperty) in g


# --------------------------------------------------------------------------- #
# ControlFlow / DataFlow relator structure
# --------------------------------------------------------------------------- #


def test_control_flow_has_source_target() -> None:
    g = _graph()
    assert (GM.flowSource, RDF.type, OWL.ObjectProperty) in g
    assert (GM.flowTarget, RDF.type, OWL.ObjectProperty) in g
    assert (GM.flowGuard, RDF.type, OWL.ObjectProperty) in g


def test_data_flow_has_source_target_entity() -> None:
    g = _graph()
    assert (GM.dataFlowSource, RDF.type, OWL.ObjectProperty) in g
    assert (GM.dataFlowTarget, RDF.type, OWL.ObjectProperty) in g
    assert (GM.dataFlowEntity, RDF.type, OWL.ObjectProperty) in g

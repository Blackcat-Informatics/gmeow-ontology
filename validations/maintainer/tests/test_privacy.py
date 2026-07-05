"""Tests for the privacy / consent / redaction facility (#73, PRIV-GEN).

Covers the ontology structure (SensitivityLevel as universal QualityValue,
hasSensitivity as domain-free non-functional ObjectProperty, privacy roles on
RightsStatement, PrivacyNotice as InformationObject), orthogonality to other
axes, no preferred/primary term, and the ODRL projection round-trip over the
coverage fixture.
"""

from __future__ import annotations

from pathlib import Path

from purrdf.compat.rdflib import RDF, Graph, Namespace, URIRef

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.projections import project_graph
from gmeow_tools.slices import module_path

GM = Namespace(NAMESPACE)
ODRL = Namespace("http://www.w3.org/ns/odrl/2/")
EX = Namespace("https://example.org/privacy/")

COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _projection_source() -> Graph:
    graph = load_merged_graph(include_imports=False)
    graph.parse(COVERAGE_FIXTURES / "privacy.ttl", format="turtle")
    return graph


# --------------------------------------------------------------------------- #
# No preferred / primary (Principle 9)
# --------------------------------------------------------------------------- #


def test_no_preferred_or_primary_sensitivity_term() -> None:
    """No gmeow:primary* / gmeow:preferred* privacy term."""
    module = Graph().parse(
        module_path("kernel"),
        format="turtle",
    )
    offenders = []
    for s in set(module.subjects()):
        if not isinstance(s, URIRef) or not str(s).startswith(NAMESPACE):
            continue
        local = str(s)[len(NAMESPACE) :].lower()
        if "/" not in local and local.startswith(("primary", "preferred")):
            offenders.append(str(s))
    assert offenders == [], offenders


# --------------------------------------------------------------------------- #
# Projections (ODRL)
# --------------------------------------------------------------------------- #
import pytest
pytestmark = pytest.mark.maintainer


def test_odrl_projection_emits_privacy_policy() -> None:
    out = project_graph("odrl", _projection_source())
    assert (EX["alice-privacy"], RDF.type, ODRL.Set) in out
    assert (EX["alice-privacy"], ODRL.permission, EX["perm-process"]) in out
    assert (EX["perm-process"], RDF.type, ODRL.Permission) in out
    assert (EX["perm-process"], ODRL.action, GM.actionProcessPersonalData) in out
    assert (EX["perm-process"], ODRL.target, EX["alice-home"]) in out
    assert (EX["perm-process"], ODRL.assignee, EX.deliveryCo) in out
    # Constraint (purpose = delivery).
    assert (EX["perm-process"], ODRL.constraint, EX["purpose-delivery"]) in out
    assert (EX["purpose-delivery"], ODRL.leftOperand, GM.leftOpPurpose) in out
    assert (EX["purpose-delivery"], ODRL.operator, GM.operatorEq) in out

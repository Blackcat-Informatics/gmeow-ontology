"""Tests for the privacy / consent / redaction facility (#73, PRIV-GEN).

Covers the ontology structure (SensitivityLevel as universal QualityValue,
hasSensitivity as domain-free non-functional ObjectProperty, privacy roles on
RightsStatement, PrivacyNotice as InformationObject), orthogonality to other
axes, no preferred/primary term, and the ODRL projection round-trip over the
coverage fixture.
"""

from __future__ import annotations

from itertools import combinations
from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.projections import project_graph
from gmeow_tools.slices import module_path
from tests._graph_nt import run_shacl

GM = Namespace(NAMESPACE)
ODRL = Namespace("http://www.w3.org/ns/odrl/2/")
EX = Namespace("https://example.org/privacy/")
GUFO = Namespace("http://purl.org/nemo/gufo#")

SHAPES_FIXTURES = Path(__file__).parent / "fixtures" / "shapes"
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    return Graph().parse(SHAPES_FIXTURES / f"{name}.ttl", format="turtle")


def _projection_source() -> Graph:
    graph = load_merged_graph(include_imports=False)
    graph.parse(COVERAGE_FIXTURES / "privacy.ttl", format="turtle")
    return graph


# --------------------------------------------------------------------------- #
# Ontology structure
# --------------------------------------------------------------------------- #


def test_sensitivity_level_class_structure() -> None:
    g = _graph()
    assert (GM.SensitivityLevel, RDF.type, OWL.Class) in g
    assert (GM.SensitivityLevel, RDF.type, GUFO.AbstractIndividualType) in g
    assert (GM.SensitivityLevel, RDFS.subClassOf, GUFO.QualityValue) in g


def test_has_sensitivity_property_structure() -> None:
    g = _graph()
    assert (GM.hasSensitivity, RDF.type, OWL.ObjectProperty) in g
    assert (GM.hasSensitivity, RDFS.range, GM.SensitivityLevel) in g
    # Domain-free (universal, like hasGranularity).
    assert g.value(GM.hasSensitivity, RDFS.domain) is None
    # NOT functional: multi-source claims coexist (Principle 9).
    assert (GM.hasSensitivity, RDF.type, OWL.FunctionalProperty) not in g


def test_value_vocab_spans_five_seeds() -> None:
    g = _graph()
    members = set(g.subjects(RDF.type, GM.SensitivityLevel))
    assert members == {
        GM.sensitivityPublic,
        GM.sensitivityInternal,
        GM.sensitivityConfidential,
        GM.sensitivityRestricted,
        GM.sensitivitySensitivePersonal,
    }


def test_privacy_roles_declared() -> None:
    g = _graph()
    # hasDataSubject / hasDataController are ObjectProperty on RightsStatement.
    assert (GM.hasDataSubject, RDF.type, OWL.ObjectProperty) in g
    assert (GM.hasDataSubject, RDFS.domain, GM.RightsStatement) in g
    assert (GM.hasDataSubject, RDFS.range, GM.Agent) in g
    assert (GM.hasDataController, RDF.type, OWL.ObjectProperty) in g
    assert (GM.hasDataController, RDFS.domain, GM.RightsStatement) in g
    assert (GM.hasDataController, RDFS.range, GM.Agent) in g


def test_privacy_notice_is_information_object() -> None:
    g = _graph()
    assert (GM.PrivacyNotice, RDF.type, OWL.Class) in g
    assert (GM.PrivacyNotice, RDFS.subClassOf, GM.InformationObject) in g


def test_has_privacy_notice_is_domain_free() -> None:
    g = _graph()
    assert (GM.hasPrivacyNotice, RDF.type, OWL.ObjectProperty) in g
    assert (GM.hasPrivacyNotice, RDFS.range, GM.PrivacyNotice) in g
    assert g.value(GM.hasPrivacyNotice, RDFS.domain) is None


def test_action_process_personal_data_is_rights_action() -> None:
    g = _graph()
    assert (GM.actionProcessPersonalData, RDF.type, GM.RightsAction) in g


# --------------------------------------------------------------------------- #
# Orthogonality (Principle 9)
# --------------------------------------------------------------------------- #


def test_sensitivity_orthogonal_to_other_axes() -> None:
    """hasSensitivity ⟂ hasDeterminacy ⟂ confidence: no inferential bridge."""
    g = _graph()
    axes = [GM.hasSensitivity, GM.hasDeterminacy, GM.confidence]
    for a, b in combinations(axes, 2):
        assert (a, RDFS.subPropertyOf, b) not in g
        assert (b, RDFS.subPropertyOf, a) not in g
        assert (a, OWL.equivalentProperty, b) not in g
        assert (b, OWL.equivalentProperty, a) not in g


def test_sensitivity_orthogonal_to_granularity() -> None:
    """hasSensitivity ⟂ hasGranularity: distinct axes."""
    g = _graph()
    assert (GM.hasSensitivity, RDFS.subPropertyOf, GM.hasGranularity) not in g
    assert (GM.hasGranularity, RDFS.subPropertyOf, GM.hasSensitivity) not in g
    assert (GM.hasSensitivity, OWL.equivalentProperty, GM.hasGranularity) not in g


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
# Closed-world SHACL shapes
# --------------------------------------------------------------------------- #


def test_wellformed_privacy_fixture_conforms() -> None:
    result = run_shacl(_fixture("privacy-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_privacy_fixture_is_flagged() -> None:
    result = run_shacl(_fixture("privacy-malformed"))
    assert not result.ok
    errors = "\n".join(result.errors)
    warnings = "\n".join(result.warnings)
    assert "must govern exactly one asset" in errors
    assert "must regulate exactly one action" in errors
    # Consent well-formedness is Warning severity (incomplete metadata is allowed).
    assert "should name exactly one data subject" in warnings
    assert "at least one data controller" in warnings


def test_sensitive_value_warns_but_does_not_fail() -> None:
    result = run_shacl(_fixture("privacy-sensitive-warning"))
    assert result.ok, f"warning-only graph must pass; errors: {result.errors}"
    assert any("sensitivitySensitivePersonal" in w for w in result.warnings), (
        result.warnings
    )


# --------------------------------------------------------------------------- #
# Projections (ODRL)
# --------------------------------------------------------------------------- #


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

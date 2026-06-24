"""Tests for the rights / IP / trademark / licensing facility (#21).

Structural TBox assertions (class stereotypes, subclass/subproperty wiring,
disjointness, value-vocabulary seeding, Principle-9 guards) have been migrated
to the declarative DSL at slices/core/rights/tests/structural.ttl and run under
the native Rust slicetest harness (crates/slicetest).

Closed-world SHACL shape conformance tests have been migrated to
crates/validate/tests/conformance_rights.rs (#867).

Retained here: the numeric action-count check (not expressible as a module-scoped
ASK) and the ODRL / CC REL / schema.org projection round-trips over the coverage
fixture.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, Graph, Namespace

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.projections import project_graph

GM = Namespace(NAMESPACE)
ODRL = Namespace("http://www.w3.org/ns/odrl/2/")
CC = Namespace("http://creativecommons.org/ns#")
SCHEMA = Namespace("https://schema.org/")
DCTERMS = Namespace("http://purl.org/dc/terms/")
SPDX = Namespace("http://spdx.org/rdf/terms#")
EX = Namespace("https://example.org/rights/")

COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _projection_source() -> Graph:
    graph = load_merged_graph(include_imports=False)
    graph.parse(COVERAGE_FIXTURES / "rights.ttl", format="turtle")
    return graph


# --------------------------------------------------------------------------- #
# Ontology structure (numeric check — not expressible as a module-scoped ASK)
# --------------------------------------------------------------------------- #


def test_expanded_action_vocabulary_is_seeded() -> None:
    """The ODRL Common-Vocabulary actions are seeded (maximal, not a thin stub)."""
    g = _graph()
    actions = set(g.subjects(RDF.type, GM.RightsAction))
    # At least ~45 actions (the 11 originals + the ODRL common vocabulary).
    assert len(actions) >= 45, len(actions)
    for a in ("actionSell", "actionStream", "actionModify", "actionAnonymize"):
        assert GM[a] in actions, a


# --------------------------------------------------------------------------- #
# Projections (ODRL / CC REL / schema.org)
# --------------------------------------------------------------------------- #


def test_odrl_projection_emits_a_policy_with_rules() -> None:
    out = project_graph("odrl", _projection_source())
    assert (EX["photo-rights"], RDF.type, ODRL.Set) in out
    # Permission rule with action, target, assignee.
    assert (EX["photo-rights"], ODRL.permission, EX["perm-reproduce"]) in out
    assert (EX["perm-reproduce"], RDF.type, ODRL.Permission) in out
    assert (EX["perm-reproduce"], ODRL.action, GM.actionReproduce) in out
    assert (EX["perm-reproduce"], ODRL.target, EX.photo) in out
    assert (EX["perm-reproduce"], ODRL.assignee, EX.acme) in out
    # Prohibition + duty.
    assert (EX["photo-rights"], ODRL.prohibition, EX["proh-commercial"]) in out
    assert (EX["proh-commercial"], RDF.type, ODRL.Prohibition) in out
    assert (EX["photo-rights"], ODRL.obligation, EX["duty-attribute"]) in out
    assert (EX["duty-attribute"], RDF.type, ODRL.Duty) in out
    # The licence becomes an ODRL Offer with its assigner.
    assert (EX["cc-by-4"], RDF.type, ODRL.Offer) in out
    assert (EX["cc-by-4"], ODRL.assigner, EX.jane) in out


def test_odrl_projection_emits_constraint_and_conflict_logic() -> None:
    out = project_graph("odrl", _projection_source())
    # The temporal constraint (valid until 2036) projects to an ODRL constraint.
    assert (EX["perm-reproduce"], ODRL.constraint, EX["until-2036"]) in out
    assert (EX["until-2036"], RDF.type, ODRL.Constraint) in out
    assert (EX["until-2036"], ODRL.leftOperand, GM.leftOpDateTime) in out
    assert (EX["until-2036"], ODRL.operator, GM.operatorLteq) in out
    assert any(out.objects(EX["until-2036"], ODRL.rightOperand))
    # Conflict-resolution strategy + a prohibition's remedy (ODRL keys remedy to
    # prohibitions, consequence to duties).
    assert (EX["photo-rights"], ODRL.conflict, GM.conflictProhibit) in out
    assert (EX["proh-commercial"], ODRL.remedy, EX["duty-compensate"]) in out
    # Asset + party typing.
    assert (EX.photo, RDF.type, ODRL.Asset) in out
    assert (EX.acme, RDF.type, ODRL.Party) in out


def test_spdx_projection_emits_listed_license() -> None:
    out = project_graph("spdx", _projection_source())
    assert (EX["cc-by-4"], RDF.type, SPDX.License) in out
    assert "CC-BY-4.0" in {str(o) for o in out.objects(EX["cc-by-4"], SPDX.licenseId)}
    assert any(out.objects(EX["cc-by-4"], SPDX.name))
    assert any(out.objects(EX["cc-by-4"], SPDX.licenseText))


def test_cc_projection_emits_license_and_attribution() -> None:
    out = project_graph("cc", _projection_source())
    assert (EX.photo, CC.license, EX["cc-by-4"]) in out
    assert (EX["cc-by-4"], RDF.type, CC.License) in out
    assert "Photo by Jane Doe / CC BY 4.0" in {
        str(o) for o in out.objects(EX.photo, CC.attributionName)
    }


def test_dcterms_projection_emits_flat_rights() -> None:
    out = project_graph("dcterms", _projection_source())
    assert (EX.photo, DCTERMS.license, EX["cc-by-4"]) in out
    assert (EX.photo, DCTERMS.rightsHolder, EX.jane) in out
    assert "© 2026 Jane Doe" in {str(o) for o in out.objects(EX.photo, DCTERMS.rights)}


def test_schema_projection_emits_rights_cluster() -> None:
    out = project_graph("schema-org", _projection_source())
    assert (EX.photo, SCHEMA.copyrightHolder, EX.jane) in out
    assert (EX.photo, SCHEMA.license, EX["cc-by-4"]) in out
    assert (EX["acme-mark"], RDF.type, SCHEMA.Brand) in out
    # Flattened copyright year + notice + credit text are present.
    assert any(out.objects(EX.photo, SCHEMA.copyrightYear))
    assert any(out.objects(EX.photo, SCHEMA.copyrightNotice))
    assert "Photo by Jane Doe / CC BY 4.0" in {
        str(o) for o in out.objects(EX.photo, SCHEMA.creditText)
    }

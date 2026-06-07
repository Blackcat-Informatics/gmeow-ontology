"""Tests for the rights / IP / trademark / licensing facility (#21).

Covers the ontology structure (relator groundings, reuse-by-subproperty, the
disjoint deontic trio, the open value vocabularies, no preferred/primary right),
the closed-world SHACL shapes (well-formed / malformed / expired-warning), and the
ODRL / CC REL / schema.org projection round-trips over the coverage fixture.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.projections import project_graph
from gmeow_tools.validate import run_shacl

GM = Namespace(NAMESPACE)
ODRL = Namespace("http://www.w3.org/ns/odrl/2/")
CC = Namespace("http://creativecommons.org/ns#")
SCHEMA = Namespace("https://schema.org/")
DCTERMS = Namespace("http://purl.org/dc/terms/")
SPDX = Namespace("http://spdx.org/rdf/terms#")
EX = Namespace("https://example.org/rights/")

SHAPES_FIXTURES = Path(__file__).parent / "fixtures" / "shapes"
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"
GUFO = Namespace("http://purl.org/nemo/gufo#")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    return Graph().parse(SHAPES_FIXTURES / f"{name}.ttl", format="turtle")


def _projection_source() -> Graph:
    graph = load_merged_graph(include_imports=False)
    graph.parse(COVERAGE_FIXTURES / "rights.ttl", format="turtle")
    return graph


# --------------------------------------------------------------------------- #
# Ontology structure
# --------------------------------------------------------------------------- #


def test_core_relators_are_grounded() -> None:
    g = _graph()
    for cls, stereo in (
        (GM.RightsStatement, GUFO.SubKind),
        (GM.Copyright, GUFO.Kind),
        (GM.Trademark, GUFO.Kind),
        (GM.Mark, GUFO.Kind),
        (GM.Rule, GUFO.Category),
        (GM.Permission, GUFO.Kind),
        (GM.Prohibition, GUFO.Kind),
        (GM.Duty, GUFO.Kind),
        (GM.License, GUFO.SubKind),
    ):
        assert (cls, RDF.type, OWL.Class) in g, cls
        assert (cls, RDF.type, stereo) in g, (cls, stereo)
    # The IP relators specialise gufo:Relator; the rules specialise gmeow:Rule.
    assert (GM.RightsStatement, RDFS.subClassOf, GUFO.Relator) in g
    assert (GM.Permission, RDFS.subClassOf, GM.Rule) in g


def test_license_is_an_agreement() -> None:
    g = _graph()
    assert (GM.License, RDFS.subClassOf, GM.Agreement) in g


def test_holder_and_party_properties_specialise_reused_terms() -> None:
    g = _graph()
    # Holder attribution reuses gmeow:wasAttributedTo (Principle: reuse, not duplicate).
    assert (GM.copyrightHolder, RDFS.subPropertyOf, GM.wasAttributedTo) in g
    assert (GM.trademarkHolder, RDFS.subPropertyOf, GM.wasAttributedTo) in g
    # Licence parties reuse gmeow:hasParty.
    assert (GM.licensor, RDFS.subPropertyOf, GM.hasParty) in g
    assert (GM.licensee, RDFS.subPropertyOf, GM.hasParty) in g


def test_deontic_trio_is_disjoint() -> None:
    g = _graph()
    members = set()
    for adc in g.subjects(RDF.type, OWL.AllDisjointClasses):
        for lst in g.objects(adc, OWL.members):
            members |= set(g.items(lst))
    assert {GM.Permission, GM.Prohibition, GM.Duty} <= members


def test_value_vocabularies_are_open_individuals() -> None:
    g = _graph()
    vocabs = (GM.RightsAction, GM.LicenseFamily, GM.TrademarkStatus, GM.CopyrightStatus)
    for vocab in vocabs:
        assert (vocab, RDFS.subClassOf, GUFO.QualityValue) in g, vocab
    # A representative seed individual from each vocabulary exists.
    assert (GM.actionReproduce, RDF.type, GM.RightsAction) in g
    assert (GM.licenseFamilyCC, RDF.type, GM.LicenseFamily) in g
    assert (GM.trademarkStatusRegistered, RDF.type, GM.TrademarkStatus) in g
    assert (GM.copyrightStatusInCopyright, RDF.type, GM.CopyrightStatus) in g


def test_spdx_license_id_property_exists() -> None:
    g = _graph()
    assert (GM.spdxLicenseId, RDF.type, OWL.DatatypeProperty) in g
    assert (GM.spdxLicenseId, RDFS.domain, GM.License) in g


def test_constraint_algebra_terms_exist() -> None:
    g = _graph()
    assert (GM.AtomicConstraint, RDFS.subClassOf, GM.Constraint) in g
    assert (GM.LogicalConstraint, RDFS.subClassOf, GM.Constraint) in g
    assert (GM.ruleConstraint, RDF.type, OWL.ObjectProperty) in g
    # The ODRL operand / operator / logic value vocabularies are seeded.
    assert (GM.leftOpDateTime, RDF.type, GM.LeftOperand) in g
    assert (GM.operatorLteq, RDF.type, GM.ConstraintOperator) in g
    assert (GM.logicAnd, RDF.type, GM.ConstraintLogic) in g
    assert (GM.conflictProhibit, RDF.type, GM.ConflictStrategy) in g


def test_rights_type_vocabulary_exists() -> None:
    g = _graph()
    for t in ("rightsTypeCopyright", "rightsTypePatent", "rightsTypeTradeSecret"):
        assert (GM[t], RDF.type, GM.RightsType) in g


def test_expanded_action_vocabulary_is_seeded() -> None:
    """The ODRL Common-Vocabulary actions are seeded (maximal, not a thin stub)."""
    g = _graph()
    actions = set(g.subjects(RDF.type, GM.RightsAction))
    # At least ~45 actions (the 11 originals + the ODRL common vocabulary).
    assert len(actions) >= 45, len(actions)
    for a in ("actionSell", "actionStream", "actionModify", "actionAnonymize"):
        assert GM[a] in actions, a


def test_no_action_value_is_a_class() -> None:
    """Actions are values, never per-value subclasses (Principle 9, no overtyping)."""
    g = _graph()
    for action in g.subjects(RDF.type, GM.RightsAction):
        assert (action, RDF.type, OWL.Class) not in g


def test_no_preferred_or_primary_rights_term() -> None:
    """No gmeow:primary* / gmeow:preferred* rights term (Principle 9)."""
    module = Graph().parse(
        Path(__file__).parents[1] / "ontology" / "modules" / "rights.ttl",
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


def test_wellformed_rights_fixture_conforms() -> None:
    result = run_shacl(_fixture("rights-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_rights_fixture_is_flagged() -> None:
    result = run_shacl(_fixture("rights-malformed"))
    assert not result.ok
    errors = "\n".join(result.errors)
    assert "must govern exactly one asset" in errors
    assert "must regulate exactly one action" in errors
    assert "must have at least one holder" in errors
    assert "must name at least one licensor" in errors
    assert "exactly one mark" in errors


def test_expired_trademark_warns_but_does_not_fail() -> None:
    result = run_shacl(_fixture("rights-expired-warning"))
    assert result.ok, f"warning-only graph must pass; errors: {result.errors}"
    assert any("displayable false" in w for w in result.warnings), result.warnings


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

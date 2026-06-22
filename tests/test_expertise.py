"""Structural + standpoint guards for the expertise module (issue #263).

Exercises the generalised ProficiencyScale / ProficiencyLevel value vocabulary,
the new SkillProficiency relator, credential depth, and the reuse of Attestation
for endorsement/verification.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    """Return the merged ontology graph without imports for fast TBox checks."""
    return load_merged_graph(include_imports=False)


def test_skill_proficiency_is_relator_with_functional_roles() -> None:
    """SkillProficiency is a gufo:Relator and its core roles are functional."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "SkillProficiency"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph
    for role in (
        "skillProficiencyAgent",
        "skillProficiencyOf",
        "skillProficiencyLevel",
        "skillProficiencyScale",
    ):
        node = URIRef(GMEOW + role)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph


def test_skill_proficiency_interval_is_optional() -> None:
    """Interval role exists but is not functional — proficiency may be unbounded."""
    graph = _graph()
    interval = URIRef(GMEOW + "skillProficiencyInterval")
    assert (interval, RDF.type, OWL.ObjectProperty) in graph
    assert (interval, RDF.type, OWL.FunctionalProperty) not in graph


def test_proficiency_scale_is_generalised() -> None:
    """ProficiencyScale is a QualityValue and all expected scales exist."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "ProficiencyScale"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    for scale in (
        "scaleCEFR",
        "scaleILR",
        "scaleACTFL",
        "scaleSelfReported",
        "scaleDreyfus",
        "scaleNIH",
        "scaleAssessed",
    ):
        assert (
            URIRef(GMEOW + scale),
            RDF.type,
            URIRef(GMEOW + "ProficiencyScale"),
        ) in graph


def test_proficiency_levels_carry_scale() -> None:
    """Each proficiency level individual is linked to its parent scale."""
    graph = _graph()
    for level, scale in (
        ("cefrB2", "scaleCEFR"),
        ("dreyfusExpert", "scaleDreyfus"),
        ("nihExpert", "scaleNIH"),
        ("assessedCompetent", "scaleAssessed"),
    ):
        assert (
            URIRef(GMEOW + level),
            URIRef(GMEOW + "levelScale"),
            URIRef(GMEOW + scale),
        ) in graph


def test_dreyfus_scale_and_levels_are_expertise_owned() -> None:
    """The Dreyfus skill-acquisition scale and its five levels are declared in the
    EXPERTISE slice — their domain slice, Dreyfus being a SKILL scale — not the
    languages extension where they were historically mis-homed. This keeps a core
    gmeow:LearningEvent trajectory (gmeow:fromLevel / gmeow:toLevel, #584) that
    references them inside its dependency closure (learning depends on expertise,
    not on the languages extension). Each carries the three mandatory annotations
    and the expertise slice IRI; the five levels point at gmeow:scaleDreyfus."""
    graph = _graph()
    skos_definition = URIRef("http://www.w3.org/2004/02/skos/core#definition")
    expertise_iri = URIRef(GMEOW + "slices/expertise")
    levels = (
        "dreyfusNovice",
        "dreyfusAdvancedBeginner",
        "dreyfusCompetent",
        "dreyfusProficient",
        "dreyfusExpert",
    )
    for name in ("scaleDreyfus", *levels):
        term = URIRef(GMEOW + name)
        assert (term, RDFS.isDefinedBy, expertise_iri) in graph, (
            f"{name} must be defined by the expertise slice (its domain slice)"
        )
        assert (term, RDFS.label, None) in graph, f"{name} missing rdfs:label"
        assert (term, skos_definition, None) in graph, f"{name} missing skos:definition"
    for level in levels:
        assert (
            URIRef(GMEOW + level),
            URIRef(GMEOW + "levelScale"),
            URIRef(GMEOW + "scaleDreyfus"),
        ) in graph, f"{level} must carry gmeow:levelScale gmeow:scaleDreyfus"


def test_credential_properties_exist() -> None:
    """Credential depth properties are declared with correct OWL types."""
    graph = _graph()
    for prop in ("holdsCredential", "credentialIssuer", "credentialFor"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
    occ_cls = URIRef(GMEOW + "occupationClassification")
    assert (occ_cls, RDF.type, OWL.DatatypeProperty) in graph
    assert (occ_cls, RDFS.domain, URIRef(GMEOW + "Occupation")) in graph


def test_credential_issuer_is_organization() -> None:
    """credentialIssuer has range Organization and is functional."""
    graph = _graph()
    issuer = URIRef(GMEOW + "credentialIssuer")
    assert (issuer, RDFS.range, URIRef(GMEOW + "Organization")) in graph
    assert (issuer, RDF.type, OWL.FunctionalProperty) in graph


def test_value_vocabularies_not_subclasses() -> None:
    """Principle 9: scales and levels are individuals, never subclasses."""
    graph = _graph()
    for banned in (
        "SkillProficiencyLevel",
        "DreyfusLevel",
        "NIHLevel",
        "AssessedLevel",
        "CredentialType",
    ):
        node = URIRef(GMEOW + banned)
        assert (node, RDF.type, OWL.Class) not in graph, f"{banned} must not be a class"


def test_no_primary_or_preferred_skill_term() -> None:
    """Principle 9: no single slot wins — no primary/preferred skill selector."""
    graph = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primarySkill",
        "preferredSkill",
        "primaryCredential",
        "preferredCredential",
        "primaryOccupation",
        "preferredOccupation",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in graph, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in graph


def test_endorsement_uses_attestation() -> None:
    """No new skill-endorsement mechanism beyond the existing Attestation relator."""
    graph = _graph()
    assert (URIRef(GMEOW + "Attestation"), RDF.type, OWL.Class) in graph
    # The trust module's endorses stays scoped to agent-to-agent web-of-trust.
    endorses = URIRef(GMEOW + "endorses")
    assert (endorses, RDF.type, OWL.ObjectProperty) in graph
    assert (endorses, RDFS.domain, URIRef(GMEOW + "Agent")) in graph
    # No skill-specific endorsement property should have been minted.
    for banned in ("endorsesSkill", "skillEndorsement", "skillEndorsedBy"):
        node = URIRef(GMEOW + banned)
        for pt in (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty):
            assert (node, RDF.type, pt) not in graph, f"{banned} must not exist"


def test_skill_proficiency_some_values_from_axioms() -> None:
    """SkillProficiency carries EL someValuesFrom restrictions on each role."""
    graph = _graph()
    relator = URIRef(GMEOW + "SkillProficiency")
    # Open-world EL axiomatisation mirrors LanguageProficiency.
    for prop, cls in (
        ("skillProficiencyAgent", "Agent"),
        ("skillProficiencyOf", "Skill"),
        ("skillProficiencyLevel", "ProficiencyLevel"),
        ("skillProficiencyScale", "ProficiencyScale"),
    ):
        restrictions = list(graph.objects(relator, RDFS.subClassOf))
        assert any(
            (rest, OWL.onProperty, URIRef(GMEOW + prop)) in graph
            and (rest, OWL.someValuesFrom, URIRef(GMEOW + cls)) in graph
            for rest in restrictions
        ), f"SkillProficiency missing someValuesFrom {prop} -> {cls}"

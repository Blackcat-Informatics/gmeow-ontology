"""Retained dynamic guards for the expertise module (issue #263).

The asserted-TBox MUST / MUST-NOT invariants have been migrated to declarative
slicetest cells in slices/core/expertise/tests/structural.ttl (#867).

Retained here (cross-slice or dynamic-sweep — not expressible as a module-scoped ASK):
  test_proficiency_scale_is_generalised  — ProficiencyScale and its scale seeds are
      defined in slices/extensions/languages/module.ttl (cross-slice subjects).
  test_proficiency_levels_carry_scale    — cefrB2 / nihExpert / assessedCompetent are
      cross-slice (languages extension); a partial local cell would under-test.
  test_endorsement_uses_attestation      — gmeow:Attestation (attestation slice) and
      gmeow:endorses (trust slice) are both cross-slice subjects.
  test_no_primary_or_preferred_skill_term — sweeps the entire merged graph's property
      set; a module-scoped ASK would silently narrow the scope.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
LOGIC = "https://blackcatinformatics.ca/logic/"


def _graph() -> Graph:
    """Return the merged ontology graph without imports for fast TBox checks."""
    return load_merged_graph(include_imports=False)


def test_proficiency_scale_is_generalised() -> None:
    """ProficiencyScale is a QualityValue and all expected scales exist."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "ProficiencyScale"),
        RDFS.subClassOf,
        URIRef(LOGIC + "QualityValue"),
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

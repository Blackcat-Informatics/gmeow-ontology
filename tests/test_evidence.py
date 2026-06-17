"""Structural + SHACL guards for the evidence / source-typing module (#224).

Tests pin the two orthogonal axes:
- Axis A (evidential warrant): value vocabulary, non-functionality, co-existence.
- Axis B (source typing): independence / tier / coverage depth + notability boolean.
- SHACL: self/private-only warning + notability-requirement violation.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, Literal, Namespace, URIRef
from rdflib.namespace import OWL, RDF, RDFS, XSD

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import ValidationResult
from tests._graph_nt import run_shacl

EVIDENCE_FIXTURES = Path(__file__).parent / "fixtures" / "evidence"

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"

EX = Namespace("https://example.org/test/evidence/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Value vocabulary structure — individuals, never subclasses (Principle 9)
# --------------------------------------------------------------------------- #


def test_evidence_class_is_value_vocabulary() -> None:
    graph = _graph()
    ec = URIRef(GMEOW + "EvidenceClass")
    assert (ec, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
    for seed in (
        "evidenceVERIFIED",
        "evidenceSELF",
        "evidenceANECDOTAL",
        "evidenceRUMOR",
        "evidenceIndependentTradePress",
        "evidencePublicRegistry",
        "evidenceLegalFiling",
        "evidenceOfficialSource",
        "evidenceSelfControlledSite",
        "evidencePrivateScan",
        "evidenceFamilyNarrative",
        "evidenceGeneratedReport",
        "evidenceOcrExtract",
        "evidenceRawArchive",
        "evidencePrivateCorrespondence",
        "evidenceSourceCodeArchive",
        "evidenceNewspaperLead",
    ):
        assert (URIRef(GMEOW + seed), RDF.type, ec) in graph
        assert (URIRef(GMEOW + seed), RDF.type, OWL.Class) not in graph


def test_source_independence_is_value_vocabulary() -> None:
    graph = _graph()
    si = URIRef(GMEOW + "SourceIndependence")
    assert (si, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
    for seed in (
        "sourceIndependenceIndependent",
        "sourceIndependenceSelfOrIssuerOriginated",
    ):
        assert (URIRef(GMEOW + seed), RDF.type, si) in graph
        assert (URIRef(GMEOW + seed), RDF.type, OWL.Class) not in graph


def test_source_tier_is_value_vocabulary() -> None:
    graph = _graph()
    st = URIRef(GMEOW + "SourceTier")
    assert (st, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
    for seed in ("sourceTierPrimary", "sourceTierSecondary", "sourceTierTertiary"):
        assert (URIRef(GMEOW + seed), RDF.type, st) in graph
        assert (URIRef(GMEOW + seed), RDF.type, OWL.Class) not in graph


def test_coverage_depth_is_value_vocabulary() -> None:
    graph = _graph()
    cd = URIRef(GMEOW + "CoverageDepth")
    assert (cd, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
    for seed in (
        "coverageDepthSignificantCoverage",
        "coverageDepthPassingMention",
        "coverageDepthRoutineFiling",
    ):
        assert (URIRef(GMEOW + seed), RDF.type, cd) in graph
        assert (URIRef(GMEOW + seed), RDF.type, OWL.Class) not in graph


# --------------------------------------------------------------------------- #
# Property structure — domains, ranges, functionality
# --------------------------------------------------------------------------- #


def test_has_evidence_class_is_non_functional() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "hasEvidenceClass")
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDFS.domain, URIRef(GMEOW + "CitationAct")) in graph
    assert (prop, RDFS.range, URIRef(GMEOW + "EvidenceClass")) in graph
    # Non-functional: competing classifications coexist (Principle 9).
    assert (prop, RDF.type, OWL.FunctionalProperty) not in graph


def test_source_typing_properties_exist() -> None:
    graph = _graph()
    for prop_name, range_name in (
        ("sourceIndependence", "SourceIndependence"),
        ("sourceTier", "SourceTier"),
        ("coverageDepth", "CoverageDepth"),
    ):
        prop = URIRef(GMEOW + prop_name)
        assert (prop, RDF.type, OWL.ObjectProperty) in graph
        assert (prop, RDFS.domain, URIRef(GMEOW + "CitationAct")) in graph
        assert (prop, RDFS.range, URIRef(GMEOW + range_name)) in graph
        # Non-functional: competing assessments coexist (Principle 9).
        assert (prop, RDF.type, OWL.FunctionalProperty) not in graph


def test_supports_notability_is_boolean_datatype_property() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "supportsNotability")
    assert (prop, RDF.type, OWL.DatatypeProperty) in graph
    assert (prop, RDFS.domain, URIRef(GMEOW + "CitationAct")) in graph
    assert (prop, RDFS.range, XSD.boolean) in graph


def _make_citation_act(graph: Graph, uri: URIRef) -> None:
    """Add a minimally well-formed CitationAct so evidence SHACL tests do not
    fail on unrelated CitationAct cardinality constraints from #211."""
    graph.add((uri, RDF.type, URIRef(GMEOW + "CitationAct")))
    graph.add((uri, URIRef(GMEOW + "citingEntity"), EX.claim))
    graph.add((uri, URIRef(GMEOW + "citedEntity"), EX.sourceWork))
    graph.add(
        (
            uri,
            URIRef(GMEOW + "citationIntent"),
            URIRef(GMEOW + "intentCitesAsDataSource"),
        )
    )


# --------------------------------------------------------------------------- #
# SHACL — self/private-only warning shape
# --------------------------------------------------------------------------- #


def test_self_private_evidence_triggers_warning() -> None:
    g = Graph()
    _make_citation_act(g, EX.citation)
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "hasEvidenceClass"),
            URIRef(GMEOW + "evidenceSELF"),
        )
    )
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "sourceIndependence"),
            URIRef(GMEOW + "sourceIndependenceSelfOrIssuerOriginated"),
        )
    )
    result = run_shacl(g)
    # The shape fires a Warning, not a Violation, so the overall result is still ok
    # but we should see the warning in the report.
    assert result.ok, f"Warning-only graph must pass; errors: {result.errors}"
    assert any("self-asserted or private evidence" in w for w in result.warnings), (
        result.warnings
    )


def test_mixed_evidence_does_not_trigger_self_private_warning() -> None:
    g = Graph()
    _make_citation_act(g, EX.citation)
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "hasEvidenceClass"),
            URIRef(GMEOW + "evidenceSELF"),
        )
    )
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "hasEvidenceClass"),
            URIRef(GMEOW + "evidenceIndependentTradePress"),
        )
    )
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "sourceIndependence"),
            URIRef(GMEOW + "sourceIndependenceSelfOrIssuerOriginated"),
        )
    )
    result = run_shacl(g)
    assert result.ok, f"Mixed evidence graph must pass; errors: {result.errors}"
    assert not any("self-asserted or private evidence" in w for w in result.warnings), (
        "Mixed evidence should not trigger the self/private-only warning"
    )


# --------------------------------------------------------------------------- #
# SHACL — notability requirement violation shape
# --------------------------------------------------------------------------- #


def test_notability_without_triad_triggers_violation() -> None:
    g = Graph()
    _make_citation_act(g, EX.citation)
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "supportsNotability"),
            Literal("true", datatype=XSD.boolean),
        )
    )
    # Missing sourceTier and coverageDepth.
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "sourceIndependence"),
            URIRef(GMEOW + "sourceIndependenceIndependent"),
        )
    )
    result = run_shacl(g)
    assert not result.ok, "Expected a notability-requirement violation"
    assert any("WP:GNG triad" in e for e in result.errors), (
        f"Expected a notability-requirement violation message; got: {result.errors}"
    )


def test_notability_with_full_triad_passes() -> None:
    g = Graph()
    _make_citation_act(g, EX.citation)
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "supportsNotability"),
            Literal("true", datatype=XSD.boolean),
        )
    )
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "sourceIndependence"),
            URIRef(GMEOW + "sourceIndependenceIndependent"),
        )
    )
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "sourceTier"),
            URIRef(GMEOW + "sourceTierSecondary"),
        )
    )
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "coverageDepth"),
            URIRef(GMEOW + "coverageDepthSignificantCoverage"),
        )
    )
    result = run_shacl(g)
    assert result.ok, "Full triad should pass notability requirement"


def test_notability_false_does_not_require_triad() -> None:
    g = Graph()
    _make_citation_act(g, EX.citation)
    g.add(
        (
            EX.citation,
            URIRef(GMEOW + "supportsNotability"),
            Literal("false", datatype=XSD.boolean),
        )
    )
    # No tier, no coverage, no independence asserted.
    result = run_shacl(g)
    assert result.ok, "supportsNotability false should not require the triad"


# --------------------------------------------------------------------------- #
# No inferential bridges — evidence properties must not imply truth or trust
# --------------------------------------------------------------------------- #


def test_evidence_properties_do_not_imply_truth() -> None:
    graph = _graph()
    for prop in (
        "hasEvidenceClass",
        "sourceIndependence",
        "sourceTier",
        "coverageDepth",
    ):
        prop_node = URIRef(GMEOW + prop)
        for banned in ("observationResult", "trustor", "trustee", "endorses"):
            banned_node = URIRef(GMEOW + banned)
            assert (prop_node, RDFS.subPropertyOf, banned_node) not in graph
            assert (banned_node, RDFS.subPropertyOf, prop_node) not in graph
            assert (prop_node, OWL.equivalentProperty, banned_node) not in graph
            assert (banned_node, OWL.equivalentProperty, prop_node) not in graph


# --------------------------------------------------------------------------- #
# Fixture-based Cogneto cases — reusable documentation for projection work
# --------------------------------------------------------------------------- #


def _load_evidence_fixture() -> Graph:
    return Graph().parse(EVIDENCE_FIXTURES / "cogneto-cases.ttl", format="turtle")


def _has_message_for_node(
    result: ValidationResult,
    node: URIRef,
    substring: str,
    *,
    bucket: str = "warnings",
) -> bool:
    """Check whether a SHACL result targets *node* and contains *substring*."""
    lines: list[str] = []
    if bucket == "warnings":
        for block in result.warnings:
            lines.extend(block.splitlines())
    else:
        for block in result.errors:
            lines.extend(block.splitlines())
    prefix = f"{node}:"
    return any(line.startswith(prefix) and substring in line for line in lines)


def test_infoworld_citation_passes() -> None:
    """InfoWorld = independent secondary significant coverage → supports notability."""
    g = _load_evidence_fixture()
    result = run_shacl(g)
    assert result.ok, f"InfoWorld citation should pass; errors: {result.errors}"
    assert not _has_message_for_node(
        result, EX.InfoWorldCognetoCitation, "self-asserted or private evidence"
    ), "InfoWorld should not trigger the self/private-only warning"


def test_orgbook_citation_passes() -> None:
    """OrgBook = official primary routine filing → factual verification only."""
    g = _load_evidence_fixture()
    result = run_shacl(g)
    assert result.ok, f"OrgBook citation should pass; errors: {result.errors}"
    assert not _has_message_for_node(
        result, EX.OrgBookCognetoCitation, "self-asserted or private evidence"
    ), "OrgBook should not trigger the self/private-only warning"


def test_private_contract_triggers_self_private_warning() -> None:
    """Private contract = self-originated private scan → Warning (Principle 10)."""
    g = _load_evidence_fixture()
    result = run_shacl(g)
    # Warning-only graphs still pass (result.ok is True).
    assert result.ok, (
        "Private-contract citation should pass at Warning severity;"
        f" errors: {result.errors}"
    )
    assert _has_message_for_node(
        result, EX.PrivateCognetoContractCitation, "self-asserted or private evidence"
    ), "Private contract should trigger the self/private-only warning"


def test_orgbook_notability_mutation_triggers_violation() -> None:
    """Flip OrgBook supportsNotability to true → Violation (primary ≠ secondary)."""
    g = _load_evidence_fixture()
    orgbook = EX.OrgBookCognetoCitation
    # Remove supportsNotability false, add true.
    g.remove((orgbook, URIRef(GMEOW + "supportsNotability"), Literal(False)))
    g.add((orgbook, URIRef(GMEOW + "supportsNotability"), Literal(True)))
    result = run_shacl(g)
    assert not result.ok, (
        "OrgBook with supportsNotability true should trigger a notability violation"
    )
    assert _has_message_for_node(
        result, EX.OrgBookCognetoCitation, "WP:GNG triad", bucket="errors"
    ), f"Expected WP:GNG triad violation; got: {result.errors}"

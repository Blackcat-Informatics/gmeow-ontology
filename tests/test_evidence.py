"""SHACL guards for the evidence / source-typing module (#224).

Structural TBox assertions (value vocabulary structure, property domains/ranges,
non-functionality, no-truth-bridge) have been migrated to the declarative
slicetest DSL in slices/core/evidence/tests/structural.ttl (#867).

Retained here: run_shacl() ExampleConformance tests and fixture-based mutation
tests that cannot be expressed as module-scoped SPARQL ASK cells.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import RDF, XSD

from gmeow_tools.validate import ValidationResult
from tests._graph_nt import run_shacl

EVIDENCE_FIXTURES = Path(__file__).parent / "fixtures" / "evidence"

GMEOW = "https://blackcatinformatics.ca/gmeow/"

EX = Namespace("https://example.org/test/evidence/")


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

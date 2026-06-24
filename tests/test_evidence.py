"""SHACL guards for the evidence / source-typing module (#224).

Structural TBox assertions (value vocabulary structure, property domains/ranges,
non-functionality, no-truth-bridge) have been migrated to the declarative
slicetest DSL in slices/core/evidence/tests/structural.ttl (#867).

Retained here: fixture-based mutation tests that cannot be expressed as
module-scoped SPARQL ASK cells.  The five inline-graph run_shacl() tests have
been migrated to crates/validate/tests/conformance_evidence.rs (#867).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, Literal, Namespace, URIRef

from gmeow_tools.validate import ValidationResult
from tests._graph_nt import run_shacl

EVIDENCE_FIXTURES = Path(__file__).parent / "fixtures" / "evidence"

GMEOW = "https://blackcatinformatics.ca/gmeow/"

EX = Namespace("https://example.org/test/evidence/")


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

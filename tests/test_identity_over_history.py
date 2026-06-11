"""Tests for issue #234 — identity over immutable history (the .mailmap model).

Demonstrates Constitution Principles 9 and 10 in the software domain:
- a contributor transition keeps both old and new identities co-equal;
- the old identity is suppressed (`displayable false`), not deleted;
- a `.mailmap` projection generates canonical + suppressed mapping lines;
- an AI agent is a first-class `SoftwareAgent` whose authorship claim carries
  statement-level confidence and self-assertion metadata.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import OWL, RDF, Graph, Literal, Namespace

from gmeow_tools.config import STATEMENT_OWL_FILE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.projections import project_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
EX = "https://blackcatinformatics.ca/gmeow/examples/"
GM = Namespace(GMEOW)
EX_NS = Namespace(EX)

FIXTURE = Path(__file__).parent / "fixtures" / "coverage" / "identity-over-history.ttl"
STATEMENT_OWL = STATEMENT_OWL_FILE


def _fixture_graph() -> Graph:
    """The merged ontology plus the issue #234 fixture."""
    g = load_merged_graph(include_imports=False)
    g.parse(FIXTURE, format="turtle")
    return g


def test_contributor_transition_preserves_both_identities() -> None:
    """Eve and Evan coexist; the historical AuthorIdentity is not erased."""
    g = _fixture_graph()
    assert (EX_NS.evanIdentity, RDF.type, GM.AuthorIdentity) in g
    assert (EX_NS.evanIdentity, GM.displayable, Literal(False)) in g
    assert (EX_NS.eveName, GM.displayable, Literal(True)) in g
    assert (EX_NS.transitionCommit, GM.commitAuthorIdentity, EX_NS.evanIdentity) in g
    assert (EX_NS.transitionCommit, GM.authoredBy, EX_NS.eve) in g
    # Principle 9: co-equal, not merged.
    assert (EX_NS.eve, OWL.sameAs, EX_NS.evanIdentity) not in g


def test_mailmap_projection_emits_canonical_and_suppressed_lines() -> None:
    """The mailmap profile emits the canonical line plus a suppressed remapping."""
    g = project_graph("mailmap", _fixture_graph())
    entries = {str(o) for o in g.objects(None, GM.mailmapEntry)}
    mappings = {str(o) for o in g.objects(None, GM.projectedMailmapMapping)}
    assert "Eve <eve@example.com>" in entries, f"canonical missing: {entries}"
    assert "Eve <eve@example.com> Evan <evan@example.com>" in mappings, (
        f"suppressed mapping missing: {mappings}"
    )


def test_ai_author_is_software_agent_with_statement_metadata() -> None:
    """GitHub-Copilot-Bot is a SoftwareAgent; the authoredBy claim is annotated."""
    fixture = _fixture_graph()
    assert (EX_NS.copilot, RDF.type, GM.SoftwareAgent) in fixture
    assert (EX_NS.aiCommit, GM.authoredBy, EX_NS.copilot) in fixture

    statements = Graph().parse(STATEMENT_OWL, format="turtle")
    axiom = None
    for ax in statements.subjects(RDF.type, OWL.Axiom):
        if (
            (ax, OWL.annotatedSource, EX_NS.aiCommit) in statements
            and (ax, OWL.annotatedProperty, GM.authoredBy) in statements
            and (ax, OWL.annotatedTarget, EX_NS.copilot) in statements
        ):
            axiom = ax
            break
    assert axiom is not None, (
        "OWL axiom for AI authorship not found in compiled statements"
    )

    confidences = list(statements.objects(axiom, GM.confidence))
    assert len(confidences) == 1, f"expected one confidence, got {confidences}"
    assert float(str(confidences[0])) == 0.9

    self_asserted = list(statements.objects(axiom, GM.selfAsserted))
    assert len(self_asserted) == 1, f"expected one selfAsserted, got {self_asserted}"
    assert self_asserted[0] == Literal(True)


def test_suppressed_identity_passes_shacl() -> None:
    """A suppressed contributor identity is retained and valid."""
    g = Graph().parse(FIXTURE, format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    assert (EX_NS.evanIdentity, RDF.type, GM.AuthorIdentity) in g
    assert (EX_NS.evanIdentity, GM.displayable, Literal(False)) in g

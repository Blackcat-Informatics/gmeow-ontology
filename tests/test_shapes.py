"""Closed-world SHACL data-shape tests (#39, epic #35).

The hybrid OWL+SHACL architecture's pure-Python, always-on negative lane: it
proves the relator/suppression/orthogonality shapes in shapes/gmeow-shapes.ttl
catch a malformed data graph and pass a well-formed one (CONSTITUTION P7/P9/P10).
The Docker ROBOT ``verify`` lane (reasoned-graph QC) and the HermiT inconsistency
lane live in ``tests/test_reasoning_entailments.py``; SHACL here is the
closed-world counterpart that needs no reasoner. See docs/reasoning.md.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import RDF, Graph, Namespace

from gmeow_tools.validate import run_shacl

SHAPES_FIXTURES = Path(__file__).parent / "fixtures" / "shapes"
GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test/")


def _fixture(name: str) -> Graph:
    return Graph().parse(SHAPES_FIXTURES / f"{name}.ttl", format="turtle")


def test_wellformed_relator_fixture_conforms() -> None:
    """A well-formed data graph passes every closed-world shape (AC#1 positive)."""
    result = run_shacl(_fixture("relator-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_relator_fixture_is_flagged() -> None:
    """A malformed data graph is rejected, and each shape names its violation (AC#1)."""
    result = run_shacl(_fixture("relator-malformed"))
    assert not result.ok
    report = "\n".join(result.errors)
    # Relator well-formedness (exactly-one cardinality), both min and max ends.
    assert "exactly one gmeow:Gender value" in report
    assert "must use exactly one appellation" in report
    # Suppression contract (Principle 10) and orthogonality (Principle 9).
    assert "should set gmeow:displayable false" in report
    assert "may fill at most one identity axis" in report


def test_orthogonality_data_check_rejects_two_axes() -> None:
    """The closed-world dual of HermiT's two-axis inconsistency test.

    A single node typed in two disjoint identity axes is caught by SHACL without a
    reasoner — the counterpart of
    test_reasoning_entailments.test_two_axis_individual_is_inconsistent.
    """
    bad = Graph()
    bad.add((EX.x, RDF.type, GMEOW.GenderIdentity))
    bad.add((EX.x, RDF.type, GMEOW.SexualOrientation))
    result = run_shacl(bad)
    assert not result.ok
    assert "may fill at most one identity axis" in "\n".join(result.errors)


def test_wellformed_facet_cardinality_passes() -> None:
    """A lone facet with exactly one value conforms (cardinality-shape control)."""
    ok = Graph()
    ok.add((EX.f, RDF.type, GMEOW.GenderIdentity))
    ok.add((EX.f, GMEOW.genderValue, GMEOW.genderNonBinary))
    assert run_shacl(ok).ok

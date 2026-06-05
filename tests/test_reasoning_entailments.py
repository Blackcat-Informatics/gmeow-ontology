"""Entailment & consistency tests for the axiomatized doctrine (#38).

Phase 2 of the reasoning-depth epic (#35) turns invariants that were only *tested*
in Python over the asserted graph into reasoner *theorems*. This module proves
they actually bite, using two reasoners for the two halves of the OWL-infers /
SHACL-validates split (Principle 8):

* **owlrl** (pure-Python OWL 2 RL) materializes POSITIVE entailments — derived
  ancestry, location-through-containment, sub-organization transitivity — fast,
  Docker-free, on every run. Each test loads the *real authored* module so it
  pins the shipped axioms, not a hand-built fixture.
* **HermiT** (via the pinned ROBOT image) is the SOUND authority for
  INCONSISTENCY: it proves the disjointness axioms reject an individual placed in
  two axes / two Kinds, and that the real ontology plus the worked example
  fixtures stay coherent under the broad disjointness. These are marked
  ``docker`` and skipped when the image is absent — but never silently passed:
  CI's reasoning job runs them for real.
"""

from __future__ import annotations

import owlrl
import pytest
from rdflib import RDF, Graph, Namespace
from rdflib.term import Node

from gmeow_tools.config import DIST_DIR, FIXTURES_DIR, ONTOLOGY_DIR, ROBOT_IMAGE
from gmeow_tools.reason import MERGED_FILE, merge_release, reason, verify
from gmeow_tools.runner import ToolExecutionError, image_available

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test/")

requires_robot = pytest.mark.skipif(
    not image_available(ROBOT_IMAGE), reason="pinned ROBOT image not present locally"
)


# --------------------------------------------------------------------------- #
# Positive entailments — owlrl (pure Python, no Docker)
# --------------------------------------------------------------------------- #


def _materialize(module: str, *abox: tuple[Node, Node, Node]) -> Graph:
    """Close a real authored module + a tiny A-Box under OWL 2 RL."""
    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / f"{module}.ttl", format="turtle")
    for triple in abox:
        graph.add(triple)
    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    return graph


def test_ancestry_is_derived_not_asserted() -> None:
    """hasParent ∘ hasParent ⊑ hasAncestor (transitive sub-property), DERIVED."""
    graph = _materialize(
        "genealogy",
        (EX.a, GMEOW.hasParent, EX.b),
        (EX.b, GMEOW.hasParent, EX.c),
    )
    # The grandparent edge is asserted nowhere yet is entailed.
    assert (EX.a, GMEOW.hasAncestor, EX.c) in graph
    # Parentage feeds ancestry; the transitive inverse closes descendants too.
    assert (EX.a, GMEOW.hasAncestor, EX.b) in graph
    assert (EX.c, GMEOW.hasDescendant, EX.a) in graph


def test_location_propagates_through_containment() -> None:
    """locatedAt ∘ containedInPlace ⊑ locatedAt: in your room means in your city."""
    graph = _materialize(
        "places",
        (EX.thing, GMEOW.locatedAt, EX.room),
        (EX.room, GMEOW.containedInPlace, EX.city),
    )
    assert (EX.thing, GMEOW.locatedAt, EX.city) in graph


def test_suborganization_is_transitive() -> None:
    """subOrganizationOf is transitive — a team is part of the parent company."""
    graph = _materialize(
        "organization",
        (EX.team, GMEOW.subOrganizationOf, EX.div),
        (EX.div, GMEOW.subOrganizationOf, EX.corp),
    )
    assert (EX.team, GMEOW.subOrganizationOf, EX.corp) in graph


# --------------------------------------------------------------------------- #
# Negative & coherence — HermiT (sound, via Docker/ROBOT)
# --------------------------------------------------------------------------- #


def _is_consistent(extra: Graph, name: str, *, reasoner: str = "hermit") -> bool:
    """Whether the merged ontology + ``extra`` is consistent under ``reasoner``.

    A genuine logical inconsistency returns ``False``; any OTHER tool failure is
    re-raised, so a tooling problem can never masquerade as a clean (in)consistency
    verdict.
    """
    if not MERGED_FILE.exists():
        merge_release(MERGED_FILE)
    graph = Graph()
    graph.parse(MERGED_FILE, format="turtle")
    graph += extra
    out = DIST_DIR / f"test-{name}.ttl"
    out.parent.mkdir(parents=True, exist_ok=True)  # survive a fresh `make clean`
    graph.serialize(destination=out, format="turtle")
    try:
        reason(reasoner=reasoner, merged=out)
        return True
    except ToolExecutionError as exc:
        text = str(exc).lower()
        if "inconsist" in text or "unsatisf" in text:
            return False
        raise  # e.g. an unsupported-datatype tool error — fail loudly, do not mask
    finally:
        out.unlink(missing_ok=True)


@pytest.mark.docker
@requires_robot
def test_two_axis_individual_is_inconsistent() -> None:
    """One individual in two disjoint identity axes is rejected by HermiT."""
    bad = Graph()
    bad.add((EX.x, RDF.type, GMEOW.GenderIdentity))
    bad.add((EX.x, RDF.type, GMEOW.GenderExpression))
    assert not _is_consistent(bad, "two-axis"), (
        "a GenderIdentity that is also a GenderExpression must be inconsistent"
    )


@pytest.mark.docker
@requires_robot
def test_two_kind_individual_is_inconsistent() -> None:
    """One individual in two disjoint ultimate Kinds (Person, Organization) is bad."""
    bad = Graph()
    bad.add((EX.y, RDF.type, GMEOW.Person))
    bad.add((EX.y, RDF.type, GMEOW.Organization))
    assert not _is_consistent(bad, "two-kind")


@pytest.mark.docker
@requires_robot
def test_well_formed_individual_stays_consistent() -> None:
    """The control: a normal person with one gender-identity facet is coherent."""
    ok = Graph()
    ok.add((EX.p, RDF.type, GMEOW.Person))
    ok.add((EX.p, GMEOW.hasGenderIdentity, EX.fac))
    ok.add((EX.fac, RDF.type, GMEOW.GenderIdentity))
    ok.add((EX.fac, GMEOW.genderValue, GMEOW.genderNonBinary))
    assert _is_consistent(ok, "well-formed")


@pytest.mark.docker
@requires_robot
def test_verify_queries_pass_on_clean_ontology() -> None:
    """The reasoned-graph QC lane: ROBOT verify finds no violations on the real
    ontology.

    ``reason.verify`` materializes the merged ontology and runs every
    queries/verify/*.rq SELECT over it; any returned row is a violation and ROBOT
    exits non-zero (raising ToolExecutionError). A clean run is the smoke test
    that the closed-world QC queries — meta-grounding completeness, orthogonality
    integrity, no class subclassing two disjoint axes — hold. See docs/reasoning.md.
    """
    report = verify()  # raises ToolExecutionError if any query returns rows
    assert "PASS" in report
    assert "FAIL" not in report


@pytest.mark.docker
@requires_robot
def test_worked_fixtures_stay_coherent_under_disjointness() -> None:
    """Broad disjointness must not turn the worked example data inconsistent.

    Checked with ELK, not HermiT: the fixtures carry xsd:date data values, which
    HermiT rejects (not in its OWL 2 datatype map) before it can rule on logic.
    ELK ignores datatypes and is complete for DisjointClasses, so it is the right
    tool to confirm no individual lands in two disjoint classes here.
    """
    fixtures = Graph()
    fixture_files = list(FIXTURES_DIR.rglob("*.ttl"))
    assert fixture_files, f"no fixtures found in {FIXTURES_DIR}"  # never pass vacuously
    for ttl in sorted(fixture_files):
        fixtures.parse(ttl, format="turtle")
    assert _is_consistent(fixtures, "fixtures", reasoner="ELK")

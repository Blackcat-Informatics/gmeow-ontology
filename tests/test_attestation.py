"""Structural + DL-safety guards for the cross-cutting attestation module.

TBox invariants that are home-asserted in slices/core/attestation/module.ttl
have been migrated to slices/core/attestation/tests/structural.ttl as
declarative slicetest cells (#867). Only the tests below were RETAINED because
they are not expressible as module-scoped SPARQL ASK cells:

- test_certification_still_exists_as_relator: gmeow:Certification is not
  defined in the attestation module; it is a cross-slice subject that
  scopeModule cannot reach.
- test_contested_attestation_coexists: loads a coverage fixture file and
  calls run_shacl(); an ABox multi-file conformance check, not a TBox cell.
- test_all_fixture_files_load: calls run_shacl() on 6 fixture files;
  ExampleConformance, not a structural TBox assertion.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"

EX_ATTEST = Namespace("https://blackcatinformatics.ca/gmeow/examples/attestation/")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Certification preserved and documented (cross-slice -- Certification is NOT
# in slices/core/attestation/module.ttl so scopeModule cannot cover it)
# --------------------------------------------------------------------------- #


def test_certification_still_exists_as_relator() -> None:
    graph = _graph()
    cert = URIRef(GMEOW + "Certification")
    assert (cert, RDF.type, OWL.Class) in graph
    assert (cert, RDFS.subClassOf, URIRef(GUFO + "Relator")) in graph
    # Documentation scopeNote should mention attestation.
    assert (cert, RDFS.comment, None) in graph or (cert, RDFS.label, None) in graph


# --------------------------------------------------------------------------- #
# Standpoint coexistence -- contested attestations (run_shacl + ABox fixture)
# --------------------------------------------------------------------------- #


def test_contested_attestation_coexists() -> None:
    """A contested attestation: one standpoint affirms, another refutes.
    Both claims load, SHACL-pass, and are retained."""
    g = Graph().parse(COVERAGE_FIXTURES / "attestation-vc.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    # The attestation itself exists.
    assert (EX_ATTEST.vcAttestation, RDF.type, URIRef(GMEOW + "Attestation")) in g
    # Both standpoint axioms coexist: affirmation and refutation.
    assert (EX_ATTEST.claimAffirmed, RDF.type, OWL.Axiom) in g
    assert (EX_ATTEST.claimRefuted, RDF.type, OWL.Axiom) in g


# --------------------------------------------------------------------------- #
# Fixture coverage -- all 6 scenarios load and SHACL-pass
# --------------------------------------------------------------------------- #

FIXTURES = [
    "attestation-software-release.ttl",
    "attestation-vc.ttl",
    "attestation-email-reuse.ttl",
    "attestation-quality-report.ttl",
    "attestation-blockchain-claim.ttl",
    "attestation-ledger-evidence.ttl",
]


def test_all_fixture_files_load() -> None:
    for name in FIXTURES:
        path = COVERAGE_FIXTURES / name
        assert path.exists(), f"missing fixture {name}"
        g = Graph().parse(path, format="turtle")
        result = run_shacl(g)
        assert result.ok, f"{name} failed SHACL:\n" + "\n".join(result.errors)

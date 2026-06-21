"""UFO/OntoUML anti-pattern checks over the meta-grounded ontology.

Every GMEOW class records two orthogonal gUFO facets: its **nature** — what an
instance *is* — via ``rdfs:subClassOf`` into the gUFO individual taxonomy
(``gufo:FunctionalComplex``, ``gufo:Relator``, ``gufo:Event`` …); and its
**stereotype** — the type's identity/rigidity status — via ``rdf:type`` into the
gUFO ``gufo:EndurantType`` / ``gufo:EventType`` / ``gufo:SituationType`` taxonomy
(OWL 2 punning; see ``imports/gufo.ttl`` ~line 1427). This module checks the
structural discipline the stereotype facet licenses, exactly the role the
native ``gmeow_validate.check_statement_invariants`` engine plays for the
statement layer and :mod:`gmeow_tools.projection_lint` for the projection stack.

The reasoning logic itself lives in the Rust ``gmeow_validate`` extension (#579):
the transitive ``rdfs:subClassOf`` closure, the ``owl:AllDisjointClasses`` /
``owl:members`` RDF-Collection walk, and the subPropertyOf/equivalentProperty
property-bridge DFS all run over an oxigraph store, with byte-exact parity to the
former pure-Python checks. Each function below is a thin shim that accepts the
graph **as N-Triples text** and routes it through the matching Rust entry point —
no graph object is constructed in this module (#579). The production
``validate_all`` path builds the merged N-Triples in Rust from file paths; tests
that hand-build a synthetic graph serialize it to N-Triples first.

The checks (each cites the OntoUML anti-pattern it guards; catalogue:
https://ontouml.readthedocs.io/en/latest/anti-patterns/):

* :func:`exactly_one_stereotype` — every GMEOW class carries exactly one gUFO
  meta-class (the precondition for every check below).
* :func:`identity_overlap` — **MixIden**: a sortal inherits identity from exactly
  one ``gufo:Kind``; no ``Kind`` specializes another ``Kind``.
* :func:`anti_rigidity_discipline` — **MixRig / FreeRole**: an anti-rigid sortal
  (``Role`` / ``Phase``) specializes a rigid sortal, and no rigid type specializes
  an anti-rigid one.
* :func:`relator_mediation` — **RelComp**: a concrete ``gufo:Relator`` mediates at
  least two relata.
* :func:`coequal_facet_orthogonality` — **Principle 9 (#281)**: co-equal facet
  axes stay orthogonal.
* :func:`frame_declaration_completeness` — **Principle 11 (#283)**: frame-pointing
  property carrier classes declare ``gmeow:requiresFrame``.
"""

from __future__ import annotations

from collections.abc import Callable

import gmeow_validate

from gmeow_tools.config import NAMESPACE


def _run(check: Callable[[str, str], dict[str, list[str]]], data_nt: str) -> list[str]:
    """Route an N-Triples graph through one Rust reasoning *check*."""
    report = check(data_nt, str(NAMESPACE))
    return list(report["errors"])


def exactly_one_stereotype(data_nt: str) -> list[str]:
    """Every GMEOW class must be punned with exactly one gUFO meta-class."""
    return _run(gmeow_validate.reasoning_exactly_one_stereotype_nt, data_nt)


def identity_overlap(data_nt: str) -> list[str]:
    """MixIden: a sortal inherits identity from exactly one Kind; no Kind ⊑ Kind."""
    return _run(gmeow_validate.reasoning_identity_overlap_nt, data_nt)


def anti_rigidity_discipline(data_nt: str) -> list[str]:
    """MixRig / FreeRole: anti-rigid sortals need a rigid super; rigid avoid them."""
    return _run(gmeow_validate.reasoning_anti_rigidity_discipline_nt, data_nt)


def relator_mediation(data_nt: str) -> list[str]:
    """RelComp: every concrete gufo:Relator mediates at least two relata."""
    return _run(gmeow_validate.reasoning_relator_mediation_nt, data_nt)


def coequal_facet_orthogonality(data_nt: str) -> list[str]:
    """Principle 9 by annotation (#281): annotated axes stay orthogonal."""
    return _run(gmeow_validate.reasoning_coequal_facet_orthogonality_nt, data_nt)


def frame_declaration_completeness(data_nt: str) -> list[str]:
    """Principle 11 by annotation (#283): the "did you forget the frame?" guard."""
    return _run(gmeow_validate.reasoning_frame_declaration_completeness_nt, data_nt)


def reasoning_invariants(data_nt: str) -> list[str]:
    """Run every UFO anti-pattern check; an empty list means the graph is clean."""
    return _run(gmeow_validate.reasoning_invariants_nt, data_nt)

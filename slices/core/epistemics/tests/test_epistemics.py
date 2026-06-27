# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Epistemics slice — the bespoke checks not expressible in the declarative DSL.

Most of this slice's structural invariants — the keystone entailment
``knowsThat ⊑ believes``, the no-truth-bit rule, the open-range doxastic spine,
the justification term shapes — were migrated to slice-resident test-DSL data
and now run in the native Rust slice-test harness (crates/slicetest):

* ``tests/structural.ttl`` — the MUST / MUST-NOT structural assertions,
* ``tests/competency.ttl`` — the agent-kind and contribution-role competency
  questions (also covers the global ``tests/test_competency.py`` versions), and
* ``tests/example-conformance.ttl`` — the flagship example and the
  missing-agent counter-example.

See ``dsl/tests/MIGRATION-LEDGER.md`` for the per-test mapping. What remains here
is the genuinely bespoke residue that the declarative DSL cannot express:
exact ``owl:unionOf`` set membership, the numeric/temporal suppression
round-trip over example data, the annotation-completeness sweep over
dynamically-discovered individuals, and a generated-artifact (SSSOM TSV) check.
"""

from __future__ import annotations

import csv
from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, Literal, URIRef
from gmeow_rdf.compat.rdflib.collection import Collection
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS
from gmeow_rdf.compat.rdflib.term import Node

GMEOW = "https://blackcatinformatics.ca/gmeow/"
SKOS_DEFINITION = URIRef("http://www.w3.org/2004/02/skos/core#definition")
_MODULE = Path(__file__).resolve().parents[1] / "module.ttl"
_SPINE = ("believes", "doubts", "suspendsJudgementOn", "accepts", "knowsThat")


def _t(name: str) -> URIRef:
    """A gmeow-namespaced term URI."""
    return URIRef(GMEOW + name)


def _graph() -> Graph:
    g = Graph()
    g.parse(_MODULE, format="turtle")
    return g


def _union_members(g: Graph, expr: Node) -> set[URIRef]:
    """Return the URIs inside an owl:unionOf class expression, if any.

    Handles both direct union expressions and unions nested via
    owl:equivalentClass (used for schema-friendly named union classes).
    """
    list_node = g.value(expr, OWL.unionOf)
    if list_node is None:
        equivalent = g.value(expr, OWL.equivalentClass)
        if equivalent is not None:
            list_node = g.value(equivalent, OWL.unionOf)
        if list_node is None:
            return set()
    return {member for member in Collection(g, list_node) if isinstance(member, URIRef)}


def test_justified_by_union_membership() -> None:
    """The EXACT owl:unionOf membership of justifiedBy's named domain and range.

    The ObjectProperty / domain / range / non-functional clauses of the former
    test_justified_by_has_named_domain_and_range migrated to the declarative
    ex:saJustifiedByDomainRange and ex:saJustifiedByNotFunctional structural
    assertions. The exact-set membership of the owl:unionOf class expressions —
    which the test-DSL's ASK patterns cannot express as "these members and no
    others" — stays here (Principle 9 keeps justifiedBy non-functional so several
    grounds may coexist).
    """
    g = _graph()
    subject_union = _union_members(g, _t("JustificationSubject"))
    assert subject_union == {_t("DoxasticState"), _t("StandpointClaim")}

    ground_union = _union_members(g, _t("JustificationGround"))
    assert ground_union == {_t("EvidenceSpan"), _t("Attestation"), _t("DoxasticState")}


def test_justification_terms_are_annotated() -> None:
    """Annotation-completeness for the justification terms (Principle 8).

    Retained in pytest: the sweep over *dynamically discovered* JustificationStatus
    individuals (g.subjects(...)) is a universal over an open set the declarative
    ASK form does not express; it also backstops the make-validate annotation
    contract for these specific terms.
    """
    g = _graph()
    for name in (
        "DoxasticStandpointClaim",
        "claimOfBelief",
        "justifiedBy",
        "defeatedBy",
        "JustificationStatus",
        "JustificationSubject",
        "JustificationGround",
    ):
        term = _t(name)
        assert (term, RDFS.label, None) in g
        assert (term, SKOS_DEFINITION, None) in g
        assert (term, RDFS.isDefinedBy, None) in g

    statuses = list(g.subjects(RDF.type, _t("JustificationStatus")))
    assert statuses, "expected at least one JustificationStatus individual to sweep"
    for status in statuses:
        assert (status, RDFS.label, None) in g
        assert (status, SKOS_DEFINITION, None) in g
        assert (status, RDFS.isDefinedBy, None) in g


def test_every_term_is_annotated() -> None:
    """Annotation-completeness for the slice's own core terms (Principle 8)."""
    g = _graph()
    for name in ("Proposition", *_SPINE):
        term = _t(name)
        assert (term, RDFS.label, None) in g
        assert (term, SKOS_DEFINITION, None) in g
        assert (term, RDFS.isDefinedBy, None) in g


def test_flagship_example_parses() -> None:
    """The flagship epistemic ledger is valid Turtle.

    Retained: the flagship references cross-slice classes, so the harness's
    slice-scoped ExampleConformance cannot validate it (it would emit
    shacl.ClassConstraintComponent for the unresolved cross-slice sh:class
    targets); `make validate` validates it in full against the merged ontology.
    """
    g = Graph()
    flagship = _MODULE.parent / "examples" / "flagship-epistemic-ledger.ttl"
    g.parse(flagship, format="turtle")
    assert len(g) > 0


def test_suppression_round_trip() -> None:
    """The flagship example retains superseded states and suppresses the tenure.

    Bespoke: a numeric credence comparison (original > revised) and temporal
    tenure navigation (endedAtTime, displayable) that the declarative ASK form
    does not express.
    """
    g = Graph()
    flagship = _MODULE.parent / "examples" / "flagship-epistemic-ledger.ttl"
    g.parse(flagship, format="turtle")

    tenures = list(g.subjects(RDF.type, _t("DoxasticTenure")))
    assert len(tenures) == 2

    original: URIRef | None = None
    revised: URIRef | None = None
    for tenure in tenures:
        interval = g.value(tenure, _t("duringInterval"))
        if interval is not None and (interval, _t("endedAtTime"), None) in g:
            original = tenure
        else:
            revised = tenure

    assert original is not None
    assert revised is not None
    assert original != revised

    revised_interval = g.value(revised, _t("duringInterval"))
    assert revised_interval is not None

    assert (original, _t("displayable"), Literal(False)) in g
    assert (revised_interval, _t("endedAtTime"), None) not in g

    original_state = g.value(original, _t("tenureOfDoxasticState"))
    revised_state = g.value(revised, _t("tenureOfDoxasticState"))
    assert isinstance(original_state, URIRef)
    assert isinstance(revised_state, URIRef)
    assert original_state != revised_state
    assert (original_state, RDF.type, _t("DoxasticState")) in g
    assert (revised_state, RDF.type, _t("DoxasticState")) in g

    original_cred = g.value(original_state, _t("credence"))
    revised_cred = g.value(revised_state, _t("credence"))
    assert original_cred is not None
    assert revised_cred is not None
    assert float(original_cred) > float(revised_cred)


def test_epistemics_mapping_set_exists_and_has_expected_rows() -> None:
    """The generated SSSOM mapping set for epistemics contains expected subjects.

    Bespoke: reads a GENERATED artifact (a TSV outside the ontology graph), which
    the graph-oriented test-DSL does not address.
    """
    mapping = (
        _MODULE.parents[3] / "generated" / "mappings" / "gmeow-epistemics.sssom.tsv"
    )
    assert mapping.exists(), f"Missing mapping file: {mapping}"

    with mapping.open("r", encoding="utf-8") as fh:
        lines = [line for line in fh if not line.startswith("#")]
        reader = csv.DictReader(lines, delimiter="\t")
        subjects = {row["subject_id"] for row in reader if row.get("subject_id")}

    expected = {
        "gmeow:DoxasticState",
        "gmeow:Proposition",
        "gmeow:believes",
        "gmeow:knowsThat",
        "gmeow:justifiedBy",
    }
    assert expected.issubset(subjects), (
        f"Missing subjects in mapping: {expected - subjects}"
    )

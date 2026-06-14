# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the OWL/gUFO adapter + round-trip IR isomorphism gate (#500 Task 2).

Modules under test: ``logic_adapter.py``.
Also exercises ``logic_frontend.parse_logic_source`` for the logic: side.

Covers:
* adapt_legacy_source: gUFO stereotype → logic: sort axiom (rdf:type mapping).
* adapt_legacy_source: OWL structural predicate → logic: predicate axiom.
* adapt_legacy_source: OWL property-characteristic type → logic: type axiom.
* adapt_legacy_source: blank-node OWL restriction → UNMAPPED_OWL_CONSTRUCT.
* adapt_legacy_source: empty graph → LogicParseError.
* adapt_legacy_source: non-existent file → LogicParseError.
* assert_ir_isomorphic: paired logic: / owl-gufo fixtures match (no raise).
* assert_ir_isomorphic: divergent pair raises IRIsomorphismError with diff.
* assert_ir_isomorphic: diff message names the differing items specifically.
* Round-trip: logic: and gufo: form of same construct are canonically equal.
* Round-trip: logic: and owl:subClassOf form normalize identically.
* Diagnostics: unmapped construct emits a named WARNING diagnostic.
"""

from __future__ import annotations

import pytest
from rdflib import RDF, BNode, Graph, Namespace
from rdflib.namespace import OWL, RDFS

from gmeow_tools.config import LOGIC_NAMESPACE, PREFIXES
from gmeow_tools.logic_adapter import (
    IRIsomorphismError,
    adapt_legacy_source,
    assert_ir_isomorphic,
)
from gmeow_tools.logic_frontend import (
    LogicParseError,
    parse_logic_source,
)
from gmeow_tools.logic_ir import LogicProgram

LOGIC = Namespace(LOGIC_NAMESPACE)
GUFO = Namespace(PREFIXES["gufo"])
EX = Namespace("https://example.org/test/")

# Convenience string for rdf:type IRI
RDF_TYPE = str(RDF.type)


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #


def _logic_prog(*axioms_triples: tuple[str, str, str]) -> LogicProgram:
    """Build a tiny LogicProgram from (subject, predicate, obj) string tuples."""
    from gmeow_tools.logic_ir import ContextualScope, LogicAxiom

    axioms = tuple(
        LogicAxiom(subject=s, predicate=p, obj=o, scope=ContextualScope())
        for s, p, o in axioms_triples
    )
    return LogicProgram(axioms=axioms, rules=(), profiles=())


# --------------------------------------------------------------------------- #
# Empty-graph and file-error paths
# --------------------------------------------------------------------------- #


def test_adapt_empty_graph_raises() -> None:
    with pytest.raises(LogicParseError, match="empty"):
        adapt_legacy_source(Graph())


def test_adapt_nonexistent_file_raises(tmp_path) -> None:
    bad = tmp_path / "nonexistent.ttl"
    with pytest.raises(LogicParseError, match="does not exist"):
        adapt_legacy_source(bad)


def test_adapt_invalid_turtle_raises(tmp_path) -> None:
    bad = tmp_path / "bad.ttl"
    bad.write_text("this is not valid turtle @@@ !!!", encoding="utf-8")
    with pytest.raises(LogicParseError):
        adapt_legacy_source(bad)


# --------------------------------------------------------------------------- #
# gUFO stereotype → logic: sort
# --------------------------------------------------------------------------- #


def test_adapt_gufo_kind_to_logic_kind() -> None:
    """gufo:Kind → logic:Kind (rdf:type axiom)."""
    g = Graph()
    g.add((EX.Person, RDF.type, GUFO.Kind))

    prog, diags = adapt_legacy_source(g)

    logic_kind_iri = LOGIC_NAMESPACE + "Kind"
    matching = [
        a for a in prog.axioms if a.predicate == RDF_TYPE and a.obj == logic_kind_iri
    ]
    assert matching, f"Expected rdf:type logic:Kind axiom; got {prog.axioms}"
    assert matching[0].subject == str(EX.Person)
    assert not diags


def test_adapt_gufo_role_to_logic_role() -> None:
    """gufo:Role → logic:Role."""
    g = Graph()
    g.add((EX.Employee, RDF.type, GUFO.Role))

    prog, _ = adapt_legacy_source(g)

    logic_role_iri = LOGIC_NAMESPACE + "Role"
    matching = [a for a in prog.axioms if a.obj == logic_role_iri]
    assert matching, "Expected logic:Role axiom"


def test_adapt_gufo_event_type_to_logic_event() -> None:
    """gufo:EventType → logic:Event."""
    g = Graph()
    g.add((EX.Meeting, RDF.type, GUFO.EventType))

    prog, _ = adapt_legacy_source(g)

    logic_event_iri = LOGIC_NAMESPACE + "Event"
    matching = [a for a in prog.axioms if a.obj == logic_event_iri]
    assert matching, "Expected logic:Event axiom from gufo:EventType"


def test_adapt_gufo_situation_type_to_logic_situation() -> None:
    """gufo:SituationType → logic:Situation."""
    g = Graph()
    g.add((EX.Crisis, RDF.type, GUFO.SituationType))

    prog, _ = adapt_legacy_source(g)

    logic_situation_iri = LOGIC_NAMESPACE + "Situation"
    matching = [a for a in prog.axioms if a.obj == logic_situation_iri]
    assert matching, "Expected logic:Situation axiom"


def test_adapt_all_gufo_sortals() -> None:
    """All nine gUFO endurant stereotypes produce a corresponding logic: type axiom."""
    gufo_to_logic = {
        GUFO.Kind: "Kind",
        GUFO.SubKind: "SubKind",
        GUFO.Phase: "Phase",
        GUFO.Role: "Role",
        GUFO.Category: "Category",
        GUFO.Mixin: "Mixin",
        GUFO.RoleMixin: "RoleMixin",
        GUFO.PhaseMixin: "PhaseMixin",
        GUFO.Relator: "Relator",
    }
    g = Graph()
    subjects = {
        gufo_iri: EX[f"Cls{local}"] for gufo_iri, local in gufo_to_logic.items()
    }
    for gufo_iri, subj in subjects.items():
        g.add((subj, RDF.type, gufo_iri))

    prog, diags = adapt_legacy_source(g)

    warning_codes = [d.code for d in diags]
    assert not warning_codes, f"Unexpected diagnostics: {diags}"

    for gufo_iri, local in gufo_to_logic.items():
        logic_iri = LOGIC_NAMESPACE + local
        subj = subjects[gufo_iri]
        matching = [
            a
            for a in prog.axioms
            if a.predicate == RDF_TYPE and a.obj == logic_iri and a.subject == str(subj)
        ]
        assert matching, f"Missing logic:{local} axiom for {subj!s}"


def test_adapt_blank_node_gufo_sort_emits_diagnostic() -> None:
    """Blank-node subject typed with gufo: stereotype → BLANK_NODE_GUFO_SORT."""
    g = Graph()
    blank = BNode("anon")
    g.add((blank, RDF.type, GUFO.Kind))

    prog, diags = adapt_legacy_source(g)

    codes = [d.code for d in diags]
    assert "BLANK_NODE_GUFO_SORT" in codes, (
        f"Expected BLANK_NODE_GUFO_SORT; got {diags}"
    )
    # The blank node must not appear in axioms
    assert not prog.axioms


# --------------------------------------------------------------------------- #
# OWL structural predicate → logic: predicate
# --------------------------------------------------------------------------- #


def test_adapt_rdfs_subclass_of() -> None:
    """rdfs:subClassOf → logic:subClassOf predicate."""
    g = Graph()
    g.add((EX.Employee, RDFS.subClassOf, EX.Person))

    prog, diags = adapt_legacy_source(g)

    logic_sub_iri = LOGIC_NAMESPACE + "subClassOf"
    matching = [a for a in prog.axioms if a.predicate == logic_sub_iri]
    assert matching, f"Expected logic:subClassOf axiom; got {prog.axioms}"
    assert matching[0].subject == str(EX.Employee)
    assert matching[0].obj == str(EX.Person)
    assert not matching[0].obj_is_literal
    assert not diags


def test_adapt_owl_equivalent_class() -> None:
    """owl:equivalentClass → logic:equivalentClass."""
    g = Graph()
    g.add((EX.Human, OWL.equivalentClass, EX.Person))

    prog, _ = adapt_legacy_source(g)

    logic_eq_iri = LOGIC_NAMESPACE + "equivalentClass"
    matching = [a for a in prog.axioms if a.predicate == logic_eq_iri]
    assert matching, "Expected logic:equivalentClass axiom"


def test_adapt_owl_disjoint_with() -> None:
    """owl:disjointWith → logic:disjointWith."""
    g = Graph()
    g.add((EX.Cat, OWL.disjointWith, EX.Dog))

    prog, _ = adapt_legacy_source(g)

    logic_disj_iri = LOGIC_NAMESPACE + "disjointWith"
    matching = [a for a in prog.axioms if a.predicate == logic_disj_iri]
    assert matching, "Expected logic:disjointWith axiom"


def test_adapt_rdfs_domain_and_range() -> None:
    """rdfs:domain and rdfs:range map to logic:domain / logic:range."""
    g = Graph()
    g.add((EX.knows, RDFS.domain, EX.Person))
    g.add((EX.knows, RDFS.range, EX.Person))

    prog, _ = adapt_legacy_source(g)

    logic_domain = LOGIC_NAMESPACE + "domain"
    logic_range = LOGIC_NAMESPACE + "range"
    domains = [a for a in prog.axioms if a.predicate == logic_domain]
    ranges = [a for a in prog.axioms if a.predicate == logic_range]
    assert domains, "Expected logic:domain axiom"
    assert ranges, "Expected logic:range axiom"


def test_adapt_owl_inverse_of() -> None:
    """owl:inverseOf → logic:inverseOf."""
    g = Graph()
    g.add((EX.partOf, OWL.inverseOf, EX.hasPart))

    prog, _ = adapt_legacy_source(g)

    logic_inv_iri = LOGIC_NAMESPACE + "inverseOf"
    matching = [a for a in prog.axioms if a.predicate == logic_inv_iri]
    assert matching, "Expected logic:inverseOf axiom"


# --------------------------------------------------------------------------- #
# OWL property characteristics
# --------------------------------------------------------------------------- #


def test_adapt_owl_transitive_property() -> None:
    """owl:TransitiveProperty → rdf:type logic:transitiveProperty."""
    g = Graph()
    g.add((EX.partOf, RDF.type, OWL.TransitiveProperty))

    prog, _ = adapt_legacy_source(g)

    logic_trans_iri = LOGIC_NAMESPACE + "transitiveProperty"
    matching = [
        a for a in prog.axioms if a.predicate == RDF_TYPE and a.obj == logic_trans_iri
    ]
    assert matching, "Expected logic:transitiveProperty type axiom"
    assert matching[0].subject == str(EX.partOf)


def test_adapt_owl_symmetric_property() -> None:
    """owl:SymmetricProperty → rdf:type logic:symmetricProperty."""
    g = Graph()
    g.add((EX.sibling, RDF.type, OWL.SymmetricProperty))

    prog, _ = adapt_legacy_source(g)

    logic_sym_iri = LOGIC_NAMESPACE + "symmetricProperty"
    matching = [a for a in prog.axioms if a.obj == logic_sym_iri]
    assert matching, "Expected logic:symmetricProperty type axiom"


def test_adapt_owl_functional_property() -> None:
    """owl:FunctionalProperty → rdf:type logic:functionalProperty."""
    g = Graph()
    g.add((EX.hasMother, RDF.type, OWL.FunctionalProperty))

    prog, _ = adapt_legacy_source(g)

    logic_func_iri = LOGIC_NAMESPACE + "functionalProperty"
    matching = [a for a in prog.axioms if a.obj == logic_func_iri]
    assert matching, "Expected logic:functionalProperty type axiom"


# --------------------------------------------------------------------------- #
# Unmapped OWL constructs → diagnostic
# --------------------------------------------------------------------------- #


def test_blank_node_restriction_emits_unmapped_diagnostic() -> None:
    """owl:someValuesFrom restriction on a named property → UNMAPPED_OWL_CONSTRUCT."""
    g = Graph()
    restriction = BNode("restr")
    g.add((restriction, RDF.type, OWL.Restriction))
    g.add((restriction, OWL.onProperty, EX.knows))
    g.add((restriction, OWL.someValuesFrom, EX.Person))
    # Named class with subClassOf to blank restriction
    g.add((EX.SomeClass, RDFS.subClassOf, restriction))

    _prog, diags = adapt_legacy_source(g)

    codes = [d.code for d in diags]
    assert "UNMAPPED_OWL_CONSTRUCT" in codes, (
        f"Expected UNMAPPED_OWL_CONSTRUCT; got {diags}"
    )


# --------------------------------------------------------------------------- #
# Round-trip isomorphism: assert_ir_isomorphic
# --------------------------------------------------------------------------- #


def test_isomorphic_identical_programs() -> None:
    """Two identical programs pass the gate without raising."""
    g = Graph()
    g.add((EX.Person, RDF.type, GUFO.Kind))

    prog_a, _ = adapt_legacy_source(g)
    prog_b, _ = adapt_legacy_source(g)

    assert_ir_isomorphic(prog_a, prog_b)  # must not raise


def test_isomorphic_divergent_programs_raise() -> None:
    """Divergent programs raise IRIsomorphismError with a directional diff."""
    prog_a = _logic_prog(
        (str(EX.Person), str(RDF.type), LOGIC_NAMESPACE + "Kind"),
    )
    prog_b = _logic_prog(
        (str(EX.Employee), str(RDF.type), LOGIC_NAMESPACE + "Role"),
    )

    with pytest.raises(IRIsomorphismError) as exc_info:
        assert_ir_isomorphic(prog_a, prog_b)

    msg = str(exc_info.value)
    # Must contain directional labels
    assert "A has, B lacks" in msg or "B has, A lacks" in msg


def test_isomorphic_diff_names_differing_items() -> None:
    """The directional diff message explicitly names what differs."""
    person_iri = str(EX.Person)
    kind_iri = LOGIC_NAMESPACE + "Kind"

    prog_a = _logic_prog((person_iri, str(RDF.type), kind_iri))
    prog_b = _logic_prog()  # empty

    with pytest.raises(IRIsomorphismError) as exc_info:
        assert_ir_isomorphic(prog_a, prog_b)

    msg = str(exc_info.value)
    # The diff must mention the subject/predicate/obj of the differing axiom
    assert person_iri in msg or kind_iri in msg


def test_isomorphic_order_independent() -> None:
    """Programs with same axioms in different construction order pass the gate."""
    prog_a = _logic_prog(
        (str(EX.Person), str(RDF.type), LOGIC_NAMESPACE + "Kind"),
        (str(EX.Employee), str(RDF.type), LOGIC_NAMESPACE + "Role"),
    )
    prog_b = _logic_prog(
        (str(EX.Employee), str(RDF.type), LOGIC_NAMESPACE + "Role"),
        (str(EX.Person), str(RDF.type), LOGIC_NAMESPACE + "Kind"),
    )

    assert_ir_isomorphic(prog_a, prog_b)  # same content, different order — must pass


# --------------------------------------------------------------------------- #
# Round-trip paired fixtures: logic: ↔ gufo: / owl:
# --------------------------------------------------------------------------- #


def test_roundtrip_gufo_kind_equals_logic_kind() -> None:
    """The same 'Person is a Kind' expressed in gufo: and logic: normalizes to equal IR.

    logic: form: ex:Person rdf:type logic:Kind
    gufo:  form: ex:Person rdf:type gufo:Kind → normalized to rdf:type logic:Kind
    """
    # logic: side
    g_logic = Graph()
    g_logic.add((EX.Person, RDF.type, LOGIC.Kind))
    prog_logic, _ = parse_logic_source(g_logic)

    # gufo: side
    g_gufo = Graph()
    g_gufo.add((EX.Person, RDF.type, GUFO.Kind))
    prog_gufo, _ = adapt_legacy_source(g_gufo)

    # Both must have exactly one axiom: (ex:Person, rdf:type, logic:Kind)
    assert len(prog_logic.axioms) == 1
    assert len(prog_gufo.axioms) == 1

    ax_logic = prog_logic.axioms[0]
    ax_gufo = prog_gufo.axioms[0]

    assert ax_logic.subject == ax_gufo.subject
    assert ax_logic.predicate == ax_gufo.predicate
    assert ax_logic.obj == ax_gufo.obj

    assert_ir_isomorphic(prog_logic, prog_gufo)


def test_roundtrip_gufo_role_equals_logic_role() -> None:
    """gufo:Role and logic:Role normalize identically."""
    g_logic = Graph()
    g_logic.add((EX.Employee, RDF.type, LOGIC.Role))
    prog_logic, _ = parse_logic_source(g_logic)

    g_gufo = Graph()
    g_gufo.add((EX.Employee, RDF.type, GUFO.Role))
    prog_gufo, _ = adapt_legacy_source(g_gufo)

    assert_ir_isomorphic(prog_logic, prog_gufo)


def test_roundtrip_owl_subclassof_equals_logic_subclassof() -> None:
    """rdfs:subClassOf and logic:subClassOf normalize identically.

    logic: form:  ex:Employee logic:subClassOf ex:Person
    owl:   form:  ex:Employee rdfs:subClassOf  ex:Person → logic:subClassOf
    """
    logic_sub_iri = LOGIC_NAMESPACE + "subClassOf"

    g_logic = Graph()
    g_logic.add((EX.Employee, LOGIC.subClassOf, EX.Person))
    prog_logic, _ = parse_logic_source(g_logic)

    g_owl = Graph()
    g_owl.add((EX.Employee, RDFS.subClassOf, EX.Person))
    prog_owl, _ = adapt_legacy_source(g_owl)

    # logic: side must have the logic:subClassOf axiom
    logic_axioms = [a for a in prog_logic.axioms if a.predicate == logic_sub_iri]
    owl_axioms = [a for a in prog_owl.axioms if a.predicate == logic_sub_iri]

    assert logic_axioms, f"No logic:subClassOf in logic: prog; got {prog_logic.axioms}"
    assert owl_axioms, f"No logic:subClassOf in owl prog; got {prog_owl.axioms}"

    assert_ir_isomorphic(prog_logic, prog_owl)


def test_roundtrip_owl_inverse_of_equals_logic_inverse_of() -> None:
    """owl:inverseOf and logic:inverseOf normalize identically."""
    logic_inv_iri = LOGIC_NAMESPACE + "inverseOf"

    g_logic = Graph()
    g_logic.add((EX.partOf, LOGIC.inverseOf, EX.hasPart))
    prog_logic, _ = parse_logic_source(g_logic)

    g_owl = Graph()
    g_owl.add((EX.partOf, OWL.inverseOf, EX.hasPart))
    prog_owl, _ = adapt_legacy_source(g_owl)

    logic_axioms = [a for a in prog_logic.axioms if a.predicate == logic_inv_iri]
    owl_axioms = [a for a in prog_owl.axioms if a.predicate == logic_inv_iri]

    assert logic_axioms
    assert owl_axioms
    assert_ir_isomorphic(prog_logic, prog_owl)


def test_roundtrip_owl_transitive_property() -> None:
    """owl:TransitiveProperty and logic:transitiveProperty normalize identically."""
    logic_trans_iri = LOGIC_NAMESPACE + "transitiveProperty"

    g_logic = Graph()
    g_logic.add((EX.partOf, RDF.type, LOGIC.transitiveProperty))
    prog_logic, _ = parse_logic_source(g_logic)

    g_owl = Graph()
    g_owl.add((EX.partOf, RDF.type, OWL.TransitiveProperty))
    prog_owl, _ = adapt_legacy_source(g_owl)

    logic_axioms = [a for a in prog_logic.axioms if a.obj == logic_trans_iri]
    owl_axioms = [a for a in prog_owl.axioms if a.obj == logic_trans_iri]

    assert logic_axioms
    assert owl_axioms
    assert_ir_isomorphic(prog_logic, prog_owl)


def test_roundtrip_divergent_pair_raises() -> None:
    """gufo:Kind on Person vs logic:Role on Employee → IRIsomorphismError."""
    g_gufo = Graph()
    g_gufo.add((EX.Person, RDF.type, GUFO.Kind))
    prog_gufo, _ = adapt_legacy_source(g_gufo)

    g_logic = Graph()
    g_logic.add((EX.Employee, RDF.type, LOGIC.Role))
    prog_logic, _ = parse_logic_source(g_logic)

    with pytest.raises(IRIsomorphismError):
        assert_ir_isomorphic(prog_gufo, prog_logic)


# --------------------------------------------------------------------------- #
# Source IRI provenance
# --------------------------------------------------------------------------- #


def test_adapt_source_iri_stored() -> None:
    g = Graph()
    g.add((EX.Person, RDF.type, GUFO.Kind))

    prog, _ = adapt_legacy_source(g, source_iri="https://example.org/legacy")

    assert prog.source_iri == "https://example.org/legacy"


def test_adapt_source_iri_from_file(tmp_path) -> None:
    ttl = tmp_path / "legacy.ttl"
    ttl.write_text(
        "@prefix gufo: <http://purl.org/nemo/gufo#> .\n"
        "@prefix ex: <https://example.org/test/> .\n"
        "ex:Person a gufo:Kind .\n",
        encoding="utf-8",
    )
    prog, _ = adapt_legacy_source(ttl)
    assert prog.source_iri == ttl.as_uri()

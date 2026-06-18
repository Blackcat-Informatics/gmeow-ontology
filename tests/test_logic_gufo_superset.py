# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Machine-checkable ``gmeow:logic ⊇ gUFO`` coverage gate (#663 Task 4).

gUFO is a generated, VALIDATION-ONLY lossy down-projection of the canonical
``gmeow:logic`` foundation (Principle 17): every gUFO class must therefore be
covered by a richer ``logic:`` term (or explicitly SUPERSEDED by the
``logic:Fluent`` + RDF-1.2 edge-property pattern). This module is the honest
floor that enforces it.

Covers:
* test_every_gufo_class_has_logic_correspondence — every ``owl:Class`` in the
  gUFO namespace is a key in ``_GUFO_CLASS_TO_LOGIC`` (the minimum baseline).
* test_correspondence_targets_exist — every non-SUPERSEDED target ``logic:`` IRI
  is actually declared (as a subject) in ``slices/core/logic/module.ttl``.
* test_superseded_set_is_the_five_reifiers — exactly the five gUFO
  temporary-situation reifiers map to SUPERSEDED (no over-supersession).
* test_new_logic_terms_carry_graphbox_role — every non-SUPERSEDED target carries
  a ``gmeow:graphBoxRole`` annotation in the module.

Plus the worked-example parse/pattern test for ``criticism-fixes.ttl``, which
demonstrates four ``gmeow:logic`` advantages over gUFO / OWL 2.
"""

from __future__ import annotations

from rdflib import RDF, Graph, Literal, Namespace, URIRef
from rdflib.namespace import OWL

from gmeow_tools.config import IMPORTS_DIR, LOGIC_NAMESPACE, PREFIXES, SLICES_DIR
from gmeow_tools.logic_adapter import (
    _GUFO_CLASS_TO_LOGIC,
    SUPERSEDED,
)

GUFO = Namespace(PREFIXES["gufo"])
GMEOW = Namespace(PREFIXES["gmeow"])
LOGIC = Namespace(LOGIC_NAMESPACE)
EX = Namespace("https://blackcatinformatics.ca/gmeow/examples/logic/")

GUFO_TTL = IMPORTS_DIR / "gufo.ttl"
LOGIC_MODULE_TTL = SLICES_DIR / "core" / "logic" / "module.ttl"
CRITICISM_EXAMPLE_TTL = (
    SLICES_DIR / "core" / "logic" / "examples" / "criticism-fixes.ttl"
)

#: The five gUFO temporary-situation reifiers that gmeow:logic deliberately
#: replaces with logic:Fluent + RDF-1.2 edge properties (see logic_adapter
#: ``_Superseded``).  The gate pins this exact set so an accidental
#: over-supersession (mapping a faithfully-coverable class to SUPERSEDED) fails.
_EXPECTED_SUPERSEDED: frozenset[URIRef] = frozenset(
    {
        GUFO.QualityValueAttributionSituation,
        GUFO.TemporaryConstitutionSituation,
        GUFO.TemporaryInstantiationSituation,
        GUFO.TemporaryParthoodSituation,
        GUFO.TemporaryRelationshipSituation,
    }
)


# --------------------------------------------------------------------------- #
# Fixtures (module-level cached graphs, built once)
# --------------------------------------------------------------------------- #


def _gufo_classes() -> set[URIRef]:
    """Every ``owl:Class`` IRI in the gUFO namespace declared in gufo.ttl."""
    graph = Graph()
    graph.parse(GUFO_TTL, format="turtle")
    return {
        s
        for s in graph.subjects(RDF.type, OWL.Class)
        if isinstance(s, URIRef) and str(s).startswith(str(GUFO))
    }


def _logic_module_graph() -> Graph:
    graph = Graph()
    graph.parse(LOGIC_MODULE_TTL, format="turtle")
    return graph


def _non_superseded_targets() -> set[URIRef]:
    """The set of distinct logic: IRIs the dict maps gUFO classes to."""
    return {URIRef(v) for v in _GUFO_CLASS_TO_LOGIC.values() if isinstance(v, str)}


# --------------------------------------------------------------------------- #
# (A1) Every gUFO class has a correspondence — the minimum-baseline floor
# --------------------------------------------------------------------------- #


def test_every_gufo_class_has_logic_correspondence() -> None:
    """Every gUFO ``owl:Class`` must be a key in ``_GUFO_CLASS_TO_LOGIC``.

    This is the ``gmeow:logic ⊇ gUFO`` floor: gUFO is a generated lossy
    projection of the canonical logic: foundation, so nothing in gUFO may lack
    a declared correspondence (a faithful logic: target or an explicit
    SUPERSEDED sentinel). A loud failure naming the missing classes is the only
    acceptable failure mode — silence would let the floor erode.
    """
    classes = _gufo_classes()
    assert classes, f"No gUFO owl:Class declarations found in {GUFO_TTL}"

    missing = sorted(str(c) for c in classes if c not in _GUFO_CLASS_TO_LOGIC)
    assert not missing, (
        "gmeow:logic ⊇ gUFO floor BREACHED — these gUFO classes have NO entry in "
        "logic_adapter._GUFO_CLASS_TO_LOGIC (add a faithful logic: target or the "
        "SUPERSEDED sentinel):\n  " + "\n  ".join(missing)
    )


# --------------------------------------------------------------------------- #
# (A2) Correspondence targets actually exist in the module
# --------------------------------------------------------------------------- #


def test_correspondence_targets_exist() -> None:
    """Every non-SUPERSEDED target IRI must be declared in module.ttl.

    A correspondence that points at a logic: IRI the module never declares is a
    dangling promise: the gate would claim coverage that does not exist. Each
    target must appear as a subject in ``slices/core/logic/module.ttl``.
    """
    graph = _logic_module_graph()
    subjects = set(graph.subjects())

    missing = sorted(str(t) for t in _non_superseded_targets() if t not in subjects)
    assert not missing, (
        "These _GUFO_CLASS_TO_LOGIC targets are NOT declared as subjects in "
        f"{LOGIC_MODULE_TTL.name} — the correspondence is dangling:\n  "
        + "\n  ".join(missing)
    )


# --------------------------------------------------------------------------- #
# (A3) The SUPERSEDED set is exactly the five reifiers
# --------------------------------------------------------------------------- #


def test_superseded_set_is_the_five_reifiers() -> None:
    """Exactly the five temporary-situation reifiers map to SUPERSEDED.

    Guards against accidental over-supersession: marking a faithfully-coverable
    gUFO class SUPERSEDED would hide a real coverage gap behind the sentinel.
    """
    actual = {
        k for k, v in _GUFO_CLASS_TO_LOGIC.items() if isinstance(v, type(SUPERSEDED))
    }
    over = sorted(str(x) for x in actual - _EXPECTED_SUPERSEDED)
    under = sorted(str(x) for x in _EXPECTED_SUPERSEDED - actual)
    assert actual == set(_EXPECTED_SUPERSEDED), (
        "SUPERSEDED set drift.\n"
        f"  unexpected (over-supersession): {over}\n"
        f"  missing (should be superseded): {under}"
    )


# --------------------------------------------------------------------------- #
# (A4) Every correspondence target carries a graphBoxRole
# --------------------------------------------------------------------------- #


def test_new_logic_terms_carry_graphbox_role() -> None:
    """Every non-SUPERSEDED target carries a ``gmeow:graphBoxRole`` annotation.

    The graph-box role (TBox / RBox / ABox) is the per-term wiring that lets the
    native solver place each foundation term in the right derivation lane
    (#664/#665). A correspondence target with no role is an unwired term. Scoped
    to the correspondence targets so pre-existing untagged terms elsewhere in
    the module do not fail this gate.
    """
    graph = _logic_module_graph()
    no_role = sorted(
        str(t)
        for t in _non_superseded_targets()
        if (t, GMEOW.graphBoxRole, None) not in graph
    )
    assert not no_role, (
        "These _GUFO_CLASS_TO_LOGIC targets lack a gmeow:graphBoxRole annotation "
        f"in {LOGIC_MODULE_TTL.name} — add one rather than weakening the gate:\n  "
        + "\n  ".join(no_role)
    )


# --------------------------------------------------------------------------- #
# (B) Worked example — criticism-fixes.ttl parses and shows the four patterns
# --------------------------------------------------------------------------- #


def _example_graph() -> Graph:
    assert CRITICISM_EXAMPLE_TTL.exists(), (
        f"worked example missing: {CRITICISM_EXAMPLE_TTL}"
    )
    graph = Graph()
    graph.parse(CRITICISM_EXAMPLE_TTL, format="turtle")
    return graph


def test_criticism_example_parses() -> None:
    """The worked example is valid Turtle and non-empty."""
    graph = _example_graph()
    assert len(graph) > 0


def test_criticism_example_has_native_edge_property() -> None:
    """§1 triple-bloat fix: an RDF-1.2 reifier typed logic:Fluent carrying the
    quoted (subject, predicate, object) and validFrom/validTo edge metadata."""
    graph = _example_graph()
    # A reifier node typed both rdf:Statement and logic:Fluent.
    fluents = set(graph.subjects(RDF.type, LOGIC.Fluent))
    statements = set(graph.subjects(RDF.type, RDF.Statement))
    reifiers = fluents & statements
    assert reifiers, "no rdf:Statement + logic:Fluent reifier found"
    reifier = next(iter(reifiers))
    # The reifier quotes a full (subject, predicate, object) triple term.
    assert (reifier, RDF.subject, None) in graph
    assert (reifier, RDF.predicate, None) in graph
    assert (reifier, RDF.object, None) in graph
    # And carries the edge time-scope metadata specifically on validFrom/validTo.
    for pred, name in ((EX.validFrom, "validFrom"), (EX.validTo, "validTo")):
        objs = [o for o in graph.objects(reifier, pred) if isinstance(o, Literal)]
        assert objs, f"reifier carries no literal {name} edge metadata"


def test_criticism_example_has_strict_partial_order() -> None:
    """§2 OWL-2 global-restriction fix: logic:properPartOf used, and the module
    types it transitive ∧ asymmetric ∧ irreflexive at once (illegal in OWL 2)."""
    graph = _example_graph()
    chain = list(graph.subject_objects(LOGIC.properPartOf))
    assert len(chain) >= 2, "expected a logic:properPartOf chain (>= 2 edges)"

    module = _logic_module_graph()
    chars = set(module.objects(LOGIC.properPartOf, RDF.type))
    for required in (
        LOGIC.transitiveProperty,
        LOGIC.asymmetricProperty,
        LOGIC.irreflexiveProperty,
    ):
        assert required in chars, (
            f"logic:properPartOf is not typed {required} in the module — the "
            "strict-partial-order characteristic is missing"
        )


def test_criticism_example_has_multilevel_instance_chain() -> None:
    """§3 no-punning fix: a logic:instanceOf chain where a type is itself an
    instance of a higher-order type, with logic:orderedType levels."""
    graph = _example_graph()
    inst = dict(graph.subject_objects(LOGIC.instanceOf))
    # A two-step chain: marv -> goldenEagle -> species.
    bridges = [mid for mid in inst.values() if mid in inst]
    assert bridges, (
        "no multi-level chain: need x logic:instanceOf y and y logic:instanceOf z"
    )
    # logic:orderedType levels are recorded.
    levels = list(graph.objects(None, LOGIC.orderedType))
    assert levels, "no logic:orderedType levels recorded"


def test_criticism_example_references_builtin() -> None:
    """§4 builtin-derived value: a derivation references a logic:Builtin
    individual via logic:invokesBuiltin."""
    graph = _example_graph()
    invocations = list(graph.subject_objects(LOGIC.invokesBuiltin))
    assert invocations, "no logic:invokesBuiltin edge found"
    module = _logic_module_graph()
    for _subj, builtin in invocations:
        assert (builtin, RDF.type, LOGIC.Builtin) in module, (
            f"{builtin} is not declared a logic:Builtin in the module"
        )

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Lint-equivalence + soundness + regression gate for the foundation lowering.

This module is Task 6 of issue #503: the mechanical proof of the three acceptance
criteria for the OntoUML-discipline lowering (:mod:`gmeow_tools.logic_foundation`).

AC#1 — lowered rules reproduce the lint verdicts EXACTLY (the core gate)
-----------------------------------------------------------------------
A **parallel-construction equivalence** harness.  For each abstract anti-pattern
scenario the harness builds BOTH forms from a single :class:`_Scenario` spec and
asserts the offending ``{class_localname -> frozenset(disciplines)}`` maps are equal
under the discipline correspondence:

==============================================  ==========================
``reasoning_lint`` discipline                   lowered ``logic:`` label
==============================================  ==========================
``exactly_one_stereotype`` (no / conflicting)   ``StereotypeCardinality``
``identity_overlap`` / MixIden                  ``MixIden``
``anti_rigidity_discipline`` / FreeRole         ``FreeRole``
``anti_rigidity_discipline`` / MixRig           ``MixRig``
``relator_mediation`` / RelComp                 ``RelComp``
==============================================  ==========================

* **gUFO form** — a gUFO graph built with ``gmeow:`` IRIs (the
  :mod:`tests.test_reasoning_lint` idiom: ``GUFO.Kind`` / ``Role`` / … stereotypes
  via ``rdf:type``, ``rdfs:subClassOf`` for hierarchy, ``owl:ObjectProperty`` +
  ``rdfs:domain`` / ``rdfs:range`` for relator mediation).  The four matching
  :mod:`gmeow_tools.reasoning_lint` functions run over it and every returned
  message is parsed into ``(class_localname, discipline)``.
* **logic: form** — the SAME scenario built with the SAME class IRIs (the
  ``gmeow:`` NAMESPACE IRIs, so the comparison is direct) as ``logic:`` facts in
  ONE named world of a :class:`~rdflib.ConjunctiveGraph` (stereotypes via
  ``rdf:type`` to ``logic:Kind/SubKind/Phase/Role/...``, ``logic:subClassOf``,
  ``logic:mediates``).  It is materialised through the SAME augmented program the
  runner builds (``foundation_rules(prog)`` + ``enable_naf=True``); the derived
  ``logic:violation`` quads are collected into ``(subject_localname, label)``.

The assertion is **full-map equality** — not subset containment — so a scenario
that trips MULTIPLE disciplines (a bare ``Role`` → FreeRole + MixIden; a conflicting
Kind+Role → StereotypeCardinality + FreeRole) must match the COMPLETE set on both
sides.  The headline **SubKind ⊑ Role MixRig** case (AC#3) is included.

AC#2 — regression: real ontology stays clean
---------------------------------------------
:func:`test_real_ontology_is_clean_over_all_lints` asserts
``reasoning_invariants(load_merged_graph())`` is empty over the FULL stereotype
set — the lowering must not have perturbed the real merged gmeow ontology.  This
duplicates :func:`tests.test_reasoning_lint.test_real_ontology_is_clean` on purpose
so the equivalence module is a self-contained AC anchor.  The foundation
conformance gate (the six ``conformance/logic/cases/foundation/`` cases) is exercised
end-to-end by :func:`test_foundation_conformance_cases_are_green`.

Soundness of the lint-silent capability (Task 3)
------------------------------------------------
The cross-world rigidity verdict is NOT lint-equivalence — the lint is silent on
cross-world facts — so its correctness is **soundness**, gated separately.  The full
soundness suite is :mod:`tests.test_logic_rigidity`; this module both imports and
re-runs two of its scenarios (:func:`test_cross_world_rigidity_soundness_is_gated`)
and asserts a focused fires/clean pair, documenting that the additional capability
is verified by soundness, not parity.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from rdflib import RDF, RDFS, ConjunctiveGraph, Graph, URIRef
from rdflib.namespace import OWL, Namespace

import tests.test_logic_rigidity as rigidity_suite
from gmeow_tools.config import LOGIC_NAMESPACE, NAMESPACE, PREFIXES
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.logic_foundation import foundation_rules
from gmeow_tools.logic_ir import LogicProgram
from gmeow_tools.logic_materialize import materialize_program
from gmeow_tools.logic_runner import diff_case, run
from gmeow_tools.reasoning_lint import (
    anti_rigidity_discipline,
    identity_overlap,
    reasoning_invariants,
    relator_mediation,
)

GUFO = Namespace(PREFIXES["gufo"])

#: The discipline label localnames the equivalence is asserted over.  These are the
#: five lowered ``logic:`` labels that correspond one-for-one to the lint verdicts;
#: every other lint (coequal-facet / frame-completeness) is out of scope for the
#: lowering and is not produced by ``foundation_rules``.
_DISCIPLINES: frozenset[str] = frozenset(
    {"StereotypeCardinality", "MixIden", "FreeRole", "MixRig", "RelComp"}
)

#: ``logic:`` predicate IRIs used to seed the logic-form ConjunctiveGraph.
_L_SUBCLASS_OF = URIRef(LOGIC_NAMESPACE + "subClassOf")
_L_MEDIATES = URIRef(LOGIC_NAMESPACE + "mediates")
_L_VIOLATION = LOGIC_NAMESPACE + "violation"

#: A single named world for the logic-form ConjunctiveGraph.  One world is enough:
#: the four lowered disciplines are in-world Datalog (cross-world rigidity, which
#: needs ≥2 worlds, is a SOUNDNESS gate handled separately below).
_WORLD = URIRef("https://example.org/foundation/equivalence/schema")


# --------------------------------------------------------------------------- #
# Abstract scenario spec — built into BOTH forms from one description.
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class _Scenario:
    """One abstract anti-pattern scenario, lowered into BOTH detector forms.

    Attributes:
        name: Human-readable scenario id (the pytest parameter id).
        classes: ``localname -> tuple of stereotype localnames`` (e.g.
            ``"Dog": ("Kind",)``).  A localname maps to the same class IRI in both
            forms (``gmeow:<name>`` for the lint, the identical IRI as a ``logic:``
            fact subject for the lowering).  An empty stereotype tuple means the
            class carries NO meta-class (the missing-stereotype anti-pattern).
        subclass_of: Direct ``subClassOf`` edges as ``(child, parent)`` localname
            pairs (``rdfs:subClassOf`` in gUFO, ``logic:subClassOf`` in logic).
        relator_parents: Localnames that are concrete relators.  The two detector
            surfaces model relator-ness DIFFERENTLY, so the builders diverge here
            (the single point of form-specific construction):
              * gUFO — a relator is a ``gufo:Kind``-stereotyped class that is
                ``rdfs:subClassOf gufo:Relator`` (relator-ness is a *nature* via
                subClassOf; the stereotype is still the Kind it instantiates — the
                exact :mod:`tests.test_reasoning_lint` relator idiom, e.g.
                ``_cls("Bond", GUFO.Kind, parent=GUFO.Relator)``).
              * logic — a relator carries the ``logic:Relator`` stereotype pun (the
                lowered surface stereotypes relators rather than subclassing them).
            A relator localname must therefore NOT also appear in ``classes`` with a
            stereotype — its stereotype is supplied per-form by the builders.
        mediations: ``relator_localname -> tuple of mediated-relatum localnames``.
            In the gUFO form each relatum becomes a non-functional
            ``owl:ObjectProperty`` with the relator as domain and the relatum as
            range (so each contributes the two ends a non-functional mediation
            weighs, matching the lint's end-counting).  In the logic form each is a
            ``logic:mediates`` edge.
    """

    name: str
    classes: dict[str, tuple[str, ...]]
    subclass_of: tuple[tuple[str, str], ...] = ()
    relator_parents: tuple[str, ...] = ()
    mediations: dict[str, tuple[str, ...]] = field(default_factory=dict)


# --------------------------------------------------------------------------- #
# Scenario corpus — mirrors the negative tests in test_reasoning_lint.py.
# --------------------------------------------------------------------------- #

_SCENARIOS: tuple[_Scenario, ...] = (
    # --- StereotypeCardinality: a class carrying NO stereotype. ----------------
    # (mirrors test_missing_stereotype_is_flagged).  A class only enters the lowered
    # class-universe when it appears in a subClassOf edge or carries a stereotype, so
    # the bare class is anchored under a clean Kind.
    _Scenario(
        name="missing-stereotype",
        classes={"Anchor": ("Kind",), "Bare": ()},
        subclass_of=(("Bare", "Anchor"),),
    ),
    # --- StereotypeCardinality: conflicting stereotypes (Kind + Role). ---------
    # (mirrors test_conflicting_stereotypes_are_flagged).  Carrying the anti-rigid
    # Role with no rigid ancestor ALSO trips FreeRole in BOTH detectors — the full
    # set is {StereotypeCardinality, FreeRole}, and the harness asserts the complete
    # set, not just the cardinality verdict.
    _Scenario(
        name="conflicting-stereotype",
        classes={"TwoFaced": ("Kind", "Role")},
    ),
    # --- MixIden: a Kind specializing a Kind. ----------------------------------
    # (mirrors test_kind_under_kind_is_flagged_mixiden).
    _Scenario(
        name="kind-under-kind-mixiden",
        classes={"Animal": ("Kind",), "Dog": ("Kind",)},
        subclass_of=(("Dog", "Animal"),),
    ),
    # --- FreeRole (+ MixIden): a bare Role under no rigid sortal. ---------------
    # (mirrors test_free_role_is_flagged).  The bare Role is a non-Kind sortal with
    # ZERO Kind ancestors, so it ALSO trips MixIden in BOTH detectors — the full set
    # is {FreeRole, MixIden}.
    _Scenario(
        name="free-role",
        classes={"Wanderer": ("Role",)},
    ),
    # --- MixRig (+ cascades): a rigid SubKind under an anti-rigid Role. ---------
    # (mirrors test_rigid_under_anti_rigid_is_flagged_mixrig — the AC#3 case).
    # HonorsStudent (SubKind) ⊑ Student (Role):
    #   HonorsStudent → {MixRig, MixIden}  (rigid under anti-rigid; non-Kind sortal,
    #                                       0 Kind ancestors)
    #   Student       → {FreeRole, MixIden} (anti-rigid Role, no rigid ancestor;
    #                                        non-Kind sortal, 0 Kind ancestors)
    _Scenario(
        name="subkind-under-role-mixrig",
        classes={"Student": ("Role",), "HonorsStudent": ("SubKind",)},
        subclass_of=(("HonorsStudent", "Student"),),
    ),
    # --- RelComp: a concrete relator mediating fewer than two relata. ----------
    # (mirrors test_under_mediated_relator_is_flagged_relcomp).  LonelyBond is a
    # concrete relator mediating exactly one relatum.
    _Scenario(
        name="under-mediated-relator",
        classes={"Person": ("Kind",)},
        relator_parents=("LonelyBond",),
        mediations={"LonelyBond": ("Person",)},
    ),
    # --- CLEAN: a well-formed relator mediating two distinct relata. -----------
    # (mirrors test_well_formed_relator_passes).  ZERO violations in BOTH detectors.
    _Scenario(
        name="well-formed-relator-clean",
        classes={
            "PersonKind": ("Kind",),
            "Husband": ("Role",),
            "Wife": ("Role",),
        },
        # Anchor the two role relata under a Kind so they do not themselves trip
        # FreeRole/MixIden — the scenario is meant to be CLEAN on both sides.
        subclass_of=(
            ("Husband", "PersonKind"),
            ("Wife", "PersonKind"),
        ),
        relator_parents=("Bond",),
        mediations={"Bond": ("Husband", "Wife")},
    ),
    # --- CLEAN multi-discipline hierarchy: no false positives / negatives. ------
    # A rigid Kind spine with a SubKind, an anti-rigid Role and Phase each anchored
    # under the Kind, and a two-relatum relator — every discipline's precondition is
    # present yet NONE fires.  ZERO violations in BOTH detectors.
    _Scenario(
        name="clean-multi-discipline-hierarchy",
        classes={
            "Person": ("Kind",),
            "Employee": ("SubKind",),
            "Adult": ("Phase",),
            "Customer": ("Role",),
            "Company": ("Kind",),
        },
        subclass_of=(
            ("Employee", "Person"),
            ("Adult", "Person"),
            ("Customer", "Person"),
        ),
        relator_parents=("Employment",),
        mediations={"Employment": ("Employee", "Company")},
    ),
)


# --------------------------------------------------------------------------- #
# gUFO form: build the graph + run the four lints, parse to (class, discipline).
# --------------------------------------------------------------------------- #


def _gmeow(local: str) -> URIRef:
    """The ``gmeow:`` vocabulary IRI for a localname (shared across both forms)."""
    return URIRef(NAMESPACE + local)


def _build_gufo_graph(scenario: _Scenario) -> Graph:
    """Build the gUFO form of a scenario — the :mod:`tests.test_reasoning_lint` idiom.

    Classes are ``owl:Class`` punned with ``gufo:<stereotype>`` via ``rdf:type``;
    hierarchy is ``rdfs:subClassOf``; a relator is ``rdfs:subClassOf gufo:Relator``;
    each mediation is a non-functional ``owl:ObjectProperty`` whose domain is the
    relator and whose range is the relatum (so it weighs the two ends the lint's
    end-counting assigns a non-functional property).
    """
    graph = Graph()
    for local, stereotypes in scenario.classes.items():
        iri = _gmeow(local)
        graph.add((iri, RDF.type, OWL.Class))
        for stereotype in stereotypes:
            graph.add((iri, RDF.type, GUFO[stereotype]))
    for local in scenario.relator_parents:
        # gUFO relator idiom (test_reasoning_lint): a gufo:Kind-stereotyped owl:Class
        # whose relator-ness is the nature ``rdfs:subClassOf gufo:Relator``.
        graph.add((_gmeow(local), RDF.type, OWL.Class))
        graph.add((_gmeow(local), RDF.type, GUFO.Kind))
        graph.add((_gmeow(local), RDFS.subClassOf, GUFO.Relator))
    for child, parent in scenario.subclass_of:
        graph.add((_gmeow(child), RDFS.subClassOf, _gmeow(parent)))
    for relator, relata in scenario.mediations.items():
        for i, relatum in enumerate(relata):
            prop = URIRef(NAMESPACE + f"{relator}_mediates_{relatum}_{i}")
            graph.add((prop, RDF.type, OWL.ObjectProperty))
            # FUNCTIONAL: each property weighs exactly ONE end, so N distinct relata
            # ⇒ N ends.  The lint's "≥ 2 ends" then coincides exactly with the logic
            # form's "≥ 2 distinct logic:mediates relata" (hasTwoMediatedRelata) — one
            # functional property per relatum makes the two RelComp counts identical.
            graph.add((prop, RDF.type, OWL.FunctionalProperty))
            graph.add((prop, RDFS.domain, _gmeow(relator)))
            graph.add((prop, RDFS.range, _gmeow(relatum)))
    return graph


def _local_of(prefixed_or_iri: str) -> str:
    """Strip a ``gmeow:`` / ``gufo:`` prefix (or a full IRI) to its localname."""
    for prefix in ("gmeow:", "gufo:"):
        if prefixed_or_iri.startswith(prefix):
            return prefixed_or_iri[len(prefix) :]
    if prefixed_or_iri.startswith(NAMESPACE):
        return prefixed_or_iri[len(NAMESPACE) :]
    return prefixed_or_iri


def _subject_localname(message: str) -> str:
    """Extract the offending class localname — always the FIRST token of a message.

    Every ``reasoning_lint`` message begins with ``_local(cls)`` (the offending
    class as ``gmeow:NAME``) followed by a space, e.g.
    ``"gmeow:Dog is a gufo:Kind but specializes ..."``.
    """
    head = message.split(" ", 1)[0]
    return _local_of(head)


def _gufo_verdicts(scenario: _Scenario) -> dict[str, frozenset[str]]:
    """Run the four lints over the gUFO form → ``{class_localname -> disciplines}``.

    Each lint's prose is mapped to its discipline by the OntoUML keyword it carries:
    ``exactly_one_stereotype`` ("no gUFO meta-class" / "conflicting gUFO
    meta-classes") → ``StereotypeCardinality``; ``MixIden`` / ``FreeRole`` /
    ``MixRig`` / ``RelComp`` map to themselves.  The offending class is the message's
    first token.
    """
    graph = _build_gufo_graph(scenario)
    verdicts: dict[str, set[str]] = {}

    def _record(local: str, discipline: str) -> None:
        verdicts.setdefault(local, set()).add(discipline)

    # exactly_one_stereotype is imported indirectly: we call it via reasoning_lint to
    # keep the message-keyword mapping in one place.
    from gmeow_tools.reasoning_lint import exactly_one_stereotype

    for msg in exactly_one_stereotype(graph):
        no_meta = "carries no gUFO meta-class" in msg
        conflicting = "conflicting gUFO meta-classes" in msg
        if no_meta or conflicting:
            _record(_subject_localname(msg), "StereotypeCardinality")
        else:  # pragma: no cover - defensive: the lint emits only these two shapes.
            raise AssertionError(f"unexpected exactly_one_stereotype message: {msg!r}")
    for msg in identity_overlap(graph):
        assert "MixIden" in msg, msg
        _record(_subject_localname(msg), "MixIden")
    for msg in anti_rigidity_discipline(graph):
        if "FreeRole" in msg:
            _record(_subject_localname(msg), "FreeRole")
        elif "MixRig" in msg:
            _record(_subject_localname(msg), "MixRig")
        else:  # pragma: no cover - defensive.
            raise AssertionError(f"unexpected anti_rigidity message: {msg!r}")
    for msg in relator_mediation(graph):
        assert "RelComp" in msg, msg
        _record(_subject_localname(msg), "RelComp")

    return {local: frozenset(disciplines) for local, disciplines in verdicts.items()}


# --------------------------------------------------------------------------- #
# logic: form: seed the ConjunctiveGraph, materialize, collect logic:violation.
# --------------------------------------------------------------------------- #


def _build_logic_cg(scenario: _Scenario) -> ConjunctiveGraph:
    """Build the logic form of a scenario in ONE named world (the same IRIs).

    Stereotypes are ``rdf:type`` puns into ``logic:<stereotype>``; a relator carries
    the ``logic:Relator`` pun; hierarchy is ``logic:subClassOf``; each mediation is a
    ``logic:mediates`` edge.  The class IRIs are the SAME ``gmeow:`` IRIs the gUFO
    form uses, so the two verdict maps are keyed identically.
    """
    cg: ConjunctiveGraph = ConjunctiveGraph()
    ctx = cg.get_context(_WORLD)
    for local, stereotypes in scenario.classes.items():
        for stereotype in stereotypes:
            ctx.add((_gmeow(local), RDF.type, URIRef(LOGIC_NAMESPACE + stereotype)))
    for local in scenario.relator_parents:
        ctx.add((_gmeow(local), RDF.type, URIRef(LOGIC_NAMESPACE + "Relator")))
    for child, parent in scenario.subclass_of:
        ctx.add((_gmeow(child), _L_SUBCLASS_OF, _gmeow(parent)))
    for relator, relata in scenario.mediations.items():
        for relatum in relata:
            ctx.add((_gmeow(relator), _L_MEDIATES, _gmeow(relatum)))
    return cg


def _logic_verdicts(scenario: _Scenario) -> dict[str, frozenset[str]]:
    """Materialize the lowered program → ``{class_localname -> disciplines}``.

    Mirrors :func:`gmeow_tools.logic_runner.run` exactly for the in-world
    disciplines: an empty source program augmented with ``foundation_rules(prog)``,
    materialised with ``enable_naf=True``.  Derived ``logic:violation`` quads are
    collected, the discipline-label object stripped to its localname.
    """
    cg = _build_logic_cg(scenario)
    program = LogicProgram(axioms=(), rules=(), profiles=())
    augmented = LogicProgram(
        axioms=program.axioms,
        rules=(*program.rules, *foundation_rules(program)),
        profiles=program.profiles,
        source_iri=program.source_iri,
    )
    result = materialize_program(augmented, cg, enable_naf=True)

    verdicts: dict[str, set[str]] = {}
    for quad in result.quads:
        if quad.predicate != _L_VIOLATION:
            continue
        # The object is canonical N3 (``<iri>``); strip to the bare label localname.
        label_iri = quad.obj[1:-1] if quad.obj.startswith("<") else quad.obj
        label = label_iri[len(LOGIC_NAMESPACE) :]
        subject_local = quad.subject[len(NAMESPACE) :]
        verdicts.setdefault(subject_local, set()).add(label)

    return {local: frozenset(disciplines) for local, disciplines in verdicts.items()}


# --------------------------------------------------------------------------- #
# AC#1 — parallel-construction equivalence (full-map equality, per scenario).
# --------------------------------------------------------------------------- #


def test_lowering_reproduces_lint_verdicts_exactly() -> None:
    """AC#1: for every scenario the gUFO-lint and logic-lowering maps are EQUAL.

    The assertion is full-map equality of ``{class_localname -> frozenset(
    disciplines)}`` — NOT subset containment — so a class that trips multiple
    disciplines must match the COMPLETE set on both sides.  This single test sweeps
    the whole scenario corpus so a mismatch names the offending scenario.
    """
    for scenario in _SCENARIOS:
        gufo_map = _gufo_verdicts(scenario)
        logic_map = _logic_verdicts(scenario)
        assert gufo_map == logic_map, (
            f"lint/lowering verdict mismatch for scenario {scenario.name!r}:\n"
            f"  gUFO lint:      {gufo_map}\n"
            f"  logic lowering: {logic_map}"
        )
        # The harness only certifies the five lowered disciplines; guard that no
        # scenario smuggled in an out-of-scope label.
        for disciplines in logic_map.values():
            assert disciplines <= _DISCIPLINES, disciplines


def test_subkind_under_role_mixrig_is_covered() -> None:
    """AC#3 anchor: the headline SubKind ⊑ Role MixRig scenario IS in the corpus.

    Asserts both the scenario's presence and its exact dual-detector verdict map —
    HonorsStudent → {MixRig, MixIden}, Student → {FreeRole, MixIden} — so the AC#3
    case is pinned independently of the corpus sweep above.
    """
    scenario = next(s for s in _SCENARIOS if s.name == "subkind-under-role-mixrig")
    expected = {
        "HonorsStudent": frozenset({"MixRig", "MixIden"}),
        "Student": frozenset({"FreeRole", "MixIden"}),
    }
    assert _gufo_verdicts(scenario) == expected
    assert _logic_verdicts(scenario) == expected


def test_clean_scenarios_yield_zero_violations_in_both_forms() -> None:
    """No false positives / negatives: CLEAN scenarios fire NOTHING on either side."""
    for name in ("well-formed-relator-clean", "clean-multi-discipline-hierarchy"):
        scenario = next(s for s in _SCENARIOS if s.name == name)
        assert _gufo_verdicts(scenario) == {}, name
        assert _logic_verdicts(scenario) == {}, name


# --------------------------------------------------------------------------- #
# AC#2 — regression: the real ontology + foundation conformance stay valid.
# --------------------------------------------------------------------------- #


def test_real_ontology_is_clean_over_all_lints() -> None:
    """AC#2: the real merged gmeow ontology is clean under EVERY reasoning lint.

    Self-contained AC anchor (also asserted by
    :func:`tests.test_reasoning_lint.test_real_ontology_is_clean`): the lowering must
    not have perturbed the real ontology, and ``reasoning_invariants`` runs the full
    stereotype set (all four lowering lints plus the coequal-facet / frame-completeness
    invariants).
    """
    assert reasoning_invariants(load_merged_graph()) == []


def _foundation_cases_root() -> Path:
    """The ``conformance/logic/cases/foundation/`` directory under the worktree."""
    return (
        Path(__file__).resolve().parents[1]
        / "conformance"
        / "logic"
        / "cases"
        / "foundation"
    )


def test_foundation_conformance_cases_are_green() -> None:
    """AC#2: the six foundation conformance cases run clean against their goldens.

    Invokes the runner over each ``conformance/logic/cases/foundation/`` case and
    asserts :func:`gmeow_tools.logic_runner.diff_case` reports no differences — the
    same gate ``gmeow-dev conformance`` (20/20) enforces, exercised here so the
    equivalence module is a self-contained AC anchor.  A case directory needs both
    ``input.logic.ttl`` and ``profile.json``; bare marker dirs (``.gitkeep``) are
    skipped.
    """
    root = _foundation_cases_root()
    assert root.is_dir(), f"foundation cases dir missing: {root}"
    case_dirs = sorted(
        d
        for d in root.iterdir()
        if d.is_dir()
        and (d / "input.logic.ttl").exists()
        and (d / "profile.json").exists()
    )
    # The six foundation cases (one per lowered discipline + the cross-world case).
    assert len(case_dirs) == 6, [d.name for d in case_dirs]
    for case_dir in case_dirs:
        outputs = run(case_dir)
        result = diff_case(outputs)
        assert result.passed, f"{case_dir.name}: {result.diffs}"


# --------------------------------------------------------------------------- #
# Soundness of the lint-silent capability (Task 3) — referenced + re-asserted.
# --------------------------------------------------------------------------- #


def test_cross_world_rigidity_soundness_is_gated() -> None:
    """Cross-world rigidity is SOUNDNESS, not parity — verified here + in its suite.

    The lint is silent on cross-world facts, so the rigidity closure has nothing to
    round-trip against; its correctness is soundness.  This re-runs the two anchor
    scenarios from :mod:`tests.test_logic_rigidity` (the full suite) — the closure
    FIRES when rigidity-persistence breaks and is CLEAN when typing is consistent —
    so the equivalence module documents that the extra capability is gated by
    soundness.  See :mod:`tests.test_logic_rigidity` for the complete suite.
    """
    # Fires on broken persistence (inst typed RigidKind in A, exists untyped in B).
    rigidity_suite.test_rigidity_fires_when_persistence_fails()
    # Clean on consistent typing (inst typed RigidKind in BOTH worlds).
    rigidity_suite.test_rigidity_clean_when_typed_in_both_worlds()

# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Projection back-ends: :class:`~.logic_ir.LogicProgram` → each target format.

This module is the **projection phase** of the #500 logic compiler (Task 3).
Each public function takes a typed :class:`~.logic_ir.LogicProgram` and emits
one target format:

* :func:`project_owl_dl` — OWL 2 DL Turtle (``generated/owl/gmeow-dl.ttl``)
* :func:`project_owl_el` — OWL 2 EL profile Turtle (``generated/owl/gmeow-el.ttl``)
* :func:`project_datalog` — Datalog text (``generated/datalog/gmeow.dl``)
* :func:`project_n3` — N3 rules (``generated/n3/gmeow.n3``)
* :func:`project_gufo` — gUFO bridge Turtle (``generated/foundation/gufo.ttl``)
* :func:`project_canonical_rdf12` — round-trippable canonical serialization
  (``generated/logic/gmeow.logic.rdf12.ttl``)

Each projection is **deterministic** (sorted output) and declares its
:class:`~.logic_ir.PreservationKind` + complexity class through the
:func:`build_projection_report` mechanism.

Overclaim detection
-------------------
:func:`assert_no_overclaim` compares a back-end's *declared* preservation kind
against what it *actually achieved*.  If a projection dropped content but still
claims ``ExactPreservation``, the function raises :class:`OverclaimError` —
the build turns red, exactly like the drift gate (LOGIC-CONFORMANCE.md
§overclaim→red).

Projection report
-----------------
:func:`build_projection_report` emits a Turtle ``Graph`` (suitable for writing
to ``generated/logic/projection-report.ttl``) with per-target
``logic:preservationKind``, ``logic:complexityClass``, and aggregated
``gmeow:lossyDrop`` records.

Dependencies: rdflib, gmeow_tools.config, gmeow_tools.logic_ir.  No I/O side
effects unless a ``path`` argument is supplied.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from rdflib import RDF, Graph, Literal, Namespace, URIRef
from rdflib.namespace import OWL, RDFS, XSD

from gmeow_tools.config import LOGIC_NAMESPACE, NAMESPACE, PREFIXES
from gmeow_tools.logic_ir import (
    LogicAxiom,
    LogicModality,
    LogicProgram,
    PreservationKind,
)

LOGIC = Namespace(LOGIC_NAMESPACE)
GMEOW = Namespace(NAMESPACE)
GUFO = Namespace(PREFIXES["gufo"])

# --------------------------------------------------------------------------- #
# Projection metadata: per-target preservation kind + complexity class
# --------------------------------------------------------------------------- #

#: Logic vocabulary IRIs used in the report — single source of truth.
LOGIC_PRESERVATION_KIND = LOGIC.preservationKind
LOGIC_COMPLEXITY_CLASS = LOGIC.complexityClass
GMEOW_LOSSY_DROP = GMEOW.lossyDrop

#: Per-target metadata: (preservationKind, complexityClass, lossyDrops[])
#: Used both in build_projection_report and for the overclaim gate.
#: Names map to the generated artifact basenames.
_TARGET_META: dict[str, tuple[PreservationKind, str, tuple[str, ...]]] = {
    "owl-dl": (
        PreservationKind.SOUND_UNDER,
        "decidable/N2EXPTIME",
        (
            "modal/world context is erased",
            "contextual scope (standpoint, time, confidence) is dropped",
            "rule bodies mapped to OWL axioms where OWL is expressive enough; "
            "existential rules beyond OWL DL expressivity are dropped",
            "probabilistic profile not representable in OWL DL",
        ),
    ),
    "owl-el": (
        PreservationKind.SOUND_UNDER,
        "PTIME",
        (
            "modal/world context is erased",
            "contextual scope (standpoint, time, confidence) is dropped",
            "only EL-safe axioms emitted (no disjointness, no inverseOf, "
            "no cardinality restrictions, no nominals)",
            "rules beyond EL expressivity are dropped",
        ),
    ),
    "datalog": (
        PreservationKind.SOUND_UNDER,
        "terminating/PTIME-data",
        (
            "modal/world context flattened to predicate reification",
            "no existential rule heads (skolemisation not emitted)",
            "OWL class expressions not representable as Datalog atoms are dropped",
        ),
    ),
    "n3": (
        PreservationKind.COMPLETE_OVER,
        "semi-decidable",
        (
            "modal context encoded as quoted graph arguments (may overgenerate)",
            "N3 builtins used for arithmetic/string predicates where available",
        ),
    ),
    "gufo": (
        PreservationKind.VALIDATION_ONLY,
        "PTIME",
        (
            "only gUFO-mapped sorts and structural predicates emitted",
            "logic: world-modal/contextual structure has no gUFO equivalent",
            "rules not representable in gUFO; only type/subtype declarations kept",
            "preservation kind is ValidationOnly: gUFO is an anti-pattern check, "
            "not an entailment surface",
        ),
    ),
    "canonical-rdf12": (
        PreservationKind.EXACT,
        "N/A (identity serialization)",
        (),
    ),
}

# --------------------------------------------------------------------------- #
# OWL predicate ↔ logic: predicate maps (mirror of adapter, reversed)
# --------------------------------------------------------------------------- #

#: logic: predicate → OWL/RDFS structural predicate.
_LOGIC_PRED_TO_OWL: dict[str, URIRef] = {
    LOGIC_NAMESPACE + "subClassOf": RDFS.subClassOf,
    LOGIC_NAMESPACE + "equivalentClass": OWL.equivalentClass,
    LOGIC_NAMESPACE + "disjointWith": OWL.disjointWith,
    LOGIC_NAMESPACE + "subPropertyOf": RDFS.subPropertyOf,
    LOGIC_NAMESPACE + "equivalentProperty": OWL.equivalentProperty,
    LOGIC_NAMESPACE + "inverseOf": OWL.inverseOf,
    LOGIC_NAMESPACE + "domain": RDFS.domain,
    LOGIC_NAMESPACE + "range": RDFS.range,
}

#: logic: sort IRI → gUFO stereotype IRI.
_LOGIC_SORT_TO_GUFO: dict[str, URIRef] = {
    LOGIC_NAMESPACE + "Kind": GUFO.Kind,
    LOGIC_NAMESPACE + "SubKind": GUFO.SubKind,
    LOGIC_NAMESPACE + "Phase": GUFO.Phase,
    LOGIC_NAMESPACE + "Role": GUFO.Role,
    LOGIC_NAMESPACE + "Category": GUFO.Category,
    LOGIC_NAMESPACE + "Mixin": GUFO.Mixin,
    LOGIC_NAMESPACE + "RoleMixin": GUFO.RoleMixin,
    LOGIC_NAMESPACE + "PhaseMixin": GUFO.PhaseMixin,
    LOGIC_NAMESPACE + "Relator": GUFO.Relator,
    LOGIC_NAMESPACE + "Event": GUFO.EventType,
    LOGIC_NAMESPACE + "Situation": GUFO.SituationType,
}

#: logic: property-characteristic sort IRI → OWL type IRI.
_LOGIC_CHAR_TO_OWL: dict[str, URIRef] = {
    LOGIC_NAMESPACE + "transitiveProperty": OWL.TransitiveProperty,
    LOGIC_NAMESPACE + "symmetricProperty": OWL.SymmetricProperty,
    LOGIC_NAMESPACE + "functionalProperty": OWL.FunctionalProperty,
    LOGIC_NAMESPACE + "inverseFunctionalProperty": OWL.InverseFunctionalProperty,
}

#: EL-safe predicates (subset of OWL DL that EL can handle).
_EL_SAFE_LOGIC_PREDS: frozenset[str] = frozenset(
    {
        LOGIC_NAMESPACE + "subClassOf",
        LOGIC_NAMESPACE + "equivalentClass",
        LOGIC_NAMESPACE + "subPropertyOf",
        LOGIC_NAMESPACE + "domain",
        LOGIC_NAMESPACE + "range",
    }
)

#: EL-safe characteristic sorts.
_EL_SAFE_CHARS: frozenset[str] = frozenset(
    {
        LOGIC_NAMESPACE + "transitiveProperty",
    }
)


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #


def _bind_prefixes(graph: Graph) -> None:
    """Bind the canonical GMEOW prefix registry onto ``graph``."""
    for prefix, iri in PREFIXES.items():
        graph.bind(prefix, iri, override=False)


def _is_modal_or_scoped(axiom: LogicAxiom) -> bool:
    """Return True if the axiom carries non-trivial contextual scope."""
    return (
        axiom.scope.modality != LogicModality.NONE
        or axiom.scope.standpoint is not None
        or axiom.scope.time is not None
        or axiom.scope.confidence is not None
        or axiom.scope.provenance is not None
    )


def _generated_banner(target: str) -> str:
    """Return a standard GENERATED header comment for a target."""
    return (
        f"# GENERATED by `gmeow logic compile` (logic_projections.py) — "
        f"DO NOT EDIT.\n"
        f"# {target} projection of the canonical logic: program.\n"
    )


def _serialize_graph(graph: Graph, banner: str) -> str:
    """Serialize *graph* to Turtle with a GENERATED banner, deterministic output."""
    turtle = graph.serialize(format="turtle").rstrip("\n") + "\n"
    return banner + turtle


# --------------------------------------------------------------------------- #
# Overclaim error
# --------------------------------------------------------------------------- #


class OverclaimError(Exception):
    """Raised when a projection's declared preservation is stronger than achieved.

    LOGIC-CONFORMANCE.md §overclaim→red: a projection declaring
    ``ExactPreservation`` on a lossy target is a build failure.
    """


def assert_no_overclaim(
    target: str,
    declared: PreservationKind,
    drops: list[str],
) -> None:
    """Assert that *declared* is not stronger than what *drops* implies.

    If *drops* is non-empty (the projection discarded content) and *declared*
    is :attr:`~.logic_ir.PreservationKind.EXACT`, this function raises
    :class:`OverclaimError`.

    Args:
        target: The projection target name (for error message).
        declared: The preservation kind the back-end declares.
        drops: Concrete items that were dropped (absent IRIs, skipped axioms…).

    Raises:
        OverclaimError: When ``declared == ExactPreservation and drops``.
    """
    if declared == PreservationKind.EXACT and drops:
        raise OverclaimError(
            f"Overclaim in projection '{target}': declared "
            f"logic:{PreservationKind.EXACT} (ExactPreservation) "
            f"but {len(drops)} item(s) were dropped:\n  " + "\n  ".join(drops[:10])
        )


# --------------------------------------------------------------------------- #
# Projection back-ends
# --------------------------------------------------------------------------- #


@dataclass
class ProjectionResult:
    """The result of running a single projection back-end.

    Attributes:
        target: Short name for the target (``"owl-dl"``, ``"owl-el"``, etc.).
        content: The serialized output (Turtle string, Datalog text, or N3).
        graph: The rdflib Graph, when the target is RDF-based (else ``None``).
        preservation: The declared preservation kind for this target.
        complexity: The declared complexity class string.
        lossy_drops: Structural lossy-drop notes (from ``_TARGET_META``).
        actual_drops: Concrete items skipped during this specific run.
    """

    target: str
    content: str
    graph: Graph | None
    preservation: PreservationKind
    complexity: str
    lossy_drops: tuple[str, ...]
    actual_drops: list[str]


def project_owl_dl(
    program: LogicProgram,
    *,
    path: Path | None = None,
) -> ProjectionResult:
    """Project a :class:`~.logic_ir.LogicProgram` to OWL 2 DL Turtle.

    Emits a well-formed OWL ontology carrying:

    * Named class / property declarations for every IRI subject.
    * ``rdfs:subClassOf``, ``owl:equivalentClass``, ``owl:disjointWith``,
      ``rdfs:subPropertyOf``, ``owl:equivalentProperty``, ``owl:inverseOf``,
      ``rdfs:domain``, ``rdfs:range`` axioms from mapped ``logic:`` predicates.
    * ``rdf:type`` axioms for OWL property characteristics and gUFO-mapped sorts.
    * Rules with exactly one body axiom emitted as ``rdfs:subClassOf`` where
      the OWL mapping is feasible.

    Modal/contextual scope is erased (SoundUnderApproximation).  The output is
    sorted for deterministic bytes.

    Args:
        program: The compiled logic program.
        path: Optional path; when given, the Turtle text is written there.

    Returns:
        A :class:`ProjectionResult` with ``target="owl-dl"``.
    """
    meta_kind, meta_cx, meta_drops = _TARGET_META["owl-dl"]
    actual_drops: list[str] = []

    g = Graph()
    _bind_prefixes(g)

    onto_iri = URIRef(NAMESPACE + "owl/gmeow-dl")
    g.add((onto_iri, RDF.type, OWL.Ontology))

    rdf_type_str = str(RDF.type)
    subjects_seen: set[URIRef] = set()

    def _declare(iri_str: str) -> URIRef:
        node = URIRef(iri_str)
        if node not in subjects_seen:
            subjects_seen.add(node)
        return node

    for axiom in sorted(program.axioms, key=lambda a: a._sort_key()):
        subj = _declare(axiom.subject)
        pred_str = axiom.predicate
        obj_str = axiom.obj

        if pred_str == rdf_type_str:
            # gUFO sort mapping
            if obj_str in _LOGIC_SORT_TO_GUFO:
                gufo_type = _LOGIC_SORT_TO_GUFO[obj_str]
                g.add((subj, RDF.type, gufo_type))
                g.add((subj, RDF.type, OWL.Class))
                continue
            # OWL property-characteristic mapping
            if obj_str in _LOGIC_CHAR_TO_OWL:
                owl_char = _LOGIC_CHAR_TO_OWL[obj_str]
                g.add((subj, RDF.type, owl_char))
                g.add((subj, RDF.type, OWL.ObjectProperty))
                continue
            # Other rdf:type axioms — pass through if object is a URI
            if not axiom.obj_is_literal:
                g.add((subj, RDF.type, URIRef(obj_str)))
            continue

        # Structural predicates
        if pred_str in _LOGIC_PRED_TO_OWL:
            owl_pred = _LOGIC_PRED_TO_OWL[pred_str]
            if axiom.obj_is_literal:
                obj_node: URIRef | Literal = Literal(obj_str)
            else:
                obj_node = URIRef(obj_str)
            g.add((subj, owl_pred, obj_node))
            continue

        # Unknown logic: predicate — drop with record
        if pred_str.startswith(LOGIC_NAMESPACE):
            local = pred_str[len(LOGIC_NAMESPACE) :]
            actual_drops.append(
                f"logic:{local} on <{axiom.subject}> has no OWL DL equivalent"
            )
            continue

    # Rules: emit head as rdfs:subClassOf when single-atom body and both sides IRI.
    for rule in sorted(program.rules, key=lambda r: r._sort_key()):
        head = rule.head
        head_pred = head.predicate
        if (
            len(rule.body) == 1
            and head_pred in _LOGIC_PRED_TO_OWL
            and not head.obj_is_literal
        ):
            body_atom = rule.body[0]
            body_pred = body_atom.predicate
            if body_pred in _LOGIC_PRED_TO_OWL:
                owl_head_pred = _LOGIC_PRED_TO_OWL[head_pred]
                g.add(
                    (
                        URIRef(head.subject),
                        owl_head_pred,
                        URIRef(head.obj),
                    )
                )
                continue
        # Rule cannot be expressed in OWL DL — drop with record
        actual_drops.append(
            f"rule head <{rule.head.subject}> {rule.head.predicate!r} "
            f"not expressible in OWL DL (body complexity)"
        )

    assert_no_overclaim("owl-dl", meta_kind, actual_drops)

    banner = _generated_banner("OWL 2 DL")
    content = _serialize_graph(g, banner)
    if path is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    return ProjectionResult(
        target="owl-dl",
        content=content,
        graph=g,
        preservation=meta_kind,
        complexity=meta_cx,
        lossy_drops=meta_drops,
        actual_drops=actual_drops,
    )


def project_owl_el(
    program: LogicProgram,
    *,
    path: Path | None = None,
) -> ProjectionResult:
    """Project a :class:`~.logic_ir.LogicProgram` to OWL 2 EL Turtle.

    Emits a strict OWL 2 EL fragment — only axiom types that ELK can certify:

    * ``rdfs:subClassOf``, ``owl:equivalentClass`` (EL-safe subsumption).
    * ``rdfs:subPropertyOf``, ``rdfs:domain``, ``rdfs:range``.
    * ``rdf:type owl:TransitiveProperty`` (EL allows transitivity).
    * gUFO-sort type assignments (subclasses of the gUFO sort classes).

    ``owl:disjointWith``, ``owl:inverseOf``, ``owl:FunctionalProperty``, and
    ``owl:InverseFunctionalProperty`` are excluded from EL.  Output is sorted.

    Args:
        program: The compiled logic program.
        path: Optional output path.

    Returns:
        A :class:`ProjectionResult` with ``target="owl-el"``.
    """
    meta_kind, meta_cx, meta_drops = _TARGET_META["owl-el"]
    actual_drops: list[str] = []

    g = Graph()
    _bind_prefixes(g)

    onto_iri = URIRef(NAMESPACE + "owl/gmeow-el")
    g.add((onto_iri, RDF.type, OWL.Ontology))

    rdf_type_str = str(RDF.type)

    for axiom in sorted(program.axioms, key=lambda a: a._sort_key()):
        subj = URIRef(axiom.subject)
        pred_str = axiom.predicate
        obj_str = axiom.obj

        if pred_str == rdf_type_str:
            # gUFO sorts — all EL-safe (type assignments)
            if obj_str in _LOGIC_SORT_TO_GUFO:
                gufo_type = _LOGIC_SORT_TO_GUFO[obj_str]
                g.add((subj, RDF.type, gufo_type))
                g.add((subj, RDF.type, OWL.Class))
                continue
            # EL-safe characteristics (only transitivity)
            if obj_str in _EL_SAFE_CHARS:
                owl_char = _LOGIC_CHAR_TO_OWL[obj_str]
                g.add((subj, RDF.type, owl_char))
                g.add((subj, RDF.type, OWL.ObjectProperty))
                continue
            # Non-EL characteristics — drop
            if obj_str in _LOGIC_CHAR_TO_OWL:
                local = obj_str[len(LOGIC_NAMESPACE) :]
                actual_drops.append(
                    f"logic:{local} on <{axiom.subject}> is not EL-safe; dropped"
                )
                continue
            if not axiom.obj_is_literal:
                g.add((subj, RDF.type, URIRef(obj_str)))
            continue

        # EL-safe structural predicates
        if pred_str in _EL_SAFE_LOGIC_PREDS:
            owl_pred = _LOGIC_PRED_TO_OWL[pred_str]
            if axiom.obj_is_literal:
                obj_node: URIRef | Literal = Literal(obj_str)
            else:
                obj_node = URIRef(obj_str)
            g.add((subj, owl_pred, obj_node))
            continue

        # Non-EL structural predicates (disjointWith, inverseOf, etc.)
        if pred_str in _LOGIC_PRED_TO_OWL:
            local = pred_str[len(LOGIC_NAMESPACE) :]
            actual_drops.append(
                f"logic:{local} on <{axiom.subject}> is not EL-safe; dropped"
            )
            continue

        if pred_str.startswith(LOGIC_NAMESPACE):
            local = pred_str[len(LOGIC_NAMESPACE) :]
            actual_drops.append(
                f"logic:{local} on <{axiom.subject}> has no EL equivalent"
            )

    # Rules: EL cannot express rules — all dropped.
    for rule in program.rules:
        actual_drops.append(
            f"rule head <{rule.head.subject}> dropped (EL has no rule surface)"
        )

    assert_no_overclaim("owl-el", meta_kind, actual_drops)

    banner = _generated_banner("OWL 2 EL")
    content = _serialize_graph(g, banner)
    if path is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    return ProjectionResult(
        target="owl-el",
        content=content,
        graph=g,
        preservation=meta_kind,
        complexity=meta_cx,
        lossy_drops=meta_drops,
        actual_drops=actual_drops,
    )


def project_datalog(
    program: LogicProgram,
    *,
    path: Path | None = None,
) -> ProjectionResult:
    """Project a :class:`~.logic_ir.LogicProgram` to Datalog text.

    Emits a Datalog program where:

    * Each :class:`~.logic_ir.LogicAxiom` becomes a ground EDB fact:
      ``pred(subject, object).``
    * Each :class:`~.logic_ir.LogicRule` becomes a Datalog rule:
      ``head_pred(X, Y) :- body_pred1(X, Y1), body_pred2(Y1, Y).``
    * The local name of the ``logic:`` predicate is used as the Datalog
      predicate name (``#`` comments identify dropped constructs).
    * Modal/contextual scope is encoded as an extra ``context`` argument when
      non-trivial (approximation).

    Output is **deterministic**: axioms and rules sorted by their canonical
    sort key.

    Args:
        program: The compiled logic program.
        path: Optional output path.

    Returns:
        A :class:`ProjectionResult` with ``target="datalog"``.
    """
    meta_kind, meta_cx, meta_drops = _TARGET_META["datalog"]
    actual_drops: list[str] = []
    lines: list[str] = []

    rdf_type_str = str(RDF.type)

    def _local(iri: str) -> str:
        """Extract a safe Datalog predicate name from an IRI."""
        for ns in (LOGIC_NAMESPACE, NAMESPACE, str(RDF), str(OWL), str(RDFS)):
            if iri.startswith(ns):
                raw = iri[len(ns) :]
                return raw.replace("-", "_").replace(".", "_").replace("#", "_")
        # Fall back to the last path segment
        return iri.rsplit("/", 1)[-1].rsplit("#", 1)[-1]

    def _iri_atom(iri: str) -> str:
        return f'"{iri}"'

    lines.append("% GENERATED by `gmeow logic compile` — DO NOT EDIT.")
    lines.append("% Datalog projection of the canonical logic: program.")
    lines.append("")

    # Ground facts from axioms
    lines.append("% === Ground facts (axioms) ===")
    for axiom in sorted(program.axioms, key=lambda a: a._sort_key()):
        pred_str = axiom.predicate
        if pred_str == rdf_type_str:
            pred_dl = "type"
        elif pred_str.startswith(LOGIC_NAMESPACE):
            pred_dl = _local(pred_str)
        else:
            pred_dl = _local(pred_str)

        obj_dl = repr(axiom.obj) if axiom.obj_is_literal else _iri_atom(axiom.obj)
        subj_dl = _iri_atom(axiom.subject)

        if _is_modal_or_scoped(axiom):
            # Encode context as an extra argument (approximate)
            modal_str = axiom.scope.modality.value if axiom.scope.modality else "none"
            lines.append(f'{pred_dl}({subj_dl}, {obj_dl}, "{modal_str}").')
        else:
            lines.append(f"{pred_dl}({subj_dl}, {obj_dl}).")

    lines.append("")
    lines.append("% === Rules ===")

    for rule in sorted(program.rules, key=lambda r: r._sort_key()):
        head = rule.head
        head_pred = "type" if head.predicate == rdf_type_str else _local(head.predicate)
        head_obj = repr(head.obj) if head.obj_is_literal else _iri_atom(head.obj)
        head_subj = _iri_atom(head.subject)

        body_parts: list[str] = []
        for body_atom in sorted(rule.body, key=lambda a: a._sort_key()):
            ba_pred = body_atom.predicate
            bp = "type" if ba_pred == rdf_type_str else _local(ba_pred)
            bs = _iri_atom(body_atom.subject)
            bo = (
                repr(body_atom.obj)
                if body_atom.obj_is_literal
                else _iri_atom(body_atom.obj)
            )
            body_parts.append(f"{bp}({bs}, {bo})")

        body_str = ",\n    ".join(body_parts) if body_parts else "true"
        lines.append(f"{head_pred}({head_subj}, {head_obj}) :-")
        lines.append(f"    {body_str}.")

    content = "\n".join(lines) + "\n"
    if path is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    return ProjectionResult(
        target="datalog",
        content=content,
        graph=None,
        preservation=meta_kind,
        complexity=meta_cx,
        lossy_drops=meta_drops,
        actual_drops=actual_drops,
    )


def project_n3(
    program: LogicProgram,
    *,
    path: Path | None = None,
) -> ProjectionResult:
    """Project a :class:`~.logic_ir.LogicProgram` to N3 rules.

    Emits an N3 document where:

    * Ground axioms are emitted as N3 assertions.
    * Rules are expressed in the N3 ``{ head } :- { body }`` syntax
      (using the ``log:implies`` predicate).
    * Modal context is encoded using N3 quoted graphs.

    Output is deterministic (sorted).

    Args:
        program: The compiled logic program.
        path: Optional output path.

    Returns:
        A :class:`ProjectionResult` with ``target="n3"``.
    """
    meta_kind, meta_cx, meta_drops = _TARGET_META["n3"]
    actual_drops: list[str] = []

    # Build an rdflib Graph in n3-compatible form (N3 rules as comments,
    # axioms as standard triples). For full N3 rule syntax we emit raw text.
    g = Graph()
    _bind_prefixes(g)

    rdf_type_str = str(RDF.type)
    lines: list[str] = [
        "@prefix logic: <" + LOGIC_NAMESPACE + "> .",
        "@prefix gmeow: <" + NAMESPACE + "> .",
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .",
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .",
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .",
        "@prefix log: <http://www.w3.org/2000/10/swap/log#> .",
        "",
        "# GENERATED by `gmeow logic compile` — DO NOT EDIT.",
        "# N3 projection of the canonical logic: program.",
        "",
        "# === Axioms ===",
    ]

    for axiom in sorted(program.axioms, key=lambda a: a._sort_key()):
        subj_n3 = f"<{axiom.subject}>"
        pred_n3 = "a" if axiom.predicate == rdf_type_str else f"<{axiom.predicate}>"
        obj_n3 = repr(axiom.obj) if axiom.obj_is_literal else f"<{axiom.obj}>"

        if _is_modal_or_scoped(axiom):
            # Modal context: emit as a quoted-graph assertion
            modal = axiom.scope.modality.value
            lines.append(f"# modal context: {modal}")
            lines.append(
                f"{{ {subj_n3} {pred_n3} {obj_n3} }} "
                f"log:implies {{ {subj_n3} <{LOGIC_NAMESPACE}holds> {obj_n3} }} ."
            )
        else:
            lines.append(f"{subj_n3} {pred_n3} {obj_n3} .")

        # Also add to graph for isomorphism testing
        if not axiom.obj_is_literal:
            g.add((URIRef(axiom.subject), URIRef(axiom.predicate), URIRef(axiom.obj)))
        else:
            g.add((URIRef(axiom.subject), URIRef(axiom.predicate), Literal(axiom.obj)))

    lines.append("")
    lines.append("# === Rules ===")

    for rule in sorted(program.rules, key=lambda r: r._sort_key()):
        head = rule.head
        head_subj = f"<{head.subject}>"
        head_pred = "a" if head.predicate == rdf_type_str else f"<{head.predicate}>"
        head_obj = repr(head.obj) if head.obj_is_literal else f"<{head.obj}>"

        body_parts = []
        for b in sorted(rule.body, key=lambda a: a._sort_key()):
            bs = f"<{b.subject}>"
            bp = "a" if b.predicate == rdf_type_str else f"<{b.predicate}>"
            bo = repr(b.obj) if b.obj_is_literal else f"<{b.obj}>"
            body_parts.append(f"{bs} {bp} {bo}")

        body_str = " . ".join(body_parts) + " ." if body_parts else "true ."
        lines.append(
            f"{{ {body_str} }} log:implies {{ {head_subj} {head_pred} {head_obj} . }} ."
        )

    content = "\n".join(lines) + "\n"
    if path is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    return ProjectionResult(
        target="n3",
        content=content,
        graph=g,
        preservation=meta_kind,
        complexity=meta_cx,
        lossy_drops=meta_drops,
        actual_drops=actual_drops,
    )


def project_gufo(
    program: LogicProgram,
    *,
    path: Path | None = None,
) -> ProjectionResult:
    """Project a :class:`~.logic_ir.LogicProgram` to gUFO bridge Turtle.

    Emits a gUFO bridge ontology (``generated/foundation/gufo.ttl``) — the
    "down-projection" of UFO⁺ onto the gUFO surface per Principle 17.

    Only gUFO-mappable content is emitted:

    * ``rdf:type gufo:Kind`` etc. for each logic: sort axiom.
    * ``rdfs:subClassOf`` for subtype axioms.

    Modal/contextual structure and rules are dropped
    (``ValidationOnly`` — this is an anti-pattern check surface).

    Args:
        program: The compiled logic program.
        path: Optional output path.

    Returns:
        A :class:`ProjectionResult` with ``target="gufo"``.
    """
    meta_kind, meta_cx, meta_drops = _TARGET_META["gufo"]
    actual_drops: list[str] = []

    g = Graph()
    _bind_prefixes(g)

    onto_iri = URIRef(NAMESPACE + "foundation/gufo")
    g.add((onto_iri, RDF.type, OWL.Ontology))

    rdf_type_str = str(RDF.type)

    for axiom in sorted(program.axioms, key=lambda a: a._sort_key()):
        subj = URIRef(axiom.subject)
        pred_str = axiom.predicate
        obj_str = axiom.obj

        if pred_str == rdf_type_str:
            if obj_str in _LOGIC_SORT_TO_GUFO:
                gufo_type = _LOGIC_SORT_TO_GUFO[obj_str]
                g.add((subj, RDF.type, gufo_type))
                continue
            # Non-gUFO type assignment — drop
            if obj_str.startswith(LOGIC_NAMESPACE):
                local = obj_str[len(LOGIC_NAMESPACE) :]
                actual_drops.append(
                    f"rdf:type logic:{local} on <{axiom.subject}> "
                    f"has no gUFO equivalent"
                )
            continue

        # Structural: subClassOf is gUFO-valid
        if pred_str == LOGIC_NAMESPACE + "subClassOf":
            if not axiom.obj_is_literal:
                g.add((subj, RDFS.subClassOf, URIRef(obj_str)))
            continue

        # Everything else — drop
        if pred_str.startswith(LOGIC_NAMESPACE):
            local = pred_str[len(LOGIC_NAMESPACE) :]
            actual_drops.append(
                f"logic:{local} on <{axiom.subject}> has no gUFO bridge equivalent"
            )

    # Rules not representable in gUFO
    for rule in program.rules:
        actual_drops.append(
            f"rule head <{rule.head.subject}> dropped (gUFO bridge has no rule surface)"
        )

    assert_no_overclaim("gufo", meta_kind, actual_drops)

    banner = _generated_banner("gUFO bridge")
    content = _serialize_graph(g, banner)
    if path is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    return ProjectionResult(
        target="gufo",
        content=content,
        graph=g,
        preservation=meta_kind,
        complexity=meta_cx,
        lossy_drops=meta_drops,
        actual_drops=actual_drops,
    )


def project_canonical_rdf12(
    program: LogicProgram,
    *,
    path: Path | None = None,
) -> ProjectionResult:
    """Project a :class:`~.logic_ir.LogicProgram` to canonical RDF 1.2 Turtle.

    This is the **round-trippable** serialization — it encodes the entire IR
    faithfully so that re-parsing via :func:`~.logic_frontend.parse_logic_source`
    and :func:`~.logic_adapter.assert_ir_isomorphic` proves the round-trip.

    Every :class:`~.logic_ir.LogicAxiom` is emitted as a ``logic:`` triple,
    with contextual scope annotations on the reifier node when non-trivial.
    Every :class:`~.logic_ir.LogicRule` is emitted as a ``logic:Rule`` node.
    Every :class:`~.logic_ir.LogicProfile` is emitted as a
    ``logic:SemanticProfile`` declaration.

    Output is deterministic (sorted).

    Args:
        program: The compiled logic program.
        path: Optional output path.

    Returns:
        A :class:`ProjectionResult` with ``target="canonical-rdf12"``.
    """
    meta_kind, meta_cx, meta_drops = _TARGET_META["canonical-rdf12"]

    g = Graph()
    _bind_prefixes(g)

    onto_iri = URIRef(NAMESPACE + "logic/gmeow.logic.rdf12")
    g.add((onto_iri, RDF.type, OWL.Ontology))

    # Counter for stable blank node labelling (used for rule/reifier nodes)
    _counter = [0]

    def _next_id() -> str:
        _counter[0] += 1
        return f"_{_counter[0]:06d}"

    # Emit axioms
    for axiom in sorted(program.axioms, key=lambda a: a._sort_key()):
        subj = URIRef(axiom.subject)
        pred = URIRef(axiom.predicate)
        if axiom.obj_is_literal:
            obj: URIRef | Literal = Literal(axiom.obj)
        else:
            obj = URIRef(axiom.obj)

        g.add((subj, pred, obj))

        # Emit scope annotations on a reifier node if non-trivial scope
        if _is_modal_or_scoped(axiom):
            # Use a blank-node stable ID based on the axiom sort key hash
            import hashlib

            key_hash = hashlib.sha256(axiom._sort_key().encode()).hexdigest()[:12]
            reifier = URIRef(LOGIC_NAMESPACE + "reifier/" + key_hash)
            # Classic reification (compatible with RDF 1.2 downstream processing)
            g.add((reifier, RDF.type, RDF.Statement))
            g.add((reifier, RDF.subject, subj))
            g.add((reifier, RDF.predicate, pred))
            g.add((reifier, RDF.object, obj))
            scope = axiom.scope
            if scope.standpoint is not None:
                g.add((reifier, LOGIC.standpoint, URIRef(scope.standpoint)))
            if scope.time is not None:
                g.add((reifier, LOGIC.time, Literal(scope.time)))
            if scope.confidence is not None:
                g.add(
                    (
                        reifier,
                        LOGIC.confidence,
                        Literal(scope.confidence, datatype=XSD.decimal),
                    )
                )
            if scope.modality != LogicModality.NONE:
                g.add(
                    (
                        reifier,
                        LOGIC.modality,
                        URIRef(LOGIC_NAMESPACE + scope.modality.value),
                    )
                )
            if scope.provenance is not None:
                g.add((reifier, LOGIC.provenance, URIRef(scope.provenance)))

    # Emit profiles
    for profile in sorted(program.profiles, key=lambda p: p._sort_key()):
        pid_iri = URIRef(LOGIC_NAMESPACE + profile.profile_id.value)
        g.add((pid_iri, RDF.type, LOGIC.SemanticProfile))
        if profile.complexity is not None:
            g.add(
                (
                    pid_iri,
                    LOGIC.complexityClass,
                    Literal(str(profile.complexity)),
                )
            )

    # Emit rules as logic:Rule nodes with classic reification for head/body
    for rule in sorted(program.rules, key=lambda r: r._sort_key()):
        rule_id = _next_id()
        rule_node = URIRef(LOGIC_NAMESPACE + "rule/" + rule_id)
        g.add((rule_node, RDF.type, LOGIC.Rule))

        # Head
        head = rule.head
        head_node = URIRef(LOGIC_NAMESPACE + "rule/" + rule_id + "/head")
        g.add((rule_node, LOGIC.head, head_node))
        g.add((head_node, RDF.type, RDF.Statement))
        g.add((head_node, RDF.subject, URIRef(head.subject)))
        g.add((head_node, RDF.predicate, URIRef(head.predicate)))
        if head.obj_is_literal:
            g.add((head_node, RDF.object, Literal(head.obj)))
        else:
            g.add((head_node, RDF.object, URIRef(head.obj)))

        # Body
        for i, body_atom in enumerate(sorted(rule.body, key=lambda a: a._sort_key())):
            body_node = URIRef(LOGIC_NAMESPACE + f"rule/{rule_id}/body/{i:04d}")
            g.add((rule_node, LOGIC.body, body_node))
            g.add((body_node, RDF.type, RDF.Statement))
            g.add((body_node, RDF.subject, URIRef(body_atom.subject)))
            g.add((body_node, RDF.predicate, URIRef(body_atom.predicate)))
            if body_atom.obj_is_literal:
                g.add((body_node, RDF.object, Literal(body_atom.obj)))
            else:
                g.add((body_node, RDF.object, URIRef(body_atom.obj)))

        # Rule scope
        scope = rule.scope
        if scope.standpoint is not None:
            g.add((rule_node, LOGIC.standpoint, URIRef(scope.standpoint)))
        if scope.time is not None:
            g.add((rule_node, LOGIC.time, Literal(scope.time)))
        if scope.confidence is not None:
            g.add(
                (
                    rule_node,
                    LOGIC.confidence,
                    Literal(scope.confidence, datatype=XSD.decimal),
                )
            )
        if scope.modality != LogicModality.NONE:
            g.add(
                (
                    rule_node,
                    LOGIC.modality,
                    URIRef(LOGIC_NAMESPACE + scope.modality.value),
                )
            )
        if scope.provenance is not None:
            g.add((rule_node, LOGIC.provenance, URIRef(scope.provenance)))

    banner = _generated_banner("Canonical RDF 1.2")
    content = _serialize_graph(g, banner)
    if path is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    return ProjectionResult(
        target="canonical-rdf12",
        content=content,
        graph=g,
        preservation=meta_kind,
        complexity=meta_cx,
        lossy_drops=meta_drops,
        actual_drops=[],
    )


# --------------------------------------------------------------------------- #
# Projection report
# --------------------------------------------------------------------------- #


def build_projection_report(
    program: LogicProgram,
    projections: list[ProjectionResult],
    *,
    path: Path | None = None,
) -> Graph:
    """Build the projection report graph (``generated/logic/projection-report.ttl``).

    For each projection in *projections*, emits:

    * A ``logic:ProjectionTarget`` node with ``rdfs:label``.
    * ``logic:preservationKind`` linking to the declared kind IRI.
    * ``logic:complexityClass`` with the complexity string.
    * One ``gmeow:lossyDrop`` literal per structural drop note.
    * One ``gmeow:lossyDrop`` literal per actual concrete drop (prefixed
      ``"actual: "`` so they are distinguishable from structural notes).
    * ``logic:axiomCount``, ``logic:ruleCount``, ``logic:profileCount`` for the
      source program.

    Then :func:`assert_no_overclaim` is called for each projection so that an
    overclaim blocks the report from being serialized.

    Args:
        program: The source program (used for counts).
        projections: List of :class:`ProjectionResult` instances.
        path: When given, the Turtle text is written there.

    Returns:
        The rdflib Graph of the report.
    """
    g = Graph()
    _bind_prefixes(g)

    report_iri = URIRef(LOGIC_NAMESPACE + "projection-report")
    g.add((report_iri, RDF.type, LOGIC.ProjectionReport))
    g.add(
        (
            report_iri,
            LOGIC.axiomCount,
            Literal(len(program.axioms), datatype=XSD.integer),
        )
    )
    g.add(
        (
            report_iri,
            LOGIC.ruleCount,
            Literal(len(program.rules), datatype=XSD.integer),
        )
    )
    g.add(
        (
            report_iri,
            LOGIC.profileCount,
            Literal(len(program.profiles), datatype=XSD.integer),
        )
    )

    for proj in sorted(projections, key=lambda p: p.target):
        # Overclaim check per projection
        assert_no_overclaim(proj.target, proj.preservation, proj.actual_drops)

        target_iri = URIRef(LOGIC_NAMESPACE + "target/" + proj.target)
        g.add((report_iri, LOGIC.hasProjection, target_iri))
        g.add((target_iri, RDF.type, LOGIC.ProjectionTarget))
        g.add((target_iri, RDFS.label, Literal(proj.target)))
        g.add(
            (
                target_iri,
                LOGIC_PRESERVATION_KIND,
                URIRef(LOGIC_NAMESPACE + proj.preservation.value),
            )
        )
        g.add(
            (
                target_iri,
                LOGIC_COMPLEXITY_CLASS,
                Literal(proj.complexity),
            )
        )
        # Structural lossy-drop notes (from _TARGET_META)
        for drop_note in sorted(proj.lossy_drops):
            g.add((target_iri, GMEOW_LOSSY_DROP, Literal(drop_note)))
        # Concrete actual drops from this run
        for actual in sorted(proj.actual_drops):
            g.add((target_iri, GMEOW_LOSSY_DROP, Literal("actual: " + actual)))

    if path is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
        banner = (
            "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n"
            "# Preservation loss ledger for all logic: projections.\n"
        )
        content = _serialize_graph(g, banner)
        path.write_text(content, encoding="utf-8")

    return g

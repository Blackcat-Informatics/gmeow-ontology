# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Front-end parser: ``logic:`` RDF 1.2 source graph → :class:`~.logic_ir.LogicProgram`.

This module parses a ``logic:``-vocabulary RDF graph (passed as an rdflib
:class:`~rdflib.Graph` or a filesystem path) and produces a typed
:class:`~.logic_ir.LogicProgram` IR together with a list of
:class:`Diagnostic` messages for recoverable issues.

Parse contract
--------------
* **Fail-soft** on recoverable issues: an axiom with a missing predicate, a
  rule with no body, or an unrecognised profile IRI emits a ``WARNING``
  diagnostic and is skipped.
* **Raise** on unparsable input: an empty graph, a file that cannot be read,
  or a graph with no namespace bindings at all raises :class:`LogicParseError`.
* **Never silently skip**: every skipped element produces a named diagnostic so
  callers can audit what was lost (ETHOS fail-fast principle).

Recognised RDF patterns
-----------------------
The parser recognises four patterns in the source graph:

1. **Axioms** — triples whose predicate is in the ``logic:`` namespace.
2. **Rules** — blank-node or IRI nodes of type ``logic:Rule`` (if declared)
   with ``logic:head`` / ``logic:body`` links.  (*Rule syntax is forward-
   compatible: no ``logic:Rule`` triples → empty rules list, not an error.*)
3. **Profiles** — ``rdf:type logic:SemanticProfile`` declarations on any
   named individual.
4. **Contextual scope** — ``logic:confidence``, ``logic:standpoint``,
   ``logic:modality``, ``logic:time``, and ``logic:provenance`` annotations
   on an axiom's reifier (RDF 1.2 triple-term / ``rdf:reifies`` statement),
   or directly on a rule node.

Dependencies: rdflib, gmeow_tools.config (for ``LOGIC_NAMESPACE`` and
``PREFIXES``).  No other GMEOW modules; no I/O side effects other than reading
the graph.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from pathlib import Path

from rdflib import RDF, Graph, Literal, Namespace, URIRef
from rdflib.term import Node

from gmeow_tools.config import LOGIC_NAMESPACE, PREFIXES
from gmeow_tools.logic_ir import (
    ComplexityClass,
    ContextualScope,
    LogicAxiom,
    LogicModality,
    LogicProfile,
    LogicProgram,
    LogicRule,
    SemanticProfileId,
)

_log = logging.getLogger(__name__)

LOGIC = Namespace(LOGIC_NAMESPACE)

# IRIs of the six SemanticProfile individuals
_PROFILE_IRI_TO_ID: dict[str, SemanticProfileId] = {
    LOGIC_NAMESPACE + pid.value: pid for pid in SemanticProfileId
}

# Modality IRI → LogicModality (the world-type taxonomy in logic:World's
# definition is prose-only; we map the canonical world-type names)
_MODALITY_STR_TO_ENUM: dict[str, LogicModality] = {
    m.value: m for m in LogicModality if m != LogicModality.NONE
}


# --------------------------------------------------------------------------- #
# Diagnostics
# --------------------------------------------------------------------------- #


class Severity(str):
    """Severity token for :class:`Diagnostic`.

    Not an enum so it can be used as a plain string in assert messages without
    .value indirection.  Canonical values: ``"ERROR"``, ``"WARNING"``,
    ``"INFO"``.
    """


ERROR = Severity("ERROR")
WARNING = Severity("WARNING")
INFO = Severity("INFO")


@dataclass(frozen=True, slots=True)
class Diagnostic:
    """A structured diagnostic emitted during parsing.

    Attributes:
        severity: ``"ERROR"``, ``"WARNING"``, or ``"INFO"``.
        code: A short machine-readable code (e.g. ``"MISSING_PREDICATE"``,
            ``"UNKNOWN_PROFILE"``).
        message: A human-readable description of the issue.
        subject: The IRI or blank-node identifier of the problematic element,
            or ``None`` if the issue is graph-global.
    """

    severity: str
    code: str
    message: str
    subject: str | None = None


class LogicParseError(Exception):
    """Raised for unparsable input (empty graph, unreadable file, etc.)."""


# --------------------------------------------------------------------------- #
# Internal helpers
# --------------------------------------------------------------------------- #


def _bind_logic_prefixes(graph: Graph) -> None:
    """Bind the ``logic:`` prefix and other GMEOW prefixes onto ``graph``."""
    for prefix, iri in PREFIXES.items():
        graph.bind(prefix, iri, override=False)


def _str_or_none(node: Node | None) -> str | None:
    """Coerce an rdflib node to str, or return None."""
    if node is None:
        return None
    return str(node)


def _confidence_from_node(node: Node | None) -> float | None:
    """Parse a ``logic:confidence`` node to a float in [0, 1], or None."""
    if node is None:
        return None
    if isinstance(node, Literal):
        try:
            val = float(node)
            if 0.0 <= val <= 1.0:
                return val
        except (ValueError, TypeError):
            pass
    return None


def _modality_from_node(node: Node | None) -> LogicModality:
    """Resolve a modality annotation node to a :class:`LogicModality`."""
    if node is None:
        return LogicModality.NONE
    raw = str(node)
    # Accept a full IRI like logic:alethic or just the local name
    if raw.startswith(LOGIC_NAMESPACE):
        raw = raw[len(LOGIC_NAMESPACE) :]
    return _MODALITY_STR_TO_ENUM.get(raw.lower(), LogicModality.NONE)


def _scope_from_graph(
    graph: Graph,
    node: Node,
    diagnostics: list[Diagnostic],
) -> ContextualScope:
    """Extract a :class:`ContextualScope` from annotations on ``node``."""
    standpoint = _str_or_none(graph.value(node, LOGIC.standpoint))
    time_val = _str_or_none(graph.value(node, LOGIC.time))
    conf_node = graph.value(node, LOGIC.confidence)
    confidence = _confidence_from_node(conf_node)
    if conf_node is not None and confidence is None:
        diagnostics.append(
            Diagnostic(
                severity=WARNING,
                code="INVALID_CONFIDENCE",
                message=(
                    f"confidence value {conf_node!r} is not a float in [0, 1]; ignored"
                ),
                subject=_str_or_none(node),
            )
        )
    modality = _modality_from_node(graph.value(node, LOGIC.modality))
    provenance = _str_or_none(graph.value(node, LOGIC.provenance))
    return ContextualScope(
        standpoint=standpoint,
        time=time_val,
        confidence=confidence,
        modality=modality,
        provenance=provenance,
    )


# --------------------------------------------------------------------------- #
# Axiom extraction
# --------------------------------------------------------------------------- #


def _extract_axioms(
    graph: Graph,
    diagnostics: list[Diagnostic],
) -> list[LogicAxiom]:
    """Collect all triples whose predicate (or rdf:type object) is ``logic:``."""
    axioms: list[LogicAxiom] = []

    # 1. Triples with a logic: predicate
    for s, p, o in graph:
        p_str = str(p)
        if not p_str.startswith(LOGIC_NAMESPACE):
            continue
        # Skip meta-triples (type declarations of the profile individuals etc.)
        if p == RDF.type:
            continue
        obj_is_literal = isinstance(o, Literal)
        obj_str = str(o)
        scope = ContextualScope()
        try:
            axiom = LogicAxiom(
                subject=str(s),
                predicate=p_str,
                obj=obj_str,
                obj_is_literal=obj_is_literal,
                scope=scope,
            )
        except ValueError as exc:
            diagnostics.append(
                Diagnostic(
                    severity=WARNING,
                    code="MALFORMED_AXIOM",
                    message=str(exc),
                    subject=str(s),
                )
            )
            continue
        axioms.append(axiom)

    # 2. rdf:type triples whose *object* is a logic: class
    for s, _, o in graph.triples((None, RDF.type, None)):
        o_str = str(o)
        if not o_str.startswith(LOGIC_NAMESPACE):
            continue
        # Avoid re-adding ontology-header type declarations (owl:Ontology etc.)
        # Only include rdf:type axioms where subject is NOT the logic: namespace itself
        if str(s).startswith(LOGIC_NAMESPACE):
            continue
        try:
            axiom = LogicAxiom(
                subject=str(s),
                predicate=str(RDF.type),
                obj=o_str,
                obj_is_literal=False,
                scope=ContextualScope(),
            )
        except ValueError as exc:
            diagnostics.append(
                Diagnostic(
                    severity=WARNING,
                    code="MALFORMED_AXIOM",
                    message=str(exc),
                    subject=str(s),
                )
            )
            continue
        axioms.append(axiom)

    return axioms


# --------------------------------------------------------------------------- #
# RDF 1.2 reified-statement scope extraction
# --------------------------------------------------------------------------- #


def _extract_scoped_axioms(
    graph: Graph,
    diagnostics: list[Diagnostic],
) -> list[LogicAxiom]:
    """Extract axioms from RDF 1.2 reified statements with contextual scope.

    Looks for nodes connected via ``rdf:reifies`` to a triple-term, or
    classic reified statements (``rdf:subject`` / ``rdf:predicate`` /
    ``rdf:object``), carrying ``logic:`` scope annotations.

    Both ``rdf:reifies`` (RDF 1.2) and the classic reification vocabulary are
    checked for maximum compatibility with the rdflib version in use.
    """
    axioms: list[LogicAxiom] = []

    # RDF 1.2 style: reifier node with rdf:reifies pointing at a triple term
    # rdflib >= 7.0 exposes rdf:reifies; we use the IRI directly for safety
    rdf_reifies = URIRef("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies")

    for reifier, _, triple_term in graph.triples((None, rdf_reifies, None)):
        scope = _scope_from_graph(graph, reifier, diagnostics)
        # If the triple-term is itself parseable as (s, p, o) via rdflib's
        # triple-term API we extract it; otherwise fall back to classic form.
        try:
            # rdflib triple-terms expose subject/predicate/object attributes
            t_s = triple_term.subject  # type: ignore[attr-defined]
            t_p = triple_term.predicate  # type: ignore[attr-defined]
            t_o = triple_term.object  # type: ignore[attr-defined]
            p_str = str(t_p)
            if not p_str.startswith(LOGIC_NAMESPACE) and t_p != RDF.type:
                continue
            try:
                axiom = LogicAxiom(
                    subject=str(t_s),
                    predicate=p_str,
                    obj=str(t_o),
                    obj_is_literal=isinstance(t_o, Literal),
                    scope=scope,
                )
                axioms.append(axiom)
            except ValueError as exc:
                diagnostics.append(
                    Diagnostic(
                        severity=WARNING,
                        code="MALFORMED_SCOPED_AXIOM",
                        message=str(exc),
                        subject=_str_or_none(reifier),
                    )
                )
        except AttributeError:
            pass  # triple_term has no .subject — not a triple-term node

    # Classic reification (rdf:Statement nodes) with logic: scope annotations
    for stmt in graph.subjects(RDF.type, RDF.Statement):
        scope = _scope_from_graph(graph, stmt, diagnostics)
        # Only process if there's at least one scope annotation
        if scope == ContextualScope():
            continue
        t_s = graph.value(stmt, RDF.subject)
        t_p = graph.value(stmt, RDF.predicate)
        t_o = graph.value(stmt, RDF.object)
        if t_p is None:
            diagnostics.append(
                Diagnostic(
                    severity=WARNING,
                    code="MISSING_PREDICATE",
                    message="rdf:Statement node has no rdf:predicate; skipped",
                    subject=_str_or_none(stmt),
                )
            )
            continue
        p_str = str(t_p)
        if not p_str.startswith(LOGIC_NAMESPACE) and t_p != RDF.type:
            continue
        try:
            axiom = LogicAxiom(
                subject=str(t_s) if t_s else "",
                predicate=p_str,
                obj=str(t_o) if t_o else "",
                obj_is_literal=isinstance(t_o, Literal),
                scope=scope,
            )
            axioms.append(axiom)
        except ValueError as exc:
            diagnostics.append(
                Diagnostic(
                    severity=WARNING,
                    code="MALFORMED_SCOPED_AXIOM",
                    message=str(exc),
                    subject=_str_or_none(stmt),
                )
            )

    return axioms


# --------------------------------------------------------------------------- #
# Profile extraction
# --------------------------------------------------------------------------- #


def _extract_profiles(
    graph: Graph,
    diagnostics: list[Diagnostic],
) -> list[LogicProfile]:
    """Collect ``logic:SemanticProfile`` declarations from the graph."""
    profiles: list[LogicProfile] = []
    seen: set[str] = set()

    for individual in graph.subjects(RDF.type, LOGIC.SemanticProfile):
        iri_str = str(individual)
        profile_id = _PROFILE_IRI_TO_ID.get(iri_str)
        if profile_id is None:
            diagnostics.append(
                Diagnostic(
                    severity=WARNING,
                    code="UNKNOWN_PROFILE",
                    message=(
                        f"{iri_str!r} is declared as logic:SemanticProfile "
                        "but is not a recognised named individual; skipped"
                    ),
                    subject=iri_str,
                )
            )
            continue
        if iri_str in seen:
            continue
        seen.add(iri_str)

        # Extract logic:complexityClass if present
        complexity_node = graph.value(individual, LOGIC.complexityClass)
        complexity: ComplexityClass | None = None
        if complexity_node is not None:
            label = str(complexity_node).strip()
            try:
                if not label:
                    raise ValueError("empty")
                complexity = ComplexityClass(label)
            except ValueError:
                diagnostics.append(
                    Diagnostic(
                        severity=WARNING,
                        code="INVALID_COMPLEXITY_CLASS",
                        message=(
                            f"complexityClass {label!r} is not a recognised "
                            "ComplexityClass value; ignored"
                        ),
                        subject=iri_str,
                    )
                )

        profiles.append(LogicProfile(profile_id=profile_id, complexity=complexity))

    return profiles


# --------------------------------------------------------------------------- #
# Rule extraction (forward-compatible: absent logic:Rule triples → empty list)
# --------------------------------------------------------------------------- #


def _extract_rules(
    graph: Graph,
    diagnostics: list[Diagnostic],
) -> list[LogicRule]:
    """Collect ``logic:Rule`` nodes (head + body axioms) from the graph.

    Rule syntax is forward-compatible: if no ``logic:Rule`` nodes are present
    in the graph, an empty list is returned without a diagnostic (the rule
    surface is minted in a later task).  A rule node with a missing head emits
    a WARNING and is skipped.
    """
    logic_rule = LOGIC.Rule
    logic_head = LOGIC.head
    logic_body = LOGIC.body
    logic_negated_body = LOGIC.negatedBody

    rules: list[LogicRule] = []

    for rule_node in graph.subjects(RDF.type, logic_rule):
        scope = _scope_from_graph(graph, rule_node, diagnostics)

        # Head
        head_node = graph.value(rule_node, logic_head)
        if head_node is None:
            diagnostics.append(
                Diagnostic(
                    severity=WARNING,
                    code="MISSING_RULE_HEAD",
                    message="logic:Rule node has no logic:head; skipped",
                    subject=_str_or_none(rule_node),
                )
            )
            continue

        head_s = graph.value(head_node, RDF.subject)
        head_p = graph.value(head_node, RDF.predicate)
        head_o = graph.value(head_node, RDF.object)
        if head_p is None:
            diagnostics.append(
                Diagnostic(
                    severity=WARNING,
                    code="MALFORMED_RULE_HEAD",
                    message="logic:head node has no rdf:predicate; skipped",
                    subject=_str_or_none(rule_node),
                )
            )
            continue
        try:
            head_axiom = LogicAxiom(
                subject=str(head_s) if head_s else "",
                predicate=str(head_p),
                obj=str(head_o) if head_o else "",
                obj_is_literal=isinstance(head_o, Literal),
            )
        except ValueError as exc:
            diagnostics.append(
                Diagnostic(
                    severity=WARNING,
                    code="MALFORMED_RULE_HEAD",
                    message=str(exc),
                    subject=_str_or_none(rule_node),
                )
            )
            continue

        # Body (zero or more).  Positive body atoms are read via logic:body
        # (negated=False); negated body atoms — the StratifiedNAF negation-as-
        # failure surface (issue #502) — are read via logic:negatedBody with
        # parallel structure and negated=True.  Both predicates are optional, so
        # a purely positive rule keeps its exact pre-#502 parse.
        body_axioms: list[LogicAxiom] = []
        for body_predicate, negated in (
            (logic_body, False),
            (logic_negated_body, True),
        ):
            for body_node in graph.objects(rule_node, body_predicate):
                body_s = graph.value(body_node, RDF.subject)
                body_p = graph.value(body_node, RDF.predicate)
                body_o = graph.value(body_node, RDF.object)
                if body_p is None:
                    diagnostics.append(
                        Diagnostic(
                            severity=WARNING,
                            code="MALFORMED_RULE_BODY",
                            message=(
                                "logic:body node has no rdf:predicate; "
                                "body atom skipped"
                            ),
                            subject=_str_or_none(rule_node),
                        )
                    )
                    continue
                try:
                    body_axioms.append(
                        LogicAxiom(
                            subject=str(body_s) if body_s else "",
                            predicate=str(body_p),
                            obj=str(body_o) if body_o else "",
                            obj_is_literal=isinstance(body_o, Literal),
                            negated=negated,
                        )
                    )
                except ValueError as exc:
                    diagnostics.append(
                        Diagnostic(
                            severity=WARNING,
                            code="MALFORMED_RULE_BODY",
                            message=str(exc),
                            subject=_str_or_none(rule_node),
                        )
                    )

        rules.append(
            LogicRule(
                head=head_axiom,
                body=tuple(body_axioms),
                scope=scope,
            )
        )

    return rules


# --------------------------------------------------------------------------- #
# Public API
# --------------------------------------------------------------------------- #


def parse_logic_source(
    graph_or_path: Graph | Path | str,
    *,
    source_iri: str | None = None,
) -> tuple[LogicProgram, list[Diagnostic]]:
    """Parse a ``logic:`` RDF source into a :class:`~.logic_ir.LogicProgram`.

    Args:
        graph_or_path: An rdflib :class:`~rdflib.Graph`, or a :class:`~pathlib.Path`
            / string path to a Turtle file.  When a path is supplied the file
            is parsed with rdflib's ``"turtle"`` format.
        source_iri: Optional IRI to record as the program's provenance source.
            When ``None`` and ``graph_or_path`` is a path, the file URI is used.

    Returns:
        A ``(LogicProgram, diagnostics)`` pair.  The ``LogicProgram`` contains
        all recognised axioms, rules, and profiles.  ``diagnostics`` lists any
        recoverable issues found during parsing.

    Raises:
        LogicParseError: When the input is empty, unreadable, or fundamentally
            unparsable (e.g. a file that does not exist).
    """
    diagnostics: list[Diagnostic] = []

    # ---- Graph loading ----
    if isinstance(graph_or_path, Graph):
        graph = graph_or_path
    else:
        path = Path(graph_or_path)
        if not path.exists():
            raise LogicParseError(f"Source file does not exist: {path}")
        if source_iri is None:
            source_iri = path.as_uri()
        graph = Graph()
        try:
            graph.parse(str(path), format="turtle")
        except Exception as exc:
            raise LogicParseError(f"Failed to parse {path}: {exc}") from exc

    _bind_logic_prefixes(graph)

    if len(graph) == 0:
        raise LogicParseError(
            "Source graph is empty — nothing to parse.  "
            "Pass a non-empty graph or a Turtle file with logic: triples."
        )

    # ---- Extraction ----
    plain_axioms = _extract_axioms(graph, diagnostics)
    scoped_axioms = _extract_scoped_axioms(graph, diagnostics)

    # Merge, deduplicate by content (set handles frozen dataclass equality)
    all_axioms_set: set[LogicAxiom] = set(plain_axioms) | set(scoped_axioms)
    all_axioms = list(all_axioms_set)

    profiles = _extract_profiles(graph, diagnostics)
    rules = _extract_rules(graph, diagnostics)

    program = LogicProgram(
        axioms=tuple(all_axioms),
        rules=tuple(rules),
        profiles=tuple(profiles),
        source_iri=source_iri,
    )

    _log.debug(
        "parse_logic_source: %d axioms, %d rules, %d profiles, %d diagnostics",
        len(program.axioms),
        len(program.rules),
        len(program.profiles),
        len(diagnostics),
    )

    return program, diagnostics

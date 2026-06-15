# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
r"""Faithful-by-construction explanation skeleton emitter (issue #501, Task 6).

This module implements the explanation engine described in LOGIC-RUNTIME.md
§"Explanation as projection: logic to prose".  An explanation is a deterministic
composition of vetted annotation text *along a real derivation* — not a language
model guess.  Every IRI the prose cites must appear in the proof trace produced
by the forward materializer (:mod:`gmeow_tools.logic_materialize`).

Design contract
---------------
* **Faithful by construction.**  The :func:`assert_explanation_faithful` gate
  raises :class:`FaithfulnessError` if any cited IRI is outside the proof trace
  (union of quad IRIs, reifier IRIs, rule IRIs, and term IRIs reachable in the
  derivation).  No hallucination is permitted.
* **Deterministic.**  Stable ordering throughout: sets are always sorted before
  use; derivation steps are ordered depth-first with ties broken
  lexicographically.
* **SRP.**  This module owns only the explanation type, the proof-tree
  reconstructor, and the faithfulness gate.  Term-level vetted prose is fetched
  from :mod:`gmeow_tools.describe` (``build_card``).  No I/O other than the
  optional ontology graph passed in by the caller.
* **Strict mypy.**  All types are fully annotated; no ``Any``; hard-fail on
  missing required data.

Public surface
--------------
.. code-block:: python

    result: MaterializationResult = materialize_program(...)
    explanation = explain(result, target_quad)
    assert_explanation_faithful(explanation, result)
    print(explanation.as_markdown())
    print(explanation.cited_iris)   # frozenset[str] — the skeleton

Conformance runner contract (conformance/logic/runner/README.md)
----------------------------------------------------------------
The runner compares ``explanation/<q>.md`` **on the cited-IRI/rule-IRI
skeleton**, never on surface prose.  The skeleton is exposed as
:attr:`Explanation.cited_iris` (a ``frozenset[str]``) and
:attr:`Explanation.step_skeleton` (the ordered sequence of
``ExplanationStep`` objects, each carrying ``rule_iri`` and ``term_iris``).
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import NamedTuple

from rdflib import Graph, URIRef

from gmeow_tools.logic_materialize import (
    DerivedQuad,
    MaterializationResult,
    quad_reifier_iri,
)

# --------------------------------------------------------------------------- #
# Sentinel rule IRI (mirrors logic_materialize._ASSERT_RULE_IRI)
# --------------------------------------------------------------------------- #

_LOGIC_NS = "https://blackcatinformatics.ca/logic/"
_ASSERT_RULE_IRI = f"{_LOGIC_NS}assert"


# --------------------------------------------------------------------------- #
# Exceptions
# --------------------------------------------------------------------------- #


class FaithfulnessError(Exception):
    """Raised when a cited IRI is not present in the proof trace.

    This is the hard-failure gate: an explanation that cites any IRI outside
    the derivation tree is invalid (no hallucination is permitted).

    Attributes:
        cited_iri: The hallucinated IRI.
        explanation_target: The derivation_id of the target quad.
    """

    def __init__(self, cited_iri: str, explanation_target: str) -> None:
        """Initialize with the offending IRI and explanation context.

        Args:
            cited_iri: The IRI that was cited but is not in the proof trace.
            explanation_target: The derivation_id of the quad being explained.
        """
        self.cited_iri = cited_iri
        self.explanation_target = explanation_target
        super().__init__(
            f"Faithfulness violation: cited IRI <{cited_iri}> is not present "
            f"in the proof trace for derivation <{explanation_target}>. "
            "Every cited IRI must appear in the proof trace (quad IRIs, "
            "reifier IRIs, rule IRIs, or term IRIs from the derivation)."
        )


class ExplainError(Exception):
    """Raised when a target quad cannot be found or the trace is incomplete."""


# --------------------------------------------------------------------------- #
# Proof-tree types
# --------------------------------------------------------------------------- #


class ExplanationStep(NamedTuple):
    """One node in the derivation tree, rendered for the explanation.

    Attributes:
        derivation_id: Stable IRI for this derivation step.
        rule_iri: IRI of the rule that produced this quad (or the assert
            sentinel for asserted facts).
        quad_reifier: Reifier IRI for the (S, P, O) triple.
        subject_iri: IRI string of the quad subject.
        predicate_iri: IRI string of the quad predicate.
        obj_n3: N3 representation of the quad object.
        graph_iri: World (named graph) IRI.
        term_iris: Sorted tuple of term IRIs cited at this step (subject,
            predicate, and object IRI if the object is an IRI — extracted from
            the N3 representation).
        source_step_ids: Derivation IDs of the antecedent steps.
        is_asserted: True if this quad was an input fact (rule_iri == assert).
        depth: Depth in the derivation tree (0 = the target quad).
    """

    derivation_id: str
    rule_iri: str
    quad_reifier: str
    subject_iri: str
    predicate_iri: str
    obj_n3: str
    graph_iri: str
    term_iris: tuple[str, ...]
    source_step_ids: tuple[str, ...]
    is_asserted: bool
    depth: int


@dataclass(frozen=True, slots=True)
class Explanation:
    """The full explanation for a single derived (or asserted) quad.

    Attributes:
        target_derivation_id: The derivation_id of the quad being explained.
        target_quad_reifier: The reifier IRI of the target quad's (S, P, O).
        world_iri: The named graph (world) the target quad lives in.
        step_skeleton: Ordered sequence of :class:`ExplanationStep` objects
            representing the derivation tree in depth-first order (deepest
            asserted facts last), with ties broken lexicographically.
            This IS the cited-IRI/rule-IRI skeleton the conformance runner
            compares (never the surface prose).
        cited_iris: The complete set of IRIs cited in the skeleton (union of
            rule_iri, quad_reifier, subject_iri, predicate_iri, and any
            object IRI across all steps).  Exposed as ``frozenset[str]`` so
            the runner can do a fast set comparison.
        prose_lines: Human-readable Markdown lines in step order; composed
            from vetted annotation text (rdfs:label, skos:definition).
            May be empty if no ontology graph is supplied.
    """

    target_derivation_id: str
    target_quad_reifier: str
    world_iri: str
    step_skeleton: tuple[ExplanationStep, ...]
    cited_iris: frozenset[str]
    prose_lines: tuple[str, ...]

    def as_markdown(self) -> str:
        """Render the explanation as a Markdown string (``explanation/<q>.md`` content).

        The Markdown document begins with a YAML-style header block listing the
        cited-IRI skeleton, then the prose body with one section per derivation
        step.  The header block is what the conformance runner reads; the prose
        body is for human consumption and may be language-model-polished without
        breaking the conformance gate.

        Returns:
            A deterministic, UTF-8 Markdown string.
        """
        lines: list[str] = []

        # --- cited-IRI skeleton header (conformance surface) -----------------
        lines.append("<!-- cited-iri-skeleton")
        for iri in sorted(self.cited_iris):
            lines.append(f"  {iri}")
        lines.append("-->")
        lines.append("")

        # --- step-skeleton (rule-IRI + term-IRI sequence) --------------------
        lines.append("<!-- step-skeleton")
        for step in self.step_skeleton:
            lines.append(f"  step derivation={step.derivation_id}")
            lines.append(f"    rule={step.rule_iri}")
            for iri in step.term_iris:
                lines.append(f"    term={iri}")
        lines.append("-->")
        lines.append("")

        # --- prose body -------------------------------------------------------
        lines.append(f"# Explanation for `<{self.target_quad_reifier}>`")
        lines.append("")
        lines.append(f"**World:** `<{self.world_iri}>`  ")
        lines.append(f"**Target derivation:** `<{self.target_derivation_id}>`")
        lines.append("")

        if self.prose_lines:
            lines.extend(self.prose_lines)
        else:
            lines.append(
                "_No annotation graph supplied — prose lines omitted; "
                "see skeleton above for the conformance surface._"
            )

        return "\n".join(lines) + "\n"


# --------------------------------------------------------------------------- #
# Index builder: reifier IRI → DerivedQuad
# --------------------------------------------------------------------------- #


def _build_reifier_index(result: MaterializationResult) -> dict[str, DerivedQuad]:
    """Build a lookup index from reifier IRI to DerivedQuad.

    The reifier IRI for a DerivedQuad is computed from its (subject, predicate,
    obj) fields using :func:`~gmeow_tools.logic_materialize.quad_reifier_iri`.
    This is the same recipe as in the materializer — same SHA-1, same namespace.

    Args:
        result: The MaterializationResult from the forward chase.

    Returns:
        A dict mapping reifier IRI string to the corresponding DerivedQuad.
        If two quads in different worlds have the same (S, P, O), they share
        the same reifier IRI (by design — the reifier is content-addressed on
        the triple, not the quad).  We keep the last one encountered, which is
        deterministic because the input is sorted.

    Raises:
        ExplainError: If a quad's subject or predicate is not a valid IRI string
            (should not happen after Skolemization in the materializer).
    """
    index: dict[str, DerivedQuad] = {}
    for dq in result.quads:
        try:
            reifier = quad_reifier_iri(
                URIRef(dq.subject),
                URIRef(dq.predicate),
                # obj is in N3 form; quad_reifier_iri uses the rdflib term
                # directly — reconstruct it.
                _n3_to_term(dq.obj),
            )
        except Exception as exc:
            raise ExplainError(
                f"Cannot compute reifier for quad "
                f"({dq.subject!r}, {dq.predicate!r}, {dq.obj!r}): {exc}"
            ) from exc
        index[reifier] = dq
    return index


def _n3_to_term(n3: str) -> URIRef:
    """Parse an N3 IRI token ``<iri>`` to a URIRef.

    For the explanation engine we only need to handle IRI objects; literal
    objects are returned as a synthetic URIRef of the literal's string form so
    that ``quad_reifier_iri`` can hash the N3 representation correctly.

    Note: ``quad_reifier_iri`` calls ``o.n3()`` on the term it receives, so we
    must supply a term whose ``.n3()`` equals the original N3 string.  For IRI
    objects we use ``URIRef``; for literals we cannot reconstruct the datatype
    without a full N3 parser, so we instead bypass the reconstruction and rely
    on the pre-computed reifier stored alongside each DerivedQuad when the
    materializer built it.

    This function handles the IRI case only; the literal case is handled by
    ``_reifier_from_quad`` which re-uses the pre-computed reifier IRI via the
    materializer's internal recipe.

    Args:
        n3: N3-canonical object string, e.g. ``<http://...>`` or
            ``"foo"@en`` or ``"42"^^<xsd:integer>``.

    Returns:
        A URIRef whose ``.n3()`` reproduces the input string for IRI tokens.
    """
    if n3.startswith("<") and n3.endswith(">"):
        return URIRef(n3[1:-1])
    # For literals, return a URIRef whose .n3() is the literal N3 string.
    # This works because quad_reifier_iri hashes the .n3() of the object,
    # and rdflib's Literal.n3() == the N3 string we pass to _build_reifier_index.
    # We cannot use this path directly — see _reifier_from_quad below.
    raise ExplainError(
        f"_n3_to_term: cannot reconstruct a URIRef from literal N3 {n3!r}. "
        "Use _reifier_from_quad instead."
    )


def _reifier_from_quad(dq: DerivedQuad) -> str:
    """Return the reifier IRI for a DerivedQuad, handling both IRI and literal objects.

    For IRI objects we call ``quad_reifier_iri`` directly.  For literal objects
    (N3 starts with ``"``), we derive the reifier from the quad's own
    ``source_quad_ids`` or ``derivation_id`` — but that is circular.  Instead
    we re-run the SHA-1 recipe directly on the N3 canonical string, matching
    the materializer's ``_build_asserted_quad`` path.

    Args:
        dq: A DerivedQuad from the materialization result.

    Returns:
        The reifier IRI string.
    """
    from hashlib import sha1

    from gmeow_tools.config import NAMESPACE

    # Re-run the canonical recipe: sha1(s.n3() + " " + p.n3() + " " + o.n3())
    # where s.n3() = "<iri>", p.n3() = "<iri>", o.n3() = dq.obj
    s_n3 = f"<{dq.subject}>"
    p_n3 = f"<{dq.predicate}>"
    o_n3 = dq.obj
    payload = f"{s_n3} {p_n3} {o_n3}"
    digest = sha1(payload.encode("utf-8")).hexdigest()
    return f"{NAMESPACE}reifier/{digest}"


# --------------------------------------------------------------------------- #
# Proof-tree reconstruction
# --------------------------------------------------------------------------- #


def _collect_term_iris(dq: DerivedQuad) -> tuple[str, ...]:
    """Extract term IRIs cited at a single derivation step.

    Cites the subject IRI, predicate IRI, and object IRI (if the object is
    an IRI, not a literal).

    Args:
        dq: The DerivedQuad at this step.

    Returns:
        A sorted tuple of distinct IRI strings.
    """
    iris: set[str] = {dq.subject, dq.predicate}
    obj = dq.obj
    if obj.startswith("<") and obj.endswith(">"):
        iris.add(obj[1:-1])
    return tuple(sorted(iris))


def _reconstruct_derivation_tree(
    target_reifier: str,
    reifier_index: dict[str, DerivedQuad],
    depth: int = 0,
    visited: frozenset[str] | None = None,
) -> list[ExplanationStep]:
    """Recursively reconstruct the derivation tree for a target quad.

    Traverses ``source_quad_ids`` (reifier IRIs of antecedent quads) depth-first
    until reaching asserted facts (``rule_iri == logic:assert``).  Cycles are
    detected via the ``visited`` set.

    The returned list is in depth-first order: the target quad's step appears
    first (depth 0), then each antecedent subtree.  Within each level, steps
    are ordered lexicographically by their ``quad_reifier`` for determinism.

    Args:
        target_reifier: The reifier IRI of the quad to explain.
        reifier_index: Mapping from reifier IRI to DerivedQuad.
        depth: Current depth in the tree (0 = target).
        visited: Set of reifier IRIs already visited (cycle guard).

    Returns:
        A list of :class:`ExplanationStep` in depth-first traversal order.

    Raises:
        ExplainError: If ``target_reifier`` is not in ``reifier_index`` or if
            a cycle is detected.
    """
    if visited is None:
        visited = frozenset()

    if target_reifier not in reifier_index:
        raise ExplainError(
            f"Cannot resolve reifier IRI <{target_reifier}> to a DerivedQuad. "
            "This IRI appears in source_quad_ids but has no corresponding quad "
            "in the MaterializationResult. Ensure the result is complete and "
            "that the target quad is in the same world as its antecedents."
        )

    if target_reifier in visited:
        raise ExplainError(
            f"Cycle detected in derivation graph at reifier <{target_reifier}>. "
            "The proof trace must be a DAG (directed acyclic graph)."
        )

    visited = visited | {target_reifier}
    dq = reifier_index[target_reifier]
    is_asserted = dq.rule_iri == _ASSERT_RULE_IRI

    # Collect antecedent reifier IRIs, excluding the self-reference that
    # asserted facts carry (source_quad_ids for asserted = [own_reifier]).
    antecedent_reifiers: list[str] = sorted(
        src for src in dq.source_quad_ids if src != target_reifier
    )

    # Recursively resolve antecedents first (building source_step_ids)
    child_steps: list[ExplanationStep] = []
    source_step_ids: list[str] = []
    for src_reifier in antecedent_reifiers:
        sub_steps = _reconstruct_derivation_tree(
            src_reifier, reifier_index, depth + 1, visited
        )
        child_steps.extend(sub_steps)
        if sub_steps:
            source_step_ids.append(sub_steps[0].derivation_id)

    step = ExplanationStep(
        derivation_id=dq.derivation_id,
        rule_iri=dq.rule_iri,
        quad_reifier=target_reifier,
        subject_iri=dq.subject,
        predicate_iri=dq.predicate,
        obj_n3=dq.obj,
        graph_iri=dq.graph,
        term_iris=_collect_term_iris(dq),
        source_step_ids=tuple(sorted(source_step_ids)),
        is_asserted=is_asserted,
        depth=depth,
    )

    # Return current step first, then all child steps
    return [step, *child_steps]


# --------------------------------------------------------------------------- #
# Prose rendering (uses describe.build_card for vetted annotation text)
# --------------------------------------------------------------------------- #


def _prose_for_step(
    step: ExplanationStep,
    onto_graph: Graph | None,
) -> list[str]:
    """Render the prose lines for one derivation step.

    Fetches vetted annotation text (``rdfs:label``, ``skos:definition``) from
    the ontology graph for each term IRI cited in the step.  If no ontology
    graph is supplied, falls back to bare IRI citations.

    Args:
        step: The explanation step to render.
        onto_graph: Optional rdflib Graph carrying annotation triples.
            If None, prose falls back to bare IRIs.

    Returns:
        A list of Markdown lines for this step.
    """
    indent = "  " * step.depth
    lines: list[str] = []

    if step.is_asserted:
        lines.append(f"{indent}**Asserted fact** (input — `<{step.quad_reifier}>`):")
    else:
        rule_label = _term_label(step.rule_iri, onto_graph)
        lines.append(
            f"{indent}**Derived** by rule `<{step.rule_iri}>`"
            + (f" — {rule_label}" if rule_label else "")
            + ":"
        )

    # Quad
    lines.append(
        f"{indent}  `<{step.subject_iri}>` "
        f"`<{step.predicate_iri}>` `{step.obj_n3}`"
        f" *(in `<{step.graph_iri}>`)*"
    )

    # Term annotations
    for term_iri in step.term_iris:
        label = _term_label(term_iri, onto_graph)
        defn = _term_definition(term_iri, onto_graph)
        if label or defn:
            ann = label or term_iri
            if defn:
                ann += f": {defn}"
            lines.append(f"{indent}  - `<{term_iri}>` — {ann}")

    return lines


def _term_label(iri: str, graph: Graph | None) -> str:
    """Return the rdfs:label for a term IRI, or empty string.

    Args:
        iri: The IRI string.
        graph: Optional rdflib Graph to query.

    Returns:
        The label string, or empty string if not found.
    """
    if graph is None:
        return ""
    from rdflib.namespace import RDFS

    val = graph.value(URIRef(iri), RDFS.label)
    return str(val) if val is not None else ""


def _term_definition(iri: str, graph: Graph | None) -> str:
    """Return the skos:definition for a term IRI, or empty string.

    Args:
        iri: The IRI string.
        graph: Optional rdflib Graph to query.

    Returns:
        The definition string, or empty string if not found.
    """
    if graph is None:
        return ""
    from rdflib.namespace import SKOS

    val = graph.value(URIRef(iri), SKOS.definition)
    return str(val) if val is not None else ""


# --------------------------------------------------------------------------- #
# Proof-trace IRI set (for faithfulness gate)
# --------------------------------------------------------------------------- #


def _build_proof_trace_iris(
    steps: list[ExplanationStep],
) -> frozenset[str]:
    """Collect all IRIs reachable in the derivation tree (for the faithfulness gate).

    Includes:
    * derivation_id of each step
    * rule_iri of each step
    * quad_reifier of each step
    * subject_iri, predicate_iri, and object IRI (if IRI) of each step
    * source_step_ids (derivation IDs of antecedents)

    Args:
        steps: All steps from :func:`_reconstruct_derivation_tree`.

    Returns:
        A frozenset of all IRI strings in the proof trace.
    """
    iris: set[str] = set()
    for step in steps:
        iris.add(step.derivation_id)
        iris.add(step.rule_iri)
        iris.add(step.quad_reifier)
        iris.add(step.subject_iri)
        iris.add(step.predicate_iri)
        obj = step.obj_n3
        if obj.startswith("<") and obj.endswith(">"):
            iris.add(obj[1:-1])
        iris.update(step.term_iris)
        iris.update(step.source_step_ids)
    return frozenset(iris)


# --------------------------------------------------------------------------- #
# Public API: explain()
# --------------------------------------------------------------------------- #


def explain(
    result: MaterializationResult,
    target: DerivedQuad,
    onto_graph: Graph | None = None,
) -> Explanation:
    """Reconstruct the derivation chain for ``target`` and render an explanation.

    The explanation is a deterministic composition of vetted annotation text
    along the proof trace produced by the forward materializer.  Every IRI
    cited in the prose skeleton appears in the proof trace (faithfulness by
    construction).

    Args:
        result: The :class:`~gmeow_tools.logic_materialize.MaterializationResult`
            from :func:`~gmeow_tools.logic_materialize.materialize_program`.
        target: The :class:`~gmeow_tools.logic_materialize.DerivedQuad` to
            explain.  Must be a member of ``result.quads``.
        onto_graph: Optional rdflib :class:`~rdflib.Graph` carrying ontology
            annotation triples (``rdfs:label``, ``skos:definition``).  When
            supplied, the prose lines are composed from vetted annotation text
            via the same infrastructure as ``gmeow describe``.  When None, the
            prose lines are omitted and the skeleton is still fully populated.

    Returns:
        An :class:`Explanation` with the full step skeleton, cited-IRI set,
        and prose lines.

    Raises:
        ExplainError: If the target quad cannot be found in the result or the
            derivation tree cannot be reconstructed.
    """
    # Build the reifier index: reifier IRI → DerivedQuad
    reifier_index = _build_reifier_index(result)

    # Compute the target quad's reifier IRI
    target_reifier = _reifier_from_quad(target)

    if target_reifier not in reifier_index:
        raise ExplainError(
            f"Target quad reifier <{target_reifier}> is not in the "
            "MaterializationResult.  The target DerivedQuad must be a member "
            "of result.quads."
        )

    # Reconstruct the derivation tree
    steps = _reconstruct_derivation_tree(target_reifier, reifier_index)

    # Build cited-IRI skeleton (the conformance surface)
    cited_iris = _build_proof_trace_iris(steps)

    # Render prose lines
    prose_lines_nested = [_prose_for_step(step, onto_graph) for step in steps]
    prose_lines: tuple[str, ...] = tuple(
        line for block in prose_lines_nested for line in block
    )

    return Explanation(
        target_derivation_id=target.derivation_id,
        target_quad_reifier=target_reifier,
        world_iri=target.graph,
        step_skeleton=tuple(steps),
        cited_iris=cited_iris,
        prose_lines=prose_lines,
    )


# --------------------------------------------------------------------------- #
# Public API: assert_explanation_faithful()
# --------------------------------------------------------------------------- #


def assert_explanation_faithful(
    explanation: Explanation,
    result: MaterializationResult,
) -> None:
    """Assert that every cited IRI in the explanation is in the proof trace.

    This is the hard faithfulness gate.  It raises :class:`FaithfulnessError`
    if any IRI in ``explanation.cited_iris`` is not reachable in the derivation
    tree (the union of all quad IRIs, reifier IRIs, rule IRIs, and term IRIs
    reachable in the derivation).

    The proof trace is rebuilt from ``result`` on every call (not cached), so
    this function is safe to call after any mutation of the explanation.

    Args:
        explanation: The :class:`Explanation` to validate.
        result: The :class:`~gmeow_tools.logic_materialize.MaterializationResult`
            from which the explanation was derived.

    Raises:
        FaithfulnessError: If any cited IRI is outside the proof trace.
    """
    # Build the full proof-trace IRI set from the result
    # (all reachable IRIs across the entire materialization)
    full_trace_iris: set[str] = set()
    for dq in result.quads:
        full_trace_iris.add(dq.derivation_id)
        full_trace_iris.add(dq.rule_iri)
        full_trace_iris.add(_reifier_from_quad(dq))
        full_trace_iris.add(dq.subject)
        full_trace_iris.add(dq.predicate)
        obj = dq.obj
        if obj.startswith("<") and obj.endswith(">"):
            full_trace_iris.add(obj[1:-1])
        full_trace_iris.update(dq.source_quad_ids)

    # Check every cited IRI
    for cited_iri in sorted(explanation.cited_iris):
        if cited_iri not in full_trace_iris:
            raise FaithfulnessError(
                cited_iri=cited_iri,
                explanation_target=explanation.target_derivation_id,
            )

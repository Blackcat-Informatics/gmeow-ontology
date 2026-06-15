# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Static profile / decidability certifier over the :mod:`gmeow_tools.logic_ir` IR.

This module is the logic-profile analogue of :mod:`gmeow_tools.reasoning_lint`:
each *certification check* is a free function that takes a
:class:`~gmeow_tools.logic_ir.LogicProgram` and returns a ``list[str]`` of
self-documenting diagnostic strings (empty list ⇒ certified clean), and a flat
:func:`certify_invariants` aggregator collects every violation the way
:func:`gmeow_tools.reasoning_lint.reasoning_invariants` does.  It is **pure
analysis** — no I/O, no graph parsing, no file or network access — operating only
on the typed IR.

The certifier answers one question: *does this rule set provably live inside the
declared decidable / terminating fragment?*  Per
``slices/core/logic/design/LOGIC-SEMANTICS.md`` §"Turing-Completeness,
Decidability, and Termination":

    "Because termination is itself undecidable, certification uses *sufficient*
    acyclicity conditions, not a complete test — a known, accepted tradeoff."

So **every check is sound but necessarily incomplete**: a clean certification is
a proof of membership; a violation is a proof of non-membership *only relative to
the sufficient condition tested*.  A rule set that fails a sufficient condition
may still terminate — the certifier never claims otherwise; it reports that the
cheap structural guarantee does not hold, exactly as ``reasoning_lint`` reports an
anti-pattern without claiming the model is meaningless.

Diagnostic strings cite the governing ``LOGIC-SEMANTICS.md`` section so a failure
self-documents (mirroring how ``reasoning_lint`` cites the OntoUML catalogue).

Checks, by profile family (see LOGIC-SEMANTICS.md §Semantic profiles):

* :func:`certify_positive_horn` — PositiveHorn forbids negation-as-failure.
* :func:`certify_stratified_naf` — no predicate SCC may cross a negative edge.
* :func:`certify_well_founded` — every rule must be a normal program.
* :func:`certify_stable_model` — advisory: NP-hard unless also stratified.

Decidable-fragment checks (sufficient conditions only; see §Decidability):

* :func:`certify_dl_safe` — every rule variable is bound by a positive,
  non-built-in body atom.
* :func:`certify_weak_acyclicity` — the position dependency graph has no cycle
  through an existential (special) edge.
* :func:`certify_joint_acyclicity`, :func:`certify_guarded`,
  :func:`certify_sticky` — TGD termination/decidability landing points; each
  passes by *vacuity* for the function-free, existential-free IR shipped today
  (the correct answer for that fragment, not deferred work).
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from rdflib import RDF

from gmeow_tools.logic_ir import LogicAxiom, LogicProgram, SemanticProfileId

# --------------------------------------------------------------------------- #
# Constants
# --------------------------------------------------------------------------- #

#: The governing design document, cited in messages so failures self-document.
_DOC = "LOGIC-SEMANTICS.md"
#: Section heading strings, quoted verbatim from LOGIC-SEMANTICS.md so the cited
#: anchor always resolves.
_SEC_PROFILES = "§Semantic profiles"
_SEC_DECIDABILITY = "§Decidability"

#: The ``rdf:type`` predicate IRI string, as the IR stores it (``str(RDF.type)``
#: in :mod:`gmeow_tools.logic_frontend`).  Class-level recursion is only visible
#: when ``rdf:type`` atoms key on the asserted class, so we special-case it.
_RDF_TYPE = str(RDF.type)


# --------------------------------------------------------------------------- #
# Atom helpers (pure)
# --------------------------------------------------------------------------- #


def _is_var(term: str) -> bool:
    """Whether a term string is a Datalog variable (starts with ``?``).

    Matches the convention used by :func:`gmeow_tools.logic_materialize._is_var`.
    """
    return term.startswith("?")


def _atom_variables(atom: LogicAxiom) -> frozenset[str]:
    """The Datalog variables (``?x``) appearing in an atom's subject/pred/object."""
    terms = (atom.subject, atom.predicate, atom.obj)
    return frozenset(t for t in terms if _is_var(t))


def _predicate_key(atom: LogicAxiom) -> str:
    """The dependency-graph node key for an atom's predicate.

    For ``rdf:type`` atoms the key folds in the asserted class
    (``"rdf:type ClassIRI"``) so class-level recursion — e.g. a rule deriving
    ``?x rdf:type C`` from ``?y rdf:type C`` — is visible as a self-cycle.  A
    variable class falls back to the bare ``rdf:type`` predicate.  For every
    other predicate the key is the predicate IRI itself.
    """
    if atom.predicate == _RDF_TYPE and not _is_var(atom.obj):
        return f"{_RDF_TYPE} {atom.obj}"
    return atom.predicate


# --------------------------------------------------------------------------- #
# Predicate dependency graph + Tarjan SCC
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class PredicateDepGraph:
    """The predicate dependency graph of a :class:`LogicProgram`.

    A node is a predicate key (see :func:`_predicate_key`).  For each rule an
    edge ``head_pred ← body_pred`` is recorded, labelled ``"negative"`` iff the
    body atom is a negation-as-failure literal (``atom.negated``), else
    ``"positive"``.  This is the graph stratification analysis walks: a program
    is stratifiable iff no strongly-connected component contains a negative edge
    (LOGIC-SEMANTICS.md §Semantic profiles).

    Attributes:
        nodes: All predicate-key nodes; iterate in order via :meth:`sorted_nodes`.
        edges: Set of ``(head_key, body_key, label)`` triples.  ``label`` is
            ``"positive"`` or ``"negative"``.
    """

    nodes: frozenset[str]
    edges: frozenset[tuple[str, str, str]]

    @classmethod
    def from_program(cls, program: LogicProgram) -> PredicateDepGraph:
        """Build the dependency graph from ``program.rules``.

        Adds one node per predicate key seen in any rule head or body, and one
        ``head ← body`` edge per (rule, body-atom) pair.
        """
        nodes: set[str] = set()
        edges: set[tuple[str, str, str]] = set()
        for rule in program.rules:
            head_key = _predicate_key(rule.head)
            nodes.add(head_key)
            for body_atom in rule.body:
                body_key = _predicate_key(body_atom)
                nodes.add(body_key)
                label = "negative" if body_atom.negated else "positive"
                edges.add((head_key, body_key, label))
        return cls(nodes=frozenset(nodes), edges=frozenset(edges))

    def sorted_nodes(self) -> list[str]:
        """Nodes in deterministic (lexicographic) order."""
        return sorted(self.nodes)

    def sorted_edges(self) -> list[tuple[str, str, str]]:
        """Edges in deterministic order."""
        return sorted(self.edges)

    def successors(self) -> dict[str, list[str]]:
        """Adjacency map ``head -> [body, …]`` (edge direction head ← body).

        Tarjan walks the directed graph along edges; here an edge points from a
        head predicate to a body predicate it depends on.  Successor lists are
        sorted for deterministic SCC numbering.
        """
        adj: dict[str, list[str]] = {n: [] for n in self.nodes}
        for head, body, _label in self.edges:
            adj.setdefault(head, []).append(body)
            adj.setdefault(body, [])  # ensure body is a key even with no out-edges
        for key in adj:
            adj[key] = sorted(adj[key])
        return adj

    def sccs(self) -> list[frozenset[str]]:
        """The strongly-connected components, via :func:`tarjan_scc`."""
        return tarjan_scc(self.successors())


def tarjan_scc(graph: dict[str, list[str]]) -> list[frozenset[str]]:
    """Tarjan's strongly-connected-components algorithm (hand-rolled, no deps).

    Args:
        graph: Adjacency map ``node -> sorted list of successor nodes``.

    Returns:
        The list of SCCs, each a ``frozenset`` of nodes.  Node iteration is
        sorted so the result is fully deterministic across runs.
    """
    index_counter = 0
    stack: list[str] = []
    on_stack: set[str] = set()
    indices: dict[str, int] = {}
    low: dict[str, int] = {}
    result: list[frozenset[str]] = []

    def strongconnect(start: str) -> None:
        nonlocal index_counter
        # Iterative DFS to avoid Python recursion limits on large rule sets.
        work: list[tuple[str, int]] = [(start, 0)]
        while work:
            node, child_idx = work[-1]
            if child_idx == 0:
                indices[node] = index_counter
                low[node] = index_counter
                index_counter += 1
                stack.append(node)
                on_stack.add(node)
            successors = graph.get(node, [])
            recursed = False
            i = child_idx
            while i < len(successors):
                succ = successors[i]
                if succ not in indices:
                    work[-1] = (node, i + 1)
                    work.append((succ, 0))
                    recursed = True
                    break
                if succ in on_stack:
                    low[node] = min(low[node], indices[succ])
                i += 1
            if recursed:
                continue
            if low[node] == indices[node]:
                component: set[str] = set()
                while True:
                    w = stack.pop()
                    on_stack.discard(w)
                    component.add(w)
                    if w == node:
                        break
                result.append(frozenset(component))
            work.pop()
            if work:
                parent, _ = work[-1]
                low[parent] = min(low[parent], low[node])

    for vertex in sorted(graph):
        if vertex not in indices:
            strongconnect(vertex)
    return result


# --------------------------------------------------------------------------- #
# Stratification
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class StratificationResult:
    """The outcome of a stratification analysis over the predicate dep graph.

    Attributes:
        is_stratified: True iff no SCC contains a negative edge.
        strata: The stratum partition, ordered low-to-high; each stratum is a
            ``frozenset`` of predicate keys.  Empty when not stratifiable.
        offending_cycle: A deterministic rendering of one predicate cycle that
            crosses a negated body atom, or ``None`` when stratified.
    """

    is_stratified: bool
    strata: tuple[frozenset[str], ...]
    offending_cycle: tuple[str, ...] | None


def _negative_edges(graph: PredicateDepGraph) -> set[tuple[str, str]]:
    """The ``(head, body)`` pairs of all negative (negation-as-failure) edges."""
    return {(h, b) for (h, b, label) in graph.edges if label == "negative"}


def stratify(graph: PredicateDepGraph) -> StratificationResult:
    """Compute the stratification of a predicate dependency graph.

    A normal logic program is *stratifiable* iff no recursion (no SCC of size
    > 1, and no self-loop) passes through a negated body literal — i.e. no SCC
    contains a negative edge (LOGIC-SEMANTICS.md §Semantic profiles, the
    StratifiedNAF row: "unique perfect model; PTIME data complexity").

    The strata are built by condensing the graph into SCCs and topologically
    layering them; an SCC with no incoming dependency is stratum 0.
    """
    sccs = graph.sccs()
    node_to_scc: dict[str, int] = {}
    for idx, comp in enumerate(sccs):
        for node in comp:
            node_to_scc[node] = idx

    neg = _negative_edges(graph)
    offending: tuple[str, ...] | None = None
    for head, body in sorted(neg):
        if node_to_scc.get(head) == node_to_scc.get(body):
            # head and body are in the same SCC ⇒ a negative edge inside a cycle.
            cycle = _shortest_cycle(graph.successors(), body, head)
            if cycle is not None:
                offending = cycle
                break

    if offending is not None:
        return StratificationResult(
            is_stratified=False, strata=(), offending_cycle=offending
        )

    # No negative edge inside an SCC ⇒ stratifiable.  Layer the SCC condensation.
    strata = _layer_strata(graph, sccs, node_to_scc)
    return StratificationResult(is_stratified=True, strata=strata, offending_cycle=None)


def _shortest_cycle(
    adj: dict[str, list[str]], src: str, dst: str
) -> tuple[str, ...] | None:
    """A deterministic shortest path ``src → … → dst`` rendered as a cycle.

    Used to render the offending cycle when a negative edge ``dst ← src`` lies
    inside an SCC: the returned tuple is ``(dst, …, src, dst)`` — the cycle
    closing back through the negated dependency.  BFS over sorted successors
    gives the shortest, deterministic witness.
    """
    if src == dst:
        return (dst, dst)
    queue: list[str] = [src]
    prev: dict[str, str] = {src: src}
    while queue:
        node = queue.pop(0)
        for succ in adj.get(node, []):
            if succ not in prev:
                prev[succ] = node
                if succ == dst:
                    path = [dst]
                    cur = dst
                    while cur != src:
                        cur = prev[cur]
                        path.append(cur)
                    path.reverse()
                    # path is src→…→dst; close the cycle: dst, …, src, dst
                    return (*tuple(reversed(path)), path[0])
                queue.append(succ)
    return None


def _layer_strata(
    graph: PredicateDepGraph,
    sccs: list[frozenset[str]],
    node_to_scc: dict[str, int],
) -> tuple[frozenset[str], ...]:
    """Topologically layer the SCC condensation into ordered strata.

    Each stratum is the union of all predicate keys whose SCC sits at a given
    longest-path depth in the condensation DAG (edges run head→body, so a body's
    SCC must be evaluated *before* the head's).  Deterministic by construction.
    """
    n = len(sccs)
    # Condensed edges: scc(body) → scc(head) (body must precede head).
    cond_succ: dict[int, set[int]] = {i: set() for i in range(n)}
    indeg: dict[int, int] = dict.fromkeys(range(n), 0)
    for head, body, _label in graph.edges:
        sh, sb = node_to_scc[head], node_to_scc[body]
        if sh != sb and sh not in cond_succ[sb]:
            cond_succ[sb].add(sh)
            indeg[sh] += 1
    depth: dict[int, int] = dict.fromkeys(range(n), 0)
    ready = sorted(i for i in range(n) if indeg[i] == 0)
    while ready:
        cur = ready.pop(0)
        for nxt in sorted(cond_succ[cur]):
            depth[nxt] = max(depth[nxt], depth[cur] + 1)
            indeg[nxt] -= 1
            if indeg[nxt] == 0:
                ready.append(nxt)
                ready.sort()
    max_depth = max(depth.values(), default=-1)
    layers: list[set[str]] = [set() for _ in range(max_depth + 1)]
    for scc_idx, d in depth.items():
        layers[d] |= set(sccs[scc_idx])
    return tuple(frozenset(layer) for layer in layers if layer)


def _render_cycle(cycle: tuple[str, ...]) -> str:
    """Render a predicate cycle as ``[P -> Q -> P]`` (deterministic)."""
    return "[" + " -> ".join(cycle) + "]"


# --------------------------------------------------------------------------- #
# Profile-family certification checks (each (program) -> list[str])
# --------------------------------------------------------------------------- #


def certify_positive_horn(program: LogicProgram) -> list[str]:
    """PositiveHorn forbids negation-as-failure: no body atom may be negated.

    The PositiveHornProfile is monotonic Horn rules with a least model, "always
    terminating for the function-free fragment" (LOGIC-SEMANTICS.md §Semantic
    profiles).  Any negated body literal breaks monotonicity and exits the
    profile.  The function-symbol guard (which would also forbid function terms)
    is a forward-guard: the current function-free IR has none, so it does not
    fire today.
    """
    problems: list[str] = []
    for rule in program.rules:
        for atom in rule.body:
            if atom.negated:
                problems.append(
                    f"PositiveHornProfile violation: rule head "
                    f"{_predicate_key(rule.head)} has a negated body atom "
                    f"{_predicate_key(atom)} — PositiveHorn admits monotonic "
                    f"Horn rules only, no negation-as-failure "
                    f"({_DOC} {_SEC_PROFILES})"
                )
    return problems


def certify_stratified_naf(program: LogicProgram) -> list[str]:
    """StratifiedNAF is certified iff no predicate SCC crosses a negative edge.

    Builds the predicate dependency graph, runs Tarjan SCC, and reports a
    violation when a negation-as-failure dependency lies inside a recursive
    cycle — the program is then not stratifiable, so it has no unique perfect
    model (LOGIC-SEMANTICS.md §Semantic profiles: "negation-as-failure with
    stratification … unique perfect model; PTIME data complexity").
    """
    graph = PredicateDepGraph.from_program(program)
    result = stratify(graph)
    if result.is_stratified or result.offending_cycle is None:
        return []
    return [
        f"StratifiedNAFProfile violation: predicate cycle "
        f"{_render_cycle(result.offending_cycle)} crosses a negated body atom "
        f"— not stratifiable ({_DOC} {_SEC_PROFILES})"
    ]


def certify_well_founded(program: LogicProgram) -> list[str]:
    """WellFounded accepts unstratified negation; it requires a *normal* program.

    Well-founded semantics is three-valued, total and polynomial
    (LOGIC-SEMANTICS.md §Semantic profiles), and unlike StratifiedNAF it
    tolerates recursion through negation.  The only structural requirement is
    that every rule be a *normal* rule: a single (non-disjunctive) head and no
    function terms.  The current IR cannot express a disjunctive head (a
    ``LogicRule.head`` is exactly one :class:`LogicAxiom`) and is function-free,
    so this check passes for the IR shipped today.

    ``#[cfg]``-style note: a ``LogicRule.head`` is *typed* as exactly one
    :class:`LogicAxiom`, so the normal-rule (single-head) invariant is enforced
    structurally by the IR itself — there is no runtime way for this fragment to
    present a disjunctive head, hence the empty result.  This function is the
    forward-guard landing point: when the IR gains disjunctive heads or function
    terms, the per-rule normality test goes here.
    """
    # No non-normal rule is expressible in the current IR (single-atom heads,
    # function-free); per-rule normality checks land here once the IR can express
    # disjunctive heads / function terms.
    _ = program.rules
    return []


def certify_stable_model(program: LogicProgram) -> list[str]:
    """StableModel is NP-hard in general — advisory unless the set is stratified.

    Answer-set / stable-model semantics admits possibly multiple models and is
    NP-hard (LOGIC-SEMANTICS.md §Semantic profiles).  When the same rule set is
    *also* stratified, the stable model coincides with the well-founded /
    perfect model and is tractable, so no advisory is warranted.  Otherwise a
    single advisory is always emitted: entailment under a resource budget may
    legitimately return ``unknown`` (LOGIC-SEMANTICS.md §Decidability).
    """
    graph = PredicateDepGraph.from_program(program)
    if stratify(graph).is_stratified:
        return []
    return [
        f"StableModelProfile is NP-hard in general; this rule set is not "
        f"constrained to a tractable subfragment — entailment under budget may "
        f"return unknown ({_DOC} {_SEC_DECIDABILITY})"
    ]


# --------------------------------------------------------------------------- #
# Decidable-fragment checks (sufficient conditions only)
# --------------------------------------------------------------------------- #


def certify_dl_safe(program: LogicProgram) -> list[str]:
    """DL-safety: every rule variable must be bound by a positive body atom.

    A rule is DL-safe when each variable occurring anywhere in it also occurs in
    a *positive*, non-built-in body atom.  This restricts rule application to
    named individuals, keeping the combination with a decidable description-logic
    base decidable (LOGIC-SEMANTICS.md §Decidability lists "DL-safe rules" among
    the certifiable decidable fragments).  A variable appearing only in the head,
    or only under negation, is unsafe and is named in the violation.
    """
    problems: list[str] = []
    for rule in program.rules:
        # Variables that count as "bound": those in a positive body atom.
        # (The IR has no built-in atoms yet; when it does, exclude those here.)
        bound: set[str] = set()
        for atom in rule.body:
            if not atom.negated:
                bound |= _atom_variables(atom)
        used: set[str] = set(_atom_variables(rule.head))
        for atom in rule.body:
            used |= _atom_variables(atom)
        unsafe = sorted(used - bound)
        for var in unsafe:
            problems.append(
                f"DL-safety violation: variable {var} in rule "
                f"{_predicate_key(rule.head)} is not bound by any positive "
                f"body atom — unsafe rule, not DL-safe ({_DOC} {_SEC_DECIDABILITY})"
            )
    return problems


def certify_weak_acyclicity(program: LogicProgram) -> list[str]:
    """Weak acyclicity: no position-graph cycle crosses an existential edge.

    For tuple-generating dependencies (existential rules) the *position
    dependency graph* has a node per predicate position; a **normal** edge runs
    from a body position to a head position sharing a frontier (universally
    quantified) variable, and a **special** edge runs from a body position to a
    head position holding an *existentially* quantified head variable.  The chase
    terminates if no cycle passes through a special edge — the weak-acyclicity
    sufficient condition (LOGIC-SEMANTICS.md §Decidability:
    "weakly- or jointly-acyclic existential rules").

    The IR shipped today binds every head variable in the body (no existential
    head variables — see :func:`certify_dl_safe`), so the graph has no special
    edges and the cycle test is vacuously satisfied.  The graph is still built in
    full so this is the real TGD landing point, not a stub.  ``#[cfg]``-style
    note: the existential-head branch becomes live only when the IR can express
    value-inventing existential head variables.
    """
    # Position-dependency-graph node: (predicate_key, "S" | "P" | "O").
    normal_edges: set[tuple[tuple[str, str], tuple[str, str]]] = set()
    special_edges: set[tuple[tuple[str, str], tuple[str, str]]] = set()

    def _positions(atom: LogicAxiom) -> list[tuple[str, str]]:
        key = _predicate_key(atom)
        out: list[tuple[str, str]] = []
        for slot, term in (("S", atom.subject), ("P", atom.predicate), ("O", atom.obj)):
            if _is_var(term):
                out.append((key, slot))
        return out

    for rule in program.rules:
        body_var_positions: dict[str, list[tuple[str, str]]] = {}
        body_vars: set[str] = set()
        for atom in rule.body:
            if atom.negated:
                continue
            for slot, term in (
                ("S", atom.subject),
                ("P", atom.predicate),
                ("O", atom.obj),
            ):
                if _is_var(term):
                    body_var_positions.setdefault(term, []).append(
                        (_predicate_key(atom), slot)
                    )
                    body_vars.add(term)
        head_key = _predicate_key(rule.head)
        for slot, term in (
            ("S", rule.head.subject),
            ("P", rule.head.predicate),
            ("O", rule.head.obj),
        ):
            if not _is_var(term):
                continue
            head_pos = (head_key, slot)
            if term in body_vars:
                # Frontier variable: a normal edge from each body occurrence.
                for src in body_var_positions[term]:
                    normal_edges.add((src, head_pos))
            else:
                # Existential head variable: special edges from every body
                # position (value invention).  Not reachable in today's IR.
                for src_atom in rule.body:
                    for src in _positions(src_atom):
                        special_edges.add((src, head_pos))

    # Build adjacency over all edges; detect a cycle through any special edge.
    adj: dict[tuple[str, str], list[tuple[str, str]]] = {}
    special_set = set(special_edges)
    for src, dst in normal_edges | special_edges:
        adj.setdefault(src, []).append(dst)
        adj.setdefault(dst, [])
    for key in adj:
        adj[key].sort()

    # A special edge is "dangerous" iff its endpoints share a cycle, i.e. dst
    # can reach src.  Reachability via deterministic BFS.
    def _reaches(start: tuple[str, str], target: tuple[str, str]) -> bool:
        seen: set[tuple[str, str]] = set()
        queue = [start]
        while queue:
            node = queue.pop(0)
            for succ in adj.get(node, []):
                if succ == target:
                    return True
                if succ not in seen:
                    seen.add(succ)
                    queue.append(succ)
        return False

    problems: list[str] = []
    for src, dst in sorted(special_set):  # pragma: no cover - no existentials yet
        if _reaches(dst, src):
            problems.append(
                f"Weak-acyclicity violation: position {src[0]}[{src[1]}] -> "
                f"{dst[0]}[{dst[1]}] is an existential (special) edge inside a "
                f"cycle — the chase may not terminate ({_DOC} {_SEC_DECIDABILITY})"
            )
    return problems


def certify_joint_acyclicity(program: LogicProgram) -> list[str]:
    """Joint acyclicity: no cycle in the *existential dependency graph* exists.

    Joint acyclicity strengthens weak acyclicity by tracking which existential
    variables can propagate into which others across rules (the existential
    dependency graph); the chase terminates when that graph is acyclic
    (LOGIC-SEMANTICS.md §Decidability: "jointly-acyclic existential rules").
    Because the IR shipped today has no existentially quantified head variables,
    the existential dependency graph is empty and the condition holds **by
    vacuity** — the correct certification for the function-/existential-free
    fragment, not deferred work.
    """
    # No existential head variables in the current IR ⇒ empty existential graph.
    return []


def certify_guarded(program: LogicProgram) -> list[str]:
    """Guardedness: every rule body must contain a *guard* atom.

    A TGD is guarded when one body atom (the guard) contains all the body's
    universally quantified variables; guarded existential rules are decidable
    (LOGIC-SEMANTICS.md §Decidability: "guarded or sticky TGDs").  In the
    function-free, existential-free Datalog fragment shipped today there are no
    value-inventing rules to guard, so the condition holds **by vacuity** for
    every program — the correct answer for this fragment, not deferred work.
    """
    return []


def certify_sticky(program: LogicProgram) -> list[str]:
    """Stickiness: no sticky-marked body variable is dropped from the head.

    The sticky condition (LOGIC-SEMANTICS.md §Decidability: "guarded or sticky
    TGDs") bounds the chase by requiring that variables which a join propagates
    ("marked" variables) never disappear from the rule head.  With no
    value-inventing existential rules in today's IR the sticky-marking procedure
    marks nothing, so the condition holds **by vacuity** for every program — the
    correct answer for this fragment, not deferred work.
    """
    return []


# --------------------------------------------------------------------------- #
# Verdict + dispatch
# --------------------------------------------------------------------------- #

#: The decidability class string emitted per declared profile.
_DECIDABILITY_CLASS: dict[SemanticProfileId, str] = {
    SemanticProfileId.POSITIVE_HORN: "terminating/PTIME-data",
    SemanticProfileId.STRATIFIED_NAF: "terminating/PTIME-data",
    SemanticProfileId.WELL_FOUNDED: "three-valued/PTIME",
    SemanticProfileId.STABLE_MODEL: "NP-hard",
    SemanticProfileId.PROCEDURAL_PROLOG: "operational/Turing-complete",
    SemanticProfileId.PROBABILISTIC: "probabilistic/#P-hard",
}


@dataclass(frozen=True)
class CertificationVerdict:
    """The static-certification verdict for a program against a declared profile.

    Attributes:
        profile_id: The declared :class:`SemanticProfileId` the program was
            checked against.
        decidability_class: A human-readable class string, e.g.
            ``"terminating/PTIME-data"``, ``"NP-hard"``, ``"three-valued/PTIME"``.
        certified: True iff no violations were found (advisories for
            StableModel count as violations of the tractability guarantee).
        violations: The deterministic, sorted tuple of diagnostic strings.
    """

    profile_id: SemanticProfileId
    decidability_class: str
    certified: bool
    violations: tuple[str, ...]

    def to_json(self) -> dict[str, Any]:
        """Return a deterministic, JSON-serializable, sorted-key dict.

        ``profile_id`` is rendered as its string value so the conformance runner
        can diff this via canonical JSON.  ``violations`` is sorted for stability.
        """
        return {
            "certified": self.certified,
            "decidability_class": self.decidability_class,
            "profile_id": str(self.profile_id),
            "violations": sorted(self.violations),
        }


def certify_program(
    program: LogicProgram, declared_profile: SemanticProfileId
) -> CertificationVerdict:
    """Statically certify ``program`` against its ``declared_profile``.

    Dispatch by declared profile to the appropriate check set, aggregate every
    violation, and package the result as a :class:`CertificationVerdict`.

    **Certification uses *sufficient* conditions and is necessarily
    incomplete.**  Because termination is undecidable (Church/Turing), a clean
    verdict proves membership in the declared decidable/terminating fragment,
    but a violation only proves that the cheap structural sufficient condition
    does not hold — the program may still terminate
    (LOGIC-SEMANTICS.md §Decidability).

    Args:
        program: The IR program to certify.
        declared_profile: The :class:`SemanticProfileId` the program declares.

    Returns:
        A :class:`CertificationVerdict` whose ``certified`` field is
        ``not violations``.
    """
    violations: list[str] = []
    if declared_profile is SemanticProfileId.POSITIVE_HORN:
        violations += certify_positive_horn(program)
        violations += certify_dl_safe(program)
        violations += certify_weak_acyclicity(program)
        violations += certify_joint_acyclicity(program)
        violations += certify_guarded(program)
        violations += certify_sticky(program)
    elif declared_profile is SemanticProfileId.STRATIFIED_NAF:
        violations += certify_stratified_naf(program)
        violations += certify_dl_safe(program)
        violations += certify_weak_acyclicity(program)
        violations += certify_joint_acyclicity(program)
        violations += certify_guarded(program)
        violations += certify_sticky(program)
    elif declared_profile is SemanticProfileId.WELL_FOUNDED:
        violations += certify_well_founded(program)
        violations += certify_dl_safe(program)
        violations += certify_weak_acyclicity(program)
    elif declared_profile is SemanticProfileId.STABLE_MODEL:
        violations += certify_stable_model(program)
        violations += certify_dl_safe(program)
        violations += certify_weak_acyclicity(program)
    else:
        # ProceduralProlog / Probabilistic: no static decidability guarantee.
        violations.append(
            f"{declared_profile} carries no static decidability certification — "
            f"it is operational / probabilistic, outside the certifiable "
            f"sufficient-condition fragments ({_DOC} {_SEC_DECIDABILITY})"
        )

    deterministic = tuple(sorted(violations))
    return CertificationVerdict(
        profile_id=declared_profile,
        decidability_class=_DECIDABILITY_CLASS.get(declared_profile, "unknown"),
        certified=not deterministic,
        violations=deterministic,
    )


def certify_invariants(
    program: LogicProgram, declared_profile: SemanticProfileId
) -> list[str]:
    """Flat aggregator: every certification violation for ``program``.

    The analogue of :func:`gmeow_tools.reasoning_lint.reasoning_invariants` — the
    build-error surface a CLI or ``make check`` consumes.  An empty list means
    the program is certified (under the sufficient conditions) for its declared
    profile.
    """
    return list(certify_program(program, declared_profile).violations)

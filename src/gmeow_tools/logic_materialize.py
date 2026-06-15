# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
r"""Python oracle forward materializer for the Logic v1 monotonic core.

This module is **Principle 7's oracle**: a semi-naive Horn/Datalog forward
chase to fixpoint that defines the authoritative materialisation semantics.
The Rust engine (Task 4) must reproduce every IRI it produces byte-for-byte.
Get it RIGHT; everything downstream gates against it.

Design contract
---------------
* **World-indexed (CRITICAL).**  The chase runs per named-graph world.  Rules
  apply *within* a world; derived quads stay in that world.  No cross-world
  union or implicit merge.  Contested facts in different worlds coexist without
  collapse.
* **Determinism.**  Every derived quad carries the full seam contract (seam
  data contract, LOGIC-RUNTIME.md): ``graph``, ``(S, P, O, G)``,
  ``derivation_id``, ``rule_iri``, ``source_quad_ids``, ``profile``,
  ``budget_status``.  All IDs are content-addressed (SHA-1, sorted for order
  independence).
* **Blank-node Skolemization.**  Input blank nodes are Skolemized to
  deterministic IRIs *before* hashing or materializing so that derived facts
  contain only IRIs/literals.
* **No-occurrence gate (Stratum B).**  After the chase, every world is checked
  for token ``gufo:Event`` instances.  Any world that contains one raises
  :class:`NoOccurrenceViolationError` — the invariant is that the
  risk/teleology/norms fixture entails zero Event instances.
* **Loss-ledger hooks.**  Constructs narrowed during the chase are recorded as
  :class:`LossEntry` items for Task 5 to aggregate into the projection report.

Implementation notes
--------------------
*Input graph:*  an rdflib :class:`~rdflib.ConjunctiveGraph` (named
graphs = worlds).  N-Quads strings are accepted via :func:`parse_nquads`.

*IR rules:*  the ``body`` atoms use :class:`~.logic_ir.LogicAxiom` with
``subject``, ``predicate``, ``obj``, ``obj_is_literal`` fields.  For the
monotonic core (v1 PositiveHornProfile) we treat each axiom as a ground or
single-variable atom.  Variable atoms have a ``subject`` or ``obj`` starting
with ``?`` (Datalog convention).

*Reifier recipe:*  reuses :func:`~.statement_dsl.mint_reifier` exactly:
``sha1(s.n3() + " " + p.n3() + " " + o.n3()).hexdigest()`` under
``{NAMESPACE}reifier/``.

*Derivation ID recipe:*
``sha1(rule_iri + "\n" + "\n".join(sorted(source_reifier_iris))).hexdigest()``
under ``{NAMESPACE}derivation/``.  Sorted sources → order-independent.

*Term-canonicalization:*  ``.n3()`` on rdflib terms already produces the
canonical form required by the Rust mirror:
- IRI: ``<iri>``
- language literal: ``"lex"@lang`` (rdflib lowercases the lang subtag)
- typed literal: ``"lex"^^<dt>`` (xsd:string/rdf:langString elided by rdflib)
- No numeric normalization (lexical form preserved verbatim).
"""

from __future__ import annotations

import io
import logging
from dataclasses import dataclass
from hashlib import sha1
from typing import NamedTuple

from rdflib import RDF, ConjunctiveGraph, Graph, Literal, URIRef
from rdflib.term import BNode, Node

from gmeow_tools.config import NAMESPACE, PREFIXES
from gmeow_tools.logic_ir import LogicProgram, PreservationKind, SemanticProfileId
from gmeow_tools.statement_dsl import QuotedTriple, mint_reifier

_log = logging.getLogger(__name__)

# --------------------------------------------------------------------------- #
# Namespace constants
# --------------------------------------------------------------------------- #

_GUFO_EVENT = URIRef("http://purl.org/nemo/gufo#Event")
_RDFS_SUB_CLASS_OF = URIRef("http://www.w3.org/2000/01/rdf-schema#subClassOf")
_DERIVATION_PREFIX = f"{NAMESPACE}derivation/"
_SKOLEM_PREFIX = f"{NAMESPACE}skolem/"

# Profile IRI used in output records — resolved from SemanticProfileId
_LOGIC_NS = PREFIXES["logic"]


# --------------------------------------------------------------------------- #
# Exceptions
# --------------------------------------------------------------------------- #


class NoOccurrenceViolationError(Exception):
    """Raised when a world contains a token typed as gufo:Event or a subclass.

    Stratum B invariant: the no-occurrence gate forbids token Event instances.
    Type-level use of gufo:Event (as a class) is permitted; only
    rdf:type assertions to gufo:Event (or a subclass) on a non-class individual
    are violations.
    """

    def __init__(self, world_iri: str, token_iri: str, event_type: str) -> None:
        """Initialize with world, token, and event-type information.

        Args:
            world_iri: The world IRI where the violation was detected.
            token_iri: The IRI of the token instance typed as an Event.
            event_type: The IRI of the gufo:Event subclass that was violated.
        """
        self.world_iri = world_iri
        self.token_iri = token_iri
        self.event_type = event_type
        super().__init__(
            f"No-occurrence gate violation in world <{world_iri}>: "
            f"token <{token_iri}> is typed as gufo:Event subclass <{event_type}>"
        )


class MaterializationError(Exception):
    """Raised for malformed input that prevents materialization."""


# --------------------------------------------------------------------------- #
# Supporting types
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class LossEntry:
    """A record of a construct narrowed during the chase.

    Used by Task 5 to aggregate the projection loss ledger.

    Attributes:
        construct: The IRI or description of the construct that was narrowed.
        reason: Human-readable explanation of why it was narrowed.
        preservation_kind: The :class:`~.logic_ir.PreservationKind` that applies.
    """

    construct: str
    reason: str
    preservation_kind: PreservationKind


class DerivedQuad(NamedTuple):
    """One materialized quad with its full seam-contract metadata.

    All fields map directly to the seam data contract in LOGIC-RUNTIME.md
    §The seam data contract.

    Attributes:
        graph: IRI of the named-graph world this quad belongs to.
        subject: IRI string of the quad subject.
        predicate: IRI string of the quad predicate.
        obj: Canonical N3 representation of the quad object.
        graph_component: Same as ``graph`` (included for seam parity; future
            extensions may split graph vs. component IRI).
        derivation_id: Stable IRI for this derivation step.
        rule_iri: IRI of the rule that produced this quad (or the
            ``assert:`` sentinel for input facts).
        source_quad_ids: Reifier IRIs of the antecedent quads.
        profile: IRI of the semantic/decidability profile in force.
        budget_status: Always ``"ok"`` (guaranteed by the monotonic core).
    """

    graph: str
    subject: str
    predicate: str
    obj: str
    graph_component: str
    derivation_id: str
    rule_iri: str
    source_quad_ids: list[str]
    profile: str
    budget_status: str


@dataclass(frozen=True, slots=True)
class MaterializationResult:
    """The result of a full forward-chase materialization.

    Attributes:
        quads: All materialized quads (input facts + derived facts) in
            deterministic order (world IRI, then S/P/O N3-lex order).
        worlds: The set of world IRIs present in the output.
        profile: The semantic profile IRI used for this run.
        loss_entries: Constructs narrowed during the chase (for Task 5).
        input_quad_count: Number of input (asserted) quads.
        derived_quad_count: Number of freshly derived quads (not in input).
    """

    quads: tuple[DerivedQuad, ...]
    worlds: frozenset[str]
    profile: str
    loss_entries: tuple[LossEntry, ...]
    input_quad_count: int
    derived_quad_count: int


# --------------------------------------------------------------------------- #
# Skolemization
# --------------------------------------------------------------------------- #


def _skolem_iri(bnode: BNode) -> URIRef:
    """Deterministically Skolemize a blank node to a stable IRI.

    The hash covers the blank-node identifier string so that the same BNode
    (within a parse session) always maps to the same IRI.  Cross-session
    stability requires that the blank-node identifier itself is stable (which
    rdflib guarantees for parsed blank nodes using the source string).
    """
    digest = sha1(str(bnode).encode("utf-8")).hexdigest()
    return URIRef(f"{_SKOLEM_PREFIX}{digest}")


def _skolemize(term: Node) -> URIRef | Literal:
    """Skolemize a blank node to an IRI; pass through IRIs and Literals.

    Args:
        term: Any rdflib node.

    Returns:
        A :class:`~rdflib.URIRef` or :class:`~rdflib.Literal`.

    Raises:
        MaterializationError: If the term is not a URIRef, Literal, or BNode.
    """
    if isinstance(term, URIRef):
        return term
    if isinstance(term, Literal):
        return term
    if isinstance(term, BNode):
        return _skolem_iri(term)
    raise MaterializationError(f"Unexpected RDF term type: {type(term)!r} for {term!r}")


# --------------------------------------------------------------------------- #
# Content-addressing helpers
# --------------------------------------------------------------------------- #


def quad_reifier_iri(
    s: URIRef | Literal,
    p: URIRef | Literal,
    o: URIRef | Literal,
) -> str:
    """Return the reifier IRI for a quad's (S, P, O) using the canonical recipe.

    Reuses :func:`~.statement_dsl.mint_reifier` exactly — the same SHA-1 hash
    over N3-canonical ``"s.n3() p.n3() o.n3()"`` under ``{NAMESPACE}reifier/``.
    This is the single source of truth for the reifier recipe; the Rust mirror
    must reproduce it byte-for-byte.

    Args:
        s: Skolemized subject.
        p: Skolemized predicate (always URIRef in practice).
        o: Skolemized object (URIRef or Literal).

    Returns:
        The reifier IRI as a string.
    """
    # mint_reifier requires URIRef subject and predicate — both are guaranteed
    # by the caller (Skolemization strips BNodes; predicates are always URIs).
    if not isinstance(s, URIRef):
        raise MaterializationError(
            f"quad_reifier_iri: subject must be URIRef after Skolemization, got {s!r}"
        )
    if not isinstance(p, URIRef):
        raise MaterializationError(
            f"quad_reifier_iri: predicate must be URIRef, got {p!r}"
        )
    qt = QuotedTriple(subject=s, predicate=p, obj=o)
    return str(mint_reifier(qt))


def derivation_id_iri(rule_iri: str, source_reifier_iris: list[str]) -> str:
    r"""Compute the derivation IRI for a rule firing.

    The hash covers ``rule_iri + "\n" + "\n".join(sorted(source_reifier_iris))``
    so the result is order-independent w.r.t. the antecedent quads.  This is
    the canonical recipe the Rust engine must mirror byte-for-byte.

    Args:
        rule_iri: The IRI of the fired rule.
        source_reifier_iris: The reifier IRIs of the consumed antecedent quads.

    Returns:
        The derivation IRI as a string under ``{NAMESPACE}derivation/``.
    """
    payload = rule_iri + "\n" + "\n".join(sorted(source_reifier_iris))
    digest = sha1(payload.encode("utf-8")).hexdigest()
    return f"{_DERIVATION_PREFIX}{digest}"


# --------------------------------------------------------------------------- #
# Profile resolution
# --------------------------------------------------------------------------- #

#: Sentinel rule IRI for asserted (input) facts.
_ASSERT_RULE_IRI = f"{_LOGIC_NS}assert"


def _resolve_profile(program: LogicProgram) -> str:
    """Return the profile IRI declared in the program.

    The v1 oracle supports only :attr:`~.logic_ir.SemanticProfileId.POSITIVE_HORN`;
    anything else is recorded as a loss entry (the caller handles this).

    Args:
        program: The compiled logic program.

    Returns:
        The profile IRI string, or the PositiveHorn IRI if no profile declared.
    """
    if not program.profiles:
        return _LOGIC_NS + SemanticProfileId.POSITIVE_HORN
    # Use the first declared profile (canonical order from LogicProgram)
    return _LOGIC_NS + str(program.profiles[0].profile_id)


# --------------------------------------------------------------------------- #
# N-Quads input parser
# --------------------------------------------------------------------------- #


def parse_nquads(nquads_text: str) -> ConjunctiveGraph:
    """Parse an N-Quads string into a :class:`~rdflib.ConjunctiveGraph`.

    Empty or whitespace-only input returns an empty ConjunctiveGraph without
    raising (the empty-case oracle parity contract).

    Args:
        nquads_text: N-Quads encoded string.

    Returns:
        A ConjunctiveGraph with all parsed quads.

    Raises:
        MaterializationError: If the N-Quads text is malformed.
    """
    cg: ConjunctiveGraph = ConjunctiveGraph()
    stripped = nquads_text.strip()
    if not stripped:
        return cg
    try:
        cg.parse(io.StringIO(nquads_text), format="nquads")
    except Exception as exc:
        raise MaterializationError(f"Failed to parse N-Quads input: {exc}") from exc
    return cg


# --------------------------------------------------------------------------- #
# Datalog variable matching (positive Horn fragment only)
# --------------------------------------------------------------------------- #


def _is_var(term_str: str) -> bool:
    """Return True if the term string is a Datalog variable (starts with ``?``)."""
    return term_str.startswith("?")


def _match_atom(
    axiom_subj: str,
    axiom_pred: str,
    axiom_obj: str,
    axiom_obj_is_literal: bool,
    fact_s: URIRef | Literal,
    fact_p: URIRef | Literal,
    fact_o: URIRef | Literal,
) -> dict[str, URIRef | Literal] | None:
    """Try to match a rule body atom against a ground fact.

    Returns a variable-binding dict on success, or None on failure.  Only
    IRIs and literals are matched; after Skolemization there are no blank nodes
    in the fact base.

    Args:
        axiom_subj: Rule atom subject (IRI string or ``?var``).
        axiom_pred: Rule atom predicate (IRI string or ``?var``).
        axiom_obj: Rule atom object (IRI string, literal string, or ``?var``).
        axiom_obj_is_literal: Whether the axiom object is a literal.
        fact_s: Ground fact subject.
        fact_p: Ground fact predicate.
        fact_o: Ground fact object.

    Returns:
        Binding dict (possibly empty) mapping variable names to ground terms,
        or None if the atom does not match.
    """
    bindings: dict[str, URIRef | Literal] = {}

    # -- subject --
    if _is_var(axiom_subj):
        if not isinstance(fact_s, URIRef):
            return None  # subject must bind to an IRI
        bindings[axiom_subj] = fact_s
    else:
        if str(fact_s) != axiom_subj:
            return None

    # -- predicate --
    if _is_var(axiom_pred):
        if not isinstance(fact_p, URIRef):
            return None
        # Intra-atom repeated variable: if this var already bound (e.g. from
        # the subject slot), the new value must agree with the existing binding.
        if axiom_pred in bindings and bindings[axiom_pred] != fact_p:
            return None
        bindings[axiom_pred] = fact_p
    else:
        if str(fact_p) != axiom_pred:
            return None

    # -- object --
    if _is_var(axiom_obj):
        # Intra-atom repeated variable: if this var is already bound (e.g. ?x
        # appears in subject and object slots, as in `?x :p ?x`), the object
        # value must equal the existing binding; otherwise this fact does not
        # match the atom.
        if axiom_obj in bindings and bindings[axiom_obj] != fact_o:
            return None
        bindings[axiom_obj] = fact_o
    else:
        # Ground match: compare canonical string representation
        if axiom_obj_is_literal:
            # For literal atoms: match the string form of the literal
            if not isinstance(fact_o, Literal):
                return None
            if str(fact_o) != axiom_obj:
                return None
        else:
            if str(fact_o) != axiom_obj:
                return None

    return bindings


def _merge_bindings(
    b1: dict[str, URIRef | Literal],
    b2: dict[str, URIRef | Literal],
) -> dict[str, URIRef | Literal] | None:
    """Merge two binding dicts; return None if they conflict."""
    merged = dict(b1)
    for var, val in b2.items():
        if var in merged:
            if merged[var] != val:
                return None
        else:
            merged[var] = val
    return merged


def _apply_bindings(
    term_str: str,
    is_literal: bool,
    bindings: dict[str, URIRef | Literal],
) -> URIRef | Literal:
    """Apply variable bindings to a term string.

    Args:
        term_str: IRI string, literal string, or ``?var``.
        is_literal: Whether the term is a literal in the IR.
        bindings: Variable bindings from body matching.

    Returns:
        The ground URIRef or Literal.

    Raises:
        MaterializationError: If a variable in the head is unbound.
    """
    if _is_var(term_str):
        if term_str not in bindings:
            raise MaterializationError(
                f"Head variable {term_str!r} is unbound after body matching. "
                "Check that all head variables appear in the rule body."
            )
        return bindings[term_str]
    if is_literal:
        return Literal(term_str)
    return URIRef(term_str)


# --------------------------------------------------------------------------- #
# World extraction from ConjunctiveGraph
# --------------------------------------------------------------------------- #


def _extract_worlds(
    cg: ConjunctiveGraph,
) -> dict[str, list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]]]:
    """Extract per-world Skolemized fact lists from the ConjunctiveGraph.

    Args:
        cg: The input ConjunctiveGraph (named graphs = worlds).

    Returns:
        A dict mapping world IRI string → list of Skolemized (S, P, O) tuples.
    """
    worlds: dict[
        str, list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]]
    ] = {}
    for ctx in cg.contexts():
        graph_id = ctx.identifier
        if not isinstance(graph_id, URIRef):
            # Skip default graph (BNode identifier) — worlds must be named
            continue
        world_iri = str(graph_id)
        facts: list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]] = []
        for s, p, o in ctx:
            sk_s = _skolemize(s)
            sk_p = _skolemize(p)
            sk_o = _skolemize(o)
            facts.append((sk_s, sk_p, sk_o))
        worlds[world_iri] = facts
    return worlds


# --------------------------------------------------------------------------- #
# No-occurrence gate
# --------------------------------------------------------------------------- #


def _collect_event_subclasses(onto_graph: Graph) -> frozenset[URIRef]:
    """Collect all classes that are rdfs:subClassOf* gufo:Event.

    Uses a simple BFS/iterative expansion over the rdfs:subClassOf relation
    present in the given graph.

    Args:
        onto_graph: An rdflib Graph containing rdfs:subClassOf triples.

    Returns:
        Frozenset of URIRefs that are subclasses of gufo:Event (including itself).
    """
    event_classes: set[URIRef] = {_GUFO_EVENT}
    frontier: set[URIRef] = {_GUFO_EVENT}
    while frontier:
        new_frontier: set[URIRef] = set()
        for cls in frontier:
            for sub, _, _ in onto_graph.triples((None, _RDFS_SUB_CLASS_OF, cls)):
                if isinstance(sub, URIRef) and sub not in event_classes:
                    event_classes.add(sub)
                    new_frontier.add(sub)
        frontier = new_frontier
    return frozenset(event_classes)


def _assert_no_occurrence(
    world_iri: str,
    facts: list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]],
    event_subclasses: frozenset[URIRef],
) -> None:
    """Raise NoOccurrenceViolationError if any token is typed as gufo:Event/subclass.

    A *token* is a subject that carries an rdf:type assertion to an Event class.
    Type-level references (where the subject IS a class) are allowed.

    Args:
        world_iri: The world IRI for error messages.
        facts: The Skolemized (S, P, O) facts in this world (input + derived).
        event_subclasses: The set of gufo:Event subclasses to check against.

    Raises:
        NoOccurrenceViolationError: If any fact typed a token as gufo:Event/subclass.
    """
    rdf_type = URIRef(str(RDF.type))
    for s, p, o in facts:
        if str(p) != str(rdf_type):
            continue
        if not isinstance(o, URIRef):
            continue
        if o not in event_subclasses:
            continue
        # s is typed as an Event class; s itself must NOT be a class
        # (type-level usage: s rdfs:subClassOf gufo:Event is ok;
        #  but s rdf:type gufo:Event where s is an instance is forbidden)
        # We treat all URIRefs as potential tokens; OWL-style punning would
        # require an onto-graph, which we don't have here. The gate checks for
        # the pattern the runtime enforces: any rdf:type → gufo:Event triple.
        if isinstance(s, URIRef):
            raise NoOccurrenceViolationError(
                world_iri=world_iri,
                token_iri=str(s),
                event_type=str(o),
            )


# --------------------------------------------------------------------------- #
# Semi-naive forward chase
# --------------------------------------------------------------------------- #


def _build_asserted_quad(
    world_iri: str,
    s: URIRef | Literal,
    p: URIRef | Literal,
    o: URIRef | Literal,
    profile_iri: str,
) -> DerivedQuad:
    """Build a DerivedQuad record for an asserted (input) fact.

    The derivation_id for an asserted fact uses the assert sentinel rule IRI
    with the single source reifier (the quad itself hashes as its own source).

    Args:
        world_iri: The world this fact belongs to.
        s: Skolemized subject.
        p: Skolemized predicate.
        o: Skolemized object.
        profile_iri: The profile IRI for this run.

    Returns:
        A :class:`DerivedQuad` with ``rule_iri = logic:assert``.
    """
    if not isinstance(s, URIRef):
        raise MaterializationError(
            f"Asserted quad subject must be URIRef after Skolemization, got {s!r}"
        )
    if not isinstance(p, URIRef):
        raise MaterializationError(f"Asserted quad predicate must be URIRef, got {p!r}")
    reifier = quad_reifier_iri(s, p, o)
    deriv_id = derivation_id_iri(_ASSERT_RULE_IRI, [reifier])
    return DerivedQuad(
        graph=world_iri,
        subject=str(s),
        predicate=str(p),
        obj=o.n3(),
        graph_component=world_iri,
        derivation_id=deriv_id,
        rule_iri=_ASSERT_RULE_IRI,
        source_quad_ids=[reifier],
        profile=profile_iri,
        budget_status="ok",
    )


def _chase_world(
    world_iri: str,
    initial_facts: list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]],
    program: LogicProgram,
    profile_iri: str,
) -> tuple[
    list[DerivedQuad],
    list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]],
    list[LossEntry],
]:
    """Run the forward chase in one world to fixpoint.

    Implements semi-naive evaluation: we track the 'delta' (newly derived facts)
    and in each round only attempt to fire rules where at least one body atom
    matches a delta fact.  For the v1 monotonic Horn profile this terminates
    when no new facts can be derived.

    Args:
        world_iri: The world IRI (for provenance records).
        initial_facts: The Skolemized asserted (S, P, O) facts for this world.
        program: The compiled logic program (provides rules).
        profile_iri: The profile IRI for seam metadata.

    Returns:
        A 3-tuple:
        - list of all DerivedQuad records (asserted + derived),
        - list of all (S, P, O) facts after closure (for the no-occurrence gate),
        - list of LossEntry records for non-Horn constructs.
    """
    loss_entries: list[LossEntry] = []

    # Warn on non-POSITIVE_HORN profile (loss in v1)
    for prof in program.profiles:
        if prof.profile_id != SemanticProfileId.POSITIVE_HORN:
            loss_entries.append(
                LossEntry(
                    construct=f"{_LOGIC_NS}{prof.profile_id}",
                    reason=(
                        f"v1 oracle supports only PositiveHornProfile; "
                        f"{prof.profile_id} semantics not applied"
                    ),
                    preservation_kind=PreservationKind.SOUND_UNDER,
                )
            )

    # Indexed fact store: (s_str, p_str, o_str) → (sk_s, sk_p, sk_o)
    # The string key is the canonical identity for deduplication.
    fact_index: dict[
        tuple[str, str, str],
        tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal],
    ] = {}
    for s, p, o in initial_facts:
        key = (s.n3(), p.n3(), o.n3())
        fact_index[key] = (s, p, o)

    # Build asserted DerivedQuad records
    all_quads: list[DerivedQuad] = []
    for s, p, o in initial_facts:
        if isinstance(s, URIRef) and isinstance(p, URIRef):
            all_quads.append(_build_asserted_quad(world_iri, s, p, o, profile_iri))
        else:
            # Subjects that are still non-URI after Skolemization (shouldn't
            # happen, but hard-fail rather than silently skip)
            raise MaterializationError(
                f"Non-URI subject {s!r} in world {world_iri!r} after Skolemization"
            )

    # Semi-naive: delta starts as all initial facts
    delta: list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]] = list(
        initial_facts
    )

    # Collect derived DerivedQuad records separately (appended after input)
    derived_quads: list[DerivedQuad] = []

    # We iterate rules: for each rule, try to join body atoms against current
    # facts (using the delta for the semi-naive optimization).
    # For v1 Horn rules: all body atoms are positive; no negation.
    for _round in range(10_000):  # hard iteration cap (should terminate much sooner)
        new_delta: list[
            tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]
        ] = []

        for rule in program.rules:
            if not rule.body:
                # Zero-body rules (facts-as-rules): emit head unconditionally
                head = rule.head
                head_s = _apply_bindings(head.subject, False, {})
                head_p = _apply_bindings(head.predicate, False, {})
                head_o = _apply_bindings(head.obj, head.obj_is_literal, {})
                if not isinstance(head_s, URIRef) or not isinstance(head_p, URIRef):
                    continue
                key = (head_s.n3(), head_p.n3(), head_o.n3())
                if key in fact_index:
                    continue
                rule_iri = str(rule.scope.provenance or f"{_LOGIC_NS}rule/anonymous")
                deriv_id = derivation_id_iri(rule_iri, [])
                fact_index[key] = (head_s, head_p, head_o)
                new_delta.append((head_s, head_p, head_o))
                derived_quads.append(
                    DerivedQuad(
                        graph=world_iri,
                        subject=str(head_s),
                        predicate=str(head_p),
                        obj=head_o.n3(),
                        graph_component=world_iri,
                        derivation_id=deriv_id,
                        rule_iri=rule_iri,
                        source_quad_ids=[],
                        profile=profile_iri,
                        budget_status="ok",
                    )
                )
                continue

            # For rules with body: join all atoms against the current fact base.
            # Semi-naive: at least one body atom must match a delta fact.
            rule_iri = str(rule.scope.provenance or f"{_LOGIC_NS}rule/anonymous")

            # Enumerate all binding combinations via recursive join
            # (for v1 we have simple Datalog; body size is small)
            binding_sets = _join_body_atoms(
                rule.body,
                fact_index,
                delta,
            )

            for bindings, source_keys in binding_sets:
                # Ground the head
                try:
                    head_s = _apply_bindings(rule.head.subject, False, bindings)
                    head_p = _apply_bindings(rule.head.predicate, False, bindings)
                    head_o = _apply_bindings(
                        rule.head.obj, rule.head.obj_is_literal, bindings
                    )
                except MaterializationError:
                    # Unbound head variable — record as loss and skip
                    loss_entries.append(
                        LossEntry(
                            construct=rule_iri,
                            reason="Head variable unbound after body matching",
                            preservation_kind=PreservationKind.SOUND_UNDER,
                        )
                    )
                    continue

                if not isinstance(head_s, URIRef) or not isinstance(head_p, URIRef):
                    # Non-IRI head subject/predicate — skip (Datalog constraint)
                    continue

                key = (head_s.n3(), head_p.n3(), head_o.n3())
                if key in fact_index:
                    continue

                # Compute provenance
                source_reifiers = [
                    quad_reifier_iri(
                        fact_index[sk_key][0]
                        if isinstance(fact_index[sk_key][0], URIRef)
                        else URIRef(str(fact_index[sk_key][0])),
                        fact_index[sk_key][1]
                        if isinstance(fact_index[sk_key][1], URIRef)
                        else URIRef(str(fact_index[sk_key][1])),
                        fact_index[sk_key][2],
                    )
                    for sk_key in source_keys
                    if isinstance(fact_index[sk_key][0], URIRef)
                    and isinstance(fact_index[sk_key][1], URIRef)
                ]
                deriv_id = derivation_id_iri(rule_iri, source_reifiers)

                fact_index[key] = (head_s, head_p, head_o)
                new_delta.append((head_s, head_p, head_o))
                derived_quads.append(
                    DerivedQuad(
                        graph=world_iri,
                        subject=str(head_s),
                        predicate=str(head_p),
                        obj=head_o.n3(),
                        graph_component=world_iri,
                        derivation_id=deriv_id,
                        rule_iri=rule_iri,
                        source_quad_ids=source_reifiers,
                        profile=profile_iri,
                        budget_status="ok",
                    )
                )

        if not new_delta:
            break  # fixpoint reached
        delta = new_delta
    else:
        # Iteration cap hit — should not happen for finite positive Horn programs
        raise MaterializationError(
            f"Chase did not reach fixpoint in world {world_iri!r} after "
            "10,000 rounds. Check for non-terminating rules."
        )

    all_quads.extend(derived_quads)
    all_facts = list(fact_index.values())
    return all_quads, all_facts, loss_entries


def _join_body_atoms(
    body: tuple[object, ...],
    fact_index: dict[
        tuple[str, str, str],
        tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal],
    ],
    delta: list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]],
) -> list[tuple[dict[str, URIRef | Literal], list[tuple[str, str, str]]]]:
    """Join all body atoms against the fact base (semi-naive).

    For v1 positive Horn: all atoms are positive; the join is a nested-loop
    over the fact base.  At least one atom must match a delta fact (the
    semi-naive condition).

    Args:
        body: The body axioms (LogicAxiom instances from the rule).
        fact_index: The current world fact store.
        delta: Newly derived facts from the previous round.

    Returns:
        A list of (bindings, source_keys) pairs — one per full join result.
        ``source_keys`` are the fact_index keys of the matched facts.
    """
    from gmeow_tools.logic_ir import LogicAxiom  # local import avoids circularity

    delta_set: set[tuple[str, str, str]] = {
        (s.n3(), p.n3(), o.n3()) for s, p, o in delta
    }

    # Start with a single empty binding + no sources
    solutions: list[tuple[dict[str, URIRef | Literal], list[tuple[str, str, str]]]] = [
        ({}, [])
    ]

    for atom in body:
        if not isinstance(atom, LogicAxiom):
            raise MaterializationError(f"Body element is not a LogicAxiom: {atom!r}")
        next_solutions: list[
            tuple[dict[str, URIRef | Literal], list[tuple[str, str, str]]]
        ] = []
        for bindings, sources in solutions:
            # Ground the atom's subject/predicate/object using current bindings
            atom_s = bindings.get(atom.subject, None)
            atom_p = bindings.get(atom.predicate, None)
            atom_o = bindings.get(atom.obj, None)

            for (fs, fp, fo), (sk_s, sk_p, sk_o) in fact_index.items():
                # Quick filter for bound terms
                if atom_s is not None and fs != atom_s.n3():
                    continue
                if atom_p is not None and fp != atom_p.n3():
                    continue
                if atom_o is not None and fo != atom_o.n3():
                    continue
                m = _match_atom(
                    atom.subject,
                    atom.predicate,
                    atom.obj,
                    atom.obj_is_literal,
                    sk_s,
                    sk_p,
                    sk_o,
                )
                if m is None:
                    continue
                merged = _merge_bindings(bindings, m)
                if merged is None:
                    continue
                next_solutions.append((merged, [*sources, (fs, fp, fo)]))

        solutions = next_solutions
        if not solutions:
            break

    # Semi-naive filter: at least one source key must be in the delta
    return [
        (bindings, sources)
        for bindings, sources in solutions
        if any(sk in delta_set for sk in sources)
    ]


# --------------------------------------------------------------------------- #
# Public API
# --------------------------------------------------------------------------- #


def materialize_program(
    program: LogicProgram,
    input_graph: ConjunctiveGraph,
    profile: SemanticProfileId = SemanticProfileId.POSITIVE_HORN,
) -> MaterializationResult:
    """Run the forward Horn chase to fixpoint over the input ConjunctiveGraph.

    This is the **Python oracle** — the authoritative executable spec that the
    Rust engine (Task 4) must match byte-for-byte (Principle 7).

    World-indexed semantics
    -----------------------
    The chase runs *per named-graph world*.  Rules apply within a world; derived
    facts stay in that world.  There is no implicit cross-world union.
    Contested facts in different worlds coexist without collapse.

    Seam data contract
    ------------------
    Every output :class:`DerivedQuad` carries the full contract from
    LOGIC-RUNTIME.md §The seam data contract:
    ``graph``, ``(S, P, O, G)``, ``derivation_id``, ``rule_iri``,
    ``source_quad_ids``, ``profile``, ``budget_status``.

    No-occurrence gate (Stratum B)
    ------------------------------
    After the chase, every world is tested for token gufo:Event instances.
    A violation raises :class:`NoOccurrenceViolationError` immediately.

    Args:
        program: The compiled :class:`~.logic_ir.LogicProgram` (provides rules).
        input_graph: The facts as a :class:`~rdflib.ConjunctiveGraph` (named
            graphs = worlds).  Use :func:`parse_nquads` to convert N-Quads.
        profile: The semantic profile to use.  v1 oracle supports only
            :attr:`~.logic_ir.SemanticProfileId.POSITIVE_HORN`; other profiles
            are recorded as loss entries and skipped.

    Returns:
        A :class:`MaterializationResult` with all quads, worlds, profile,
        loss entries, and counts.

    Raises:
        NoOccurrenceViolationError: If any world contains a token gufo:Event instance.
        MaterializationError: If the input is malformed.
    """
    profile_iri = _LOGIC_NS + str(profile)

    # Build a minimal onto-graph from the input for subClassOf closure
    # (only needed for the no-occurrence gate; uses the combined input facts)
    onto_graph = Graph()
    for ctx in input_graph.contexts():
        for s, p, o in ctx:
            valid_s = isinstance(s, URIRef | BNode)
            valid_p = isinstance(p, URIRef)
            valid_o = isinstance(o, URIRef | BNode | Literal)
            if valid_s and valid_p and valid_o:
                onto_graph.add((s, p, o))
    event_subclasses = _collect_event_subclasses(onto_graph)

    # Collect all worlds from the ConjunctiveGraph
    world_facts = _extract_worlds(input_graph)

    all_quads: list[DerivedQuad] = []
    all_loss_entries: list[LossEntry] = []
    all_worlds: set[str] = set(world_facts.keys())
    input_quad_count = 0
    derived_quad_count = 0

    for world_iri, facts in sorted(world_facts.items()):
        input_quad_count += len(facts)
        world_quads, closed_facts, loss = _chase_world(
            world_iri, facts, program, profile_iri
        )
        derived_quad_count += len(world_quads) - len(facts)
        all_quads.extend(world_quads)
        all_loss_entries.extend(loss)

        # No-occurrence gate (Stratum B)
        _assert_no_occurrence(world_iri, closed_facts, event_subclasses)

    # Sort output deterministically: world IRI, then S/P/O N3-lex order
    all_quads.sort(key=lambda q: (q.graph, q.subject, q.predicate, q.obj))

    return MaterializationResult(
        quads=tuple(all_quads),
        worlds=frozenset(all_worlds),
        profile=profile_iri,
        loss_entries=tuple(all_loss_entries),
        input_quad_count=input_quad_count,
        derived_quad_count=derived_quad_count,
    )

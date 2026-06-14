# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Context-aware up-projection — the graph-descent resolver (#451).

The per-term lift (``up_projection.up_project``) resolves each triple in
isolation: ``schema:about`` *always* becomes the same gmeow term, blind to where
the edge sits. But a consumer predicate's meaning depends on its position in the
graph — the subject's type, the surrounding shape. ``schema:about`` on a
``MediaObject`` is ``gmeow:depicts``; on any other entity it is ``gmeow:isAbout``.

This module reads the graph **by position**. It resolves a property edge using
the *subject's resolved gmeow type*: among the gmeow terms that down-project to
the consumer predicate, it picks the one whose ``rdfs:domain`` the subject most
specifically satisfies (``eventLocation`` for an ``Event`` beats the catch-all
``locatedAt``). When the subject type adds no signal — no candidate is
type-compatible, or two remain tied — the edge falls through to the per-term
floor (#480), so nothing is lost and nothing is guessed.

This is **stage 1** of the descent (the node's *own* type, the most local
context). Path context (the ancestor chain of resolved types) and sibling-shape
recognition (multi-atom constellations) extend the same frame outward later; the
two-pass scaffold (index types, then resolve) and the floor are shared.

Derived, not hand-authored: the candidate terms and their type-contexts are the
alignment layers read backwards, exactly as the floor's rules are.
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass
from functools import lru_cache

from rdflib import RDF, RDFS, Graph, Literal, URIRef
from rdflib.term import Node

from gmeow_tools.graph import shared_merged_graph
from gmeow_tools.up_projection import (
    GM,
    LiftMap,
    UpProjection,
    _Acc,
    _edoalpath_pairs,
    _lift_edge,
    _sssom_clean_pairs,
    _sssom_closematch_pairs,
    _structural_pairs,
    build_lift_map,
)
from gmeow_tools.up_projection_audit import _canon_qname


@dataclass(frozen=True)
class _Candidate:
    """One gmeow term a consumer predicate could lift to, with its type-context.

    ``context_type`` is the gmeow class the subject must satisfy for this
    candidate to apply (the term's ``rdfs:domain``), or ``None`` when the term
    carries no single domain — a context-free candidate that the type layer
    cannot select on. ``relation`` carries the floor's fact/claim disposition:
    ``"="`` lifts as a fact, ``"<="``/``"closeMatch"`` as a claim.
    """

    gmeow: str
    context_type: str | None
    relation: str
    confidence: str


@dataclass(frozen=True)
class _Context:
    """The reversed alignment, indexed for type-conditioned resolution."""

    candidates: dict[str, list[_Candidate]]  # consumer predicate IRI → candidates
    ancestors: dict[str, frozenset[str]]  # gmeow class IRI → its superclasses (+self)


def _ancestor_closure(graph: Graph) -> dict[str, frozenset[str]]:
    """Map each gmeow class to its reflexive-transitive ``rdfs:subClassOf`` set."""
    direct: dict[str, set[str]] = defaultdict(set)
    for sub, obj in graph.subject_objects(RDFS.subClassOf):
        if isinstance(sub, URIRef) and isinstance(obj, URIRef):
            direct[str(sub)].add(str(obj))
    closure: dict[str, frozenset[str]] = {}

    def walk(cls: str, seen: set[str]) -> set[str]:
        if cls in seen:
            return set()
        seen.add(cls)
        acc = {cls}
        for parent in direct.get(cls, ()):
            acc |= walk(parent, seen)
        return acc

    for cls in set(direct) | {p for ps in direct.values() for p in ps}:
        closure[cls] = frozenset(walk(cls, set()))
    return closure


@lru_cache(maxsize=1)
def build_context() -> _Context:
    """Derive the type-conditioned candidate map from the alignment layers.

    Every layer the floor uses contributes its ``target → gmeow`` pairs, each
    tagged with the gmeow term's ``rdfs:domain`` (the type-context) and its
    fact/claim relation. Inverse-path rules are left to the floor (their reversal
    swaps endpoints, orthogonal to type selection).

    Cached (the alignment layers and class hierarchy are static for a process);
    the returned ``_Context`` is read-only — never mutate it.
    """
    graph = shared_merged_graph(include_imports=False)

    def domain(gmeow_iri: str) -> str | None:
        doms = [
            str(o)
            for o in graph.objects(URIRef(gmeow_iri), RDFS.domain)
            if isinstance(o, URIRef)
        ]
        return doms[0] if len(doms) == 1 else None

    candidates: dict[str, list[_Candidate]] = defaultdict(list)

    def add(target: str, gmeow: str, relation: str, conf: str) -> None:
        candidates[target].append(_Candidate(gmeow, domain(gmeow), relation, conf))

    for target, gmeows in _sssom_clean_pairs().items():  # identity → fact
        for gmeow in gmeows:
            add(target, gmeow, "=", "")
    exact, generalizing = _structural_pairs()
    for target, gmeows in exact.items():  # = structural → fact
        for gmeow in gmeows:
            add(target, gmeow, "=", "")
    for target, gmeow_confs in generalizing.items():  # <= → claim
        for gmeow, conf in gmeow_confs.items():
            add(target, gmeow, "<=", conf)
    for target, gmeow_confs in _sssom_closematch_pairs().items():  # closeMatch → claim
        for gmeow, conf in gmeow_confs.items():
            add(target, gmeow, "closeMatch", conf)
    direct_path, _inverse = _edoalpath_pairs()
    for target, gmeows in direct_path.items():  # direct edoalPath → fact
        for gmeow in gmeows:
            add(target, gmeow, "=", "")

    return _Context(candidates=dict(candidates), ancestors=_ancestor_closure(graph))


def _resolve(
    predicate: str, subject_types: set[str], ctx: _Context
) -> _Candidate | None:
    """Pick the most-specific type-compatible candidate, or defer to the floor.

    A candidate is compatible when its ``context_type`` is a superclass-or-equal
    of one of the subject's gmeow types. Resolution is **tiered by relation** so
    the type layer never weakens the floor's epistemics: a ``=`` fact candidate
    always outranks a ``<=``/closeMatch claim candidate (identity beats a typed
    inference), and only *within* the chosen tier does the narrowest type-context
    decide. If no typed candidate is compatible, or the narrowest is not unique,
    the type adds no decisive signal and the edge defers to the floor.
    """
    cands = ctx.candidates.get(predicate)
    if not cands or not subject_types:
        return None
    supers: set[str] = set()
    for t in subject_types:
        # the ancestor closure is reflexive and its fallback includes t, so the
        # subject's own type is always present — no separate union with {t}
        supers |= ctx.ancestors.get(t, frozenset({t}))
    typed = [
        c for c in cands if c.context_type is not None and c.context_type in supers
    ]
    facts = [c for c in typed if c.relation == "="]
    tier = facts or typed  # facts win the tier; else resolve among the claims
    if not tier:
        return None

    def narrower_or_equal(a: str | None, b: str | None) -> bool:  # a ⊑ b
        return a is not None and (a == b or b in ctx.ancestors.get(a, frozenset()))

    minima = [
        c
        for c in tier
        if all(narrower_or_equal(c.context_type, d.context_type) for d in tier)
    ]
    chosen = {c.gmeow for c in minima}
    return minima[0] if len(chosen) == 1 else None


def up_project_descend(source: Graph, lift: LiftMap | None = None) -> UpProjection:
    """Lift consumer RDF up to GMEOW, resolving each edge *by graph position*.

    Two passes: (1) index every node's gmeow type(s) by reversing its
    ``rdf:type`` edges; (2) resolve each property edge with the subject's type as
    context, falling through to the per-term floor whenever position adds no
    decisive signal. The output is pure GMEOW, same as the floor.
    """
    if len(source) == 0:
        raise ValueError("up_project_descend: source graph is empty")
    if lift is None:
        lift = build_lift_map()
    ctx = build_context()

    # pass 1 — index node → resolved gmeow type(s)
    node_types: dict[Node, set[str]] = defaultdict(set)
    for s, _p, t in source.triples((None, RDF.type, None)):
        if not isinstance(t, URIRef):
            continue
        key = str(t)
        if key in lift.rules:
            node_types[s].add(lift.rules[key])
        elif key in lift.claim_rules:
            node_types[s].add(lift.claim_rules[key][0])

    # pass 2 — resolve property edges by context, defer the rest to the floor
    acc = _Acc(out=Graph())
    acc.out.bind("gmeow", GM)
    context_resolved = 0
    context_terms: dict[str, int] = defaultdict(int)
    for s, p, o in source:
        # The floor already resolves rdf:type and any unique identity/exact/
        # inverse FACT — the strongest mapping there is, nothing position can
        # improve. The descent only refines what the floor would claim or hold
        # ambiguous, so a fact is never downgraded to a typed claim.
        key = str(p)
        if (
            p == RDF.type
            or not isinstance(p, URIRef)
            or key in lift.rules
            or key in lift.inverse_rules
        ):
            _lift_edge(acc, s, p, o, lift)
            continue
        cand = _resolve(key, node_types.get(s, set()), ctx)
        if cand is None:
            _lift_edge(acc, s, p, o, lift)
            continue
        if cand.relation == "=":
            acc.fact(s, URIRef(cand.gmeow), o)
        elif isinstance(s, URIRef) and isinstance(o, URIRef | Literal):
            # p is a URIRef here (the gate above continued otherwise)
            acc.claim(s, URIRef(cand.gmeow), o, p, cand.confidence)
        else:
            # a claim with an unquotable blank-node endpoint — defer to the floor
            _lift_edge(acc, s, p, o, lift)
            continue
        context_resolved += 1
        context_terms[_canon_qname(str(p))] += 1

    return UpProjection(
        graph=acc.out,
        lifted=acc.lifted,
        gap_terms=dict(acc.gaps),
        ambiguous_terms=dict(acc.ambig),
        claimed=acc.claimed,
        claim_terms=dict(acc.claims),
        context_resolved=context_resolved,
        context_terms=dict(context_terms),
    )

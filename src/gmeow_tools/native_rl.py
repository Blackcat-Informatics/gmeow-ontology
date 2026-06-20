# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Native OWL 2 RL deductive closure — the Docker-free primary entailment authority.

This is the drop-in replacement for the ``owlrl`` deductive-closure baseline
(``owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)``) the conformance
suites used to call (issue #666, Task 5). ``owlrl`` is relocated to the
classic-cross-check lane as the agreement *oracle* — it is no longer required for
normal use; the primary path is native and Java/Docker-free.

Architecture
------------
The closure is computed **RDF-1.2-first** by the native Rust engine
(``gmeow_logic.rl_closure``): every triple is encoded into a generic 4-ary
``triple(?s, ?p, ?o, ?w)`` Datalog relation (predicate-as-DATA, the world axis
preserved) and the fixed OWL 2 RL/RDF rule set runs through the Nemo chase. The
per-property ternary ``materialize`` seam cannot express RL's property-quantifying
meta-rules (``prp-dom``/``prp-rng``/``prp-trp``/``prp-inv``/``prp-spo*`` etc.),
so the generic-triple encoding in ``crates/logic/src/reason/rl.rs`` is used.

rdflib-free core (issue #630)
-----------------------------
This module is the **production reasoning path** and is now rdflib-free: the
closure is computed and round-tripped through the in-repo ``gmeow_rdf`` kernel
(the native oxigraph binding), never rdflib. :func:`native_rl_closure` takes the
serialized graph (N-Triples for a single default world, or N-Quads for named
worlds) and returns the full closure as an N-Triples string. The rdflib I/O
adapter that the rdflib-native callers (the competency/observation test suites and
the ``rl_agreement`` cross-check oracle, which all build and SPARQL-query rdflib
graphs) need lives in :mod:`gmeow_tools.native_rl_rdflib`, at the caller boundary.
"""

from __future__ import annotations

#: The sentinel world IRI the Rust engine encodes a default-graph triple under
#: (mirrors ``crate::reason::rl::DEFAULT_WORLD``). A default-graph (N-Triples)
#: input closes in this single world; the world component is dropped when the
#: closure is re-serialized as N-Triples.
DEFAULT_WORLD = "https://blackcatinformatics.ca/gmeow/graph/rl-default"

#: Skolem IRI prefix the engine mints for a blank node (mirrors
#: ``crate::encode::skolem_iri``). A derived triple whose subject/object is one of
#: these is a blank node in the source graph; it is mapped back to a stable blank
#: node label (``_:`` + the skolem tail) so the closure carries no spurious
#: ``http(s)://…/skolem/…`` IRI resource.
_SKOLEM_PREFIX = "https://blackcatinformatics.ca/gmeow/skolem/"


def _term_to_nt_subject(value: str) -> str:
    """Render an engine subject/predicate IRI (bare) as an N-Triples term.

    A skolem IRI becomes a blank-node label; every other value is a NamedNode.
    """
    if value.startswith(_SKOLEM_PREFIX):
        return "_:" + _skolem_label(value)
    return f"<{value}>"


def _term_to_nt_object(obj_nt: str) -> str:
    """Render an engine object term (already N-Triples display form) for re-parse.

    The engine emits IRIs as ``<iri>``, literals as ``"v"`` / ``"v"@lang`` /
    ``"v"^^<dt>`` (already valid N-Triples), and blank nodes as the skolem IRI
    form (``<https://…/skolem/…>``), which is rewritten to a blank-node label so
    the closure round-trips a blank node as a blank node.
    """
    if obj_nt.startswith("<") and obj_nt.endswith(">"):
        inner = obj_nt[1:-1]
        if inner.startswith(_SKOLEM_PREFIX):
            return "_:" + _skolem_label(inner)
        return obj_nt
    if obj_nt.startswith('"'):
        # Literal: the engine display form is already valid N-Triples.
        return obj_nt
    # Bare IRI (defensive — objects normally arrive bracketed).
    if obj_nt.startswith(_SKOLEM_PREFIX):
        return "_:" + _skolem_label(obj_nt)
    return f"<{obj_nt}>"


def _skolem_label(skolem_iri: str) -> str:
    """Derive a syntactically-valid N-Triples blank-node label from a skolem IRI.

    The tail after :data:`_SKOLEM_PREFIX` is a minted identifier; it is prefixed
    with ``b`` and stripped of any character not permitted in an N-Triples
    blank-node label so the re-parse never fails on an exotic skolem tail.
    """
    tail = skolem_iri[len(_SKOLEM_PREFIX) :]
    safe = "".join(ch if (ch.isalnum() or ch in "_-") else "_" for ch in tail)
    return "b" + safe


def native_rl_closure(serialized: str, *, named_worlds: bool = False) -> str:
    """Compute the OWL 2 RL closure of a graph natively — the ``owlrl.expand`` twin.

    Computes the RL deductive closure natively (Rust, Docker/Java-free) over the
    in-repo ``gmeow_rdf`` kernel and returns the **full** closure (asserted +
    derived) as an N-Triples string. Blank nodes round-trip as blank nodes;
    literals keep their datatype/language.

    Args:
        serialized: The source graph as N-Triples (default-graph triples close in
            one sentinel world) or, when ``named_worlds`` is set, N-Quads (each
            named-graph triple closes in its own world).
        named_worlds: Reserved for the named-graph close. When ``False`` (the
            suites' single-default-graph case) the world component of every closed
            triple is dropped and the result is plain N-Triples.

    Returns:
        The full RL closure as an N-Triples string (one canonical line per
        triple, sorted for determinism), suitable for ``gmeow_rdf.parse`` or an
        rdflib adapter to fold back into a graph.
    """
    import gmeow_logic

    rows = gmeow_logic.rl_closure(serialized)

    lines: list[str] = []
    for subject, predicate, obj_nt, _world, _is_edb in rows:
        s = _term_to_nt_subject(subject)
        p = _term_to_nt_subject(predicate)
        o = _term_to_nt_object(obj_nt)
        lines.append(f"{s} {p} {o} .")
    return "".join(line + "\n" for line in sorted(set(lines)))

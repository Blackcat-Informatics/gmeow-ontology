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
The closure is computed **RDF-1.2-first** by the native Rust engine: every triple
is encoded into a generic 4-ary ``triple(?s, ?p, ?o, ?w)`` Datalog relation
(predicate-as-DATA, the world axis preserved) and the fixed OWL 2 RL/RDF rule set
runs through the Nemo chase. The per-property ternary ``materialize`` seam cannot
express RL's property-quantifying meta-rules (``prp-dom``/``prp-rng``/``prp-trp``/
``prp-inv``/``prp-spo*`` etc.), so the generic-triple encoding in
``crates/logic/src/reason/rl.rs`` is used.

Rust-first, single FFI boundary (issue #630)
--------------------------------------------
This module is the **production reasoning path** and is now a thin shim over the
native engine: ``gmeow_logic.rl_closure_nt`` returns the full closure already
**rendered as N-Triples in Rust** (skolem IRI → blank node, literal display,
de-dup and sort all happen in ``RlClosure::to_ntriples``), so the reasoning path
no longer re-renders rows in Python. The rdflib I/O adapter the rdflib-native
callers need (the competency/observation test suites and the ``rl_agreement``
classic-cross-check oracle) lives in :mod:`gmeow_tools.native_rl_rdflib`; it calls
``gmeow_logic.rl_closure_quads`` to fold the closure back into a graph with no
intermediate N-Triples round-trip at all.
"""

from __future__ import annotations


def native_rl_closure(serialized: str, *, named_worlds: bool = False) -> str:
    """Compute the OWL 2 RL closure of a graph natively — the ``owlrl.expand`` twin.

    Computes the RL deductive closure natively (Rust, Docker/Java-free) and returns
    the **full** closure (asserted + derived) as an N-Triples string. Blank nodes
    round-trip as blank nodes; literals keep their datatype/language. All rendering
    happens in Rust (``gmeow_logic.rl_closure_nt``); this function only forwards.

    Args:
        serialized: The source graph as N-Triples (default-graph triples close in
            one sentinel world) or N-Quads (each named-graph triple closes in its
            own world).
        named_worlds: Reserved for the named-graph close. The suites use a single
            default graph, which closes in one world; the world component is
            dropped and the result is plain N-Triples.

    Returns:
        The full RL closure as an N-Triples string (one canonical line per triple,
        sorted for determinism), suitable for ``gmeow_rdf.parse`` to fold back.
    """
    import gmeow_logic

    return gmeow_logic.rl_closure_nt(serialized)

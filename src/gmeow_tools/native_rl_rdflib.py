# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Graph I/O adapter for the native OWL 2 RL closure (issue #630).

The production reasoning core (:mod:`gmeow_tools.native_rl`) is rdflib-free. This
module is the **caller-boundary adapter** for the graph-native consumers — the
competency/observation suites (which use the native ``gmeow_rdf.compat.rdflib``
``Graph``) and the ``rl_agreement`` classic-cross-check oracle (which uses the
upstream rdflib ``Graph`` to compare against ``owlrl``).

It is **engine-agnostic**: the closure is computed by one native call over the
graph's N-Triples serialization and folded back in via the graph's own ``parse``
— so any graph that can ``serialize``/``parse`` N-Triples works, whether it is the
native compat ``Graph`` or upstream rdflib's. No per-term construction is needed,
so neither term model leaks into this module.
"""

from __future__ import annotations

from typing import Protocol


class _GraphIO(Protocol):
    """A graph that can round-trip N-Triples (compat ``Graph`` or rdflib's)."""

    def serialize(self, *, format: str) -> str | bytes:
        """Serialize the graph in ``format``."""
        ...

    def parse(self, *, data: str | bytes, format: str) -> object:
        """Parse ``data`` (in ``format``) into the graph, merging triples."""
        ...


def native_rl_closure[Graph: _GraphIO](graph: Graph) -> Graph:
    """Expand ``graph`` under OWL 2 RL in place — the native ``owlrl.expand`` twin.

    Computes the RL deductive closure via a single native call
    (``gmeow_logic.rl_closure_nt``) over the graph's N-Triples form, then merges
    the closed N-Triples back into ``graph`` via its own ``parse`` (so both the
    in-place-mutation and returned-graph call styles work, on either engine). The
    suites use a single default graph, which closes in one world.

    Args:
        graph: The graph to close (mutated in place).

    Returns:
        The same ``graph`` object, now carrying the RL closure.
    """
    import gmeow_logic

    serialized = graph.serialize(format="nt")
    text = serialized.decode("utf-8") if isinstance(serialized, bytes) else serialized
    closure = gmeow_logic.rl_closure_nt(text)
    if closure.strip():
        graph.parse(data=closure, format="nt")
    return graph

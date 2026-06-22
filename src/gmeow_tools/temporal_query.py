"""TQL — the GMEOW Temporal Query Language.

A small *query algebra* over the events model, realized as parameterized SPARQL 1.1
queries (``slices/core/temporal/queries/tql/*.rq``) rather than a bespoke
temporal-query engine.
GMEOW aligns to the temporal-query literature — stSPARQL, T-SPARQL, SPARQL-ST — **by
reference** (CONSTITUTION Principle 5): the model carries the Allen interval algebra
(``gmeow:before``/``after``/``during``/… on ``gmeow:Event``) and the four clocks
(``validFrom``/``validUntil``/``assertedAt``/``recordedNoLaterThan``), and standard
SPARQL 1.1 property paths compute the transitive temporal closures with no
materializing reasoner. This module is their *executor*: it binds query parameters
and runs them over an asserted graph.

Each query is parameterized by named SPARQL variables, bound at call time via
rdflib ``initBindings`` (the values never touch the query text, so there is no
injection surface):

- ``allen-closure`` — every transitively-ordered (earlier, later) event pair.
- ``before-event`` — events before a ``?focus`` event (Allen closure or datetime).
- ``during-event`` — events temporally within a ``?focus`` event.
- ``timeline`` — every event with its effective start instant, ordered.
- ``overlapping-window`` — events overlapping a ``?windowStart``..``?windowEnd`` span.
- ``bitemporal`` — the four-clocks query: claims valid at ``?validAt`` and asserted
  by ``?asOf`` (standpoint-indexed, no winner).
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import cache

from gmeow_rdf.compat.rdflib import Graph
from gmeow_rdf.compat.rdflib.query import ResultRow
from gmeow_rdf.compat.rdflib.term import Identifier

from gmeow_tools.config import TEMPORAL_QUERY_DIR


@dataclass(frozen=True, slots=True)
class TemporalQuery:
    """A named TQL query and the parameters it expects."""

    name: str
    parameters: tuple[str, ...]
    summary: str


#: The TQL query registry — name → its parameters + one-line summary.
TEMPORAL_QUERIES: dict[str, TemporalQuery] = {
    "allen-closure": TemporalQuery(
        "allen-closure", (), "every transitively-ordered (earlier, later) event pair"
    ),
    "before-event": TemporalQuery(
        "before-event", ("focus",), "events temporally before a focus event"
    ),
    "during-event": TemporalQuery(
        "during-event", ("focus",), "events temporally within a focus event"
    ),
    "timeline": TemporalQuery(
        "timeline", (), "every event with its effective start instant, ordered"
    ),
    "overlapping-window": TemporalQuery(
        "overlapping-window",
        ("windowStart", "windowEnd"),
        "events overlapping a [windowStart, windowEnd] span",
    ),
    "bitemporal": TemporalQuery(
        "bitemporal",
        ("validAt", "asOf"),
        "claims valid at ?validAt and asserted by ?asOf (four clocks)",
    ),
    "interval-allen-closure": TemporalQuery(
        "interval-allen-closure",
        (),
        "transitively-ordered TimeInterval pairs via intervalBefore+",
    ),
    "period-containment": TemporalQuery(
        "period-containment",
        (),
        "named periods and their containing ancestors via periodPartOf+",
    ),
    "frame-matching": TemporalQuery(
        "frame-matching",
        ("frame",),
        "instants/intervals expressed in a given temporal frame",
    ),
}


@cache
def _query_text(name: str) -> str:
    """Read and cache a TQL query's SPARQL text."""
    if name not in TEMPORAL_QUERIES:
        known = ", ".join(sorted(TEMPORAL_QUERIES))
        raise KeyError(f"unknown temporal query {name!r}; known: {known}")
    return (TEMPORAL_QUERY_DIR / f"{name}.rq").read_text(encoding="utf-8")


def run_temporal_query(
    name: str,
    source: Graph,
    bindings: dict[str, Identifier] | None = None,
) -> list[ResultRow]:
    """Run a named TQL query over a source graph.

    Args:
        name: A key of :data:`TEMPORAL_QUERIES`.
        source: The graph to query (ontology + instance data).
        bindings: Values for the query's parameters (e.g. ``{"focus": URIRef(...)}``),
            bound via rdflib ``initBindings`` — never interpolated into the text.

    Returns:
        The query's result rows.

    Raises:
        KeyError: If ``name`` is not a known TQL query.
        ValueError: If a required parameter is missing.
    """
    spec = TEMPORAL_QUERIES[name]
    supplied = bindings or {}
    missing = [p for p in spec.parameters if p not in supplied]
    if missing:
        raise ValueError(
            f"temporal query {name!r} needs parameter(s) {', '.join(missing)}"
        )
    result = source.query(_query_text(name), initBindings=dict(supplied))
    return [row for row in result if isinstance(row, ResultRow)]

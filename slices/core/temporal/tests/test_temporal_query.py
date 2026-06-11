"""TQL — the Temporal Query Language toolkit (#41 temporal deepening).

The queries are a temporal algebra in standard SPARQL 1.1: Allen-relation property-
path closures (no materializing reasoner), the event timeline, interval overlap, and
the bitemporal four-clocks query. These tests run each over the events worked-example
fixture and assert the temporal answers — including that transitivity is computed by
the path, and that the subclass-aware type test reaches gmeow:LifeEvent occurrences.
"""

from __future__ import annotations

from functools import lru_cache

import pytest
from rdflib import Graph, Literal, Namespace
from rdflib.namespace import XSD
from rdflib.query import ResultRow
from rdflib.term import Identifier

from gmeow_tools.config import FIXTURES_DIR
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.temporal_query import TEMPORAL_QUERIES, run_temporal_query

EX = Namespace("https://blackcatinformatics.ca/gmeow/examples/events/")
COVERAGE = FIXTURES_DIR  # the shared coverage corpus (tests/fixtures/coverage/)


@lru_cache(maxsize=2)
def _source(fixture: str = "events.ttl") -> Graph:
    g = load_merged_graph(include_imports=False)
    g.parse(COVERAGE / fixture, format="turtle")
    return g


def _events(rows: list[ResultRow], var: int = 0) -> set[Identifier]:
    return {row[var] for row in rows}


def _dt(value: str) -> Literal:
    return Literal(value, datatype=XSD.dateTime)


def test_registry_covers_every_query_file() -> None:
    """Each registered TQL query has its .rq file, and vice versa."""
    from gmeow_tools.config import TEMPORAL_QUERY_DIR

    on_disk = {p.stem for p in TEMPORAL_QUERY_DIR.glob("*.rq")}
    assert on_disk == set(TEMPORAL_QUERIES)


def test_allen_closure_is_transitive() -> None:
    """The property path computes the transitive ordering closure with no reasoner:
    dawn before noon, noon before dusk ⊢ dawn before dusk.
    """
    rows = run_temporal_query("allen-closure", _source())
    pairs = {(r[0], r[1]) for r in rows}
    assert (EX.dawn, EX.noon) in pairs
    assert (EX.noon, EX.dusk) in pairs
    assert (EX.dawn, EX.dusk) in pairs  # the entailed transitive edge


def test_before_event_reaches_lifeevents_and_orders_by_time() -> None:
    """before-event(reception) includes the gmeow:LifeEvent birth (subclass-aware
    type test) and the interval/fuzzy events, and excludes later events.
    """
    rows = run_temporal_query("before-event", _source(), {"focus": EX.reception})
    events = _events(rows)
    assert EX.alexBirth in events  # a LifeEvent, reached via rdfs:subClassOf*
    assert EX.wedding in events  # an interval event (effective end before reception)
    assert EX.siege in events  # a fuzzy event
    assert EX.standup1 not in events  # 2024 — after the 2015 reception


def test_during_event_follows_relation_and_inverse() -> None:
    """during-event(conference) finds the directly-asserted during (talk) AND the
    inverse of an asserted contains (keynote), both over the asserted graph.
    """
    rows = run_temporal_query("during-event", _source(), {"focus": EX.conference})
    events = _events(rows)
    assert {EX.talk, EX.keynote} <= events


def test_timeline_orders_all_events_by_effective_start() -> None:
    rows = run_temporal_query("timeline", _source())
    ordered = [r[0] for r in rows]
    assert EX.siege in ordered and EX.standup2 in ordered
    # The fuzzy 1453 siege precedes the 2024 standup on the timeline.
    assert ordered.index(EX.siege) < ordered.index(EX.standup2)


def test_overlapping_window_matches_crisp_point_and_fuzzy() -> None:
    rows = run_temporal_query(
        "overlapping-window",
        _source(),
        {
            "windowStart": _dt("2015-06-20T00:00:00Z"),
            "windowEnd": _dt("2015-06-20T23:59:59Z"),
        },
    )
    events = _events(rows)
    assert {EX.wedding, EX.reception} <= events  # the interval + the point that day
    assert EX.standup1 not in events


def test_bitemporal_four_clocks_returns_standpoint_indexed_claims() -> None:
    """Over the contested fixture: a claim valid at 1895 and asserted by 2020 is
    returned with its standpoint, and coexisting frames are NOT collapsed (#43).
    """
    rows = run_temporal_query(
        "bitemporal",
        _source("events-contested.ttl"),
        {"validAt": _dt("1895-01-01T00:00:00Z"), "asOf": _dt("2020-01-01T00:00:00Z")},
    )
    subjects = {r[0] for r in rows}
    standpoints = {r[3] for r in rows if r[3] is not None}
    assert EX.disputedEvent in subjects
    # Both asserting standpoints coexist in the result — no single winner.
    assert {EX["standpoint-A"], EX["standpoint-B"]} <= standpoints


def test_missing_parameter_is_rejected() -> None:
    with pytest.raises(ValueError, match="needs parameter"):
        run_temporal_query("before-event", _source(), {})


def test_unknown_query_is_rejected() -> None:
    with pytest.raises(KeyError):
        run_temporal_query("no-such-query", _source())

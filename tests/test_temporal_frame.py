"""Tests for the temporal frame profile (issue #67, refactored for #70)."""

from __future__ import annotations

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=True)


def test_temporal_frame_subclasses_reference_frame() -> None:
    g = _graph()
    assert (
        URIRef(GMEOW + "TemporalFrame"),
        RDFS.subClassOf,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in g
    assert (
        URIRef(GMEOW + "TemporalFrame"),
        RDFS.subClassOf,
        URIRef(GUFO + "Object"),
    ) in g


def test_temporal_frame_component_classes_exist() -> None:
    g = _graph()
    for term in ("TimeScale", "CalendarSystem", "ReferencePosition"):
        assert (URIRef(GMEOW + term), RDF.type, OWL.Class) in g


def test_temporal_frame_seed_individuals() -> None:
    g = _graph()
    # At least 2 time scales
    scales = set(g.subjects(RDF.type, URIRef(GMEOW + "TimeScale")))
    assert len(scales) >= 2

    # At least 2 calendars, including one non-Gregorian
    calendars = set(g.subjects(RDF.type, URIRef(GMEOW + "CalendarSystem")))
    assert len(calendars) >= 2
    assert URIRef(GMEOW + "calendarGregorian") in calendars
    non_gregorian = {
        URIRef(GMEOW + "calendarJulian"),
        URIRef(GMEOW + "calendarHebrew"),
        URIRef(GMEOW + "calendarIslamic"),
        URIRef(GMEOW + "calendarChinese"),
        URIRef(GMEOW + "calendarPersian"),
        URIRef(GMEOW + "calendarEthiopian"),
        URIRef(GMEOW + "calendarCoptic"),
        URIRef(GMEOW + "calendarISOWeek"),
    }
    assert any(c in calendars for c in non_gregorian)

    # All temporal frames use frameRealmTemporal (from places.ttl #70)
    frames = set(g.subjects(RDF.type, URIRef(GMEOW + "TemporalFrame")))
    assert len(frames) >= 2
    for frame in frames:
        assert (
            frame,
            URIRef(GMEOW + "frameRealm"),
            URIRef(GMEOW + "frameRealmTemporal"),
        ) in g


def test_temporal_frame_utc_gregorian_exists_with_components() -> None:
    g = _graph()
    frame = URIRef(GMEOW + "temporalFrameUTCGregorian")
    assert (frame, RDF.type, URIRef(GMEOW + "TemporalFrame")) in g
    assert (
        frame,
        URIRef(GMEOW + "frameTimeScale"),
        URIRef(GMEOW + "timeScaleUTC"),
    ) in g
    assert (
        frame,
        URIRef(GMEOW + "frameCalendarSystem"),
        URIRef(GMEOW + "calendarGregorian"),
    ) in g
    assert (
        frame,
        URIRef(GMEOW + "frameRealm"),
        URIRef(GMEOW + "frameRealmTemporal"),
    ) in g
    assert (
        frame,
        URIRef(GMEOW + "frameKind"),
        URIRef(GMEOW + "frameKindTemporal"),
    ) in g


def test_frame_time_scale_is_functional() -> None:
    g = _graph()
    assert (URIRef(GMEOW + "frameTimeScale"), RDF.type, OWL.FunctionalProperty) in g


def test_has_temporal_frame_is_subproperty_of_has_reference_frame() -> None:
    g = _graph()
    prop = URIRef(GMEOW + "hasTemporalFrame")
    assert (prop, RDF.type, OWL.ObjectProperty) in g
    assert (prop, RDFS.subPropertyOf, URIRef(GMEOW + "hasReferenceFrame")) in g
    assert (prop, RDFS.domain, URIRef(GMEOW + "TimeInterval")) in g
    assert (prop, RDFS.range, URIRef(GMEOW + "TemporalFrame")) in g


def test_in_temporal_frame_is_subproperty_of_has_reference_frame() -> None:
    g = _graph()
    prop = URIRef(GMEOW + "inTemporalFrame")
    assert (prop, RDF.type, OWL.ObjectProperty) in g
    assert (prop, RDFS.subPropertyOf, URIRef(GMEOW + "hasReferenceFrame")) in g
    assert (prop, RDFS.domain, URIRef(GMEOW + "Instant")) in g
    assert (prop, RDFS.range, URIRef(GMEOW + "TemporalFrame")) in g

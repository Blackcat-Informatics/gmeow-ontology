"""Oral tradition & performance lineage guards.

Principles 4, 5, 9, 10, 16.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, RDFS, BNode, Graph, Literal, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
SHACL = Namespace("http://www.w3.org/ns/shacl#")
EX = Namespace("https://example.org/test-music-oral-tradition/")

_QUERIES_DIR = Path(__file__).parents[4] / "queries" / "competency"

_MERGED_GRAPH: Graph | None = None

# Concepts that, if required by a shape targeting MusicalWork/Work, violate the
# oral-tradition guarantee.
_NOTATED_CONCEPTS: frozenset[URIRef] = frozenset(
    {
        GMEOW.realizationModeNotated,
        GMEOW.ScoreEdition,
    }
)


def _graph() -> Graph:
    global _MERGED_GRAPH
    if _MERGED_GRAPH is None:
        _MERGED_GRAPH = load_merged_graph(include_imports=False)
    return _MERGED_GRAPH


def _shapes_requiring_notated(graph: Graph) -> list[tuple[URIRef, URIRef, str]]:
    """Return (nodeShape, propertyShape, reason) tuples that target MusicalWork/Work
    and require a notated Expression.
    """
    violations: list[tuple[URIRef, URIRef, str]] = []
    work_targets = {GMEOW.MusicalWork, GMEOW.Work}

    for node_shape in graph.subjects(RDF.type, SHACL.NodeShape):
        targets = set(graph.objects(node_shape, SHACL.targetClass))
        if not targets & work_targets:
            continue

        visited: set[URIRef] = set()
        to_visit = list(graph.objects(node_shape, SHACL.property))
        to_visit.extend(graph.objects(node_shape, SHACL.node))

        while to_visit:
            current = to_visit.pop()
            if current in visited:
                continue
            visited.add(current)

            # Direct property shape inspection.
            if (current, RDF.type, SHACL.PropertyShape) in graph or any(
                True for _s, _p, _o in graph.triples((current, SHACL.path, None))
            ):
                path = graph.value(current, SHACL.path)
                if path is not None:
                    # Simple path requiring a notated concept.
                    if path in _NOTATED_CONCEPTS:
                        min_count = graph.value(current, SHACL.minCount)
                        if min_count is not None and int(min_count) > 0:
                            violations.append(
                                (
                                    node_shape,
                                    current,
                                    f"sh:minCount {min_count} on notated path {path}",
                                )
                            )

                    # Path sequence containing a notated concept with minCount > 0.
                    if _path_sequence_contains(
                        graph, path, _NOTATED_CONCEPTS
                    ) and _has_positive_min_count(graph, current):
                        violations.append(
                            (
                                node_shape,
                                current,
                                f"path sequence requires a notated concept: {path}",
                            )
                        )

                # sh:hasValue realizationModeNotated.
                if any(
                    obj in _NOTATED_CONCEPTS
                    for obj in graph.objects(current, SHACL.hasValue)
                ):
                    violations.append(
                        (
                            node_shape,
                            current,
                            "sh:hasValue requires a notated concept",
                        )
                    )

                # sh:qualifiedValueShape requiring notated.
                for qvs in graph.objects(current, SHACL.qualifiedValueShape):
                    to_visit.append(qvs)

            # Recurse into nested node shapes.
            for nested in graph.objects(current, SHACL.node):
                to_visit.append(nested)
            for prop in graph.objects(current, SHACL.property):
                to_visit.append(prop)

    return violations


def _path_sequence_contains(
    graph: Graph, path: URIRef | BNode | Literal, concepts: frozenset[URIRef]
) -> bool:
    """Return True if a SHACL path (possibly an RDF list sequence) contains any
    of the given concepts.
    """
    if isinstance(path, URIRef) and path in concepts:
        return True
    if isinstance(path, Literal):
        return False
    # path is URIRef or BNode; walk RDF list sequences via rdf:rest*/rdf:first
    for step in graph.objects(path, RDF.first):
        if step in concepts:
            return True
        # Recurse into nested lists (e.g. alternative paths).
        if _path_sequence_contains(graph, step, concepts):
            return True
    for rest in graph.objects(path, RDF.rest):
        if rest != RDF.nil and _path_sequence_contains(graph, rest, concepts):
            return True
    return False


def _has_positive_min_count(graph: Graph, property_shape: URIRef) -> bool:
    min_count = graph.value(property_shape, SHACL.minCount)
    return min_count is not None and int(min_count) > 0


def test_oral_tradition_work_fixture_exists() -> None:
    """The oral-tradition Raga Yaman work fixture exists and is declared vague."""
    graph = _graph()
    work = URIRef(GMEOW + "fixtureOralRagaYamanWork")
    assert (work, RDF.type, GMEOW.MusicalWork) in graph
    assert (work, GMEOW.hasDeterminacy, GMEOW.determinacyVague) in graph


def test_oral_tradition_expressions_have_no_notated_member() -> None:
    """All Expressions of the oral work are oral, performed, or improvised."""
    graph = _graph()
    expressions = (
        "fixtureRagaYamanOralExpression",
        "fixtureRagaYamanPerformed1960",
        "fixtureRagaYamanImprovised1975",
        "fixtureRagaYamanPerformed1980",
    )
    modes = {
        GMEOW.realizationModeOral,
        GMEOW.realizationModePerformed,
        GMEOW.realizationModeImprovised,
    }
    for term in expressions:
        expr = URIRef(GMEOW + term)
        assert (expr, RDF.type, GMEOW.Expression) in graph
        mode = graph.value(expr, GMEOW.realizationMode)
        assert mode in modes, f"{term} has unexpected realization mode {mode}"


def test_performance_lineage_derivation_chain() -> None:
    """Performances form a wasDerivedFrom descent chain."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "fixtureRagaYamanPerformed1960"),
        GMEOW.wasDerivedFrom,
        URIRef(GMEOW + "fixtureRagaYamanOralExpression"),
    ) in graph
    assert (
        URIRef(GMEOW + "fixtureRagaYamanImprovised1975"),
        GMEOW.wasDerivedFrom,
        URIRef(GMEOW + "fixtureRagaYamanPerformed1960"),
    ) in graph
    assert (
        URIRef(GMEOW + "fixtureRagaYamanPerformed1980"),
        GMEOW.wasDerivedFrom,
        URIRef(GMEOW + "fixtureRagaYamanImprovised1975"),
    ) in graph


def test_tune_family_is_versionset() -> None:
    """The tune family is a VersionSet; memberships are VersionMembership relators."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "fixtureRagaYamanKiranaSet"),
        RDF.type,
        GMEOW.VersionSet,
    ) in graph
    for term in (
        "fixtureRagaYamanKiranaMembership1960",
        "fixtureRagaYamanKiranaMembership1975",
        "fixtureRagaYamanKiranaMembership1980",
        "fixtureRagaYamanContestedMembership",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            GMEOW.VersionMembership,
        ) in graph, f"{term} should be a VersionMembership"


def test_versionset_reused_unchanged() -> None:
    """No new tune-family-specific terms were added to the versions slice."""
    graph = _graph()
    versions_defined_by = URIRef("https://blackcatinformatics.ca/gmeow/slices/versions")
    # There should be no VersionSet or VersionMembership subclass in versions.ttl.
    for term in ("TuneFamily", "TuneFamilyMembership"):
        iri = URIRef(GMEOW + term)
        assert (
            iri,
            RDFS.isDefinedBy,
            versions_defined_by,
        ) not in graph, f"versions.ttl should not define {term}"


def test_contested_membership_is_suppressed_not_deleted() -> None:
    """The contested membership is displayable false and retained in the graph."""
    graph = _graph()
    membership = URIRef(GMEOW + "fixtureRagaYamanContestedMembership")
    assert (membership, RDF.type, GMEOW.VersionMembership) in graph
    assert (membership, GMEOW.displayable, Literal(False)) in graph


def test_transmission_event_and_roles() -> None:
    """Transmission event uses eventTypeTransmission and transmitter/learner roles."""
    graph = _graph()
    event = URIRef(GMEOW + "fixtureKiranaTransmissionEvent")
    assert (event, RDF.type, GMEOW.Event) in graph
    assert (event, GMEOW.eventType, GMEOW.eventTypeTransmission) in graph
    for term in (
        "fixtureKiranaTransmitterParticipation",
        "fixtureKiranaLearnerParticipation",
    ):
        part = URIRef(GMEOW + term)
        assert (part, RDF.type, GMEOW.Participation) in graph
        assert (part, GMEOW.participationEvent, event) in graph


def test_no_shape_requires_notated_expression() -> None:
    """The oral-tradition guarantee: no SHACL shape targeting MusicalWork/Work
    requires a notated Expression.
    """
    violations = _shapes_requiring_notated(_graph())
    assert not violations, (
        "SHACL shapes violate the oral-tradition guarantee:\n"
        + "\n".join(f"  {ns} / {ps}: {reason}" for ns, ps, reason in violations)
    )


def test_competency_query_oral_works() -> None:
    """CQ6 returns the oral Raga Yaman work with at least three performances."""
    graph = _graph()
    query = (_QUERIES_DIR / "music-oral-works.rq").read_text()
    results = list(graph.query(query))
    assert len(results) >= 1, "Expected at least one oral-tradition work result"
    works = {row[0] for row in results}
    assert GMEOW.fixtureOralRagaYamanWork in works
    row = next(row for row in results if row[0] == GMEOW.fixtureOralRagaYamanWork)
    assert int(row[1]) >= 3


def test_competency_query_gharana_memberships() -> None:
    """CQ7 returns three Kirana-gharana memberships and excludes the suppressed one."""
    graph = _graph()
    query = (_QUERIES_DIR / "music-gharana-memberships.rq").read_text()
    results = list(graph.query(query))
    assert len(results) == 3, f"Expected 3 displayed memberships, got {len(results)}"
    memberships = {row[0] for row in results}
    assert GMEOW.fixtureRagaYamanContestedMembership not in memberships
    performances = {row[1] for row in results}
    assert GMEOW.fixtureRagaYamanPerformed1960 in performances
    assert GMEOW.fixtureRagaYamanImprovised1975 in performances
    assert GMEOW.fixtureRagaYamanPerformed1980 in performances

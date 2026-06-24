"""Closed-world SHACL data-shape tests (#39, epic #35).

ExampleConformance SHACL tests (positive / negative lane for ABox fixtures and
inline synthetic graphs) have been migrated to the Rust integration test at
``crates/validate/tests/ontology_conformance.rs`` (#867).

Retained here (not migratable to the Rust harness):
  test_no_nodeshape_iri_collision_across_shape_files — whole-tooling IRI
    uniqueness sweep across all shape files; requires Python's filesystem +
    rdflib graph scan and has no stable mapping to a Rust integration test.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, Graph
from gmeow_rdf.compat.rdflib.term import Node


def test_no_nodeshape_iri_collision_across_shape_files() -> None:
    """Every sh:NodeShape IRI is owned by exactly one shape file (#478).

    ``_shapes_turtle`` merges hand-authored shapes, generated shapes, and slice
    shapes into a single document. If two files declare the same ``sh:NodeShape``
    subject, the definitions fuse, producing a shape whose meaning depends on
    which files happen to be parsed together. This guard fails CI mechanically
    if that ever happens.
    """
    from collections import defaultdict

    from gmeow_rdf.compat.rdflib.namespace import SH

    from gmeow_tools.config import GENERATED_SHAPES_DIR, SHAPES_DIR
    from gmeow_tools.slices import iter_slice_shape_files

    files = [
        *sorted(SHAPES_DIR.glob("*.ttl")),
        *sorted(GENERATED_SHAPES_DIR.glob("*.ttl")),
        *iter_slice_shape_files(),
    ]
    iri_to_files: dict[Node, list[Path]] = defaultdict(list)
    for path in files:
        graph = Graph().parse(path, format="turtle")
        for iri in graph.subjects(RDF.type, SH.NodeShape):
            iri_to_files[iri].append(path)

    collisions = {iri: paths for iri, paths in iri_to_files.items() if len(paths) > 1}
    assert not collisions, (
        "sh:NodeShape IRIs declared in more than one shape file: "
        + "; ".join(
            f"{iri} in {', '.join(str(p) for p in paths)}"
            for iri, paths in sorted(collisions.items(), key=lambda kv: str(kv[0]))
        )
    )
